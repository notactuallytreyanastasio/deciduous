defmodule DeciduousMcp.MCP.AddNodeParentTest do
  @moduledoc """
  add_node with parent_id creates the node and its incoming edge in one call,
  so an agent never has to write an id it has not received yet. The failure
  this replaces: add_node and add_edge sent in the same batch, the edge
  carrying "PLACEHOLDER" where the new id would go.
  """
  use DeciduousMcp.DataCase

  alias DeciduousMcp.Graph.Nodes
  alias DeciduousMcp.MCP.Component
  alias DeciduousMcp.MCP.Tools.{AddEdge, AddNode}
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.{Edge, Node}

  setup do
    ws = create_test_workspace("parent")
    {:ok, goal} = Nodes.create_node(ws.id, %{node_type: "goal", title: "the goal"})
    frame = %Hermes.Server.Frame{private: %{session_id: "session_P"}, assigns: %{}}
    %{ws: ws, goal: goal, frame: frame}
  end

  defp call(tool, args, frame), do: Component.dispatch_tool(tool, args, frame)

  defp text({:reply, resp, _}), do: resp |> Map.from_struct() |> inspect()

  test "parent_id creates the node and links it in one call", %{goal: goal, frame: frame} do
    reply =
      call(
        AddNode,
        %{
          "workspace" => "parent",
          "node_type" => "action",
          "title" => "do it",
          "parent_id" => goal.id,
          "rationale" => "because"
        },
        frame
      )

    assert text(reply) =~ "Node created and linked"
    child = Repo.get_by!(Node, title: "do it")
    edge = Repo.get_by!(Edge, from_node_id: goal.id, to_node_id: child.id)
    assert edge.edge_type == "leads_to"
    assert edge.rationale == "because"
  end

  test "a parent that is not a node leaves nothing behind", %{frame: frame} do
    before = Repo.aggregate(Node, :count)
    missing = Ecto.UUID.generate()

    assert {:error, %Hermes.MCP.Error{message: message}, _} =
             call(
               AddNode,
               %{
                 "workspace" => "parent",
                 "node_type" => "action",
                 "title" => "orphan?",
                 "parent_id" => missing
               },
               frame
             )

    assert message =~ "parent_id #{missing} is not a node in this workspace; nothing was created"
    assert Repo.aggregate(Node, :count) == before
  end

  test "a placeholder parent_id is refused in one line", %{frame: frame} do
    for bad <- ["PLACEHOLDER", "PLACEHOLDER_SKIP"] do
      assert {:error, %Hermes.MCP.Error{message: message}, _} =
               call(
                 AddNode,
                 %{
                   "workspace" => "parent",
                   "node_type" => "action",
                   "title" => "x",
                   "parent_id" => bad
                 },
                 frame
               )

      assert message =~ "parent_id is not a node id"
    end
  end

  test "add_edge with a placeholder points at parent_id", %{goal: goal, frame: frame} do
    assert {:error, %Hermes.MCP.Error{message: message}, _} =
             call(
               AddEdge,
               %{
                 "workspace" => "parent",
                 "from_node_id" => goal.id,
                 "to_node_id" => "PLACEHOLDER"
               },
               frame
             )

    assert message =~ "to_node_id is not a node id"
    assert message =~ "pass parent_id to add_node"
  end
end
