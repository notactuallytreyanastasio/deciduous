defmodule Deciduex.Commands.StatusTest do
  use ExUnit.Case

  import ExUnit.CaptureIO
  import Deciduex.TestFixtures

  alias Deciduex.Commands.Status
  alias Deciduex.Queries

  setup do
    create_tables!()
    insert_node!(%{id: 1, node_type: "goal", title: "Test goal", status: "pending", created_at: "2024-01-01T10:00:00Z"})
    :ok
  end

  describe "status command" do
    test "updates node status to active" do
      output =
        capture_io(fn ->
          Status.run(["1", "active"])
        end)

      assert output =~ "Updated node 1 status"
      assert output =~ "pending -> active"

      node = Queries.get_node(1)
      assert node.status == "active"
    end

    test "updates node status to superseded" do
      capture_io(fn ->
        Status.run(["1", "superseded"])
      end)

      node = Queries.get_node(1)
      assert node.status == "superseded"
    end

    test "updates node status to abandoned" do
      capture_io(fn ->
        Status.run(["1", "abandoned"])
      end)

      node = Queries.get_node(1)
      assert node.status == "abandoned"
    end

    test "can revert status back to pending" do
      # First set to active
      capture_io(fn -> Status.run(["1", "active"]) end)
      assert Queries.get_node(1).status == "active"

      # Then back to pending
      capture_io(fn -> Status.run(["1", "pending"]) end)
      assert Queries.get_node(1).status == "pending"
    end
  end
end
