defmodule DeciduousMcp.Graph.Related do
  @moduledoc """
  Relations that exact shared identifiers already imply (Jev-Mem, section
  3.2: "exact shared identifiers directly create entity relations").

  A node's metadata may carry `files` (paths it touched) and `commit` (the
  git commit it was linked to). Two live nodes in one workspace that name the
  same path, or the same commit, are related. Nothing here writes an edge:
  the relation is read off the metadata on every call, so it can never go
  stale when a node's files are edited, and it never shows up in get_graph,
  /export or find_orphans as if someone had drawn it.

  ## Identity

  Measured on the dev database (29,853 nodes, 2,335 with files, 2,169 with
  a commit), not assumed:

    * A path is compared after `normalize_path/1`: whitespace trimmed, a
      leading `./` dropped, `//` and `/./` collapsed. Case is kept (two
      paths differing only by case never occur in the data, and on Linux
      they are different files). A leading `/` is kept: the six such paths
      are outside their repository, so stripping it would equate them with
      an unrelated repo-relative path.
    * A commit is an identifier only when it is 7 to 40 hex digits. The
      stored lengths are 7 (1,618), 9 (394), 40 (127) and 8 (20), and 36
      same-workspace pairs hold a short hash that is a prefix of the other
      node's full one, so two commits are the same when one is a prefix of
      the other. Ten nodes store `HEAD`, `HEAD~1`..`HEAD~3` literally: those
      were never resolved and name different commits on different nodes, so
      they match nothing (`commit_key/1` says why).
    * A path ending in `/` is a directory. `related/2` matches it exactly
      only (a node that touched `src/` is not thereby about every file in
      it); `nodes_for_file/2` also reports containment, labelled as such.

  ## Ranking

  Overlap first: one point per shared path, two for the same commit (a
  commit is one identifier but a much narrower one than a path). Ties are
  broken by rarity -- the sum over shared paths of 1/(nodes naming that
  path), so sharing a file only two nodes touched beats sharing `src/main.rs`
  -- then by recency.

  ## Cost

  PostgreSQL does the matching (see "the database side" in the source):
  per call, at most 50 nodes per path and per commit come back, whatever
  the size of the workspace. With 30,000 nodes naming one file, show_node
  went from 1.1-1.4 s to 9 ms. What the bound gives up is written down
  next to it.
  """

  import Ecto.Query

  require Logger

  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.{Edge, Node}

  @commit_score 2
  @hex ~r/\A[0-9a-f]{7,40}\z/

  @type rel :: %{
          id: Ecto.UUID.t(),
          node_type: String.t(),
          title: String.t(),
          status: String.t(),
          score: non_neg_integer(),
          shared_files: [String.t()],
          shared_commit: String.t() | nil
        }

  # --- identity ---------------------------------------------------------

  @doc """
  The form a path is compared in. Raises on anything that is not a
  non-empty string, rather than comparing a guess.

      iex> DeciduousMcp.Graph.Related.normalize_path("./src//a/./b.rs ")
      "src/a/b.rs"
  """
  def normalize_path(path) when is_binary(path) do
    p =
      path
      |> String.trim()
      |> collapse()
      |> strip_dot_slash()

    if p == "", do: raise(ArgumentError, "not a path: #{inspect(path)}"), else: p
  end

  def normalize_path(other), do: raise(ArgumentError, "not a path: #{inspect(other)}")

  defp collapse(p) do
    next = p |> String.replace("//", "/") |> String.replace("/./", "/")
    if next == p, do: p, else: collapse(next)
  end

  defp strip_dot_slash("./" <> rest), do: strip_dot_slash(rest)
  defp strip_dot_slash(p), do: p

  @doc """
  `{:ok, key}` for a commit that identifies something (lowercased hex, 7-40
  digits), `{:error, reason}` otherwise.
  """
  def commit_key(commit) when is_binary(commit) do
    key = commit |> String.trim() |> String.downcase()

    cond do
      Regex.match?(@hex, key) ->
        {:ok, key}

      String.starts_with?(key, "head") ->
        {:error,
         "commit #{inspect(commit)} is a ref relative to whatever HEAD was when it was " <>
           "written, not a hash, so it names no particular commit"}

      true ->
        {:error, "commit #{inspect(commit)} is not a 7-40 digit hex hash"}
    end
  end

  def commit_key(other), do: {:error, "commit #{inspect(other)} is not a string"}

  @doc "True when two commit keys name the same commit (one is a prefix of the other)."
  def same_commit?(a, b), do: String.starts_with?(a, b) or String.starts_with?(b, a)

  # --- public reads -------------------------------------------------------

  @doc """
  Nodes related to `node` (a `%Node{}` or an id) through a shared path or
  commit, best first.

  Returns `{:ok, %{related: [rel], commit_ignored: nil | reason}}`, or
  `{:error, message}` for a missing or deleted node or one whose own
  metadata cannot be read as files/commit. Options: `:limit` (default 10).
  """
  def related(node, opts \\ [])

  def related(id, opts) when is_binary(id) do
    case Repo.get(Node, id) do
      nil -> {:error, "Node not found: #{id}"}
      node -> related(node, opts)
    end
  end

  def related(%Node{deleted_at: deleted} = node, _opts) when not is_nil(deleted),
    do: {:error, "Node #{node.id} is deleted"}

  def related(%Node{} = node, opts) do
    with {:ok, files} <- read_files(node.metadata),
         {commit, commit_ignored} <- read_own_commit(node.metadata) do
      rels =
        node.workspace_id
        |> neighbours([{node.id, files, commit}], [], opts[:limit] || 10)
        |> Map.fetch!(node.id)

      {:ok, %{related: rels, commit_ignored: commit_ignored}}
    end
  end

  @doc """
  The traversal route for retrieval: for each id in `node_ids`, the nodes
  related to it by path or commit. One read of the workspace serves the
  whole frontier.

  Returns `%{node_id => [rel]}` with an entry (possibly `[]`) for every id
  given that is a live node of `workspace_id`; an id that is not is absent.
  Options: `:limit` per node (default 5), `:exclude` ids never to return
  (pass the visited set to keep a walk moving outward). A node is never
  listed as related to itself.

  A neighbour whose metadata is malformed is left out: the SQL side reads
  its files as NULL, so it matches nothing and is never seen, and so is not
  logged. A node of the frontier whose own metadata is malformed maps to
  `[]` and is logged.
  """
  def expand(workspace_id, node_ids, opts \\ [])

  def expand(:global, _ids, _opts),
    do:
      raise(
        ArgumentError,
        "Related.expand needs one workspace: a path is not an identifier across repositories"
      )

  def expand(workspace_id, node_ids, opts) when is_binary(workspace_id) and is_list(node_ids) do
    sources =
      from(n in Node,
        where:
          n.id in ^Enum.uniq(node_ids) and n.workspace_id == ^workspace_id and
            is_nil(n.deleted_at),
        select: {n.id, n.metadata}
      )
      |> Repo.all()
      |> Enum.map(fn {id, meta} ->
        case read_files(meta) do
          {:ok, files} ->
            {id, files, neighbour_commit(meta)}

          {:error, reason} ->
            Logger.warning("Related: node #{id} expands to nothing: #{reason}")
            {id, [], nil}
        end
      end)

    neighbours(workspace_id, sources, opts[:exclude] || [], opts[:limit] || 5)
  end

  @doc """
  A non-edge retrieval route in the shape `DeciduousMcp.Graph.Retrieval`
  takes as an `extra_routes` entry (agreed on the jev-mem board):

      %{name:, cues:, weight:, expand: fn workspace_id, frontier_ids, visited_ids ->
          [{from_id, neighbour_node, usefulness, step_extra_map}] end}

  usefulness is score / (score + 1): one shared path 0.5, two paths or the
  same commit 0.67, three 0.75. step_extra carries `shared_files` and
  `shared_commit`, so a result can say which identifier it was reached by.
  Under `:global` the frontier is split by each node's own workspace: a
  path is never compared across repositories.
  """
  def route(opts \\ []) do
    limit = opts[:limit] || 5

    %{
      name: "shared_identifier",
      cues:
        ~w(file files path paths module commit sha touched changed .rs .ex .exs .ts .tsx .py .md),
      weight: opts[:weight] || 1.0,
      expand: fn workspace, frontier, visited ->
        route_expand(workspace, frontier, visited, limit)
      end
    }
  end

  defp route_expand(_workspace, [], _visited, _limit), do: []

  defp route_expand(:global, frontier, visited, limit) do
    from(n in Node, where: n.id in ^frontier, select: {n.workspace_id, n.id})
    |> Repo.all()
    |> Enum.group_by(&elem(&1, 0), &elem(&1, 1))
    |> Enum.flat_map(fn {ws, ids} -> route_expand(ws, ids, visited, limit) end)
  end

  defp route_expand(workspace_id, frontier, visited, limit) do
    exclude = Enum.to_list(visited) ++ frontier
    by_node = expand(workspace_id, frontier, limit: limit, exclude: exclude)
    ids = by_node |> Map.values() |> List.flatten() |> Enum.map(& &1.id) |> Enum.uniq()

    nodes =
      if ids == [],
        do: %{},
        else: from(n in Node, where: n.id in ^ids) |> Repo.all() |> Map.new(&{&1.id, &1})

    for {from_id, rels} <- by_node, rel <- rels do
      {from_id, Map.fetch!(nodes, rel.id), rel.score / (rel.score + 1),
       %{shared_files: rel.shared_files, shared_commit: rel.shared_commit}}
    end
  end

  @doc """
  The path-shaped tokens in a free-text question, normalised: a token with a
  `/` in it, or ending in a short extension (`db.rs`, `CLAUDE.md`).
  Surrounding quotes, backticks, brackets and sentence punctuation are
  dropped. For retrieval to seed anchors from paths, which a word tokenizer
  destroys ("src/db.rs" -> "srcdbrs", D-eval finding 1).

      iex> DeciduousMcp.Graph.Related.path_terms("what did we decide about `src/db.rs` and lib/?")
      ["src/db.rs", "lib/"]
  """
  def path_terms(question) when is_binary(question) do
    question
    |> String.split(~r/[\s,;]+/, trim: true)
    |> Enum.map(&String.replace(&1, ~r/\A[`'"(\[{<]+|[`'")\]}>.:?!]+\z/, ""))
    |> Enum.filter(&path_shaped?/1)
    |> Enum.map(&normalize_path/1)
    |> Enum.uniq()
  end

  # A bare name needs two stem characters, so "e.g" and "i.e" stay prose.
  defp path_shaped?(t) do
    t != "" and not String.contains?(t, "://") and
      (String.contains?(t, "/") or Regex.match?(~r/\A[\w.-]*\w\w\.[A-Za-z][A-Za-z0-9]{0,5}\z/, t)) and
      Regex.match?(~r/\A[\w.\/\-*~]+\z/, t)
  end

  @doc """
  Nodes naming any path-shaped token of `question`, for use as retrieval
  anchors: `[%{node: %Node{}, match: atom, term: path}]`, per term in
  question order, each node once (first term that reached it).
  """
  def anchors_for_question(workspace_id, question) when is_binary(workspace_id) do
    question
    |> path_terms()
    |> Enum.flat_map(fn term ->
      {:ok, hits} = nodes_for_file(workspace_id, term)
      Enum.map(hits, &Map.put(&1, :term, term))
    end)
    |> Enum.uniq_by(& &1.node.id)
  end

  @doc """
  Every live node in the workspace whose files name `path`.

  `{:ok, [%{node: %Node{}, match: :exact | :under_dir | :dir_contains}]}`,
  exact matches first, then newest first. `:under_dir` is a stored path
  inside the directory asked about (the query ends in `/`); `:dir_contains`
  is a stored directory that contains the file asked about.
  """
  def nodes_for_file(:global, _path),
    do: {:error, "file needs one workspace: the same path in two repositories is two files"}

  def nodes_for_file(workspace_id, path) when is_binary(workspace_id) do
    case safe_normalize(path) do
      {:ok, wanted} ->
        wanted_dir? = String.ends_with?(wanted, "/")

        {:ok,
         workspace_id
         |> file_candidates(wanted, wanted_dir?)
         |> Enum.flat_map(fn {node, files} ->
           files
           |> Enum.map(&file_match(&1, wanted, wanted_dir?))
           |> Enum.reject(&is_nil/1)
           |> case do
             [] -> []
             ms -> [%{node: node, match: Enum.reduce(ms, &better/2)}]
           end
         end)
         |> Enum.sort_by(&{match_rank(&1.match), sortable_time(&1.node.inserted_at)})}

      {:error, _} = error ->
        error
    end
  end

  @doc """
  The decisions a set of nodes were carried out under: for each id, walk
  real incoming edges up to `max_depth` levels, stopping on each path at the
  first live decision (and at goals). Returns `[%{decision: %Node{}, via:
  [id]}]`, nearest level first.

  Why this exists: on the dev database only 65 of 1,587 decisions carry
  `files`, against 1,345 of 5,219 actions. Asked "what did we decide about
  eval.zig", a file filter alone returns 26 actions and 0 decisions; the
  decisions are one or two edges above those actions.
  """
  def decisions_above(ids, max_depth \\ 3),
    do: climb(Enum.uniq(ids), max_depth, MapSet.new(ids), %{}, [])

  defp climb([], _depth, _seen, _via, found), do: finish(found)
  defp climb(_frontier, 0, _seen, _via, found), do: finish(found)

  defp climb(frontier, depth, seen, via, found) do
    # {parent, child} over live parents; via maps a node to the matched ids it stands for.
    pairs =
      from(e in Edge,
        join: p in Node,
        on: p.id == e.from_node_id,
        where: e.to_node_id in ^frontier and is_nil(p.deleted_at),
        select: {p, e.to_node_id}
      )
      |> Repo.all()

    origin = fn child -> Map.get(via, child, [child]) end

    {decisions, others} = Enum.split_with(pairs, fn {p, _} -> p.node_type == "decision" end)

    found =
      Enum.reduce(decisions, found, fn {p, child}, acc ->
        [{p, origin.(child)} | acc]
      end)

    next = for {p, _} <- others, p.node_type != "goal", not MapSet.member?(seen, p.id), do: p.id

    via =
      Enum.reduce(others, via, fn {p, child}, acc ->
        Map.update(acc, p.id, origin.(child), &Enum.uniq(&1 ++ origin.(child)))
      end)

    seen = Enum.reduce(pairs, seen, fn {p, _}, acc -> MapSet.put(acc, p.id) end)
    climb(Enum.uniq(next), depth - 1, seen, via, found)
  end

  # Found in walk order reversed; keep each decision once, at its nearest
  # level, with every matched id that reaches it.
  defp finish(found) do
    found
    |> Enum.reverse()
    |> Enum.reduce({[], %{}}, fn {d, via}, {order, acc} ->
      if Map.has_key?(acc, d.id),
        do: {order, Map.update!(acc, d.id, fn {n, v} -> {n, Enum.uniq(v ++ via)} end)},
        else: {[d.id | order], Map.put(acc, d.id, {d, via})}
    end)
    |> then(fn {order, acc} ->
      order
      |> Enum.reverse()
      |> Enum.map(fn id ->
        {d, via} = Map.fetch!(acc, id)
        %{decision: d, via: Enum.sort(via)}
      end)
    end)
  end

  # --- internals ------------------------------------------------------------

  defp file_match(stored, wanted, _dir?) when stored == wanted, do: :exact

  defp file_match(stored, wanted, true) do
    if String.starts_with?(stored, wanted), do: :under_dir
  end

  defp file_match(stored, wanted, false) do
    if String.ends_with?(stored, "/") and String.starts_with?(wanted, stored), do: :dir_contains
  end

  defp match_rank(:exact), do: 0
  defp match_rank(:under_dir), do: 1
  defp match_rank(:dir_contains), do: 2

  defp better(a, b), do: if(match_rank(a) <= match_rank(b), do: a, else: b)

  # Negated so an ascending sort puts the newest first.
  defp sortable_time(%DateTime{} = t), do: -DateTime.to_unix(t, :microsecond)

  defp safe_normalize(path) do
    {:ok, normalize_path(path)}
  rescue
    ArgumentError -> {:error, "file must be a non-empty path, got #{inspect(path)}"}
  end

  defp read_files(meta) do
    case (meta || %{})["files"] do
      nil ->
        {:ok, []}

      list when is_list(list) ->
        if Enum.all?(list, &(is_binary(&1) and String.trim(&1) != "")),
          do: {:ok, list |> Enum.map(&normalize_path/1) |> Enum.uniq()},
          else: {:error, "metadata.files is not a list of paths: #{inspect(list, limit: 5)}"}

      other ->
        {:error, "metadata.files is not a list of paths: #{inspect(other, limit: 5)}"}
    end
  end

  # The asked-about node's own commit: an unusable one is reported, not
  # silently treated as absent.
  defp read_own_commit(meta) do
    case (meta || %{})["commit"] do
      nil ->
        {nil, nil}

      c ->
        case commit_key(c) do
          {:ok, key} -> {key, nil}
          {:error, reason} -> {nil, reason}
        end
    end
  end

  # A neighbour's commit, as the frontier reads it: unusable is absent.
  defp neighbour_commit(meta) do
    case (meta || %{})["commit"] do
      nil ->
        nil

      c ->
        case commit_key(c) do
          {:ok, key} -> key
          {:error, _} -> nil
        end
    end
  end

  # --- the database side ------------------------------------------------------
  #
  # Nothing here loads the workspace. For the paths and commits the sources
  # hold, PostgreSQL returns at most @per_path nodes naming each (newest
  # first, served by idx_nodes_files and idx_nodes_ws_commit7) and how many
  # nodes name each path, counted up to @count_cap. The path and commit
  # rules are the SQL functions deciduous_node_files/1 and
  # deciduous_commit_key/1 (migration 20260926020000), which RelatedTest
  # holds to normalize_path/1 and commit_key/1.
  #
  # What the bound gives up, by name: a node reached only through paths that
  # more than @per_path other nodes also name is considered only if it is
  # among the newest @per_path of one of them. Its score, when considered,
  # is exact (its whole files list is compared). So an old node sharing two
  # such hub paths with the source can lose its place to newer nodes
  # sharing one; a node sharing any rarer path, or the commit, is always
  # considered. Rarity counts a path named by more than @count_cap nodes as
  # @count_cap, which only matters between two such hubs.

  @per_path 50
  @count_cap 1_000

  # sources: [{id, normalised files, commit key | nil}]. Returns
  # %{source_id => [rel]}, best first, at most `limit` each; neither a
  # source itself nor anything in `exclude` is ever returned.
  defp neighbours(workspace_id, sources, exclude, limit) do
    files = sources |> Enum.flat_map(&elem(&1, 1)) |> Enum.uniq()
    commits = sources |> Enum.map(&elem(&1, 2)) |> Enum.reject(&is_nil/1) |> Enum.uniq()
    exclude = Enum.uniq(exclude)
    # A source can sit in another source's lists; it is dropped for itself
    # only after the per-path limit, so the limit leaves room for them.
    per = @per_path + length(sources)

    by_path = path_hits(workspace_id, files, exclude, per)
    by_commit = commit_hits(workspace_id, commits, exclude, per)

    rows =
      by_path
      |> Map.values()
      |> Enum.flat_map(& &1.ids)
      |> Kernel.++(Enum.flat_map(by_commit, fn {_c, hits} -> Enum.map(hits, &elem(&1, 0)) end))
      |> Enum.uniq()
      |> details()

    named_by = Map.new(by_path, fn {f, %{named_by: n}} -> {f, n} end)

    Map.new(sources, fn {id, src_files, commit} ->
      {id, rank(id, src_files, commit, by_path, by_commit, rows, named_by, limit)}
    end)
  end

  defp path_hits(_workspace_id, [], _exclude, _per), do: %{}

  defp path_hits(workspace_id, files, exclude, per) do
    %{rows: rows} =
      Repo.query!(
        """
        WITH wanted AS (SELECT DISTINCT unnest($2::text[]) AS f),
        counted AS MATERIALIZED (
          SELECT w.f,
                 (SELECT count(*) FROM (
                    SELECT 1 FROM decision_nodes c
                    WHERE public.deciduous_node_files(c.metadata) && ARRAY[w.f]
                      AND c.deleted_at IS NULL AND c.workspace_id = $1
                    LIMIT $4) AS capped) AS named_by
          FROM wanted w
        )
        SELECT counted.f, counted.named_by, h.id
        FROM counted
        LEFT JOIN LATERAL (
          SELECT m.id FROM decision_nodes m
          WHERE public.deciduous_node_files(m.metadata) && ARRAY[counted.f]
            AND m.deleted_at IS NULL AND m.workspace_id = $1
            AND NOT (m.id = ANY($3::uuid[]))
          ORDER BY m.inserted_at DESC, m.id
          LIMIT $5
        ) h ON true
        """,
        [dump(workspace_id), files, Enum.map(exclude, &dump/1), @count_cap, per]
      )

    Enum.reduce(rows, %{}, fn [f, n, id], acc ->
      entry = Map.get(acc, f, %{named_by: n, ids: []})
      entry = if id, do: %{entry | ids: [load(id) | entry.ids]}, else: entry
      Map.put(acc, f, entry)
    end)
  end

  defp commit_hits(_workspace_id, [], _exclude, _per), do: %{}

  defp commit_hits(workspace_id, commits, exclude, per) do
    %{rows: rows} =
      Repo.query!(
        """
        SELECT w.c, h.id, h.k
        FROM (SELECT DISTINCT unnest($2::text[]) AS c) w
        JOIN LATERAL (
          SELECT m.id, public.deciduous_commit_key(m.metadata) AS k FROM decision_nodes m
          WHERE m.workspace_id = $1 AND m.deleted_at IS NULL
            AND left(public.deciduous_commit_key(m.metadata), 7) = left(w.c, 7)
            AND (starts_with(public.deciduous_commit_key(m.metadata), w.c)
                 OR starts_with(w.c, public.deciduous_commit_key(m.metadata)))
            AND NOT (m.id = ANY($3::uuid[]))
          ORDER BY m.inserted_at DESC, m.id
          LIMIT $4
        ) h ON true
        """,
        [dump(workspace_id), commits, Enum.map(exclude, &dump/1), per]
      )

    Enum.group_by(rows, fn [c, _, _] -> c end, fn [_, id, k] -> {load(id), k} end)
  end

  defp details([]), do: %{}

  defp details(ids) do
    from(n in Node,
      where: n.id in ^ids,
      select: %{
        id: n.id,
        node_type: n.node_type,
        title: n.title,
        status: n.status,
        inserted_at: n.inserted_at,
        files: fragment("public.deciduous_node_files(?)", n.metadata)
      }
    )
    |> Repo.all()
    |> Map.new(&{&1.id, &1})
  end

  # Nodes a query_nodes file filter can match, with their normalised files:
  # the exact path and every directory above it, through idx_nodes_files.
  # A directory question ("lib/a/") has to find stored paths *under* it,
  # which no index here serves: that one is a scan of the workspace's nodes,
  # in SQL.
  defp file_candidates(workspace_id, wanted, true) do
    from(n in Node,
      where:
        n.workspace_id == ^workspace_id and is_nil(n.deleted_at) and
          fragment(
            "EXISTS (SELECT 1 FROM unnest(public.deciduous_node_files(?)) AS x(p) WHERE starts_with(x.p, ?))",
            n.metadata,
            ^wanted
          ),
      select: {n, fragment("public.deciduous_node_files(?)", n.metadata)}
    )
    |> Repo.all()
  end

  defp file_candidates(workspace_id, wanted, false) do
    # Every directory above the file: "src/a/b.rs" -> "src/", "src/a/";
    # "/x/y.md" -> "/", "/x/". A stored directory matches exactly one of
    # these when it contains the file.
    dirs =
      wanted
      |> String.split("/")
      |> Enum.drop(-1)
      |> Enum.scan("", &(&2 <> &1 <> "/"))

    from(n in Node,
      where:
        n.workspace_id == ^workspace_id and is_nil(n.deleted_at) and
          fragment(
            "public.deciduous_node_files(?) && ?::text[]",
            n.metadata,
            ^[wanted | dirs]
          ),
      select: {n, fragment("public.deciduous_node_files(?)", n.metadata)}
    )
    |> Repo.all()
  end

  defp rank(self, files, commit, by_path, by_commit, rows, named_by, limit) do
    src = MapSet.new(files)

    from_paths = Enum.flat_map(files, &Map.get(by_path, &1, %{ids: []}).ids)

    shas =
      if commit do
        by_commit
        |> Map.get(commit, [])
        |> Map.new(fn {id, other} ->
          {id, if(byte_size(other) > byte_size(commit), do: other, else: commit)}
        end)
      else
        %{}
      end

    (from_paths ++ Map.keys(shas))
    |> Enum.uniq()
    |> Enum.reject(&(&1 == self))
    |> Enum.map(fn id ->
      row = Map.fetch!(rows, id)
      shared = (row.files || []) |> Enum.filter(&MapSet.member?(src, &1)) |> Enum.sort()
      sha = Map.get(shas, id)
      rarity = Enum.reduce(shared, 0.0, &(&2 + 1 / Map.fetch!(named_by, &1)))

      %{
        id: id,
        node_type: row.node_type,
        title: row.title,
        status: row.status,
        score: length(shared) + if(sha, do: @commit_score, else: 0),
        shared_files: shared,
        shared_commit: sha,
        rarity: rarity,
        inserted_at: row.inserted_at
      }
    end)
    |> Enum.sort_by(&{-&1.score, -&1.rarity, sortable_time(&1.inserted_at), &1.id})
    |> Enum.take(limit)
    |> Enum.map(&Map.drop(&1, [:rarity, :inserted_at]))
  end

  defp dump(uuid), do: Ecto.UUID.dump!(uuid)
  defp load(bin), do: Ecto.UUID.load!(bin)
end
