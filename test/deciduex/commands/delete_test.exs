defmodule Deciduex.Commands.DeleteTest do
  use ExUnit.Case

  import ExUnit.CaptureIO
  import Deciduex.TestFixtures

  alias Deciduex.Commands.Delete
  alias Deciduex.Queries

  setup do
    create_tables!()
    insert_node!(%{id: 1, node_type: "goal", title: "Goal 1", created_at: "2024-01-01T10:00:00Z"})

    insert_node!(%{
      id: 2,
      node_type: "option",
      title: "Option 1",
      created_at: "2024-01-01T11:00:00Z"
    })

    insert_edge!(%{id: 1, from_node_id: 1, to_node_id: 2, edge_type: "leads_to"})
    :ok
  end

  describe "delete command" do
    test "deletes node and its edges" do
      assert length(Queries.list_nodes()) == 2
      assert length(Queries.list_edges()) == 1

      output =
        capture_io(fn ->
          Delete.run(["2"])
        end)

      assert output =~ "Deleted node #2"
      assert output =~ "Option 1"

      assert length(Queries.list_nodes()) == 1
      assert Queries.list_edges() == []
    end

    test "dry run shows what would be deleted" do
      output =
        capture_io(fn ->
          Delete.run(["2", "--dry-run"])
        end)

      assert output =~ "Would delete node #2"
      assert output =~ "Option 1"
      assert output =~ "Incoming edges: 1"

      # Nothing should be deleted
      assert length(Queries.list_nodes()) == 2
      assert length(Queries.list_edges()) == 1
    end

    test "deleting source node removes outgoing edges" do
      output =
        capture_io(fn ->
          Delete.run(["1"])
        end)

      assert output =~ "Deleted node #1"
      assert Queries.list_edges() == []
    end
  end
end
