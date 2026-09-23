defmodule DeciduousMcp.MCP.LogDecisionTest do
  @moduledoc """
  log_decision writes a decision, its options and their edges all or not at
  all. On production (2026-09-23 05:04) rejected_options given as strings
  crashed it after the decision node was already written, leaving a decision
  with no edges; and a parent_node_id that was not a node orphaned the
  decision while the call reported success.
  """
  use DeciduousMcp.DataCase

  alias DeciduousMcp.Graph.Nodes
  alias DeciduousMcp.MCP.Component
  alias DeciduousMcp.MCP.Tools.LogDecision
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.{Edge, Node}

  setup do
    ws = create_test_workspace("decide")
    {:ok, goal} = Nodes.create_node(ws.id, %{node_type: "goal", title: "g"})
    frame = %Hermes.Server.Frame{private: %{session_id: "session_D"}, assigns: %{}}
    %{goal: goal, frame: frame}
  end

  defp call(args, frame),
    do: Component.dispatch_tool(LogDecision, Map.put(args, "workspace", "decide"), frame)

  defp counts, do: {Repo.aggregate(Node, :count), Repo.aggregate(Edge, :count)}

  test "options as plain strings are titles, and everything is linked", %{
    goal: goal,
    frame: frame
  } do
    assert {:reply, _, _} =
             call(
               %{
                 "title" => "Pick a store",
                 "chosen_option" => "Postgres",
                 "rejected_options" => [
                   "Append-only event log, replayed on every clone",
                   "SQLite"
                 ],
                 "parent_node_id" => goal.id
               },
               frame
             )

    d = Repo.get_by!(Node, title: "Pick a store")

    kinds =
      Repo.all(Edge)
      |> Enum.filter(&(&1.from_node_id == d.id))
      |> Enum.map(& &1.edge_type)
      |> Enum.sort()

    assert kinds == ["chosen", "rejected", "rejected"]
    assert Repo.get_by(Edge, from_node_id: goal.id, to_node_id: d.id)
  end

  test "a parent that is not a node writes nothing", %{frame: frame} do
    before = counts()
    missing = Ecto.UUID.generate()

    assert {:error, %Hermes.MCP.Error{message: message}, _} =
             call(
               %{
                 "title" => "orphan?",
                 "chosen_option" => %{"title" => "a"},
                 "parent_node_id" => missing
               },
               frame
             )

    assert message =~ "is not a node in this workspace; nothing was written"
    assert counts() == before
  end

  test "an option that is neither a title nor an object with one writes nothing", %{frame: frame} do
    before = counts()

    for bad <- [
          %{"title" => "x", "chosen_option" => %{"name" => "a"}},
          %{"title" => "x", "chosen_option" => "a", "rejected_options" => ["b", 42]}
        ] do
      assert {:error, %Hermes.MCP.Error{message: message}, _} = call(bad, frame)
      assert message =~ "nothing was written"
    end

    assert counts() == before
  end

  test "objects with descriptions and reasons still work", %{frame: frame} do
    assert {:reply, _, _} =
             call(
               %{
                 "title" => "Pick a queue",
                 "chosen_option" => %{"title" => "Oban", "description" => "in Postgres"},
                 "rejected_options" => [%{"title" => "Kafka", "reason" => "one more system"}]
               },
               frame
             )

    rejected = Repo.get_by!(Node, title: "Kafka")
    assert Repo.get_by!(Edge, to_node_id: rejected.id).rationale == "one more system"
  end
end
