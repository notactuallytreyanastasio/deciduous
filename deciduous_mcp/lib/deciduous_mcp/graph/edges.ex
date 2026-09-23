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

  Every path that writes an edge comes through here (the MCP tools, POST
  /ops), and here two rules hold under one lock:

    * `{:error, {:edge_exists, edge}}` when the edge is already there.
      The check before the insert used to be each caller's, or nobody's:
      an MCP add_edge that lost a race to an /ops create of the same edge
      failed on the unique index as "from_node_id: has already been taken".
    * `{:error, {:reverse_exists, edge}}` when the reverse edge is (a
      2-cycle: the two nodes would be each other's parent). took_from, a
      borrow between branches rather than the tree, is exempt on both
      sides. This lived in the add_edge tool alone, as a read before the
      insert: two sessions adding A -> B and B -> A at once wrote both
      in 15 rounds of 15, /ops create_edge never asked, and add_node's
      change_id retry linked a node under its own child (T6, T3).

  The lock is on the unordered pair of nodes, so creates between two
  nodes, in either direction, take turns; the second then sees what the
  first committed.
  """
  def create_edge(workspace_id, attrs) do
    edge_attrs = Map.put(attrs, :workspace_id, workspace_id)
    type = attrs[:edge_type] || attrs["edge_type"] || "leads_to"

    Repo.transaction(fn ->
      # Verify both nodes exist and are in the same workspace
      with {:ok, from_node} <-
             get_workspace_node(workspace_id, attrs[:from_node_id] || attrs["from_node_id"]),
           {:ok, to_node} <-
             get_workspace_node(workspace_id, attrs[:to_node_id] || attrs["to_node_id"]),
           :ok <- lock_pair(from_node.id, to_node.id),
           :ok <- absent(from_node.id, to_node.id, type),
           :ok <- no_reverse(from_node.id, to_node.id, type) do
        edge_attrs =
          edge_attrs
          |> Map.put(:from_change_id, from_node.change_id)
          |> Map.put(:to_change_id, to_node.change_id)

        # A CLI's link keeps the time it was made (see Sync.Ops): it is
        # what an unlink of the same edge elsewhere is ordered against.
        changeset =
          case edge_attrs[:inserted_at] do
            %DateTime{} = at ->
              %Edge{} |> Edge.changeset(edge_attrs) |> Ecto.Changeset.put_change(:inserted_at, at)

            _ ->
              Edge.changeset(%Edge{}, edge_attrs)
          end

        case Repo.insert(changeset) do
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
  Makes every create of an edge between these two nodes, in either
  direction, wait for the others, until the calling transaction ends.
  The key is hashed (hashtextextended); two pairs that collide only wait
  for each other.
  """
  def lock_pair(a, b) do
    [x, y] = Enum.sort([a, b])
    Repo.query!("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))", ["pair|#{x}|#{y}"])
    :ok
  end

  defp absent(from_id, to_id, type) do
    case Repo.one(
           from e in Edge,
             where: e.from_node_id == ^from_id and e.to_node_id == ^to_id and e.edge_type == ^type
         ) do
      nil -> :ok
      edge -> {:error, {:edge_exists, edge}}
    end
  end

  defp no_reverse(_from_id, _to_id, "took_from"), do: :ok

  defp no_reverse(from_id, to_id, _type) do
    case Repo.one(
           from e in Edge,
             where:
               e.from_node_id == ^to_id and e.to_node_id == ^from_id and
                 e.edge_type != "took_from",
             limit: 1
         ) do
      nil -> :ok
      edge -> {:error, {:reverse_exists, edge}}
    end
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

  # A non-UUID id would raise Ecto.Query.CastError in the query below; the
  # tools refuse those before they get here, this is the belt for callers
  # that do not go through a tool.
  defp get_workspace_node(_workspace_id, node_id) when not is_binary(node_id),
    do: {:error, {:node_not_found, node_id}}

  defp get_workspace_node(workspace_id, node_id) do
    case Ecto.UUID.cast(node_id) do
      :error ->
        {:error, {:node_not_found, node_id}}

      {:ok, _} ->
        # FOR SHARE: a concurrent delete_node (FOR UPDATE) waits for this
        # edge to commit, or this read waits for the delete and then sees
        # the node as deleted. Without it both committed, leaving a live
        # child under a parent deleted a moment earlier.
        Node
        |> where([n], n.id == ^node_id and n.workspace_id == ^workspace_id)
        |> where([n], is_nil(n.deleted_at))
        |> lock("FOR SHARE")
        |> Repo.one()
        |> case do
          nil -> {:error, {:node_not_found, node_id}}
          node -> {:ok, node}
        end
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
