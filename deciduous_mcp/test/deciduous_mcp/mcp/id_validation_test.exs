defmodule DeciduousMcp.MCP.IdValidationTest do
  @moduledoc """
  An argument that names a node but is not a UUID is answered, not raised.
  The production shape was add_edge with from_node_id "", which raised
  Ecto.Query.CastError inside the handler and took the server down.
  """
  use DeciduousMcp.DataCase

  alias DeciduousMcp.Graph.{Edges, Nodes}
  alias DeciduousMcp.MCP.Component
  alias DeciduousMcp.MCP.Tools.{AddEdge, GetDescendants, LogObservation, ShowNode, UpdateNode}

  setup do
    ws = create_test_workspace("ids")
    {:ok, n} = Nodes.create_node(ws.id, %{node_type: "goal", title: "real"})

    %{
      ws: ws,
      n: n,
      frame: %Hermes.Server.Frame{private: %{session_id: "session_I"}, assigns: %{}}
    }
  end

  test "every id-taking tool refuses a blank or garbage id with a one-line error", %{
    n: n,
    frame: frame
  } do
    cases = [
      {AddEdge, %{"workspace" => "ids", "from_node_id" => "", "to_node_id" => n.id},
       "from_node_id"},
      {AddEdge, %{"workspace" => "ids", "from_node_id" => n.id, "to_node_id" => "garbage"},
       "to_node_id"},
      {UpdateNode, %{"node_id" => "garbage", "title" => "x"}, "node_id"},
      {ShowNode, %{"node_id" => ""}, "node_id"},
      {GetDescendants, %{"node_id" => "nope"}, "node_id"},
      {LogObservation, %{"workspace" => "ids", "title" => "t", "took_from" => "zzz"},
       "took_from"},
      {LogObservation, %{"workspace" => "ids", "title" => "t", "related_to" => 42}, "related_to"}
    ]

    for {tool, args, key} <- cases do
      assert {:error, %Hermes.MCP.Error{} = err, _frame} =
               Component.dispatch_tool(tool, args, frame)

      assert err.message =~ "#{key} is not a node id", "#{inspect(tool)}: #{err.message}"
    end
  end

  test "a valid id still dispatches", %{n: n, frame: frame} do
    assert {:reply, _, _} = Component.dispatch_tool(ShowNode, %{"node_id" => n.id}, frame)
  end

  test "the graph layer answers not found for a non-UUID instead of raising", %{ws: ws, n: n} do
    assert {:error, :not_found} = Nodes.get_node("garbage")
    assert {:error, :not_found} = Nodes.get_node("")

    assert {:error, {:node_not_found, ""}} =
             Edges.create_edge(ws.id, %{from_node_id: "", to_node_id: n.id})
  end
end
