defmodule DeciduousMcp.Graph.Candidates do
  @moduledoc """
  Write-time candidate discovery (Jev-Mem section 3.2): for a node that has
  no parent, a short ranked list of nodes it probably belongs under.

  The paper separates candidate discovery from relation judgment: cheap,
  deterministic retrieval proposes at most K candidates and something
  smarter decides. Here the something smarter is the agent that wrote the
  node. Nothing in this module links anything; `add_node` and
  `find_orphans` only show the list.

  ## Pool (the cost bound)

  Every call scores at most #{200 + 50 + 50} rows, whatever the workspace
  size, drawn by three id queries on the workspace's own btree indexes:

    * the #{200} most recent live nodes of a type that fits as a parent;
    * the #{50} most title-similar (pg_trgm `similarity` > 0.1) among the
      #{1000} most recent live nodes of a fitting type: at most 1,000
      similarity evaluations;
    * up to #{50} most recent live nodes of a fitting type on the same branch,
      only when a branch other than main/master is given.

  The union is deduplicated, then one query fetches those rows with their
  trigram similarities and the ranking runs in Elixir.

  Why not `title % $1` on the existing GIN index (`idx_nodes_title_trgm`):
  that index is over every workspace, so its cost follows the whole table,
  not the workspace or the bound. On the dev database (29,853 nodes) one
  such query on blimp rechecked 3,858 index hits to keep 4 rows: 141 ms.
  The 1,000-row window over `decision_nodes_workspace_id_inserted_at_index`
  took 16 ms for the same title and found 224 rows above 0.1.

  The price: a node older than the 1,000 most recent of fitting type is
  found only through the branch pool (the 50 most recent on the same
  branch). A long-lived goal on another branch is the case this misses.

  ## Signals and weights

  | signal             | contribution                                       |
  |--------------------|----------------------------------------------------|
  | `type_fit`         | 3 x fit, fit from `parent_fit/1` (0 excludes)       |
  | `text_similarity`  | 4 x max(title sim, title+description sim)          |
  | `same_branch`      | +2 same feature branch; +0.5 same main/master      |
  | `shared_files`     | +1.5 per shared file, at most 2 counted            |
  | `age_hours`        | 1.5 / (1 + age_hours / 24)                         |
  | `open`             | +1 for a pending/active goal or decision           |
  | `closed`           | -2 for rejected/superseded/abandoned               |

  Ties break on newer `inserted_at`, then id, so the same graph gives the
  same list.
  """
  import Ecto.Query

  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.Node

  @recent_pool 200
  @trigram_pool 50
  @text_window 1000
  @min_similarity 0.1
  @branch_pool 50
  # Everything lands on a trunk eventually; sharing it says little.
  @trunk_branches ~w(main master)
  @default_limit 5

  @open_statuses ~w(pending active)
  @dead_statuses ~w(rejected superseded abandoned)

  # Canonical flow goal -> option -> decision -> action -> outcome: the
  # type the flow names scores 1.0. The smaller weights are shapes the docs
  # allow and the live graphs are full of (dev DB, live non-took_from edges:
  # action under goal 2,085 vs under decision 1,415; observation under
  # observation 51,434, under goal 10,087, under action 1,163). Shapes the
  # docs forbid are 0 and never suggested, however common: option under
  # decision (663 edges) is "options after a decision", and goal -> decision
  # without an option in between is kept low rather than equal.
  @fit %{
    "goal" => %{},
    "option" => %{"goal" => 1.0},
    "decision" => %{"option" => 1.0, "goal" => 0.4, "observation" => 0.3},
    "action" => %{"decision" => 1.0, "goal" => 0.5, "action" => 0.4, "observation" => 0.3},
    "outcome" => %{"action" => 1.0, "outcome" => 0.3, "decision" => 0.3, "goal" => 0.2},
    "observation" => %{
      "action" => 1.0,
      "goal" => 0.9,
      "observation" => 0.8,
      "decision" => 0.8,
      "outcome" => 0.7,
      "option" => 0.5
    },
    "revisit" => %{"decision" => 1.0, "outcome" => 0.8, "observation" => 0.8, "action" => 0.5}
  }

  @doc "The pool bound: the most rows one call reads."
  def pool_bound, do: @recent_pool + @trigram_pool + @branch_pool

  @doc """
  Parent types that fit under the flow for `child_type`, with a weight in
  (0, 1]. A goal has none: goals are roots. `feedback` (imported, not
  writable through MCP) ranks like an observation. Any other type raises.
  """
  def parent_fit("feedback"), do: @fit["observation"]

  def parent_fit(child_type) do
    case Map.fetch(@fit, child_type) do
      {:ok, fit} -> fit
      :error -> raise ArgumentError, "no parent fit for node type #{inspect(child_type)}"
    end
  end

  @doc """
  Ranked parent suggestions for a node described by `node`, a map with
  `:node_type` and `:title` (required) and `:description`, `:branch`,
  `:files` (optional; `:files` may be a list or a comma-separated string).

  Options: `:limit` (default #{@default_limit}), `:exclude` (ids never
  suggested: the node itself, and for an existing node its descendants so a
  suggestion cannot close a cycle), `:as_of` (a DateTime: rank as if at
  that moment, seeing only nodes inserted before it and measuring recency
  from it; for replaying past writes; default now).

  Returns a list of maps: `id`, `change_id`, `node_type`, `title`, `status`,
  `branch`, `score` and `signals`.
  """
  def suggest_parents(workspace_id, node, opts \\ []) do
    fit = parent_fit(fetch!(node, :node_type))
    title = fetch!(node, :title)

    if fit == %{} do
      []
    else
      rank(workspace_id, node, title, fit, opts)
    end
  end

  defp rank(workspace_id, node, title, fit, opts) do
    limit = Keyword.get(opts, :limit, @default_limit)
    exclude = Keyword.get(opts, :exclude, [])
    as_of = Keyword.get(opts, :as_of)
    now = as_of || DateTime.utc_now()
    branch = blank_to_nil(Map.get(node, :branch))
    description = blank_to_nil(Map.get(node, :description))
    files = normalize_files(Map.get(node, :files))
    types = Map.keys(fit)

    base =
      from(n in Node,
        where: n.workspace_id == ^workspace_id and is_nil(n.deleted_at),
        where: n.node_type in ^types,
        where: n.id not in ^exclude,
        select: n.id
      )

    base = if as_of, do: where(base, [n], n.inserted_at < ^as_of), else: base

    recent = base |> order_by([n], desc: n.inserted_at) |> limit(@recent_pool) |> Repo.all()

    window =
      base
      |> order_by([n], desc: n.inserted_at)
      |> limit(@text_window)
      |> exclude(:select)
      |> select([n], %{id: n.id, title: n.title})

    similar =
      from(w in subquery(window),
        where: fragment("similarity(?, ?)", w.title, ^title) > @min_similarity,
        order_by: [desc: fragment("similarity(?, ?)", w.title, ^title)],
        limit: @trigram_pool,
        select: w.id
      )
      |> Repo.all()

    same_branch =
      if branch && branch not in @trunk_branches,
        do:
          base
          |> where([n], fragment("?->>'branch'", n.metadata) == ^branch)
          |> order_by([n], desc: n.inserted_at)
          |> limit(@branch_pool)
          |> Repo.all(),
        else: []

    ids = Enum.uniq(recent ++ similar ++ same_branch)
    text = if description, do: title <> " " <> description, else: title

    from(n in Node,
      where: n.id in ^ids,
      select:
        {n, fragment("similarity(?, ?)", n.title, ^title),
         fragment("similarity(? || ' ' || coalesce(?, ''), ?)", n.title, n.description, ^text)}
    )
    |> Repo.all()
    |> Enum.map(&score(&1, fit, branch, files, now))
    |> Enum.sort_by(fn {s, n, _} ->
      {-s, -DateTime.to_unix(n.inserted_at, :microsecond), n.id}
    end)
    |> Enum.take(limit)
    |> Enum.map(fn {s, n, signals} ->
      %{
        id: n.id,
        change_id: n.change_id,
        node_type: n.node_type,
        title: n.title,
        status: n.status,
        branch: Node.branch(n),
        score: Float.round(s, 3),
        signals: signals
      }
    end)
  end

  defp score({n, title_sim, text_sim}, fit, branch, files, now) do
    type_fit = Map.fetch!(fit, n.node_type)
    sim = max(title_sim, text_sim)
    same_branch? = branch != nil and Node.branch(n) == branch
    branch_weight = if branch in @trunk_branches, do: 0.5, else: 2

    shared =
      if files == [],
        do: [],
        else: Enum.filter(normalize_files(n.metadata["files"]), &(&1 in files))

    age_hours = max(DateTime.diff(now, n.inserted_at, :second), 0) / 3600
    open? = n.node_type in ~w(goal decision) and n.status in @open_statuses
    dead? = n.status in @dead_statuses

    s =
      3 * type_fit + 4 * sim + if(same_branch?, do: branch_weight, else: 0) +
        1.5 * min(length(shared), 2) + 1.5 / (1 + age_hours / 24) +
        if(open?, do: 1, else: 0) + if(dead?, do: -2, else: 0)

    signals =
      %{type_fit: type_fit, age_hours: Float.round(age_hours, 1)}
      |> put_if(sim > 0, :text_similarity, Float.round(sim, 3))
      |> put_if(same_branch?, :same_branch, true)
      |> put_if(shared != [], :shared_files, shared)
      |> put_if(open?, :open, true)
      |> put_if(dead?, :closed, n.status)

    {s, n, signals}
  end

  defp put_if(map, true, key, value), do: Map.put(map, key, value)
  defp put_if(map, false, _key, _value), do: map

  defp fetch!(node, key) do
    case Map.get(node, key) do
      v when is_binary(v) and v != "" -> v
      other -> raise ArgumentError, "suggest_parents needs #{key}, got #{inspect(other)}"
    end
  end

  defp blank_to_nil(v) when is_binary(v), do: if(String.trim(v) == "", do: nil, else: v)
  defp blank_to_nil(_), do: nil

  # The CLI's `-f "a.rs,b.rs"` and MCP's array both reach metadata.
  defp normalize_files(list) when is_list(list), do: Enum.filter(list, &is_binary/1)

  defp normalize_files(s) when is_binary(s),
    do: s |> String.split(",") |> Enum.map(&String.trim/1) |> Enum.reject(&(&1 == ""))

  defp normalize_files(_), do: []
end
