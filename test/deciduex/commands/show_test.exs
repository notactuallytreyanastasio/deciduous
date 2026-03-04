defmodule Deciduex.Commands.ShowTest do
  use ExUnit.Case

  alias Deciduex.Commands.Show

  import ExUnit.CaptureIO
  import Deciduex.TestFixtures

  setup do
    create_tables!()

    insert_node!(%{
      id: 1,
      node_type: "goal",
      title: "Add authentication",
      description: "Full auth system",
      created_at: "2024-01-01T00:00:00Z",
      metadata_json:
        Jason.encode!(%{
          "branch" => "feature-auth",
          "confidence" => 90,
          "commit" => "abc123",
          "files" => ["src/auth.rs", "src/main.rs"],
          "prompt" => "Add user authentication\nwith JWT tokens"
        })
    })

    insert_node!(%{
      id: 2,
      node_type: "option",
      title: "Use JWT",
      created_at: "2024-01-02T00:00:00Z"
    })

    insert_edge!(%{
      id: 1,
      from_node_id: 1,
      to_node_id: 2,
      edge_type: "leads_to",
      rationale: "possible approach"
    })

    :ok
  end

  test "renders node with all fields" do
    output = capture_io(fn -> Show.run(["1"]) end)

    assert output =~ "Node #1 goal"
    assert output =~ String.duplicate("─", 60)
    assert output =~ "Title: Add authentication"
    assert output =~ "Description: Full auth system"
    assert output =~ "Status: active"
    assert output =~ "Created: 2024-01-01T00:00:00Z"
    assert output =~ "Updated: 2024-01-01T00:00:00Z"
  end

  test "renders metadata" do
    output = capture_io(fn -> Show.run(["1"]) end)

    assert output =~ "Metadata"
    assert output =~ "Confidence: 90%"
    assert output =~ "Branch: feature-auth"
    assert output =~ "Commit: abc123"
    assert output =~ "Files: src/auth.rs, src/main.rs"
  end

  test "renders prompt" do
    output = capture_io(fn -> Show.run(["1"]) end)

    assert output =~ "Prompt"
    assert output =~ "Add user authentication"
    assert output =~ "with JWT tokens"
  end

  test "renders connections" do
    output = capture_io(fn -> Show.run(["1"]) end)

    assert output =~ "Connections"
    assert output =~ "Outgoing (1):"
    assert output =~ "here ─[leads_to]→ #2: possible approach"
  end

  test "renders incoming connections" do
    output = capture_io(fn -> Show.run(["2"]) end)

    assert output =~ "Incoming (1):"
    assert output =~ "#1 ─[leads_to]→ here: possible approach"
  end

  test "renders node without metadata" do
    output = capture_io(fn -> Show.run(["2"]) end)

    assert output =~ "Node #2 option"
    assert output =~ "Title: Use JWT"
    refute output =~ "Metadata"
  end

  test "--json outputs valid JSON" do
    output = capture_io(fn -> Show.run(["1", "--json"]) end)

    assert {:ok, decoded} = Jason.decode(output)
    assert decoded["id"] == 1
    assert decoded["title"] == "Add authentication"
    assert decoded["node_type"] == "goal"
  end
end
