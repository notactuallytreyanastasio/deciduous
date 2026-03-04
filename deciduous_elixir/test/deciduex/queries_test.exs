defmodule Deciduex.QueriesTest do
  use ExUnit.Case

  import Deciduex.TestFixtures

  setup do
    create_tables!()
    :ok
  end

  describe "list_nodes/0" do
    test "returns empty list when no nodes" do
      assert Deciduex.Queries.list_nodes() == []
    end

    test "returns nodes ordered by created_at" do
      insert_node!(%{id: 1, node_type: "goal", title: "Second", created_at: "2024-01-02T00:00:00Z"})
      insert_node!(%{id: 2, node_type: "action", title: "First", created_at: "2024-01-01T00:00:00Z"})

      nodes = Deciduex.Queries.list_nodes()

      assert length(nodes) == 2
      assert Enum.at(nodes, 0).title == "First"
      assert Enum.at(nodes, 1).title == "Second"
    end

    test "includes metadata_json" do
      metadata = Jason.encode!(%{"branch" => "main", "confidence" => 90})

      insert_node!(%{
        id: 1,
        node_type: "goal",
        title: "With metadata",
        created_at: "2024-01-01T00:00:00Z",
        metadata_json: metadata
      })

      [node] = Deciduex.Queries.list_nodes()
      assert {:ok, %{"branch" => "main"}} = Jason.decode(node.metadata_json)
    end
  end

  describe "list_edges/0" do
    test "returns empty list when no edges" do
      assert Deciduex.Queries.list_edges() == []
    end

    test "returns edges ordered by created_at" do
      insert_edge!(%{id: 1, from_node_id: 1, to_node_id: 2, created_at: "2024-01-02T00:00:00Z"})
      insert_edge!(%{id: 2, from_node_id: 2, to_node_id: 3, created_at: "2024-01-01T00:00:00Z"})

      edges = Deciduex.Queries.list_edges()

      assert length(edges) == 2
      assert Enum.at(edges, 0).id == 2
      assert Enum.at(edges, 1).id == 1
    end
  end

  describe "get_node/1" do
    test "returns node when found" do
      insert_node!(%{id: 42, node_type: "goal", title: "My goal", created_at: "2024-01-01T00:00:00Z"})

      node = Deciduex.Queries.get_node(42)
      assert node.title == "My goal"
      assert node.node_type == "goal"
    end

    test "returns nil when not found" do
      assert Deciduex.Queries.get_node(999) == nil
    end
  end

  describe "get_node_edges/1" do
    test "returns incoming and outgoing edges" do
      insert_node!(%{id: 1, node_type: "goal", title: "A", created_at: "2024-01-01T00:00:00Z"})
      insert_node!(%{id: 2, node_type: "option", title: "B", created_at: "2024-01-02T00:00:00Z"})
      insert_node!(%{id: 3, node_type: "decision", title: "C", created_at: "2024-01-03T00:00:00Z"})

      insert_edge!(%{id: 1, from_node_id: 1, to_node_id: 2, rationale: "in"})
      insert_edge!(%{id: 2, from_node_id: 2, to_node_id: 3, rationale: "out"})

      {incoming, outgoing} = Deciduex.Queries.get_node_edges(2)

      assert length(incoming) == 1
      assert Enum.at(incoming, 0).from_node_id == 1

      assert length(outgoing) == 1
      assert Enum.at(outgoing, 0).to_node_id == 3
    end

    test "returns empty lists when no edges" do
      {incoming, outgoing} = Deciduex.Queries.get_node_edges(999)
      assert incoming == []
      assert outgoing == []
    end
  end

  describe "get_graph/0" do
    test "returns nodes, edges, and documents" do
      insert_node!(%{id: 1, node_type: "goal", title: "A", created_at: "2024-01-01T00:00:00Z"})
      insert_edge!(%{id: 1, from_node_id: 1, to_node_id: 2})

      graph = Deciduex.Queries.get_graph()

      assert length(graph.nodes) == 1
      assert length(graph.edges) == 1
      assert graph.documents == []
    end
  end

  describe "list_recent_commands/1" do
    test "returns empty list when no commands" do
      assert Deciduex.Queries.list_recent_commands() == []
    end

    test "returns commands ordered by started_at descending" do
      insert_command!(%{id: 1, command: "first", started_at: "2024-01-01T10:00:00Z"})
      insert_command!(%{id: 2, command: "second", started_at: "2024-01-02T10:00:00Z"})

      commands = Deciduex.Queries.list_recent_commands()

      assert length(commands) == 2
      assert Enum.at(commands, 0).command == "second"
      assert Enum.at(commands, 1).command == "first"
    end

    test "respects limit" do
      insert_command!(%{id: 1, command: "first", started_at: "2024-01-01T10:00:00Z"})
      insert_command!(%{id: 2, command: "second", started_at: "2024-01-02T10:00:00Z"})
      insert_command!(%{id: 3, command: "third", started_at: "2024-01-03T10:00:00Z"})

      commands = Deciduex.Queries.list_recent_commands(2)

      assert length(commands) == 2
      assert Enum.at(commands, 0).command == "third"
    end
  end
end
