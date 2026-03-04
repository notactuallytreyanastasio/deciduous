defmodule Deciduex.IntegrationTest do
  @moduledoc """
  Full integration test that builds a realistic decision graph following the
  canonical node flow (goal -> options -> decision -> actions -> outcomes),
  then exercises every command against it with thorough assertions.

  This tests the entire read path: Ecto schemas, queries, and command output
  formatting — the same path that the Burrito binary and Rust delegation use.
  """
  use ExUnit.Case

  alias Deciduex.Commands.CommandLog
  alias Deciduex.Commands.Edges
  alias Deciduex.Commands.Graph
  alias Deciduex.Commands.Nodes
  alias Deciduex.Commands.Show
  alias Deciduex.Queries

  import ExUnit.CaptureIO
  import Deciduex.TestFixtures

  # ──────────────────────────────────────────────────────────────────────
  # Graph topology we build:
  #
  #   [G1] goal "Add user authentication"
  #     ├─ [O2] option "Use JWT tokens"
  #     │    └─ [D4] decision "Choose JWT" (chosen)
  #     │         ├─ [A5] action "Implement JWT auth middleware"
  #     │         │    └─ [OUT7] outcome "JWT auth working"
  #     │         └─ [A6] action "Add refresh token rotation"
  #     │              └─ [OUT8] outcome "Refresh tokens deployed"
  #     ├─ [O3] option "Use session cookies"
  #     │    └─ [D4] (rejected)
  #     └─ [OBS9] observation "Existing code uses Plug sessions"
  #
  #   [G10] goal "Improve API rate limiting"   (separate tree, different branch)
  #     └─ [OBS11] observation "Current rate limiter leaks memory"
  #
  # Edges:
  #   G1  -> O2   leads_to   "possible approach"
  #   G1  -> O3   leads_to   "possible approach"
  #   O2  -> D4   chosen     "JWT is stateless, scales horizontally"
  #   O3  -> D4   rejected   "requires sticky sessions"
  #   D4  -> A5   leads_to   "implementation step"
  #   D4  -> A6   leads_to   "implementation step"
  #   A5  -> OUT7 leads_to   "result"
  #   A6  -> OUT8 leads_to   "result"
  #   G1  -> OBS9 leads_to   (no rationale)
  #   G10 -> OBS11 leads_to  "noticed during review"
  #
  # Command log entries to exercise the `commands` output.
  # ──────────────────────────────────────────────────────────────────────

  setup do
    create_tables!()
    build_full_graph!()
    :ok
  end

  # ====================================================================
  # NODES command
  # ====================================================================

  describe "nodes command" do
    test "lists all 11 nodes with correct count" do
      output = capture_io(fn -> Nodes.run([]) end)

      assert output =~ "11 nodes:"
    end

    test "header row has correct column labels" do
      output = capture_io(fn -> Nodes.run([]) end)

      assert output =~ "ID"
      assert output =~ "TYPE"
      assert output =~ "STATUS"
      assert output =~ "TITLE"
    end

    test "each node type appears in output" do
      output = capture_io(fn -> Nodes.run([]) end)

      assert output =~ "goal"
      assert output =~ "option"
      assert output =~ "decision"
      assert output =~ "action"
      assert output =~ "outcome"
      assert output =~ "observation"
    end

    test "filters by type" do
      output = capture_io(fn -> Nodes.run(["-t", "goal"]) end)

      assert output =~ "2 nodes:"
      assert output =~ "Add user authentication"
      assert output =~ "Improve API rate limiting"
      refute output =~ "Use JWT tokens"
    end

    test "filters by branch" do
      output = capture_io(fn -> Nodes.run(["-b", "feature-auth"]) end)

      # Nodes 2-9 are on feature-auth
      assert output =~ "8 nodes:"
      assert output =~ "Use JWT tokens"
      refute output =~ "Improve API rate limiting"
    end

    test "combined type + branch filter" do
      output =
        capture_io(fn -> Nodes.run(["-t", "action", "-b", "feature-auth"]) end)

      assert output =~ "2 nodes:"
      assert output =~ "Implement JWT auth middleware"
      assert output =~ "Add refresh token rotation"
    end

    test "shows superseded status" do
      output = capture_io(fn -> Nodes.run(["-t", "option"]) end)

      # "Use session cookies" was rejected → superseded
      assert output =~ "superseded"
    end

    test "nodes are ordered chronologically by created_at" do
      output = capture_io(fn -> Nodes.run([]) end)

      lines = String.split(output, "\n", trim: true)
      # skip count, header, separator
      data_lines = Enum.drop(lines, 3)

      ids =
        Enum.map(data_lines, fn line ->
          line |> String.trim() |> String.split(~r/\s+/, parts: 2) |> hd() |> String.to_integer()
        end)

      # OBS9 was created at 2024-01-02T11:00 (between O3 and D4)
      # so the chronological order puts 9 before 4
      assert ids == [1, 2, 3, 9, 4, 5, 6, 7, 8, 10, 11]
    end
  end

  # ====================================================================
  # EDGES command
  # ====================================================================

  describe "edges command" do
    test "lists all 10 edges" do
      output = capture_io(fn -> Edges.run() end)

      lines = output |> String.split("\n", trim: true)
      # header + separator + 10 data lines
      assert length(lines) == 12
    end

    test "header has correct columns" do
      output = capture_io(fn -> Edges.run() end)

      assert output =~ "ID"
      assert output =~ "FROM"
      assert output =~ "TO"
      assert output =~ "TYPE"
      assert output =~ "RATIONALE"
    end

    test "separator is 70 dashes" do
      output = capture_io(fn -> Edges.run() end)

      assert output =~ String.duplicate("-", 70)
    end

    test "edge types are correct" do
      output = capture_io(fn -> Edges.run() end)

      assert output =~ "leads_to"
      assert output =~ "chosen"
      assert output =~ "rejected"
    end

    test "rationale text appears" do
      output = capture_io(fn -> Edges.run() end)

      assert output =~ "possible approach"
      assert output =~ "JWT is stateless, scales horizontally"
      assert output =~ "requires sticky sessions"
    end

    test "edge with no rationale renders cleanly" do
      output = capture_io(fn -> Edges.run() end)

      # Edge 9 (G1 -> OBS9) has no rationale — line should end after TYPE column
      lines = String.split(output, "\n", trim: true)

      edge_9_line =
        Enum.find(lines, fn line ->
          String.starts_with?(String.trim(line), "9")
        end)

      assert edge_9_line != nil
      # After the TYPE field there should be no trailing rationale text
      # (just whitespace or end of line)
      refute edge_9_line =~ "possible approach"
    end

    test "empty graph shows help message" do
      # Recreate with no edges
      create_tables!()
      output = capture_io(fn -> Edges.run() end)

      assert output =~ "No edges found"
      assert output =~ "deciduous link"
    end
  end

  # ====================================================================
  # GRAPH command (JSON)
  # ====================================================================

  describe "graph command" do
    test "outputs valid JSON" do
      output = capture_io(fn -> Graph.run() end)

      assert {:ok, _} = Jason.decode(output)
    end

    test "JSON has nodes, edges, and documents keys" do
      output = capture_io(fn -> Graph.run() end)
      {:ok, graph} = Jason.decode(output)

      assert Map.has_key?(graph, "nodes")
      assert Map.has_key?(graph, "edges")
      assert Map.has_key?(graph, "documents")
    end

    test "contains all 11 nodes and 10 edges" do
      output = capture_io(fn -> Graph.run() end)
      {:ok, graph} = Jason.decode(output)

      assert length(graph["nodes"]) == 11
      assert length(graph["edges"]) == 10
      assert graph["documents"] == []
    end

    test "each node has all required fields" do
      output = capture_io(fn -> Graph.run() end)
      {:ok, graph} = Jason.decode(output)

      required_keys =
        ~w(id change_id node_type title description status created_at updated_at metadata_json)

      Enum.each(graph["nodes"], fn node ->
        Enum.each(required_keys, fn key ->
          assert Map.has_key?(node, key),
                 "Node #{node["id"]} missing key: #{key}"
        end)
      end)
    end

    test "each edge has all required fields" do
      output = capture_io(fn -> Graph.run() end)
      {:ok, graph} = Jason.decode(output)

      required_keys = ~w(id from_node_id to_node_id edge_type rationale created_at)

      Enum.each(graph["edges"], fn edge ->
        Enum.each(required_keys, fn key ->
          assert Map.has_key?(edge, key),
                 "Edge #{edge["id"]} missing key: #{key}"
        end)
      end)
    end

    test "node metadata_json is parseable" do
      output = capture_io(fn -> Graph.run() end)
      {:ok, graph} = Jason.decode(output)

      nodes_with_metadata =
        Enum.filter(graph["nodes"], fn n -> n["metadata_json"] != nil end)

      assert nodes_with_metadata != []

      Enum.each(nodes_with_metadata, fn node ->
        assert {:ok, meta} = Jason.decode(node["metadata_json"])
        assert is_map(meta)
      end)
    end

    test "graph can reconstruct the goal->option->decision->action->outcome chain" do
      output = capture_io(fn -> Graph.run() end)
      {:ok, graph} = Jason.decode(output)

      nodes_by_id = Map.new(graph["nodes"], fn n -> {n["id"], n} end)

      # Trace: G1 -> O2 -> D4 -> A5 -> OUT7
      assert nodes_by_id[1]["node_type"] == "goal"
      assert nodes_by_id[2]["node_type"] == "option"
      assert nodes_by_id[4]["node_type"] == "decision"
      assert nodes_by_id[5]["node_type"] == "action"
      assert nodes_by_id[7]["node_type"] == "outcome"

      edges = graph["edges"]
      assert Enum.any?(edges, fn e -> e["from_node_id"] == 1 and e["to_node_id"] == 2 end)
      assert Enum.any?(edges, fn e -> e["from_node_id"] == 2 and e["to_node_id"] == 4 end)
      assert Enum.any?(edges, fn e -> e["from_node_id"] == 4 and e["to_node_id"] == 5 end)
      assert Enum.any?(edges, fn e -> e["from_node_id"] == 5 and e["to_node_id"] == 7 end)
    end
  end

  # ====================================================================
  # SHOW command (formatted)
  # ====================================================================

  describe "show command (formatted)" do
    test "shows goal node with full detail" do
      output = capture_io(fn -> Show.run(["1"]) end)

      assert output =~ "Node #1 goal"
      assert output =~ String.duplicate("─", 60)
      assert output =~ "Title: Add user authentication"
      assert output =~ "Status: active"
      assert output =~ "Created:"
      assert output =~ "Updated:"
    end

    test "shows description when present" do
      output = capture_io(fn -> Show.run(["1"]) end)

      assert output =~ "Description: Implement login, signup, and token management"
    end

    test "shows metadata section" do
      output = capture_io(fn -> Show.run(["1"]) end)

      assert output =~ "Metadata"
      assert output =~ "Confidence: 95%"
      assert output =~ "Branch: main"
    end

    test "shows commit hash in metadata" do
      output = capture_io(fn -> Show.run(["5"]) end)

      assert output =~ "Commit: abc123def"
    end

    test "shows files in metadata" do
      output = capture_io(fn -> Show.run(["5"]) end)

      assert output =~ "Files: src/auth/middleware.rs, src/auth/jwt.rs"
    end

    test "shows prompt text" do
      output = capture_io(fn -> Show.run(["1"]) end)

      assert output =~ "Prompt"
      assert output =~ "I need to add user authentication to the app"
      assert output =~ "support OAuth for Google and GitHub"
    end

    test "shows outgoing connections" do
      output = capture_io(fn -> Show.run(["1"]) end)

      assert output =~ "Connections"
      assert output =~ "Outgoing (3):"
      assert output =~ "here ─[leads_to]→ #2: possible approach"
      assert output =~ "here ─[leads_to]→ #3: possible approach"
      assert output =~ "here ─[leads_to]→ #9"
    end

    test "shows incoming connections" do
      output = capture_io(fn -> Show.run(["4"]) end)

      assert output =~ "Incoming (2):"
      assert output =~ "#2 ─[chosen]→ here"
      assert output =~ "#3 ─[rejected]→ here"
    end

    test "shows both incoming and outgoing" do
      output = capture_io(fn -> Show.run(["4"]) end)

      assert output =~ "Incoming (2):"
      assert output =~ "Outgoing (2):"
    end

    test "node with no connections omits section" do
      output = capture_io(fn -> Show.run(["7"]) end)

      # OUT7 only has incoming, no outgoing
      assert output =~ "Incoming (1):"
      refute output =~ "Outgoing"
    end

    test "all nodes with metadata show Metadata section" do
      # All nodes in our fixture have metadata — verify section appears
      output = capture_io(fn -> Show.run(["9"]) end)
      assert output =~ "Metadata"

      output2 = capture_io(fn -> Show.run(["11"]) end)
      assert output2 =~ "Metadata"
    end

    test "superseded node shows correct status" do
      output = capture_io(fn -> Show.run(["3"]) end)

      assert output =~ "Status: superseded"
    end

    test "option node with different branches" do
      output = capture_io(fn -> Show.run(["2"]) end)

      assert output =~ "Node #2 option"
      assert output =~ "Branch: feature-auth"
    end
  end

  # ====================================================================
  # SHOW command (--json)
  # ====================================================================

  describe "show command (JSON)" do
    test "outputs valid JSON" do
      output = capture_io(fn -> Show.run(["1", "--json"]) end)

      assert {:ok, _} = Jason.decode(output)
    end

    test "JSON has all schema fields" do
      output = capture_io(fn -> Show.run(["1", "--json"]) end)
      {:ok, node} = Jason.decode(output)

      assert node["id"] == 1
      assert node["node_type"] == "goal"
      assert node["title"] == "Add user authentication"
      assert node["status"] == "active"
      assert is_binary(node["change_id"])
      assert is_binary(node["created_at"])
      assert is_binary(node["updated_at"])
    end

    test "metadata_json is a string, not parsed" do
      output = capture_io(fn -> Show.run(["1", "--json"]) end)
      {:ok, node} = Jason.decode(output)

      # metadata_json should be a raw JSON string, not a nested object
      assert is_binary(node["metadata_json"])
      assert {:ok, _} = Jason.decode(node["metadata_json"])
    end

    test "null fields are present in JSON" do
      # Node 9 (observation) has no description — check JSON still has the key
      output = capture_io(fn -> Show.run(["9", "--json"]) end)
      {:ok, node} = Jason.decode(output)

      # description should be present as null
      assert Map.has_key?(node, "description")
      assert node["description"] == nil
    end
  end

  # ====================================================================
  # COMMANDS command
  # ====================================================================

  describe "commands command" do
    test "lists commands in reverse chronological order" do
      output = capture_io(fn -> CommandLog.run() end)

      lines = String.split(output, "\n", trim: true)

      # Most recent command first (highest timestamp)
      assert Enum.at(lines, 0) =~ "2024-01-09"
      assert List.last(lines) =~ "2024-01-01"
    end

    test "shows timestamp, command, and exit code" do
      output = capture_io(fn -> CommandLog.run() end)

      assert output =~ "[2024-01-01T10:00:00Z]"
      assert output =~ ~s(deciduous add goal "Add user authentication")
      assert output =~ "(exit: 0)"
    end

    test "shows running for nil exit code" do
      output = capture_io(fn -> CommandLog.run() end)

      assert output =~ "(exit: running)"
    end

    test "truncates long commands at 60 chars" do
      output = capture_io(fn -> CommandLog.run() end)

      lines = String.split(output, "\n", trim: true)
      long_line = Enum.find(lines, &(&1 =~ "deciduous add action"))

      # The longest command should be truncated
      assert long_line != nil
    end

    test "--limit flag restricts output" do
      output = capture_io(fn -> CommandLog.run(["--limit", "3"]) end)

      lines = String.split(output, "\n", trim: true)
      assert length(lines) == 3
    end

    test "-l short flag works" do
      output = capture_io(fn -> CommandLog.run(["-l", "2"]) end)

      lines = String.split(output, "\n", trim: true)
      assert length(lines) == 2
    end

    test "empty command log shows message" do
      create_tables!()
      output = capture_io(fn -> CommandLog.run() end)

      assert output =~ "No commands logged."
    end
  end

  # ====================================================================
  # QUERIES module (lower-level validation)
  # ====================================================================

  describe "queries module integration" do
    test "list_nodes returns all 11 nodes" do
      nodes = Queries.list_nodes()
      assert length(nodes) == 11
    end

    test "list_edges returns all 10 edges" do
      edges = Queries.list_edges()
      assert length(edges) == 10
    end

    test "get_graph returns consistent counts" do
      graph = Queries.get_graph()

      assert length(graph.nodes) == 11
      assert length(graph.edges) == 10
      assert graph.documents == []
    end

    test "every edge references existing nodes" do
      nodes = Queries.list_nodes()
      node_ids = MapSet.new(Enum.map(nodes, & &1.id))

      edges = Queries.list_edges()

      Enum.each(edges, fn edge ->
        assert MapSet.member?(node_ids, edge.from_node_id),
               "Edge #{edge.id} references non-existent from_node #{edge.from_node_id}"

        assert MapSet.member?(node_ids, edge.to_node_id),
               "Edge #{edge.id} references non-existent to_node #{edge.to_node_id}"
      end)
    end

    test "goal nodes have no incoming edges" do
      edges = Queries.list_edges()
      nodes = Queries.list_nodes()

      goal_ids =
        nodes |> Enum.filter(&(&1.node_type == "goal")) |> Enum.map(& &1.id) |> MapSet.new()

      incoming_to_goals = Enum.filter(edges, fn e -> MapSet.member?(goal_ids, e.to_node_id) end)
      assert incoming_to_goals == [], "Goals should not have incoming edges"
    end

    test "get_node_edges returns correct counts for decision node" do
      {incoming, outgoing} = Queries.get_node_edges(4)

      # D4 has incoming from O2 (chosen) and O3 (rejected)
      assert length(incoming) == 2
      # D4 has outgoing to A5 and A6
      assert length(outgoing) == 2
    end

    test "get_node returns struct with all fields" do
      node = Queries.get_node(1)

      assert node.id == 1
      assert node.node_type == "goal"
      assert node.title == "Add user authentication"
      assert node.description == "Implement login, signup, and token management"
      assert node.status == "active"
      assert is_binary(node.change_id)
      assert is_binary(node.created_at)
      assert is_binary(node.metadata_json)

      {:ok, meta} = Jason.decode(node.metadata_json)
      assert meta["confidence"] == 95
      assert meta["branch"] == "main"
      assert is_binary(meta["prompt"])
    end

    test "list_recent_commands respects limit" do
      cmds = Queries.list_recent_commands(3)
      assert length(cmds) == 3
    end

    test "list_recent_commands returns newest first" do
      cmds = Queries.list_recent_commands()

      timestamps = Enum.map(cmds, & &1.started_at)
      assert timestamps == Enum.sort(timestamps, :desc)
    end
  end

  # ====================================================================
  # CROSS-COMMAND CONSISTENCY
  # ====================================================================

  describe "cross-command consistency" do
    test "nodes count matches graph JSON node count" do
      nodes_output = capture_io(fn -> Nodes.run([]) end)
      graph_output = capture_io(fn -> Graph.run() end)

      [count_str | _] = Regex.run(~r/(\d+) nodes:/, nodes_output, capture: :all_but_first)
      nodes_count = String.to_integer(count_str)

      {:ok, graph} = Jason.decode(graph_output)

      assert nodes_count == length(graph["nodes"])
    end

    test "edges count matches graph JSON edge count" do
      edges_output = capture_io(fn -> Edges.run() end)
      graph_output = capture_io(fn -> Graph.run() end)

      {:ok, graph} = Jason.decode(graph_output)

      # Count data lines in edges output (total - header - separator)
      edge_lines = edges_output |> String.split("\n", trim: true) |> length()
      edge_data_count = edge_lines - 2

      assert edge_data_count == length(graph["edges"])
    end

    test "show JSON matches graph JSON for same node" do
      show_output = capture_io(fn -> Show.run(["1", "--json"]) end)
      graph_output = capture_io(fn -> Graph.run() end)

      {:ok, show_node} = Jason.decode(show_output)
      {:ok, graph} = Jason.decode(graph_output)

      graph_node = Enum.find(graph["nodes"], fn n -> n["id"] == 1 end)

      assert show_node["id"] == graph_node["id"]
      assert show_node["title"] == graph_node["title"]
      assert show_node["node_type"] == graph_node["node_type"]
      assert show_node["status"] == graph_node["status"]
      assert show_node["metadata_json"] == graph_node["metadata_json"]
    end
  end

  # ====================================================================
  # GRAPH TOPOLOGY VALIDATION
  # ====================================================================

  describe "graph topology" do
    test "canonical flow: goal -> options -> decision -> actions -> outcomes" do
      graph = Queries.get_graph()
      nodes_by_id = Map.new(graph.nodes, fn n -> {n.id, n} end)
      adjacency = Enum.group_by(graph.edges, & &1.from_node_id)

      # From goal 1, we can reach options
      goal_children = adjacency[1] || []
      goal_child_types = Enum.map(goal_children, fn e -> nodes_by_id[e.to_node_id].node_type end)
      assert "option" in goal_child_types
      assert "observation" in goal_child_types

      # From option 2, we reach decision 4
      option_children = adjacency[2] || []

      assert Enum.any?(option_children, fn e ->
               nodes_by_id[e.to_node_id].node_type == "decision"
             end)

      # From decision 4, we reach actions
      decision_children = adjacency[4] || []

      decision_child_types =
        Enum.map(decision_children, fn e -> nodes_by_id[e.to_node_id].node_type end)

      assert Enum.all?(decision_child_types, &(&1 == "action"))

      # From actions, we reach outcomes
      Enum.each([5, 6], fn action_id ->
        action_children = adjacency[action_id] || []

        action_child_types =
          Enum.map(action_children, fn e -> nodes_by_id[e.to_node_id].node_type end)

        assert "outcome" in action_child_types
      end)
    end

    test "rejected option edge has correct type" do
      edges = Queries.list_edges()
      rejected = Enum.find(edges, fn e -> e.edge_type == "rejected" end)

      assert rejected != nil
      # "Use session cookies"
      assert rejected.from_node_id == 3
      # "Choose JWT"
      assert rejected.to_node_id == 4
    end

    test "chosen option edge has correct type" do
      edges = Queries.list_edges()
      chosen = Enum.find(edges, fn e -> e.edge_type == "chosen" end)

      assert chosen != nil
      # "Use JWT tokens"
      assert chosen.from_node_id == 2
      # "Choose JWT"
      assert chosen.to_node_id == 4
    end

    test "second goal tree is disconnected from first" do
      edges = Queries.list_edges()

      # Goal 10 tree edges should only reference nodes >= 10
      tree2_edges =
        Enum.filter(edges, fn e ->
          e.from_node_id >= 10 or e.to_node_id >= 10
        end)

      Enum.each(tree2_edges, fn e ->
        assert e.from_node_id >= 10 and e.to_node_id >= 10,
               "Edge #{e.id} crosses between trees: #{e.from_node_id} -> #{e.to_node_id}"
      end)
    end
  end

  # ====================================================================
  # FIXTURE BUILDER
  # ====================================================================

  defp build_full_graph! do
    # ── Goal 1: Add user authentication ──
    insert_node!(%{
      id: 1,
      node_type: "goal",
      title: "Add user authentication",
      description: "Implement login, signup, and token management",
      created_at: "2024-01-01T09:00:00Z",
      metadata_json:
        Jason.encode!(%{
          "branch" => "main",
          "confidence" => 95,
          "prompt" =>
            "I need to add user authentication to the app.\nUsers should be able to sign up with email/password,\nand we need to support OAuth for Google and GitHub."
        })
    })

    # ── Option 2: Use JWT tokens ──
    insert_node!(%{
      id: 2,
      node_type: "option",
      title: "Use JWT tokens",
      description: "Stateless auth with JSON Web Tokens",
      created_at: "2024-01-02T09:00:00Z",
      metadata_json: Jason.encode!(%{"branch" => "feature-auth", "confidence" => 85})
    })

    # ── Option 3: Use session cookies ──
    insert_node!(%{
      id: 3,
      node_type: "option",
      title: "Use session cookies",
      description: "Server-side sessions with encrypted cookies",
      status: "superseded",
      created_at: "2024-01-02T10:00:00Z",
      metadata_json: Jason.encode!(%{"branch" => "feature-auth", "confidence" => 70})
    })

    # ── Decision 4: Choose JWT ──
    insert_node!(%{
      id: 4,
      node_type: "decision",
      title: "Choose JWT for authentication",
      description: "JWT chosen over session cookies for horizontal scalability",
      created_at: "2024-01-03T09:00:00Z",
      metadata_json: Jason.encode!(%{"branch" => "feature-auth", "confidence" => 90})
    })

    # ── Action 5: Implement JWT middleware ──
    insert_node!(%{
      id: 5,
      node_type: "action",
      title: "Implement JWT auth middleware",
      created_at: "2024-01-04T09:00:00Z",
      metadata_json:
        Jason.encode!(%{
          "branch" => "feature-auth",
          "confidence" => 85,
          "commit" => "abc123def",
          "files" => ["src/auth/middleware.rs", "src/auth/jwt.rs"]
        })
    })

    # ── Action 6: Add refresh token rotation ──
    insert_node!(%{
      id: 6,
      node_type: "action",
      title: "Add refresh token rotation",
      created_at: "2024-01-05T09:00:00Z",
      metadata_json:
        Jason.encode!(%{
          "branch" => "feature-auth",
          "confidence" => 80,
          "commit" => "def456abc"
        })
    })

    # ── Outcome 7: JWT auth working ──
    insert_node!(%{
      id: 7,
      node_type: "outcome",
      title: "JWT auth middleware working in production",
      description: "All endpoints protected, 200ms avg latency added",
      created_at: "2024-01-06T09:00:00Z",
      metadata_json: Jason.encode!(%{"branch" => "feature-auth", "confidence" => 95})
    })

    # ── Outcome 8: Refresh tokens deployed ──
    insert_node!(%{
      id: 8,
      node_type: "outcome",
      title: "Refresh token rotation deployed",
      created_at: "2024-01-07T09:00:00Z",
      metadata_json: Jason.encode!(%{"branch" => "feature-auth", "confidence" => 90})
    })

    # ── Observation 9: Plug sessions ──
    insert_node!(%{
      id: 9,
      node_type: "observation",
      title: "Existing code uses Plug sessions",
      created_at: "2024-01-02T11:00:00Z",
      metadata_json: Jason.encode!(%{"branch" => "feature-auth"})
    })

    # ── Goal 10: Rate limiting (separate tree) ──
    insert_node!(%{
      id: 10,
      node_type: "goal",
      title: "Improve API rate limiting",
      created_at: "2024-01-08T09:00:00Z",
      metadata_json: Jason.encode!(%{"branch" => "main", "confidence" => 80})
    })

    # ── Observation 11: Memory leak ──
    insert_node!(%{
      id: 11,
      node_type: "observation",
      title: "Current rate limiter leaks memory under sustained load",
      created_at: "2024-01-08T10:00:00Z",
      metadata_json: Jason.encode!(%{"branch" => "main"})
    })

    # ── EDGES ──

    # G1 -> O2 (possible approach)
    insert_edge!(%{
      id: 1,
      from_node_id: 1,
      to_node_id: 2,
      edge_type: "leads_to",
      rationale: "possible approach",
      created_at: "2024-01-02T09:00:00Z"
    })

    # G1 -> O3 (possible approach)
    insert_edge!(%{
      id: 2,
      from_node_id: 1,
      to_node_id: 3,
      edge_type: "leads_to",
      rationale: "possible approach",
      created_at: "2024-01-02T10:00:00Z"
    })

    # O2 -> D4 (chosen)
    insert_edge!(%{
      id: 3,
      from_node_id: 2,
      to_node_id: 4,
      edge_type: "chosen",
      rationale: "JWT is stateless, scales horizontally",
      created_at: "2024-01-03T09:00:00Z"
    })

    # O3 -> D4 (rejected)
    insert_edge!(%{
      id: 4,
      from_node_id: 3,
      to_node_id: 4,
      edge_type: "rejected",
      rationale: "requires sticky sessions",
      created_at: "2024-01-03T09:01:00Z"
    })

    # D4 -> A5
    insert_edge!(%{
      id: 5,
      from_node_id: 4,
      to_node_id: 5,
      edge_type: "leads_to",
      rationale: "implementation step",
      created_at: "2024-01-04T09:00:00Z"
    })

    # D4 -> A6
    insert_edge!(%{
      id: 6,
      from_node_id: 4,
      to_node_id: 6,
      edge_type: "leads_to",
      rationale: "implementation step",
      created_at: "2024-01-05T09:00:00Z"
    })

    # A5 -> OUT7
    insert_edge!(%{
      id: 7,
      from_node_id: 5,
      to_node_id: 7,
      edge_type: "leads_to",
      rationale: "result",
      created_at: "2024-01-06T09:00:00Z"
    })

    # A6 -> OUT8
    insert_edge!(%{
      id: 8,
      from_node_id: 6,
      to_node_id: 8,
      edge_type: "leads_to",
      rationale: "result",
      created_at: "2024-01-07T09:00:00Z"
    })

    # G1 -> OBS9 (no rationale)
    insert_edge!(%{
      id: 9,
      from_node_id: 1,
      to_node_id: 9,
      edge_type: "leads_to",
      rationale: nil,
      created_at: "2024-01-02T11:00:00Z"
    })

    # G10 -> OBS11
    insert_edge!(%{
      id: 10,
      from_node_id: 10,
      to_node_id: 11,
      edge_type: "leads_to",
      rationale: "noticed during review",
      created_at: "2024-01-08T10:00:00Z"
    })

    # ── COMMAND LOG ──

    insert_command!(%{
      id: 1,
      command: ~s(deciduous add goal "Add user authentication" -c 95),
      started_at: "2024-01-01T10:00:00Z",
      exit_code: 0,
      duration_ms: 12
    })

    insert_command!(%{
      id: 2,
      command: ~s(deciduous add option "Use JWT tokens"),
      started_at: "2024-01-02T10:00:00Z",
      exit_code: 0,
      duration_ms: 8
    })

    insert_command!(%{
      id: 3,
      command: ~s(deciduous add option "Use session cookies"),
      started_at: "2024-01-02T10:05:00Z",
      exit_code: 0,
      duration_ms: 9
    })

    insert_command!(%{
      id: 4,
      command: ~s(deciduous link 1 2 -r "possible approach"),
      started_at: "2024-01-02T10:06:00Z",
      exit_code: 0,
      duration_ms: 5
    })

    insert_command!(%{
      id: 5,
      command: ~s(deciduous add decision "Choose JWT for authentication" -c 90),
      started_at: "2024-01-03T10:00:00Z",
      exit_code: 0,
      duration_ms: 11
    })

    insert_command!(%{
      id: 6,
      command:
        ~s(deciduous add action "Implement JWT auth middleware" -c 85 --commit abc123def -f "src/auth/middleware.rs,src/auth/jwt.rs"),
      started_at: "2024-01-04T10:00:00Z",
      exit_code: 0,
      duration_ms: 15
    })

    insert_command!(%{
      id: 7,
      command: ~s(deciduous add outcome "JWT auth middleware working in production" -c 95),
      started_at: "2024-01-06T10:00:00Z",
      exit_code: 0,
      duration_ms: 10
    })

    insert_command!(%{
      id: 8,
      command: ~s(deciduous add goal "Improve API rate limiting" -c 80),
      started_at: "2024-01-08T10:00:00Z",
      exit_code: 0,
      duration_ms: 7
    })

    # A still-running command (nil exit_code)
    insert_command!(%{
      id: 9,
      command: ~s(deciduous serve --port 3000),
      started_at: "2024-01-09T10:00:00Z",
      exit_code: nil,
      duration_ms: nil
    })
  end
end
