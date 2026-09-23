defmodule DeciduousMcp.Graph.Workspaces do
  @moduledoc """
  Context module for workspace management.
  A workspace isolates a team's decision graph data.
  """
  import Ecto.Query
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.{Workspace, Node, Edge}

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
    trimmed = String.trim(raw)

    cond do
      trimmed == "" ->
        {:error, :blank}

      String.contains?(trimmed, ["/", "\\"]) ->
        {:error, :looks_like_a_path}

      String.length(trimmed) > @max_name_length ->
        {:error, :too_long}

      true ->
        {:ok, String.downcase(trimmed)}
    end
  end

  def normalize_name(_), do: {:error, :not_a_string}
end
