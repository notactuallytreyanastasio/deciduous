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
      order_by: [desc: coalesce(n.count, 0)],
      select: %{
        id: w.id,
        name: w.name,
        description: w.description,
        node_count: coalesce(n.count, 0),
        edge_count: coalesce(e.count, 0),
        updated_at: w.updated_at
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

  defp validate_roots(nil), do: {:ok, :none}

  defp validate_roots(roots) when is_list(roots) do
    case Enum.reject(
           roots,
           &(is_binary(&1) and Regex.match?(~r/\A[0-9a-f]{40}([0-9a-f]{24})?\z/, &1))
         ) do
      [] ->
        {:ok, roots |> Enum.uniq() |> Enum.sort()}

      bad ->
        {:error,
         "repo_roots must be git commit ids (40 or 64 lowercase hex); got #{inspect(bad)}"}
    end
  end

  defp validate_roots(other), do: {:error, "repo_roots must be a list, got #{inspect(other)}"}

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
end
