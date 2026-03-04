defmodule Deciduex.Commands.LinkTest do
  use ExUnit.Case

  import ExUnit.CaptureIO
  import Deciduex.TestFixtures

  alias Deciduex.Commands.Link
  alias Deciduex.Queries

  setup do
    create_tables!()
    # Create two nodes to link
    insert_node!(%{id: 1, node_type: "goal", title: "Goal 1", created_at: "2024-01-01T10:00:00Z"})

    insert_node!(%{
      id: 2,
      node_type: "option",
      title: "Option 1",
      created_at: "2024-01-01T11:00:00Z"
    })

    :ok
  end

  describe "link command" do
    test "creates edge between two nodes" do
      output =
        capture_io(fn ->
          Link.run(["1", "2"])
        end)

      assert output =~ "Created edge"
      assert output =~ "1 -> 2"
      assert output =~ "leads_to"

      # Verify edge exists
      edges = Queries.list_edges()
      assert length(edges) == 1
      edge = hd(edges)
      assert edge.from_node_id == 1
      assert edge.to_node_id == 2
      assert edge.edge_type == "leads_to"
    end

    test "creates edge with rationale" do
      output =
        capture_io(fn ->
          Link.run(["1", "2", "-r", "This is the reason"])
        end)

      assert output =~ "Created edge"

      edge = hd(Queries.list_edges())
      assert edge.rationale == "This is the reason"
    end

    test "creates edge with custom type" do
      output =
        capture_io(fn ->
          Link.run(["1", "2", "-t", "chosen"])
        end)

      assert output =~ "chosen"

      edge = hd(Queries.list_edges())
      assert edge.edge_type == "chosen"
    end

    test "creates edge with type and rationale" do
      capture_io(fn ->
        Link.run(["1", "2", "-t", "rejected", "-r", "Not suitable"])
      end)

      edge = hd(Queries.list_edges())
      assert edge.edge_type == "rejected"
      assert edge.rationale == "Not suitable"
    end
  end
end
