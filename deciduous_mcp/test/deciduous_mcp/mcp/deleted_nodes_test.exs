defmodule DeciduousMcp.MCP.DeletedNodesTest do
  @moduledoc """
  S5: a soft-deleted node is gone from every read and refused by every
  write, over the wire. It was readable by id with no deleted flag,
  editable, deletable again (which reset deleted_at), and walked through by
  get_descendants/get_ancestors and listed by ask_graph as a neighbour.

  Fixture: A -> B -> C in one workspace, B deleted through the tool.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.{Edges, Nodes, Workspaces}
  alias DeciduousMcp.Test.McpClient

  setup do
    {:ok, ws} = Workspaces.find_or_create("dn-ws")
    {:ok, a} = Nodes.create_node(ws.id, %{node_type: "goal", title: "dn quokka A"})
    {:ok, b} = Nodes.create_node(ws.id, %{node_type: "action", title: "dn quokka B"})
    {:ok, c} = Nodes.create_node(ws.id, %{node_type: "outcome", title: "dn quokka C"})
    {:ok, _} = Edges.create_edge(ws.id, %{from_node_id: a.id, to_node_id: b.id})
    {:ok, _} = Edges.create_edge(ws.id, %{from_node_id: b.id, to_node_id: c.id})

    client = McpClient.connect()
    McpClient.call!(client, "delete_node", %{"node_id" => b.id, "branch" => "dn"})
    {:ok, %{deleted_at: deleted_at}} = Nodes.get_node(b.id)
    assert deleted_at

    %{client: client, a: a, b: b, c: c, deleted_at: deleted_at}
  end

  test "show_node refuses a deleted node", %{client: client, b: b} do
    assert {:error, message} = McpClient.call(client, "show_node", %{"node_id" => b.id})
    assert message =~ b.id and message =~ "deleted"
  end

  test "show_node on a live node does not list edges to a deleted one", %{
    client: client,
    a: a,
    c: c
  } do
    assert %{"edges_from" => []} = McpClient.call!(client, "show_node", %{"node_id" => a.id})
    assert %{"edges_to" => []} = McpClient.call!(client, "show_node", %{"node_id" => c.id})
  end

  test "update_node refuses a deleted node and leaves it as it was", %{client: client, b: b} do
    assert {:error, message} =
             McpClient.call(client, "update_node", %{
               "node_id" => b.id,
               "title" => "dn zombie",
               "branch" => "dn"
             })

    assert message =~ "deleted"
    assert {:ok, %{title: "dn quokka B"}} = Nodes.get_node(b.id)
  end

  test "delete_node twice is refused the second time and keeps the first deleted_at", %{
    client: client,
    b: b,
    deleted_at: deleted_at
  } do
    assert {:error, message} =
             McpClient.call(client, "delete_node", %{"node_id" => b.id, "branch" => "dn"})

    assert message =~ "was deleted at"
    assert {:ok, %{deleted_at: ^deleted_at}} = Nodes.get_node(b.id)
  end

  test "walks do not start at, include, or pass through a deleted node", %{
    client: client,
    a: a,
    b: b,
    c: c
  } do
    assert %{"nodes" => down} =
             McpClient.call!(client, "get_descendants", %{"node_id" => a.id})

    assert Enum.map(down, & &1["id"]) == [a.id]

    assert %{"nodes" => up} = McpClient.call!(client, "get_ancestors", %{"node_id" => c.id})
    assert Enum.map(up, & &1["id"]) == [c.id]

    assert {:error, message} = McpClient.call(client, "get_descendants", %{"node_id" => b.id})
    assert message =~ "deleted"
  end

  test "ask_graph does not name a deleted neighbour", %{client: client} do
    %{"results" => results} =
      McpClient.call!(client, "ask_graph", %{"workspace" => "dn-ws", "question" => "quokka"})

    titles = Enum.map(results, & &1["title"])
    assert "dn quokka A" in titles
    refute "dn quokka B" in titles

    neighbours =
      Enum.flat_map(results, fn r ->
        Enum.map((r["connects_to"] || []) ++ (r["connected_from"] || []), & &1["title"])
      end)

    refute "dn quokka B" in neighbours
  end

  # write_scope_for_node checked the from node only, so an edge into a
  # deleted node could be deleted ("Edge deleted") while one out of it
  # was refused.
  test "delete_edge into a deleted node is refused and the edge row stays", %{
    client: client,
    a: a,
    b: b
  } do
    assert {:error, message} =
             McpClient.call(client, "delete_edge", %{
               "from_node_id" => a.id,
               "to_node_id" => b.id,
               "branch" => "dn"
             })

    assert message =~ b.id and message =~ "deleted"
    assert [_] = Edges.edges_to(b.id)
  end

  # node_count was live-only and edge_count was not: the two edges through
  # B were counted, while /export and get_graph leave both out.
  test "list_workspaces counts only edges between live nodes", %{client: client} do
    %{"workspaces" => ws} = McpClient.call!(client, "list_workspaces", %{})
    assert %{"node_count" => 2, "edge_count" => 0} = Enum.find(ws, &(&1["name"] == "dn-ws"))

    %{"workspaces" => [pinned]} =
      McpClient.call!(McpClient.connect(pin: "dn-ws"), "list_workspaces", %{})

    assert %{"node_count" => 2, "edge_count" => 0} = pinned
  end

  test "add_edge to a deleted node is refused", %{client: client, a: a, b: b} do
    assert {:error, _} =
             McpClient.call(client, "add_edge", %{
               "from_node_id" => b.id,
               "to_node_id" => a.id,
               "workspace" => "dn-ws",
               "branch" => "dn"
             })
  end

  test "the context refuses too, for callers that do not go through Scope", %{
    b: b,
    deleted_at: deleted_at
  } do
    assert {:error, :deleted} = Nodes.update_node(b.id, %{title: "dn zombie"})
    assert {:error, :already_deleted} = Nodes.delete_node(b.id)
    assert {:ok, %{title: "dn quokka B", deleted_at: ^deleted_at}} = Nodes.get_node(b.id)
  end
end
