defmodule DeciduousMcp.Graph.Nodes do
  @moduledoc """
  Context module for decision graph node operations.
  All queries are scoped to a workspace for team isolation.
  """
  import Ecto.Query
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.{Node, AuditLog}

  @doc """
  Creates a new decision node in the given workspace.
  Generates a change_id (UUID) for sync compatibility with the Rust CLI.
  """
  def create_node(workspace_id, attrs) do
    change_id = Map.get(attrs, :change_id) || Map.get(attrs, "change_id") || UUID.uuid4()

    node_attrs =
      attrs
      |> Map.put(:workspace_id, workspace_id)
      |> Map.put(:change_id, change_id)

    Repo.transaction(fn ->
      case %Node{} |> Node.changeset(node_attrs) |> Repo.insert() do
        {:ok, node} ->
          audit_change(workspace_id, node, "create")
          broadcast(workspace_id, {:node_created, node})
          node

        {:error, changeset} ->
          Repo.rollback(changeset)
      end
    end)
  end

  @doc """
  Updates an existing node. Only provided fields are changed.
  """
  def update_node(node_id, attrs) do
    Repo.transaction(fn ->
      case Repo.get(Node, node_id) do
        nil ->
          Repo.rollback(:not_found)

        node ->
          case node |> Node.update_changeset(attrs) |> Repo.update() do
            {:ok, updated} ->
              audit_change(node.workspace_id, updated, "update", attrs)
              broadcast(node.workspace_id, {:node_updated, updated})
              updated

            {:error, changeset} ->
              Repo.rollback(changeset)
          end
      end
    end)
  end

  @doc """
  Soft-deletes a node by setting deleted_at.
  """
  def delete_node(node_id) do
    case Repo.get(Node, node_id) do
      nil ->
        {:error, :not_found}

      node ->
        now = DateTime.utc_now()

        node
        |> Node.update_changeset(%{deleted_at: now})
        |> Repo.update()
        |> tap(fn
          {:ok, deleted} ->
            audit_change(node.workspace_id, deleted, "delete")
            broadcast(node.workspace_id, {:node_deleted, deleted})

          _ ->
            :ok
        end)
    end
  end

  @doc """
  Gets a single node by ID, with optional preloads.
  """
  def get_node(node_id, preloads \\ []) do
    Node
    |> Repo.get(node_id)
    |> case do
      nil -> {:error, :not_found}
      node -> {:ok, Repo.preload(node, preloads)}
    end
  end

  @doc """
  Gets a node by its change_id within a workspace.
  Used for sync operations where the CLI references nodes by change_id.
  """
  def get_node_by_change_id(workspace_id, change_id) do
    Node
    |> where([n], n.workspace_id == ^workspace_id and n.change_id == ^change_id)
    |> where([n], is_nil(n.deleted_at))
    |> Repo.one()
    |> case do
      nil -> {:error, :not_found}
      node -> {:ok, node}
    end
  end

  @doc """
  Lists nodes in a workspace with optional filters.

  Options:
  - `:type` — filter by node_type
  - `:status` — filter by status
  - `:branch` — filter by metadata.branch
  - `:search` — text search in title/description
  - `:limit` — max results (default 100)
  - `:offset` — pagination offset
  """
  def list_nodes(workspace_id, opts \\ []) do
    Node
    |> where([n], n.workspace_id == ^workspace_id)
    |> where([n], is_nil(n.deleted_at))
    |> maybe_filter_type(opts[:type])
    |> maybe_filter_status(opts[:status])
    |> maybe_filter_branch(opts[:branch])
    |> maybe_search(opts[:search])
    |> order_by([n], desc: n.inserted_at)
    |> limit(^(opts[:limit] || 100))
    |> offset(^(opts[:offset] || 0))
    |> Repo.all()
  end

  @doc """
  Counts nodes by type in a workspace. Useful for dashboard/pulse.
  """
  def count_by_type(workspace_id) do
    Node
    |> where([n], n.workspace_id == ^workspace_id)
    |> where([n], is_nil(n.deleted_at))
    |> group_by([n], n.node_type)
    |> select([n], {n.node_type, count(n.id)})
    |> Repo.all()
    |> Map.new()
  end

  # --- Private helpers ---

  defp maybe_filter_type(query, nil), do: query
  defp maybe_filter_type(query, type), do: where(query, [n], n.node_type == ^type)

  defp maybe_filter_status(query, nil), do: query
  defp maybe_filter_status(query, status), do: where(query, [n], n.status == ^status)

  defp maybe_filter_branch(query, nil), do: query

  defp maybe_filter_branch(query, branch) do
    where(query, [n], fragment("? ->> 'branch' = ?", n.metadata, ^branch))
  end

  defp maybe_search(query, nil), do: query
  defp maybe_search(query, ""), do: query

  defp maybe_search(query, search_term) do
    pattern = "%#{search_term}%"

    where(
      query,
      [n],
      ilike(n.title, ^pattern) or ilike(n.description, ^pattern)
    )
  end

  defp audit_change(workspace_id, node, action, changes \\ %{}) do
    %AuditLog{}
    |> AuditLog.changeset(%{
      workspace_id: workspace_id,
      entity_type: "node",
      entity_id: node.id,
      entity_change_id: node.change_id,
      action: action,
      changes: changes,
      source: "mcp"
    })
    |> Repo.insert()
  end

  defp broadcast(workspace_id, message) do
    Phoenix.PubSub.broadcast(
      DeciduousMcp.PubSub,
      "graph:#{workspace_id}",
      message
    )
  end
end
