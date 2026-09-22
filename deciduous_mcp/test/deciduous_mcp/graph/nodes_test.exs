defmodule DeciduousMcp.Graph.NodesTest do
  use DeciduousMcp.DataCase

  alias DeciduousMcp.Graph.Nodes

  setup do
    workspace = create_test_workspace()
    %{workspace_id: workspace.id}
  end

  describe "create_node/2" do
    test "creates a goal node", %{workspace_id: wid} do
      assert {:ok, node} =
               Nodes.create_node(wid, %{
                 node_type: "goal",
                 title: "Add user authentication",
                 description: "Users should be able to sign up with email/password"
               })

      assert node.node_type == "goal"
      assert node.title == "Add user authentication"
      assert node.status == "pending"
      assert node.change_id != nil
    end

    test "creates a node with metadata", %{workspace_id: wid} do
      assert {:ok, node} =
               Nodes.create_node(wid, %{
                 node_type: "action",
                 title: "Implementing JWT middleware",
                 metadata: %{"confidence" => 85, "commit" => "abc1234", "branch" => "feature-auth"}
               })

      assert node.metadata["confidence"] == 85
      assert node.metadata["commit"] == "abc1234"
    end

    test "rejects invalid node type", %{workspace_id: wid} do
      assert {:error, _changeset} =
               Nodes.create_node(wid, %{
                 node_type: "invalid",
                 title: "Bad node"
               })
    end

    test "preserves provided change_id", %{workspace_id: wid} do
      custom_id = UUID.uuid4()

      assert {:ok, node} =
               Nodes.create_node(wid, %{
                 change_id: custom_id,
                 node_type: "goal",
                 title: "Test"
               })

      assert node.change_id == custom_id
    end
  end

  describe "list_nodes/2" do
    test "filters by node type", %{workspace_id: wid} do
      {:ok, _goal} = Nodes.create_node(wid, %{node_type: "goal", title: "Goal 1"})
      {:ok, _action} = Nodes.create_node(wid, %{node_type: "action", title: "Action 1"})

      goals = Nodes.list_nodes(wid, type: "goal")
      assert length(goals) == 1
      assert hd(goals).title == "Goal 1"
    end

    test "filters by status", %{workspace_id: wid} do
      {:ok, node} = Nodes.create_node(wid, %{node_type: "goal", title: "Goal 1"})
      {:ok, _} = Nodes.update_node(node.id, %{status: "completed"})

      completed = Nodes.list_nodes(wid, status: "completed")
      assert length(completed) == 1
    end

    test "text search in title and description", %{workspace_id: wid} do
      {:ok, _} = Nodes.create_node(wid, %{node_type: "goal", title: "Add authentication"})
      {:ok, _} = Nodes.create_node(wid, %{node_type: "goal", title: "Fix bug in parser"})

      results = Nodes.list_nodes(wid, search: "auth")
      assert length(results) == 1
      assert hd(results).title == "Add authentication"
    end
  end

  describe "delete_node/1" do
    test "soft-deletes a node", %{workspace_id: wid} do
      {:ok, node} = Nodes.create_node(wid, %{node_type: "goal", title: "To delete"})

      assert {:ok, deleted} = Nodes.delete_node(node.id)
      assert deleted.deleted_at != nil

      # Should not appear in list queries
      nodes = Nodes.list_nodes(wid)
      assert Enum.empty?(nodes)
    end
  end
end
