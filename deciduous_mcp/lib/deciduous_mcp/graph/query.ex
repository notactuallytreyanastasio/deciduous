defmodule DeciduousMcp.Graph.Query do
  @moduledoc """
  Graph traversal and query operations.
  Returns the full graph structure that the Deciduous web viewer expects.
  """
  import Ecto.Query
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.{Node, Edge, Theme, NodeTheme, Document}

  @doc """
  Returns the full decision graph for a workspace.
  Output format matches the Rust CLI's `deciduous graph` JSON output
  so the existing web viewer can consume it.

  Options:
  - `:branch` — filter nodes/edges to a specific git branch
  - `:include_deleted` — include soft-deleted nodes (default false)
  """
  def get_full_graph(scope, opts \\ []) do
    nodes = fetch_nodes(scope, opts)
    node_ids = Enum.map(nodes, & &1.id)
    edges = fetch_edges(scope, node_ids)
    themes = fetch_themes(scope)
    documents = fetch_documents(scope, node_ids)
    node_themes = fetch_node_themes(node_ids)

    %{
      nodes: Enum.map(nodes, &serialize_node/1),
      edges: Enum.map(edges, &serialize_edge/1),
      themes: Enum.map(themes, &serialize_theme/1),
      documents: Enum.map(documents, &serialize_document/1),
      node_themes: Enum.map(node_themes, &serialize_node_theme/1),
      metadata: %{
        workspace_id: scope,
        node_count: length(nodes),
        edge_count: length(edges),
        exported_at: DateTime.utc_now() |> DateTime.to_iso8601()
      }
    }
  end

  @doc """
  Finds orphan nodes (nodes with no incoming edges and not of type 'goal').
  These indicate missing connections in the graph.
  """
  def find_orphans(scope) do
    # Nodes that have no incoming edges and aren't goals
    connected_node_ids =
      Edge
      |> scope_ws(scope)
      |> select([e], e.to_node_id)
      |> Repo.all()

    Node
    |> scope_ws(scope)
    |> where([n], is_nil(n.deleted_at))
    |> where([n], n.node_type != "goal")
    |> where([n], n.id not in ^connected_node_ids)
    |> Repo.all()
  end

  @doc """
  Gets the ancestors of a node (walks edges backward).
  Returns nodes in order from root to the given node.
  """
  def ancestors(node_id, max_depth \\ 50) do
    walk_graph(node_id, :backward, max_depth)
  end

  @doc """
  Gets the descendants of a node (walks edges forward).
  """
  def descendants(node_id, max_depth \\ 50) do
    walk_graph(node_id, :forward, max_depth)
  end

  # --- Private helpers ---

  # `:global` drops the workspace predicate, giving the cross-project view.
  # Defined once per schema because Ecto binds the field to the queried table.
  defp scope_ws(query, :global), do: query
  defp scope_ws(query, workspace_id), do: where(query, [x], x.workspace_id == ^workspace_id)

  defp fetch_nodes(scope, opts) do
    query =
      Node
      |> scope_ws(scope)
      |> then(fn q ->
        if opts[:include_deleted], do: q, else: where(q, [n], is_nil(n.deleted_at))
      end)

    query =
      case opts[:branch] do
        nil -> query
        branch -> where(query, [n], fragment("? ->> 'branch' = ?", n.metadata, ^branch))
      end

    query
    |> order_by([n], asc: n.inserted_at)
    |> Repo.all()
  end

  defp fetch_edges(scope, node_ids) do
    Edge
    |> scope_ws(scope)
    |> where([e], e.from_node_id in ^node_ids and e.to_node_id in ^node_ids)
    |> order_by([e], asc: e.inserted_at)
    |> Repo.all()
  end

  defp fetch_themes(scope) do
    Theme
    |> scope_ws(scope)
    |> Repo.all()
  end

  defp fetch_documents(scope, node_ids) do
    Document
    |> scope_ws(scope)
    |> where([d], d.node_id in ^node_ids)
    |> where([d], is_nil(d.detached_at))
    |> Repo.all()
  end

  defp fetch_node_themes(node_ids) do
    NodeTheme
    |> where([nt], nt.node_id in ^node_ids)
    |> Repo.all()
  end

  defp walk_graph(start_node_id, direction, max_depth) do
    do_walk([start_node_id], MapSet.new(), direction, max_depth, [])
  end

  defp do_walk([], _visited, _direction, _depth, acc), do: Enum.reverse(acc)
  defp do_walk(_queue, _visited, _direction, 0, acc), do: Enum.reverse(acc)

  defp do_walk([current | rest], visited, direction, depth, acc) do
    if MapSet.member?(visited, current) do
      do_walk(rest, visited, direction, depth, acc)
    else
      visited = MapSet.put(visited, current)
      node = Repo.get(Node, current)

      neighbors =
        case direction do
          :forward ->
            Edge
            |> where([e], e.from_node_id == ^current)
            |> select([e], e.to_node_id)
            |> Repo.all()

          :backward ->
            Edge
            |> where([e], e.to_node_id == ^current)
            |> select([e], e.from_node_id)
            |> Repo.all()
        end

      new_acc = if node, do: [node | acc], else: acc
      do_walk(rest ++ neighbors, visited, direction, depth - 1, new_acc)
    end
  end

  # --- Serializers (match Rust CLI output format) ---

  defp serialize_node(node) do
    %{
      id: node.id,
      change_id: node.change_id,
      node_type: node.node_type,
      title: node.title,
      description: node.description,
      status: node.status,
      metadata: node.metadata || %{},
      created_at: DateTime.to_iso8601(node.inserted_at),
      updated_at: DateTime.to_iso8601(node.updated_at)
    }
  end

  defp serialize_edge(edge) do
    %{
      id: edge.id,
      from_node_id: edge.from_node_id,
      to_node_id: edge.to_node_id,
      from_change_id: edge.from_change_id,
      to_change_id: edge.to_change_id,
      edge_type: edge.edge_type,
      weight: edge.weight,
      rationale: edge.rationale,
      created_at: DateTime.to_iso8601(edge.inserted_at)
    }
  end

  defp serialize_theme(theme) do
    %{
      id: theme.id,
      change_id: theme.change_id,
      name: theme.name,
      color: theme.color,
      description: theme.description
    }
  end

  defp serialize_document(doc) do
    %{
      id: doc.id,
      change_id: doc.change_id,
      node_id: doc.node_id,
      original_filename: doc.original_filename,
      mime_type: doc.mime_type,
      file_size: doc.file_size,
      description: doc.description,
      description_source: doc.description_source
    }
  end

  defp serialize_node_theme(nt) do
    %{
      node_id: nt.node_id,
      theme_id: nt.theme_id,
      source: nt.source
    }
  end
end
