defmodule DeciduousMcp.Graph.Edges do
  @moduledoc """
  Context module for decision graph edge operations.
  Edges connect nodes and carry relationship semantics (leads_to, chosen, rejected, etc.).
  """
  import Ecto.Query
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.{Edge, Node, AuditLog}

  @doc """
  Creates a directed edge between two nodes.
  Both nodes must be in the same workspace.
  """
  def create_edge(workspace_id, attrs) do
    edge_attrs = Map.put(attrs, :workspace_id, workspace_id)

    Repo.transaction(fn ->
      # Verify both nodes exist and are in the same workspace
      with {:ok, from_node} <- get_workspace_node(workspace_id, attrs[:from_node_id] || attrs["from_node_id"]),
           {:ok, to_node} <- get_workspace_node(workspace_id, attrs[:to_node_id] || attrs["to_node_id"]) do
        edge_attrs =
          edge_attrs
          |> Map.put(:from_change_id, from_node.change_id)
          |> Map.put(:to_change_id, to_node.change_id)

        case %Edge{} |> Edge.changeset(edge_attrs) |> Repo.insert() do
          {:ok, edge} ->
            audit_change(workspace_id, edge, "create")
            broadcast(workspace_id, {:edge_created, edge})
            edge

          {:error, changeset} ->
            Repo.rollback(changeset)
        end
      else
        {:error, reason} -> Repo.rollback(reason)
      end
    end)
  end

  @doc """
  Creates an edge using change_ids (for CLI sync compatibility).
  Resolves change_ids to node IDs within the workspace.
  """
  def create_edge_by_change_id(workspace_id, from_change_id, to_change_id, attrs \\ %{}) do
    with {:ok, from_node} <- get_node_by_change_id(workspace_id, from_change_id),
         {:ok, to_node} <- get_node_by_change_id(workspace_id, to_change_id) do
      edge_attrs =
        attrs
        |> Map.put(:from_node_id, from_node.id)
        |> Map.put(:to_node_id, to_node.id)
        |> Map.put(:from_change_id, from_change_id)
        |> Map.put(:to_change_id, to_change_id)

      create_edge(workspace_id, edge_attrs)
    end
  end

  @doc """
  Deletes an edge between two nodes.
  """
  def delete_edge(from_node_id, to_node_id, edge_type \\ "leads_to") do
    Edge
    |> where([e], e.from_node_id == ^from_node_id and e.to_node_id == ^to_node_id)
    |> where([e], e.edge_type == ^edge_type)
    |> Repo.one()
    |> case do
      nil ->
        {:error, :not_found}

      edge ->
        case Repo.delete(edge) do
          {:ok, deleted} ->
            audit_change(edge.workspace_id, deleted, "delete")
            broadcast(edge.workspace_id, {:edge_deleted, deleted})
            {:ok, deleted}

          error ->
            error
        end
    end
  end

  @doc """
  Lists all edges in a workspace.
  """
  def list_edges(workspace_id) do
    Edge
    |> where([e], e.workspace_id == ^workspace_id)
    |> order_by([e], asc: e.inserted_at)
    |> Repo.all()
  end

  @doc """
  Gets edges going out from a node.
  """
  def edges_from(node_id) do
    Edge
    |> where([e], e.from_node_id == ^node_id)
    |> Repo.all()
  end

  @doc """
  Gets edges coming into a node.
  """
  def edges_to(node_id) do
    Edge
    |> where([e], e.to_node_id == ^node_id)
    |> Repo.all()
  end

  # --- Private helpers ---

  defp get_workspace_node(workspace_id, node_id) do
    Node
    |> where([n], n.id == ^node_id and n.workspace_id == ^workspace_id)
    |> where([n], is_nil(n.deleted_at))
    |> Repo.one()
    |> case do
      nil -> {:error, {:node_not_found, node_id}}
      node -> {:ok, node}
    end
  end

  defp get_node_by_change_id(workspace_id, change_id) do
    Node
    |> where([n], n.workspace_id == ^workspace_id and n.change_id == ^change_id)
    |> where([n], is_nil(n.deleted_at))
    |> Repo.one()
    |> case do
      nil -> {:error, {:node_not_found_by_change_id, change_id}}
      node -> {:ok, node}
    end
  end

  defp audit_change(workspace_id, edge, action) do
    %AuditLog{}
    |> AuditLog.changeset(%{
      workspace_id: workspace_id,
      entity_type: "edge",
      entity_id: edge.id,
      action: action,
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
