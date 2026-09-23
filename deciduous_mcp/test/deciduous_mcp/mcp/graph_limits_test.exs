defmodule DeciduousMcp.MCP.GraphLimitsTest do
  @moduledoc """
  get_graph refuses a graph it should not try to serialise, and the walks say
  when they stopped. Before: get_graph returned 31 MB for a 7,805-node
  workspace and get_descendants returned exactly 50 nodes from any hub with
  nothing to say it had cut the subtree.
  """
  use DeciduousMcp.DataCase

  alias DeciduousMcp.Graph.{Edges, Nodes}
  alias DeciduousMcp.MCP.Tools.{GetDescendants, GetGraph}

  setup do
    ws = create_test_workspace("limits")
    frame = %Hermes.Server.Frame{private: %{session_id: "session_L"}, assigns: %{}}

    {:ok, a} = Nodes.create_node(ws.id, %{node_type: "goal", title: "a", description: "why a"})
    {:ok, b} = Nodes.create_node(ws.id, %{node_type: "action", title: "b"})
    {:ok, c} = Nodes.create_node(ws.id, %{node_type: "outcome", title: "c"})
    {:ok, _} = Edges.create_edge(ws.id, %{from_node_id: a.id, to_node_id: b.id})
    {:ok, _} = Edges.create_edge(ws.id, %{from_node_id: b.id, to_node_id: c.id})
    %{ws: ws, frame: frame, a: a}
  end

  defp graph(frame, args) do
    {:ok, json} = GetGraph.call(%{arguments: Map.put(args, "workspace", "limits"), server: frame})
    Jason.decode!(json)
  end

  test "the default projection carries no descriptions; include_details does", %{frame: frame} do
    slim = graph(frame, %{})
    assert Enum.all?(slim["nodes"], &(not Map.has_key?(&1, "description")))
    assert Enum.all?(slim["nodes"], &Map.has_key?(&1, "branch"))
    assert Enum.all?(slim["edges"], &(not Map.has_key?(&1, "rationale")))

    full = graph(frame, %{"include_details" => true})
    assert Enum.find(full["nodes"], &(&1["title"] == "a"))["description"] == "why a"
    assert Enum.all?(full["edges"], &Map.has_key?(&1, "rationale"))
  end

  test "a graph over max_nodes is refused with the count and the ways out", %{frame: frame} do
    {:error, %{message: message}} =
      GetGraph.call(%{arguments: %{"workspace" => "limits", "max_nodes" => 2}, server: frame})

    assert message =~ "3 live nodes, over max_nodes=2"
    assert message =~ "query_nodes"
    assert message =~ "/export"
  end

  test "a walk that hits max_nodes says so", %{a: a} do
    {:ok, json} = GetDescendants.call(%{arguments: %{"node_id" => a.id, "max_nodes" => 2}})
    %{"count" => 2, "truncated" => true} = Jason.decode!(json)

    {:ok, json} = GetDescendants.call(%{arguments: %{"node_id" => a.id}})
    %{"count" => 3, "truncated" => false} = Jason.decode!(json)
  end
end
