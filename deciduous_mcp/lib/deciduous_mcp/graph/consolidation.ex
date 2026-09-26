defmodule DeciduousMcp.Graph.Consolidation do
  @moduledoc """
  A read-only consolidation pass over one workspace: what an agent might
  tidy, with the node ids to do it with, never the tidying itself.

  Jev-Mem (section 3.2, appendix B.3) runs periodic maintenance over a
  bounded neighbourhood for redundancy, contradiction and obsolescence, and
  records decisions and links without replacing the source memories. The
  deciduous translation keeps that property strictly: this module issues
  SELECTs only. Every finding carries node ids and a suggested
  `update_node` / `add_edge`, and the agent decides.

  Four sections:

    * `duplicate_goals` -- goal pairs whose titles are trigram-similar
      (pg_trgm `similarity/2`), labelled `merge`, `keep_separate` or
      `uncertain` with the evidence that chose the label. The labels are
      B.3's representation Choice minus `promote`, which needs a System-Two
      summarizer deciduous does not have.
    * `competing_decisions` -- two live decisions with similar titles that
      share a direct parent (usually the option) or a nearest goal ancestor,
      with no revisit between them. That is the shape a missing revisit
      leaves: the second decision replaced the first and nobody said so.
    * `stale_actions` -- pending/active actions older than `stale_days` from
      which no outcome is reachable without passing through another action.
    * `parentless` -- live actions and outcomes with no live parent. The
      first 10 listed carry `suggested_parent`, the top hit of
      `DeciduousMcp.Graph.Candidates.suggest_parents/3` (or nil).

  "Live" means not soft-deleted. A decision is current unless its status
  is superseded, abandoned or rejected: status `active` is almost never set
  (the default is `pending`; on the dev database 0 of 1,587 decisions are
  `active`), so filtering on it would find nothing.

  Bounded: at most `max_nodes` goals and decisions (most recent first) are
  compared pairwise, at most `max_pairs` similar pairs per section are
  examined, and at most `max_items` stale or parentless nodes are listed.
  Every cap and every total it cut appears in the result.
  """

  import Ecto.Query

  alias DeciduousMcp.Graph.Candidates
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.{Edge, Node}

  @defaults %{similarity: 0.6, stale_days: 14, max_pairs: 50, max_items: 100, max_nodes: 300}
  @limits %{
    similarity: {0.3, 1.0},
    stale_days: {0, 3650},
    max_pairs: {1, 1000},
    max_items: {1, 1000},
    max_nodes: {2, 1000}
  }

  # Not current: a decision in one of these has already been replaced or
  # dropped, so it cannot compete with anything.
  @closed_statuses ~w(superseded abandoned rejected)
  @open_action_statuses ~w(pending active)

  # parentless nodes that get a ranked parent guess from Candidates.
  @suggest_max 10

  # merge needs this much title similarity when descriptions do not
  # contradict it; keep_separate fires when both descriptions exist and
  # share less than @distinct_description of their trigrams.
  @merge_title 0.85
  @merge_description 0.5
  @distinct_description 0.2

  def defaults, do: @defaults
  def limits, do: @limits

  @doc """
  The report for `workspace_id`. `opts` is a map or keyword list with any of
  `similarity`, `stale_days`, `max_pairs`, `max_items`, `max_nodes`; an
  out-of-range value is an error, not clamped.

  Returns `{:ok, map}` or `{:error, message}`.
  """
  def report(workspace_id, opts \\ %{}) do
    with {:ok, o} <- options(opts) do
      started = System.monotonic_time(:millisecond)
      graph = load_graph(workspace_id)

      {goal_pairs, goal_stats} = similar_pairs(workspace_id, "goal", o)
      {decision_pairs, decision_stats} = similar_pairs(workspace_id, "decision", o)
      {stale, stale_total} = stale_actions(graph, o)
      {parentless, parentless_total} = parentless(graph, o)
      competing = competing_decisions(decision_pairs, graph)

      {:ok,
       %{
         read_only: true,
         settings: o,
         elapsed_ms: System.monotonic_time(:millisecond) - started,
         duplicate_goals:
           Map.put(goal_stats, :findings, Enum.map(goal_pairs, &label_goal_pair(&1, graph))),
         competing_decisions:
           decision_stats
           |> Map.put(:findings, competing)
           |> Map.put(
             :note,
             "pairs_examined similar pairs were checked for a shared parent or goal and a revisit between them"
           ),
         stale_actions: %{
           older_than_days: o.stale_days,
           total: stale_total,
           listed: length(stale),
           truncated: stale_total > length(stale),
           findings: stale
         },
         parentless: %{
           total: parentless_total,
           listed: length(parentless),
           truncated: parentless_total > length(parentless),
           findings: parentless
         }
       }}
    end
  end

  # --- options ---

  defp options(opts) when is_list(opts), do: options(Map.new(opts))

  defp options(opts) when is_map(opts) do
    Enum.reduce_while(@defaults, {:ok, %{}}, fn {key, default}, {:ok, acc} ->
      value = Map.get(opts, key, Map.get(opts, Atom.to_string(key))) || default
      {lo, hi} = @limits[key]

      cond do
        not is_number(value) ->
          {:halt, {:error, "#{key} must be a number, got #{inspect(value)}"}}

        key != :similarity and not is_integer(value) ->
          {:halt, {:error, "#{key} must be an integer, got #{inspect(value)}"}}

        value < lo or value > hi ->
          {:halt, {:error, "#{key} must be between #{lo} and #{hi}, got #{inspect(value)}"}}

        true ->
          {:cont, {:ok, Map.put(acc, key, value)}}
      end
    end)
  end

  # --- the graph, flat ---

  # Two flat reads and walks in memory, the way Query.find_orphans does it:
  # the recursive-CTE version of that walk took 25 s on an 8,000-node chain.
  defp load_graph(workspace_id) do
    nodes =
      from(n in Node,
        where: n.workspace_id == ^workspace_id and is_nil(n.deleted_at),
        select: %{
          id: n.id,
          node_type: n.node_type,
          status: n.status,
          title: n.title,
          branch: fragment("?->>'branch'", n.metadata),
          workspace_id: n.workspace_id,
          inserted_at: n.inserted_at
        }
      )
      |> Repo.all()
      |> Map.new(&{&1.id, &1})

    edges =
      from(e in Edge,
        where: e.workspace_id == ^workspace_id,
        select: {e.from_node_id, e.to_node_id}
      )
      |> Repo.all()
      |> Enum.filter(fn {f, t} -> Map.has_key?(nodes, f) and Map.has_key?(nodes, t) end)

    %{
      nodes: nodes,
      children: Enum.group_by(edges, &elem(&1, 0), &elem(&1, 1)),
      parents: Enum.group_by(edges, &elem(&1, 1), &elem(&1, 0))
    }
  end

  # --- similar pairs, in SQL ---

  # One self-join over the newest max_nodes live nodes of the type. The
  # window count is every pair above the threshold, so the output can say
  # how many the pair cap left out.
  #
  # The cost is n^2/2 similarity() calls, and max_nodes is what bounds it.
  # Measured on the dev database's goal titles: 500 nodes 1.5 s, 1,000
  # nodes 4.9 s, 2,000 nodes 16.4 s. The pg_trgm GIN index does not help:
  # with `b.title % a.title` (threshold set LOCAL) the planner still chose
  # a nested loop over the CTE (33.5 s at 2,000), and a LATERAL probe per
  # row took 20.0 s. Hence a default of 300 and a ceiling of 1,000. The
  # largest real workspace has 238 decisions and 189 goals, so the default
  # covers every workspace on the server today, in ~150-230 ms.
  defp similar_pairs(workspace_id, type, o) do
    # Goals pass an empty list: every goal is a candidate whatever its status.
    closed = if type == "decision", do: @closed_statuses, else: []

    sql = """
    WITH c AS (
      SELECT id, title, description, metadata->>'prompt' AS prompt
      FROM decision_nodes
      WHERE workspace_id = $1 AND node_type = $2 AND deleted_at IS NULL AND NOT (status = ANY($5))
      ORDER BY inserted_at DESC
      LIMIT $3
    )
    SELECT a.id, b.id,
           similarity(a.title, b.title) AS ts,
           CASE WHEN coalesce(a.description, '') <> '' AND coalesce(b.description, '') <> ''
                THEN similarity(a.description, b.description) END AS ds,
           coalesce(a.prompt <> '' AND a.prompt = b.prompt, false) AS same_prompt,
           count(*) OVER () AS above
    FROM c a JOIN c b ON a.id < b.id
    WHERE similarity(a.title, b.title) >= $4
    ORDER BY ts DESC, a.id, b.id
    LIMIT $6
    """

    %{rows: rows} =
      Repo.query!(sql, [
        Ecto.UUID.dump!(workspace_id),
        type,
        o.max_nodes,
        o.similarity * 1.0,
        closed,
        o.max_pairs
      ])

    total = count_candidates(workspace_id, type)
    considered = min(total, o.max_nodes)

    above =
      case rows do
        [] -> 0
        [row | _] -> List.last(row)
      end

    pairs =
      Enum.map(rows, fn [a, b, ts, ds, same_prompt, _] ->
        %{
          a: Ecto.UUID.load!(a),
          b: Ecto.UUID.load!(b),
          title_similarity: round4(ts),
          description_similarity: ds && round4(ds),
          same_prompt: same_prompt
        }
      end)

    {pairs,
     %{
       nodes_total: total,
       nodes_compared: considered,
       node_cap: o.max_nodes,
       pairs_compared: div(considered * (considered - 1), 2),
       pairs_above_threshold: above,
       pairs_examined: length(pairs),
       pair_cap: o.max_pairs,
       truncated: above > length(pairs)
     }}
  end

  defp count_candidates(workspace_id, type) do
    q =
      from(n in Node,
        where: n.workspace_id == ^workspace_id and n.node_type == ^type and is_nil(n.deleted_at)
      )

    q = if type == "decision", do: where(q, [n], n.status not in ^@closed_statuses), else: q
    Repo.aggregate(q, :count)
  end

  defp round4(x), do: Float.round(x * 1.0, 4)

  # --- duplicate goals ---

  defp label_goal_pair(pair, graph) do
    %{a: a, b: b} = pair
    [older, newer] = Enum.sort_by([a, b], &{graph.nodes[&1].inserted_at, &1}, &sort_le/2)
    nested? = reaches?(a, b, graph.children) or reaches?(b, a, graph.children)
    ds = pair.description_similarity
    ts = pair.title_similarity

    {only_older, only_newer} =
      token_difference(graph.nodes[older].title, graph.nodes[newer].title)

    substitution? = only_older != [] and only_newer != []

    {label, reasons} =
      cond do
        nested? ->
          {"keep_separate", ["one goal is an ancestor of the other: a sub-goal, not a duplicate"]}

        ds != nil and ds < @distinct_description ->
          {"keep_separate",
           ["descriptions share #{ds} of their trigrams: similar titles, different work"]}

        # pg_trgm drops every non-alphanumeric character, so "C++ Backend"
        # and "C# Backend" are both "c backend" and score 1.0. Each title
        # having a word the other lacks is a substitution, not a rewording,
        # and trigrams alone cannot tell which.
        substitution? ->
          {"uncertain",
           [
             "each title has words the other lacks (#{inspect(only_older)} vs #{inspect(only_newer)}): " <>
               "trigram similarity #{ts} cannot tell a rewording from a different subject"
           ]}

        pair.same_prompt ->
          {"merge", ["both goals carry the same verbatim prompt: one request logged twice"]}

        ts >= @merge_title and (ds == nil or ds >= @merge_description) ->
          {"merge",
           [
             "title similarity #{ts} >= #{@merge_title}, one title's words contain the other's" <>
               if(ds,
                 do: ", description similarity #{ds} >= #{@merge_description}",
                 else: ", no descriptions to contradict it"
               )
           ]}

        true ->
          {"uncertain",
           [
             "title similarity #{ts} is below #{@merge_title}, or the descriptions (#{inspect(ds)}) neither match nor clearly differ"
           ]}
      end

    %{
      label: label,
      nodes: [node_ref(graph, older), node_ref(graph, newer)],
      evidence: %{
        title_similarity: ts,
        description_similarity: ds,
        same_prompt: pair.same_prompt,
        one_is_ancestor_of_other: nested?,
        words_only_in: %{older => only_older, newer => only_newer},
        children: %{
          older => length(Map.get(graph.children, older, [])),
          newer => length(Map.get(graph.children, newer, []))
        }
      },
      reasons: reasons,
      suggestion: goal_suggestion(label, older, newer)
    }
  end

  # Words, lowercased, split on whitespace and trimmed of surrounding
  # punctuation, so "C++" and "C#" stay distinct where pg_trgm merges them.
  defp token_difference(x, y) do
    tx = tokens(x)
    ty = tokens(y)
    {MapSet.difference(tx, ty) |> Enum.sort(), MapSet.difference(ty, tx) |> Enum.sort()}
  end

  defp tokens(title) do
    title
    |> String.downcase()
    |> String.split()
    |> Enum.map(&String.trim(&1, ~s{.,:;()[]"'-}))
    |> Enum.reject(&(&1 == ""))
    |> MapSet.new()
  end

  defp goal_suggestion("merge", older, newer),
    do:
      "keep #{older}; add_edge #{older} -> each child of #{newer}, then update_node #{newer} status=superseded"

  defp goal_suggestion("keep_separate", _, _), do: "none: leave both"

  defp goal_suggestion("uncertain", older, newer),
    do: "show_node #{older} and #{newer} and compare their subtrees before merging"

  # --- competing decisions ---

  defp competing_decisions(pairs, graph) do
    Enum.flat_map(pairs, fn pair ->
      %{a: a, b: b} = pair
      shared_parents = shared(graph.parents, a, b)

      shared_goals =
        MapSet.intersection(nearest_goals(a, graph), nearest_goals(b, graph)) |> Enum.sort()

      revisit = revisit_between(a, b, graph)

      if (shared_parents != [] or shared_goals != []) and revisit == nil do
        [older, newer] = Enum.sort_by([a, b], &{graph.nodes[&1].inserted_at, &1}, &sort_le/2)

        [
          %{
            nodes: [node_ref(graph, older), node_ref(graph, newer)],
            evidence: %{
              title_similarity: pair.title_similarity,
              description_similarity: pair.description_similarity,
              shared_parents: shared_parents,
              shared_goal_ancestors: shared_goals,
              revisit_between: false
            },
            suggestion:
              "if #{newer} replaced #{older}: add_node revisit with parent_id #{older}, add_edge <revisit> -> #{newer}, update_node #{older} status=superseded; if both hold, leave them"
          }
        ]
      else
        []
      end
    end)
  end

  defp shared(index, a, b) do
    MapSet.intersection(MapSet.new(Map.get(index, a, [])), MapSet.new(Map.get(index, b, [])))
    |> Enum.sort()
  end

  # The first goals met walking up from the node. Not every goal above it:
  # a workspace hung under one root goal would make every pair share that.
  defp nearest_goals(id, graph) do
    walk_up(Map.get(graph.parents, id, []), graph, MapSet.new([id]), MapSet.new())
  end

  defp walk_up([], _graph, _seen, goals), do: goals

  defp walk_up(frontier, graph, seen, goals) do
    {found, rest} = Enum.split_with(frontier, &(graph.nodes[&1].node_type == "goal"))
    seen = Enum.reduce(frontier, seen, &MapSet.put(&2, &1))

    next =
      rest
      |> Enum.flat_map(&Map.get(graph.parents, &1, []))
      |> Enum.reject(&MapSet.member?(seen, &1))
      |> Enum.uniq()

    walk_up(next, graph, seen, Enum.reduce(found, goals, &MapSet.put(&2, &1)))
  end

  # A revisit connects the two when it sits on a directed path from one to
  # the other, or is a direct neighbour (either direction) of both.
  defp revisit_between(a, b, graph) do
    revisit? = &(graph.nodes[&1].node_type == "revisit")
    down_a = descendants(a, graph.children)
    down_b = descendants(b, graph.children)
    up_a = descendants(a, graph.parents)
    up_b = descendants(b, graph.parents)
    near = fn x -> MapSet.new(Map.get(graph.children, x, []) ++ Map.get(graph.parents, x, [])) end

    [
      MapSet.intersection(down_a, up_b),
      MapSet.intersection(down_b, up_a),
      MapSet.intersection(near.(a), near.(b))
    ]
    |> Enum.flat_map(&MapSet.to_list/1)
    |> Enum.find(revisit?)
  end

  defp descendants(id, index), do: reach(Map.get(index, id, []), index, MapSet.new())

  defp reach([], _index, seen), do: seen

  defp reach(frontier, index, seen) do
    fresh = frontier |> Enum.reject(&MapSet.member?(seen, &1)) |> Enum.uniq()
    seen = Enum.reduce(fresh, seen, &MapSet.put(&2, &1))
    reach(Enum.flat_map(fresh, &Map.get(index, &1, [])), index, seen)
  end

  defp reaches?(from, to, index), do: MapSet.member?(descendants(from, index), to)

  # --- stale actions ---

  # An action has an outcome when one is reachable from it without passing
  # through another action or a goal: one multi-source walk up from every
  # outcome, stopping at the first action on each path.
  defp stale_actions(graph, o) do
    cutoff = DateTime.add(DateTime.utc_now(), -o.stale_days * 86_400, :second)
    concluded = actions_with_outcome(graph)

    stale =
      graph.nodes
      |> Map.values()
      |> Enum.filter(fn n ->
        n.node_type == "action" and n.status in @open_action_statuses and
          DateTime.compare(n.inserted_at, cutoff) == :lt and not MapSet.member?(concluded, n.id)
      end)
      |> Enum.sort_by(&{&1.inserted_at, &1.id}, &sort_le/2)

    listed =
      stale
      |> Enum.take(o.max_items)
      |> Enum.map(fn n ->
        n.id
        |> then(&node_ref(graph, &1))
        |> Map.put(:age_days, DateTime.diff(DateTime.utc_now(), n.inserted_at, :day))
        |> Map.put(
          :suggestion,
          "add_node outcome with parent_id #{n.id}, or update_node #{n.id} status=abandoned"
        )
      end)

    {listed, length(stale)}
  end

  defp actions_with_outcome(graph) do
    outcomes = for {id, %{node_type: "outcome"}} <- graph.nodes, do: id
    climb(outcomes, graph, MapSet.new(outcomes), MapSet.new())
  end

  defp climb([], _graph, _seen, found), do: found

  defp climb(frontier, graph, seen, found) do
    parents =
      frontier
      |> Enum.flat_map(&Map.get(graph.parents, &1, []))
      |> Enum.reject(&MapSet.member?(seen, &1))
      |> Enum.uniq()

    seen = Enum.reduce(parents, seen, &MapSet.put(&2, &1))
    {actions, others} = Enum.split_with(parents, &(graph.nodes[&1].node_type == "action"))
    others = Enum.reject(others, &(graph.nodes[&1].node_type == "goal"))
    climb(others, graph, seen, Enum.reduce(actions, found, &MapSet.put(&2, &1)))
  end

  # --- parentless ---

  defp parentless(graph, o) do
    all =
      graph.nodes
      |> Map.values()
      |> Enum.filter(
        &(&1.node_type in ~w(action outcome) and not Map.has_key?(graph.parents, &1.id))
      )
      |> Enum.sort_by(&{&1.inserted_at, &1.id}, &sort_le/2)

    listed = Enum.take(all, o.max_items)
    guesses = parent_guesses(Enum.take(listed, @suggest_max), graph)

    listed =
      Enum.map(listed, fn n ->
        want =
          if n.node_type == "outcome",
            do: "the action that produced it",
            else: "the decision that spawned it"

        node_ref(graph, n.id)
        |> Map.put(:suggestion, "add_edge <#{want}> -> #{n.id}")
        |> Map.put(:suggested_parent, Map.get(guesses, n.id))
      end)

    {listed, length(all)}
  end

  # Candidates.suggest_parents (02-candidate-parents) for the first
  # @suggest_max listed nodes, called the way find_orphans calls it: the
  # node's own descendants excluded (linking to one would close a cycle)
  # and ranked as of a second after the node was written. About 20 ms a
  # node on the dev database, hence the cap. Only the top hit is kept.
  defp parent_guesses([], _graph), do: %{}

  defp parent_guesses(nodes, graph) do
    ids = Enum.map(nodes, & &1.id)

    details =
      from(n in Node,
        where: n.id in ^ids,
        select: {n.id, %{description: n.description, files: fragment("?->'files'", n.metadata)}}
      )
      |> Repo.all()
      |> Map.new()

    Map.new(nodes, fn n ->
      d = details[n.id]

      top =
        Candidates.suggest_parents(
          n.workspace_id,
          %{
            node_type: n.node_type,
            title: n.title,
            description: d.description,
            branch: n.branch,
            files: d.files
          },
          limit: 1,
          exclude: [n.id | MapSet.to_list(descendants(n.id, graph.children))],
          as_of: DateTime.add(n.inserted_at, 1, :second)
        )
        |> List.first()

      {n.id, top && Map.take(top, [:id, :node_type, :title, :status, :score, :signals])}
    end)
  end

  # --- helpers ---

  defp node_ref(graph, id) do
    n = graph.nodes[id]

    %{
      id: id,
      node_type: n.node_type,
      status: n.status,
      title: n.title,
      branch: n.branch,
      created_at: DateTime.to_iso8601(n.inserted_at)
    }
  end

  defp sort_le({t1, id1}, {t2, id2}) do
    case DateTime.compare(t1, t2) do
      :lt -> true
      :gt -> false
      :eq -> id1 <= id2
    end
  end
end
