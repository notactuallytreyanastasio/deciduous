defmodule DeciduousMcp.Web.ExportTombstonesTest do
  @moduledoc """
  C4 (server half): GET /export carries a node deleted on the server as a
  tombstone, so a pull can delete it locally. /export omitted deleted rows,
  so a node an agent deleted never left a local graph: pull imported 0,
  push re-sent the dead node on every write, and `remote status` said
  "Drift" forever.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.{Edges, Nodes, Workspaces}
  alias DeciduousMcp.Test.McpClient

  setup do
    {:ok, ws} = Workspaces.find_or_create("ex-ws")
    {:ok, a} = Nodes.create_node(ws.id, %{node_type: "goal", title: "ex A"})
    {:ok, b} = Nodes.create_node(ws.id, %{node_type: "action", title: "ex B"})
    {:ok, _} = Edges.create_edge(ws.id, %{from_node_id: a.id, to_node_id: b.id})

    client = McpClient.connect()
    McpClient.call!(client, "delete_node", %{"node_id" => b.id, "branch" => "ex"})
    {:ok, %{deleted_at: deleted_at}} = Nodes.get_node(b.id)

    %{client: client, a: a, b: b, deleted_at: deleted_at}
  end

  defp export(client) do
    conn = McpClient.get(client, "/export?workspace=ex-ws")
    assert conn.status == 200
    Jason.decode!(conn.resp_body)
  end

  test "a node deleted on the server is exported with its deleted_at", %{
    client: client,
    a: a,
    b: b,
    deleted_at: deleted_at
  } do
    graph = export(client)
    by_change = Map.new(graph["nodes"], &{&1["change_id"], &1})

    assert %{"deleted_at" => nil, "title" => "ex A"} = by_change[a.change_id]
    assert %{"deleted_at" => stamp, "title" => "ex B"} = by_change[b.change_id]
    assert {:ok, ^deleted_at, 0} = DateTime.from_iso8601(stamp)
    # The tombstone is newer than the node's last edit, so a merge that
    # orders by updated_at lets it win over a local copy of the live row.
    assert by_change[b.change_id]["updated_at"] >= by_change[b.change_id]["created_at"]
  end

  test "edges touching a deleted node are not exported, and counts say what is live", %{
    client: client
  } do
    graph = export(client)
    assert graph["edges"] == []
    assert graph["metadata"]["node_count"] == 1
    assert graph["metadata"]["deleted_node_count"] == 1
  end

  test "get_graph over MCP still shows live nodes only", %{client: client, b: b} do
    %{"nodes" => nodes} = McpClient.call!(client, "get_graph", %{"workspace" => "ex-ws"})
    refute Enum.any?(nodes, &(&1["id"] == b.id))
    refute Enum.any?(nodes, &Map.has_key?(&1, "deleted_at"))
  end
end
