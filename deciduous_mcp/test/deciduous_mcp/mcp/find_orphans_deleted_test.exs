defmodule DeciduousMcp.MCP.FindOrphansDeletedTest do
  @moduledoc """
  S6: an edge from a deleted node does not make its child connected.
  find_orphans counted every edge's to_node_id as connected, so C in
  A -> B -> C stayed hidden after B was deleted, and a delete racing a link
  left live children under deleted parents that find_orphans reported as 0.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.{Edges, Nodes, Workspaces}
  alias DeciduousMcp.Test.McpClient

  setup do
    {:ok, ws} = Workspaces.find_or_create("fo-ws")
    %{ws: ws, client: McpClient.connect()}
  end

  # Edges.create_edge refuses a 2-cycle now; graphs written before it, and
  # local graphs mirrored by POST /import, still hold them, and find_orphans
  # has to read those.
  defp legacy_back_edge(ws, from, to) do
    %DeciduousMcp.Schema.Edge{}
    |> DeciduousMcp.Schema.Edge.changeset(%{
      workspace_id: ws.id,
      from_node_id: from.id,
      to_node_id: to.id,
      edge_type: "leads_to"
    })
    |> DeciduousMcp.Repo.insert!()
  end

  defp orphan_ids(client) do
    McpClient.call!(client, "find_orphans", %{"workspace" => "fo-ws"})["orphans"]
    |> Enum.map(& &1["id"])
    |> MapSet.new()
  end

  test "a child whose only parent was deleted is an orphan", %{ws: ws, client: client} do
    {:ok, a} = Nodes.create_node(ws.id, %{node_type: "goal", title: "A"})
    {:ok, b} = Nodes.create_node(ws.id, %{node_type: "action", title: "B"})
    {:ok, c} = Nodes.create_node(ws.id, %{node_type: "outcome", title: "C"})
    {:ok, _} = Edges.create_edge(ws.id, %{from_node_id: a.id, to_node_id: b.id})
    {:ok, _} = Edges.create_edge(ws.id, %{from_node_id: b.id, to_node_id: c.id})

    refute MapSet.member?(orphan_ids(client), c.id)
    McpClient.call!(client, "delete_node", %{"node_id" => b.id, "branch" => "fo"})

    orphans = orphan_ids(client)
    assert MapSet.member?(orphans, c.id)
    # B itself is deleted, so it is not reported.
    refute MapSet.member?(orphans, b.id)
  end

  test "a child with another live parent is not an orphan", %{ws: ws, client: client} do
    {:ok, a} = Nodes.create_node(ws.id, %{node_type: "goal", title: "A"})
    {:ok, b} = Nodes.create_node(ws.id, %{node_type: "action", title: "B"})
    {:ok, c} = Nodes.create_node(ws.id, %{node_type: "outcome", title: "C"})
    {:ok, _} = Edges.create_edge(ws.id, %{from_node_id: a.id, to_node_id: c.id})
    {:ok, _} = Edges.create_edge(ws.id, %{from_node_id: b.id, to_node_id: c.id})

    McpClient.call!(client, "delete_node", %{"node_id" => b.id, "branch" => "fo"})
    refute MapSet.member?(orphan_ids(client), c.id)
  end

  test "delete racing add_node with parent_id: every stranded child is reported", %{
    ws: ws,
    client: client
  } do
    rounds =
      for i <- 1..30 do
        {:ok, parent} =
          Nodes.create_node(ws.id, %{node_type: "action", title: "parent #{i}"})

        deleter = McpClient.connect(name: "deleter-#{i}")
        linker = McpClient.connect(name: "linker-#{i}")

        [del, add] =
          [
            Task.async(fn ->
              McpClient.call(deleter, "delete_node", %{
                "node_id" => parent.id,
                "branch" => "del-#{i}"
              })
            end),
            Task.async(fn ->
              McpClient.call(linker, "add_node", %{
                "node_type" => "outcome",
                "title" => "child #{i}",
                "parent_id" => parent.id,
                "workspace" => "fo-ws",
                "branch" => "add-#{i}"
              })
            end)
          ]
          |> Task.await_many(30_000)

        assert {:ok, _} = del
        {parent, add}
      end

    orphans = orphan_ids(client)

    stranded =
      for {_parent, {:ok, %{"id" => child_id}}} <- rounds do
        assert MapSet.member?(orphans, child_id),
               "child #{child_id} sits under a deleted parent but find_orphans omits it"

        child_id
      end

    # Every add either linked under the parent before it was deleted
    # (stranded, reported above) or was refused because it came after.
    refused = for {_p, {:error, m}} <- rounds, do: m
    assert length(stranded) + length(refused) == 30
    assert Enum.all?(refused, &(&1 =~ "parent"))
  end

  # X(goal) -> Y -> Z, Z -> Y, then X deleted. Y and Z each keep a live
  # incoming edge from the other, so "no incoming live edge" never named
  # either, and the pair was cut off from every goal with find_orphans
  # reporting nothing. W hangs below the cycle and is cut off with it.
  test "a cycle stranded by deleting its root is reported, with what hangs below it", %{
    ws: ws,
    client: client
  } do
    {:ok, x} = Nodes.create_node(ws.id, %{node_type: "goal", title: "X"})
    {:ok, y} = Nodes.create_node(ws.id, %{node_type: "action", title: "Y"})
    {:ok, z} = Nodes.create_node(ws.id, %{node_type: "outcome", title: "Z"})
    {:ok, w} = Nodes.create_node(ws.id, %{node_type: "observation", title: "W"})
    {:ok, _} = Edges.create_edge(ws.id, %{from_node_id: x.id, to_node_id: y.id})
    {:ok, _} = Edges.create_edge(ws.id, %{from_node_id: y.id, to_node_id: z.id})
    legacy_back_edge(ws, z, y)
    {:ok, _} = Edges.create_edge(ws.id, %{from_node_id: z.id, to_node_id: w.id})

    assert orphan_ids(client) == MapSet.new()
    McpClient.call!(client, "delete_node", %{"node_id" => x.id, "branch" => "fo"})
    assert orphan_ids(client) == MapSet.new([y.id, z.id, w.id])
  end

  test "a cycle still reachable from a live goal is not an orphan", %{ws: ws, client: client} do
    {:ok, g} = Nodes.create_node(ws.id, %{node_type: "goal", title: "G"})
    {:ok, y} = Nodes.create_node(ws.id, %{node_type: "action", title: "Y"})
    {:ok, z} = Nodes.create_node(ws.id, %{node_type: "outcome", title: "Z"})
    {:ok, _} = Edges.create_edge(ws.id, %{from_node_id: g.id, to_node_id: y.id})
    {:ok, _} = Edges.create_edge(ws.id, %{from_node_id: y.id, to_node_id: z.id})
    legacy_back_edge(ws, z, y)

    assert orphan_ids(client) == MapSet.new()
  end

  # The existing definition is unchanged: a parentless action is the
  # orphan; its children hang off it and are not reported again.
  test "a chain under a parentless non-goal reports only its head", %{ws: ws, client: client} do
    {:ok, a} = Nodes.create_node(ws.id, %{node_type: "action", title: "A"})
    {:ok, b} = Nodes.create_node(ws.id, %{node_type: "outcome", title: "B"})
    {:ok, _} = Edges.create_edge(ws.id, %{from_node_id: a.id, to_node_id: b.id})

    assert orphan_ids(client) == MapSet.new([a.id])
  end
end
