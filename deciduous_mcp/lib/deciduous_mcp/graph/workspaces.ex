defmodule DeciduousMcp.Graph.Workspaces do
  @moduledoc """
  Context module for workspace management.
  A workspace isolates a team's decision graph data.
  """
  import Ecto.Query
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.{Workspace, Node, Edge}

  # The global view, readable across every project. It is a valid name to
  # read with and never one to write to: find_or_create/1 refuses it, so no
  # path (argument, header pin, import) can create a workspace called "*".
  @global "*"

  @doc "The token that names every workspace at once."
  def global_token, do: @global

  @doc """
  Finds a workspace by name, or creates it if it doesn't exist.

  Safe under concurrency. The first calls to a new project arrive together
  (a swarm starting in a fresh repo, a client reconnecting several sessions),
  and a check-then-insert lets several of them see "absent" and all but one
  lose on the unique index: 5 of 120 parallel add_node calls failed with
  "has already been taken", and 23 of 30 parallel pinned initializes got an
  empty HTTP 500. The insert is `ON CONFLICT DO NOTHING` instead, and the row
  is read back afterwards, because on a conflict Ecto still returns the
  struct it tried to insert, with a client-generated id that names nothing.
  """
  def find_or_create(@global), do: {:error, :global}

  def find_or_create(name) do
    case get_by_name(name) do
      {:ok, workspace} ->
        {:ok, workspace}

      {:error, :not_found} ->
        changeset = Workspace.changeset(%Workspace{}, %{name: name})

        with {:ok, _maybe_phantom} <-
               Repo.insert(changeset, on_conflict: :nothing, conflict_target: :name) do
          {:ok, Repo.get_by!(Workspace, name: name)}
        end
    end
  end

  @doc """
  Looks a workspace up by its (already normalized) name, never creating it.
  """
  def get_by_name(name) do
    case Repo.get_by(Workspace, name: name) do
      nil -> get_by_normalized_name(name)
      workspace -> {:ok, workspace}
    end
  end

  # Names are stored NFC since normalize_name/1 began composing them. A
  # workspace created earlier under a decomposed spelling is still found by
  # the composed one, so the fix does not split it in two; if both spellings
  # exist, the older is the one used. Only on a miss, so a known name still
  # costs one indexed lookup.
  defp get_by_normalized_name(name) do
    from(w in Workspace,
      where: fragment("normalize(?, NFC)", w.name) == ^name,
      order_by: [asc: w.inserted_at],
      limit: 1
    )
    |> Repo.one()
    |> case do
      nil -> {:error, :not_found}
      workspace -> {:ok, workspace}
    end
  end

  @doc """
  Gets a workspace by ID.
  """
  def get_workspace(id) do
    case Repo.get(Workspace, id) do
      nil -> {:error, :not_found}
      workspace -> {:ok, workspace}
    end
  end

  @doc """
  Lists all workspaces.
  """
  def list_workspaces do
    Repo.all(Workspace)
  end

  @doc """
  Lists every workspace with its live node and edge counts.

  This is the index for the global view: which projects exist, and how much is
  in each. Counts come from subqueries rather than preloads so a workspace with
  24MB of graph does not get loaded into memory to be counted.
  """
  def list_with_counts do
    node_counts =
      from n in Node,
        where: is_nil(n.deleted_at),
        group_by: n.workspace_id,
        select: %{workspace_id: n.workspace_id, count: count(n.id)}

    # updated_at is the last write: the workspace row is written once, when
    # it is created, and list_workspaces showed that time for a workspace
    # written to a minute ago (team probe T10). A delete is a write too
    # (it sets deleted_at and updated_at), so deleted nodes count here.
    # An edge delete leaves no row and is the one write this cannot see.
    last_node_write =
      from n in Node,
        group_by: n.workspace_id,
        select: %{workspace_id: n.workspace_id, at: max(n.updated_at)}

    last_edge_write =
      from e in Edge,
        group_by: e.workspace_id,
        select: %{workspace_id: e.workspace_id, at: max(e.updated_at)}

    # An edge counts when both its ends are live, the rule /export and
    # get_graph already apply; counting every row put edges through a
    # deleted node into edge_count beside a live-only node_count.
    edge_counts =
      from e in Edge,
        join: f in Node,
        on: f.id == e.from_node_id and is_nil(f.deleted_at),
        join: t in Node,
        on: t.id == e.to_node_id and is_nil(t.deleted_at),
        group_by: e.workspace_id,
        select: %{workspace_id: e.workspace_id, count: count(e.id)}

    from(w in Workspace,
      left_join: n in subquery(node_counts),
      on: n.workspace_id == w.id,
      left_join: e in subquery(edge_counts),
      on: e.workspace_id == w.id,
      left_join: ln in subquery(last_node_write),
      on: ln.workspace_id == w.id,
      left_join: le in subquery(last_edge_write),
      on: le.workspace_id == w.id,
      order_by: [desc: coalesce(n.count, 0)],
      select: %{
        id: w.id,
        name: w.name,
        description: w.description,
        node_count: coalesce(n.count, 0),
        edge_count: coalesce(e.count, 0),
        updated_at:
          type(fragment("GREATEST(?, ?, ?)", w.updated_at, ln.at, le.at), :utc_datetime_usec)
      }
    )
    |> Repo.all()
  end

  @doc """
  Ties a workspace to the repository writing to it, by root commit ids.

  Names come from directory names, so two unrelated repositories called
  `bridge-api` asked for one workspace and 1.0.7 let both write to it. The
  CLI sends `git rev-list --max-parents=0 HEAD`: the same in every clone and
  every worktree of a repository, different for an unrelated one, and
  unchanged by a rename.

    * no `repo_roots` at all (`nil`: a 1.0.7 client, a shallow clone whose
      root is not in its history, or a directory outside git):
      `{:ok, :unchecked}`. There is nothing to compare, and refusing would
      lock every 1.0.7 teammate out of their own workspace.
    * an empty list (a repository with no commit yet): `{:ok, :unchecked}`
      while the workspace is unclaimed, and refused once it is claimed. An
      empty list is not "no information": it is a repository that cannot
      show it is the one that claimed the workspace, and letting it in is
      how a fresh `git init` of a same-named project wrote into another's
      graph and pulled its nodes.
    * workspace unclaimed: the roots are recorded, `{:ok, :claimed}`
    * any root in common: `{:ok, :verified}` (new roots, from a merged-in
      history, are added)
    * none in common and `adopt?`: `{:ok, :adopted}`, the roots are added.
      The CLI sets it only when the user named the workspace explicitly.
    * none in common: `{:error, {:claimed_by_other_repository, held}}`

  The row is locked for the check so two first claims cannot both win.
  """
  def claim(%Workspace{} = ws, roots, adopt?) do
    with {:ok, roots} <- validate_roots(roots) do
      cond do
        roots == :none ->
          {:ok, :unchecked}

        roots == [] ->
          case (ws.settings || %{})["repo_roots"] || [] do
            [] -> {:ok, :unchecked}
            held -> {:error, {:no_commit_yet, held}}
          end

        true ->
          claim_roots(ws, roots, adopt?)
      end
    end
  end

  defp claim_roots(ws, roots, adopt?) do
    Repo.transaction(fn ->
      ws = Repo.one!(from w in Workspace, where: w.id == ^ws.id, lock: "FOR UPDATE")
      settings = ws.settings || %{}
      held = settings["repo_roots"] || []

      {outcome, merged} =
        cond do
          held == [] -> {:claimed, roots}
          Enum.any?(roots, &(&1 in held)) -> {:verified, Enum.uniq(held ++ roots)}
          adopt? -> {:adopted, Enum.uniq(held ++ roots)}
          true -> Repo.rollback({:claimed_by_other_repository, held})
        end

      if merged != held do
        ws
        |> Workspace.changeset(%{settings: Map.put(settings, "repo_roots", merged)})
        |> Repo.update!()
      end

      outcome
    end)
  end

  @doc """
  Checks a claim without making one: the answer `claim/3` would give, but
  an unclaimed workspace stays unclaimed and no root is added.

  For reads. GET /export with the CLI's X-Deciduous-Repo-Roots header used
  `claim/3`, so the first repository of a name to run `remote status` or
  `pull` owned the workspace, including an unrelated one, and the real
  repository then got 409 on its own workspace (SERVER-N7).
  """
  def check_claim(%Workspace{} = ws, roots) do
    with {:ok, roots} <- validate_roots(roots) do
      held = (ws.settings || %{})["repo_roots"] || []

      cond do
        roots == :none -> {:ok, :unchecked}
        held == [] -> {:ok, :unchecked}
        roots == [] -> {:error, {:no_commit_yet, held}}
        Enum.any?(roots, &(&1 in held)) -> {:ok, :verified}
        true -> {:error, {:claimed_by_other_repository, held}}
      end
    end
  end

  # A repository has one root commit, a few when histories were merged.
  # 20,000 were stored without complaint (SERVER-N6), and every later
  # claim check reads them all.
  @max_roots 100

  @doc "`{:ok, sorted_roots}`, `{:ok, :none}` for nil, or `{:error, sentence}`."
  def validate_roots(nil), do: {:ok, :none}

  def validate_roots(roots) when is_list(roots) and length(roots) > @max_roots,
    do:
      {:error,
       "repo_roots has #{length(roots)} entries; a repository has a handful of root " <>
         "commits at most, and the limit is #{@max_roots}"}

  def validate_roots(roots) when is_list(roots) do
    case Enum.reject(
           roots,
           &(is_binary(&1) and Regex.match?(~r/\A[0-9a-f]{40}([0-9a-f]{24})?\z/, &1))
         ) do
      [] ->
        {:ok, roots |> Enum.uniq() |> Enum.sort()}

      bad ->
        {:error,
         "repo_roots must be git commit ids (40 or 64 lowercase hex); got " <>
           inspect(bad, limit: 5, printable_limit: 80)}
    end
  end

  def validate_roots(other),
    do:
      {:error, "repo_roots must be a list, got #{inspect(other, limit: 5, printable_limit: 80)}"}

  @doc """
  Workspaces holding any of `change_ids`, with how many each holds, most
  first. Deleted nodes count: they were written there.
  """
  def holding([]), do: []

  def holding(change_ids) do
    from(n in Node,
      join: w in Workspace,
      on: w.id == n.workspace_id,
      where: n.change_id in ^change_ids,
      group_by: w.name,
      select: %{name: w.name, nodes: count(n.id)},
      order_by: [desc: count(n.id), asc: w.name]
    )
    |> Repo.all()
  end

  @max_name_length 128

  @doc """
  Normalizes a workspace name coming from a header or a tool argument.

  Names are used as a stable key across machines, so they are lowercased and
  stripped of anything that would make one project addressable under two
  spellings. Path separators are rejected outright rather than rewritten: a
  caller sending `/Users/bg/code/blog` has sent a path where a project name was
  asked for, and silently turning that into `users-bg-code-blog` would scatter
  one project across several workspaces depending on the machine it was logged
  from.
  """
  def normalize_name(raw) when is_binary(raw) do
    if String.valid?(raw), do: normalize_valid_name(raw), else: {:error, :not_utf8}
  end

  def normalize_name(_), do: {:error, :not_a_string}

  # Composed (NFC) as well as lowercased: "café" typed precomposed and the
  # decomposed "cafe" + U+0301 that macOS file APIs often return were two
  # workspaces listed under the same name. A header is bytes, not text, and
  # one that was not UTF-8 crashed the plug (an empty HTTP 500).
  defp normalize_valid_name(raw) do
    trimmed = String.trim(raw)

    cond do
      trimmed == "" ->
        {:error, :blank}

      String.contains?(trimmed, ["/", "\\"]) ->
        {:error, :looks_like_a_path}

      # NUL reached Postgres and came back as a Postgrex struct with a stack
      # trace; a right-to-left override made a workspace whose name displays
      # as a different one. Neither is ever part of a repo's basename.
      String.match?(trimmed, ~r/[\p{Cc}\p{Cf}]/u) ->
        {:error, :control_character}

      String.length(trimmed) > @max_name_length ->
        {:error, :too_long}

      true ->
        {:ok, trimmed |> String.downcase() |> String.normalize(:nfc)}
    end
  end

  @doc "One sentence for a `normalize_name/1` refusal, naming the input."
  def describe_name_error(raw, reason) do
    why =
      case reason do
        :blank ->
          "is blank"

        :looks_like_a_path ->
          "looks like a path; pass the repo root's basename"

        :too_long ->
          "is longer than #{@max_name_length} characters"

        :control_character ->
          "contains a control or formatting character"

        :not_a_string ->
          "is not a string"

        :not_utf8 ->
          "is not valid UTF-8"

        :global ->
          "is \"#{@global}\", the global view across every project, which can be read but not written to or pinned"
      end

    shown =
      if is_binary(raw) and not String.valid?(raw),
        do: inspect(raw, binaries: :as_binaries, limit: 20),
        else: inspect(raw, binaries: :as_strings)

    "invalid workspace name #{shown}: it #{why}"
  end

  @doc """
  Shares the workspace's write lock until the calling transaction ends.
  Every create keyed by a change_id or by a pair of nodes takes it
  (`Nodes.lock_change_id/2`, `Edges.lock_pair/3`) before its own key, so
  a bulk import, which takes it exclusively (`lock_exclusively/1`), runs
  alone against them.

  One exclusive lock per import, not one lock per row it writes: an
  advisory lock takes a slot in the shared lock table, and a graph of
  51,158 edges would run it out ("out of shared memory").
  """
  def lock_shared(workspace_id) do
    Repo.query!("SELECT pg_advisory_xact_lock_shared(hashtextextended($1, 0))", [
      "ws|#{workspace_id}"
    ])

    :ok
  end

  @doc "The exclusive side of `lock_shared/1`, for POST /import."
  def lock_exclusively(workspace_id) do
    Repo.query!("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))", ["ws|#{workspace_id}"])
    :ok
  end
end
