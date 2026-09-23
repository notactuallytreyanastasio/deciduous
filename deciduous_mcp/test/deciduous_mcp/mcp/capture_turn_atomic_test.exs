defmodule DeciduousMcp.MCP.CaptureTurnAtomicTest do
  @moduledoc """
  capture_conversation_turn writes a whole step (goal, observations,
  options, decision, action, outcome) in one call, all or nothing. Before,
  each write stood alone and edge results were discarded, so a bad parent or
  a failure halfway left part of the turn in the graph.
  """
  use DeciduousMcp.DataCase

  alias DeciduousMcp.Graph.Nodes
  alias DeciduousMcp.MCP.Component
  alias DeciduousMcp.MCP.Tools.CaptureConversationTurn
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.{Edge, Node}

  setup do
    ws = create_test_workspace("turn")
    {:ok, root} = Nodes.create_node(ws.id, %{node_type: "goal", title: "root"})
    frame = %Hermes.Server.Frame{private: %{session_id: "session_T"}, assigns: %{}}
    %{root: root, frame: frame}
  end

  defp call(args, frame),
    do:
      Component.dispatch_tool(
        CaptureConversationTurn,
        Map.merge(%{"workspace" => "turn", "summary" => "s"}, args),
        frame
      )

  defp counts, do: {Repo.aggregate(Node, :count), Repo.aggregate(Edge, :count)}

  test "a whole step in one call, sections as plain strings, all linked", %{
    root: root,
    frame: frame
  } do
    before = counts()

    assert {:reply, _, _} =
             call(
               %{
                 "parent_node_id" => root.id,
                 "goal" => "Ship the site",
                 "observations" => ["deploy is rsync"],
                 "options_considered" => [%{"title" => "rsync", "chosen" => true}, "delete first"],
                 "decision" => "rsync without --delete",
                 "action" => "rsynced docs",
                 "outcome" => "site live"
               },
               frame
             )

    {n, e} = counts()
    assert n - elem(before, 0) == 7
    assert e - elem(before, 1) >= 6
    assert Repo.get_by!(Node, title: "rsync without --delete").node_type == "decision"
  end

  test "a parent that is not a node writes nothing", %{frame: frame} do
    before = counts()

    assert {:error, %Hermes.MCP.Error{message: message}, _} =
             call(
               %{"parent_node_id" => Ecto.UUID.generate(), "goal" => "g", "action" => "a"},
               frame
             )

    assert message =~ "is not a node in this workspace; nothing was written"
    assert counts() == before
  end

  test "a failure halfway through takes the earlier writes with it", %{root: root, frame: frame} do
    before = counts()

    # The goal and the first observation are valid; the second observation
    # has no title, so its insert fails after two nodes were written.
    assert {:error, %Hermes.MCP.Error{message: message}, _} =
             call(
               %{
                 "parent_node_id" => root.id,
                 "goal" => "g",
                 "observations" => ["fine", %{"description" => "no title"}]
               },
               frame
             )

    assert message =~ "nothing was written"
    assert counts() == before
  end
end
