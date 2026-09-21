defmodule DeciduousMcp.Graph.EdgesTest do
  use DeciduousMcp.DataCase

  alias DeciduousMcp.Graph.{Nodes, Edges}

  setup do
    workspace = create_test_workspace()

    {:ok, goal} =
      Nodes.create_node(workspace.id, %{node_type: "goal", title: "Add auth"})

    {:ok, option} =
      Nodes.create_node(workspace.id, %{node_type: "option", title: "Use JWT"})

    %{workspace_id: workspace.id, goal: goal, option: option}
  end

  describe "create_edge/2" do
    test "creates an edge between nodes", %{workspace_id: wid, goal: goal, option: option} do
      assert {:ok, edge} =
               Edges.create_edge(wid, %{
                 from_node_id: goal.id,
                 to_node_id: option.id,
                 edge_type: "leads_to",
                 rationale: "Exploring options"
               })

      assert edge.from_node_id == goal.id
      assert edge.to_node_id == option.id
      assert edge.edge_type == "leads_to"
    end

    test "prevents duplicate edges", %{workspace_id: wid, goal: goal, option: option} do
      {:ok, _} =
        Edges.create_edge(wid, %{
          from_node_id: goal.id,
          to_node_id: option.id,
          edge_type: "leads_to"
        })

      assert {:error, _} =
               Edges.create_edge(wid, %{
                 from_node_id: goal.id,
                 to_node_id: option.id,
                 edge_type: "leads_to"
               })
    end

    test "allows different edge types between same nodes", %{
      workspace_id: wid,
      goal: goal,
      option: option
    } do
      {:ok, _} =
        Edges.create_edge(wid, %{
          from_node_id: goal.id,
          to_node_id: option.id,
          edge_type: "leads_to"
        })

      assert {:ok, _} =
               Edges.create_edge(wid, %{
                 from_node_id: goal.id,
                 to_node_id: option.id,
                 edge_type: "chosen"
               })
    end

    test "prevents self-loops", %{workspace_id: wid, goal: goal} do
      assert {:error, _} =
               Edges.create_edge(wid, %{
                 from_node_id: goal.id,
                 to_node_id: goal.id,
                 edge_type: "leads_to"
               })
    end
  end
end
