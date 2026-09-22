defmodule DeciduousMcp.Sync.ProcessorTest do
  use DeciduousMcp.DataCase

  alias DeciduousMcp.Sync.{Event, Processor}
  alias DeciduousMcp.Graph.Nodes

  setup do
    workspace = create_test_workspace()
    %{workspace_id: workspace.id}
  end

  describe "apply_event/2 - AddNode" do
    test "creates a node from a CLI event", %{workspace_id: wid} do
      event = %Event{
        type: "AddNode",
        data: %{
          "change_id" => UUID.uuid4(),
          "node_type" => "goal",
          "title" => "Add dark mode",
          "description" => "Support light and dark themes",
          "status" => "active",
          "metadata_json" => Jason.encode!(%{"confidence" => 90, "branch" => "main"})
        },
        timestamp: System.system_time(:millisecond),
        author: "developer"
      }

      assert {:ok, node} = Processor.apply_event(event, wid)
      assert node.title == "Add dark mode"
      assert node.node_type == "goal"
    end

    test "is idempotent — skips duplicate change_ids", %{workspace_id: wid} do
      change_id = UUID.uuid4()

      event = %Event{
        type: "AddNode",
        data: %{
          "change_id" => change_id,
          "node_type" => "goal",
          "title" => "Duplicate test"
        },
        timestamp: System.system_time(:millisecond),
        author: "developer"
      }

      assert {:ok, _} = Processor.apply_event(event, wid)
      assert {:ok, :already_exists} = Processor.apply_event(event, wid)

      # Only one node should exist
      nodes = Nodes.list_nodes(wid)
      assert length(nodes) == 1
    end
  end

  describe "apply_event/2 - UpdateNode" do
    test "updates an existing node", %{workspace_id: wid} do
      change_id = UUID.uuid4()

      # First create
      add_event = %Event{
        type: "AddNode",
        data: %{
          "change_id" => change_id,
          "node_type" => "goal",
          "title" => "Original title"
        },
        timestamp: System.system_time(:millisecond),
        author: "developer"
      }

      {:ok, _} = Processor.apply_event(add_event, wid)

      # Then update
      update_event = %Event{
        type: "UpdateNode",
        data: %{
          "change_id" => change_id,
          "title" => "Updated title",
          "status" => "completed"
        },
        timestamp: System.system_time(:millisecond),
        author: "developer"
      }

      assert {:ok, updated} = Processor.apply_event(update_event, wid)
      assert updated.title == "Updated title"
      assert updated.status == "completed"
    end
  end
end
