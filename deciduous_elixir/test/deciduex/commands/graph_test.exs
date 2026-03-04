defmodule Deciduex.Commands.GraphTest do
  use ExUnit.Case

  alias Deciduex.Commands.Graph

  import ExUnit.CaptureIO
  import Deciduex.TestFixtures

  setup do
    create_tables!()
    :ok
  end

  test "outputs valid JSON" do
    seed_sample_data!()
    output = capture_io(fn -> Graph.run() end)

    assert {:ok, decoded} = Jason.decode(output)
    assert is_list(decoded["nodes"])
    assert is_list(decoded["edges"])
    assert is_list(decoded["documents"])
  end

  test "includes all nodes" do
    seed_sample_data!()
    output = capture_io(fn -> Graph.run() end)
    {:ok, decoded} = Jason.decode(output)

    assert length(decoded["nodes"]) == 3
    titles = Enum.map(decoded["nodes"], & &1["title"])
    assert "Add auth" in titles
    assert "Use JWT" in titles
    assert "Choose JWT" in titles
  end

  test "includes all edges" do
    seed_sample_data!()
    output = capture_io(fn -> Graph.run() end)
    {:ok, decoded} = Jason.decode(output)

    assert length(decoded["edges"]) == 2
  end

  test "documents is empty list" do
    seed_sample_data!()
    output = capture_io(fn -> Graph.run() end)
    {:ok, decoded} = Jason.decode(output)

    assert decoded["documents"] == []
  end

  test "node has expected fields" do
    seed_sample_data!()
    output = capture_io(fn -> Graph.run() end)
    {:ok, decoded} = Jason.decode(output)

    node = Enum.at(decoded["nodes"], 0)
    assert Map.has_key?(node, "id")
    assert Map.has_key?(node, "change_id")
    assert Map.has_key?(node, "node_type")
    assert Map.has_key?(node, "title")
    assert Map.has_key?(node, "status")
    assert Map.has_key?(node, "created_at")
    assert Map.has_key?(node, "updated_at")
    assert Map.has_key?(node, "metadata_json")
  end

  test "edge has expected fields" do
    seed_sample_data!()
    output = capture_io(fn -> Graph.run() end)
    {:ok, decoded} = Jason.decode(output)

    edge = Enum.at(decoded["edges"], 0)
    assert Map.has_key?(edge, "id")
    assert Map.has_key?(edge, "from_node_id")
    assert Map.has_key?(edge, "to_node_id")
    assert Map.has_key?(edge, "edge_type")
    assert Map.has_key?(edge, "rationale")
    assert Map.has_key?(edge, "created_at")
  end

  test "empty graph outputs valid JSON" do
    output = capture_io(fn -> Graph.run() end)
    {:ok, decoded} = Jason.decode(output)

    assert decoded["nodes"] == []
    assert decoded["edges"] == []
    assert decoded["documents"] == []
  end
end
