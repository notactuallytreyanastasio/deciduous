defmodule DeciduousMcp.Web.ImportTombstonesTest do
  @moduledoc """
  S5 bypass: POST /import, the CLI's push path, wrote to soft-deleted rows.
  Its upsert replaced title, status and metadata on a change_id whatever
  its deleted_at, and resolved edge endpoints against deleted rows too. So
  a tombstone could be rewritten and a new edge hung off a deleted node,
  while every MCP write tool refused both. Over the router.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.{Edges, Nodes, Workspaces}
  alias DeciduousMcp.Schema.Edge
  alias DeciduousMcp.Test.McpClient

  setup do
    {:ok, ws} = Workspaces.find_or_create("it-ws")
    {:ok, p} = Nodes.create_node(ws.id, %{node_type: "goal", title: "it P"})
    {:ok, d} = Nodes.create_node(ws.id, %{node_type: "action", title: "it D"})
    {:ok, _} = Edges.create_edge(ws.id, %{from_node_id: p.id, to_node_id: d.id})
    {:ok, dead} = Nodes.delete_node(d.id)
    %{ws: ws, p: p, d: dead, client: McpClient.connect()}
  end

  defp push(client, graph),
    do: McpClient.post_json(client, "/import", %{"workspace" => "it-ws", "graph" => graph})

  test "a deleted node is not rewritten, and the report names it", %{client: client, d: d, p: p} do
    {status, report} =
      push(client, %{
        "nodes" => [
          %{
            "change_id" => d.change_id,
            "node_type" => "action",
            "title" => "REWRITTEN VIA IMPORT",
            "status" => "completed"
          },
          %{"change_id" => p.change_id, "node_type" => "goal", "title" => "it P edited"}
        ]
      })

    assert status == 200, inspect(report)
    assert report["nodes"]["upserted"] == 1
    assert report["nodes"]["refused_deleted"] == 1

    assert [%{"change_id" => cid, "deleted_at" => stamp}] =
             report["nodes"]["refused_deleted_examples"]

    assert cid == d.change_id
    assert stamp == DateTime.to_iso8601(d.deleted_at)

    row = Repo.get!(DeciduousMcp.Schema.Node, d.id)
    assert {row.title, row.status, row.updated_at} == {"it D", "pending", d.updated_at}
    # The live node in the same payload was still applied.
    assert Repo.get!(DeciduousMcp.Schema.Node, p.id).title == "it P edited"
  end

  test "no edge is written from or to a deleted node", %{client: client, d: d, p: p} do
    {status, report} =
      push(client, %{
        "nodes" => [
          %{"id" => 9, "change_id" => "it-new", "node_type" => "outcome", "title" => "new"}
        ],
        "edges" => [
          %{"from_change_id" => d.change_id, "to_node_id" => 9, "edge_type" => "leads_to"},
          %{
            "from_change_id" => p.change_id,
            "to_change_id" => d.change_id,
            "edge_type" => "chosen"
          }
        ]
      })

    assert status == 200, inspect(report)
    assert report["edges"]["upserted"] == 0
    assert report["edges"]["refused_deleted"] == 2
    assert Repo.aggregate(from(e in Edge, where: e.from_node_id == ^d.id), :count) == 0
    assert Repo.aggregate(from(e in Edge, where: e.edge_type == "chosen"), :count) == 0
    assert {:ok, _} = Nodes.get_node_by_change_id(d.workspace_id, "it-new")
  end
end
