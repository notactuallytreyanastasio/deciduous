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
  - `:tombstones` — also return soft-deleted nodes, each carrying
    `deleted_at` (default false; `GET /export` sets it). Edges touching a
    deleted node are never returned.
  """
  def get_full_graph(scope, opts \\ []) do
    # `details: false` drops description, metadata and rationale. On the
    # 7,805-node epstein graph those three fields are 10.8MB of a 31.3MB body,
    # and the MCP client re-parses the body as a string inside JSON-RPC, so
    # every byte is paid for three times (encode, escape, decode).
    details? = Keyword.get(opts, :details, true)
    tombstones? = Keyword.get(opts, :tombstones, false)

    fetched =
      fetch_nodes(scope, if(tombstones?, do: [{:include_deleted, true} | opts], else: opts))

    {nodes, dead} = Enum.split_with(fetched, &is_nil(&1.deleted_at))
    node_ids = Enum.map(nodes, & &1.id)
    edges = fetch_edges(scope, node_ids)
    themes = fetch_themes(scope)
    documents = fetch_documents(scope, node_ids)
    node_themes = fetch_node_themes(node_ids)

    serialize = &serialize_node(&1, details?)

    # A tombstone is the whole row with `deleted_at` set, in `nodes` beside
    # the live ones: the shape the CLI's pull already reads (RemoteNode has
    # a deleted_at field and hands it to reconcile, which deletes). Only
    # /export asks for them; `deleted_at` is on every node there, nil for a
    # live one, so a reader need not guess what a missing key means.
    serialized =
      if tombstones?,
        do:
          Enum.map(fetched, fn n ->
            Map.put(serialize.(n), :deleted_at, n.deleted_at && DateTime.to_iso8601(n.deleted_at))
          end),
        else: Enum.map(nodes, serialize)

    %{
      nodes: serialized,
      edges: Enum.map(edges, &serialize_edge(&1, details?)),
      themes: Enum.map(themes, &serialize_theme/1),
      documents: Enum.map(documents, &serialize_document/1),
      node_themes: Enum.map(node_themes, &serialize_node_theme/1),
      metadata: %{
        workspace_id: scope,
        node_count: length(nodes),
        deleted_node_count: length(dead),
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
    # Nodes with no incoming edge from a live node, that aren't goals. An
    # edge from a deleted node connects nothing: every other read drops it,
    # so counting it here hid exactly the nodes a delete had just stranded.
    # One NOT EXISTS rather than the old list of every to_node_id in the
    # workspace sent back to Postgres as a parameter.
    from(n in Node, as: :node)
    |> scope_ws(scope)
    |> where([n], is_nil(n.deleted_at))
    |> where([n], n.node_type != "goal")
    |> where(
      [n],
      not exists(
        from e in Edge,
          join: p in Node,
          on: p.id == e.from_node_id,
          where: e.to_node_id == parent_as(:node).id and is_nil(p.deleted_at),
          select: 1
      )
    )
    |> Repo.all()
  end

  @doc """
  Gets the ancestors of a node (walks edges backward).
  Returns nodes in order from root to the given node.
  """
  def ancestors(node_id, max_depth \\ 50) do
    {nodes, _truncated} = walk_graph(node_id, :backward, max_depth)
    nodes
  end

  @doc """
  Gets the descendants of a node (walks edges forward).
  """
  def descendants(node_id, max_depth \\ 50) do
    {nodes, _truncated} = walk_graph(node_id, :forward, max_depth)
    nodes
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

  # Both endpoints of an edge are in its workspace by construction
  # (`Edges.create_edge/2` checks it), so the only edges the IN-list ever
  # removed were ones touching a soft-deleted node. Filtering those here saves
  # sending the node id list back to Postgres as two 7,805-element arrays: a
  # 600KB statement with 16ms of planning on production for a 54ms query.
  defp fetch_edges(scope, node_ids) do
    live = MapSet.new(node_ids)

    Edge
    |> scope_ws(scope)
    |> order_by([e], asc: e.inserted_at)
    |> Repo.all()
    |> Enum.filter(
      &(MapSet.member?(live, &1.from_node_id) and MapSet.member?(live, &1.to_node_id))
    )
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

  @walk_max_nodes 1_000

  # Breadth-first by level, two queries per level rather than two per node.
  # The previous version decremented `depth` once per dequeued node, so
  # `max_depth: 50` returned exactly 50 nodes from any hub and said nothing
  # about having stopped: `get_descendants` on epstein's root goal returned
  # `count: 50` from a subtree of thousands. Returns `{nodes, truncated?}`.
  #
  # Only live nodes are visited, and a walk does not pass through a deleted
  # one: A -> B -> C with B deleted is [A] from A. get_graph and /export
  # already drop edges touching a deleted node, so a walk that went through
  # B described a path no other read shows (the probe got
  # [A, "B zombie", C]). A deleted start node yields nothing.
  def walk_graph(start_node_id, direction, max_depth, max_nodes \\ @walk_max_nodes) do
    visited = MapSet.new([start_node_id])

    acc =
      case Repo.get(Node, start_node_id) do
        %Node{deleted_at: nil} = start -> [start]
        _ -> []
      end

    frontier = Enum.map(acc, & &1.id)
    do_walk_levels(frontier, visited, direction, max_depth, max_nodes, acc, false)
  end

  defp do_walk_levels([], _visited, _dir, _depth, _max, acc, truncated),
    do: {Enum.reverse(acc), truncated}

  defp do_walk_levels(_frontier, _visited, _dir, 0, _max, acc, _truncated),
    do: {Enum.reverse(acc), true}

  defp do_walk_levels(frontier, visited, direction, depth, max_nodes, acc, _truncated) do
    next_ids =
      case direction do
        :forward ->
          Edge
          |> where([e], e.from_node_id in ^frontier)
          |> select([e], e.to_node_id)
          |> Repo.all()

        :backward ->
          Edge
          |> where([e], e.to_node_id in ^frontier)
          |> select([e], e.from_node_id)
          |> Repo.all()
      end
      |> Enum.uniq()
      |> Enum.reject(&MapSet.member?(visited, &1))

    # Mark the dead ones visited too, so they are not asked for again.
    visited = Enum.reduce(next_ids, visited, &MapSet.put(&2, &1))

    live =
      if next_ids == [],
        do: [],
        else:
          Node
          |> where([n], n.id in ^next_ids and is_nil(n.deleted_at))
          |> Repo.all()

    room = max_nodes - length(acc)
    {nodes, dropped} = Enum.split(live, max(room, 0))
    acc = Enum.reverse(nodes) ++ acc

    if dropped != [] do
      {Enum.reverse(acc), true}
    else
      do_walk_levels(
        Enum.map(nodes, & &1.id),
        visited,
        direction,
        depth - 1,
        max_nodes,
        acc,
        false
      )
    end
  end

  def ancestors_bounded(node_id, opts \\ []),
    do:
      walk_graph(node_id, :backward, opts[:max_depth] || 50, opts[:max_nodes] || @walk_max_nodes)

  def descendants_bounded(node_id, opts \\ []),
    do: walk_graph(node_id, :forward, opts[:max_depth] || 50, opts[:max_nodes] || @walk_max_nodes)

  # --- Serializers (match Rust CLI output format) ---

  defp serialize_node(node, details?) do
    base = %{
      id: node.id,
      change_id: node.change_id,
      node_type: node.node_type,
      title: node.title,
      status: node.status,
      # The branch is the one metadata key every reader needs; keep it in
      # the slim form so a branch filter stays possible client-side.
      branch: node.metadata && node.metadata["branch"],
      created_at: DateTime.to_iso8601(node.inserted_at),
      updated_at: DateTime.to_iso8601(node.updated_at)
    }

    if details?,
      do: Map.merge(base, %{description: node.description, metadata: node.metadata || %{}}),
      else: base
  end

  # The slim edge is what an LLM needs to follow the graph: which two nodes
  # and how. The change-id pair duplicates the node ids in a second
  # namespace and the timestamp is rarely read; together they were 12MB of
  # the 21MB slim epstein body.
  defp serialize_edge(edge, false) do
    %{
      id: edge.id,
      from_node_id: edge.from_node_id,
      to_node_id: edge.to_node_id,
      edge_type: edge.edge_type
    }
  end

  defp serialize_edge(edge, true) do
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
