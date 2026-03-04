defmodule Deciduex.Commands.UnlinkTest do
  use ExUnit.Case

  import ExUnit.CaptureIO
  import Deciduex.TestFixtures

  alias Deciduex.Commands.Unlink
  alias Deciduex.Queries

  setup do
    create_tables!()
    # Create two nodes and an edge
    insert_node!(%{id: 1, node_type: "goal", title: "Goal 1", created_at: "2024-01-01T10:00:00Z"})
    insert_node!(%{id: 2, node_type: "option", title: "Option 1", created_at: "2024-01-01T11:00:00Z"})
    insert_edge!(%{id: 1, from_node_id: 1, to_node_id: 2, edge_type: "leads_to"})
    :ok
  end

  describe "unlink command" do
    test "deletes edge between two nodes" do
      # Verify edge exists first
      assert length(Queries.list_edges()) == 1

      output =
        capture_io(fn ->
          Unlink.run(["1", "2"])
        end)

      assert output =~ "Deleted edge"
      assert output =~ "1 -> 2"

      # Verify edge is deleted
      assert length(Queries.list_edges()) == 0
    end

    test "multiple edges - only deletes specified one" do
      # Add another node and edge
      insert_node!(%{id: 3, node_type: "decision", title: "Decision 1", created_at: "2024-01-01T12:00:00Z"})
      insert_edge!(%{id: 2, from_node_id: 2, to_node_id: 3, edge_type: "chosen"})

      assert length(Queries.list_edges()) == 2

      capture_io(fn ->
        Unlink.run(["1", "2"])
      end)

      # Only one edge should remain
      edges = Queries.list_edges()
      assert length(edges) == 1
      assert hd(edges).from_node_id == 2
      assert hd(edges).to_node_id == 3
    end
  end
end
