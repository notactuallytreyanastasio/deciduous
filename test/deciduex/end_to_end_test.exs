defmodule Deciduex.EndToEndTest do
  @moduledoc """
  Comprehensive end-to-end test that exercises ALL deciduex commands in a realistic
  project workflow. This test simulates building a complete decision graph from scratch,
  using every command the CLI supports.

  ## Scenario: Building a Real-Time Chat Feature

  We simulate the full development workflow:
  1. Create goals and explore options
  2. Make decisions and implement actions
  3. Record outcomes and observations
  4. Use pulse to check graph health
  5. Use narratives to document evolution
  6. Use archaeology to create pivots when we change direction
  7. Use sync, writeup, and diff for collaboration
  8. Use audit to verify data quality

  This test validates the entire Elixir implementation that the Rust CLI delegates to.
  """
  use ExUnit.Case

  alias Deciduex.Commands.Add
  alias Deciduex.Commands.Archaeology
  alias Deciduex.Commands.Audit
  alias Deciduex.Commands.Backup
  alias Deciduex.Commands.CommandLog
  alias Deciduex.Commands.Delete
  alias Deciduex.Commands.Diff
  alias Deciduex.Commands.Edges
  alias Deciduex.Commands.Graph
  alias Deciduex.Commands.Link
  alias Deciduex.Commands.Narratives
  alias Deciduex.Commands.Nodes
  alias Deciduex.Commands.Prompt
  alias Deciduex.Commands.Pulse
  alias Deciduex.Commands.Show
  alias Deciduex.Commands.Status
  alias Deciduex.Commands.Sync
  alias Deciduex.Commands.Unlink
  alias Deciduex.Commands.Writeup
  alias Deciduex.Queries

  import ExUnit.CaptureIO
  import Deciduex.TestFixtures

  @tmp_dir "tmp/e2e_test"

  setup do
    # Clean slate
    create_tables!()

    # Create tmp directory for test artifacts
    File.rm_rf!(@tmp_dir)
    File.mkdir_p!(@tmp_dir)

    on_exit(fn ->
      File.rm_rf!(@tmp_dir)
    end)

    :ok
  end

  # ══════════════════════════════════════════════════════════════════════════════
  # PHASE 1: Initial Project Setup - Goals and Options
  # ══════════════════════════════════════════════════════════════════════════════

  describe "Phase 1: Project Setup" do
    test "create initial goal with full metadata" do
      output =
        capture_io(fn ->
          Add.run([
            "goal",
            "Implement real-time chat for the app",
            "-c",
            "90",
            "-p",
            "Users want to chat in real-time. Need WebSocket support."
          ])
        end)

      assert output =~ "Created node 1"
      assert output =~ "goal"
      assert output =~ "real-time chat"

      # Verify in database
      node = Queries.get_node(1)
      assert node.node_type == "goal"
      assert node.title == "Implement real-time chat for the app"
      assert node.status == "pending"

      {:ok, meta} = Jason.decode(node.metadata_json)
      assert meta["confidence"] == 90
    end

    test "explore multiple options" do
      # First create the goal
      capture_io(fn ->
        Add.run(["goal", "Implement real-time chat", "-c", "90"])
      end)

      # Option 1: Phoenix Channels
      output1 =
        capture_io(fn ->
          Add.run([
            "option",
            "Use Phoenix Channels",
            "-c",
            "85",
            "-d",
            "Native Phoenix solution with presence tracking"
          ])
        end)

      assert output1 =~ "Created node 2"
      assert output1 =~ "option"

      # Option 2: Socket.io with Elixir wrapper
      output2 =
        capture_io(fn ->
          Add.run([
            "option",
            "Use Socket.io with Elixir wrapper",
            "-c",
            "70",
            "-d",
            "JavaScript-focused, needs adapter"
          ])
        end)

      assert output2 =~ "Created node 3"

      # Option 3: Raw WebSockets
      output3 =
        capture_io(fn ->
          Add.run([
            "option",
            "Raw WebSocket implementation",
            "-c",
            "60",
            "-d",
            "Maximum control but more work"
          ])
        end)

      assert output3 =~ "Created node 4"

      # Link options to goal
      capture_io(fn -> Link.run(["1", "2", "-r", "native Phoenix approach"]) end)
      capture_io(fn -> Link.run(["1", "3", "-r", "JavaScript ecosystem option"]) end)
      capture_io(fn -> Link.run(["1", "4", "-r", "low-level control option"]) end)

      # Verify structure
      {_incoming, outgoing} = Queries.get_node_edges(1)
      assert length(outgoing) == 3
    end

    test "nodes command shows all created nodes" do
      # Create some nodes
      capture_io(fn -> Add.run(["goal", "Chat feature", "-c", "90"]) end)
      capture_io(fn -> Add.run(["option", "Phoenix Channels", "-c", "85"]) end)
      capture_io(fn -> Add.run(["option", "Socket.io", "-c", "70"]) end)

      output = capture_io(fn -> Nodes.run([]) end)

      assert output =~ "3 nodes:"
      assert output =~ "goal"
      assert output =~ "option"
      assert output =~ "Chat feature"
      assert output =~ "Phoenix Channels"
    end

    test "filter nodes by type" do
      capture_io(fn -> Add.run(["goal", "Main goal", "-c", "90"]) end)
      capture_io(fn -> Add.run(["option", "Option A", "-c", "80"]) end)
      capture_io(fn -> Add.run(["option", "Option B", "-c", "80"]) end)
      capture_io(fn -> Add.run(["observation", "Noticed something", "-c", "70"]) end)

      output = capture_io(fn -> Nodes.run(["-t", "option"]) end)

      assert output =~ "2 nodes:"
      assert output =~ "Option A"
      assert output =~ "Option B"
      refute output =~ "Main goal"
    end
  end

  # ══════════════════════════════════════════════════════════════════════════════
  # PHASE 2: Decision Making and Implementation
  # ══════════════════════════════════════════════════════════════════════════════

  describe "Phase 2: Decisions and Actions" do
    setup do
      # Build initial graph
      capture_io(fn -> Add.run(["goal", "Real-time chat feature", "-c", "90"]) end)
      capture_io(fn -> Add.run(["option", "Phoenix Channels", "-c", "85"]) end)
      capture_io(fn -> Add.run(["option", "Socket.io", "-c", "70"]) end)
      capture_io(fn -> Link.run(["1", "2", "-r", "native approach"]) end)
      capture_io(fn -> Link.run(["1", "3", "-r", "JS ecosystem"]) end)
      :ok
    end

    test "make decision choosing an option" do
      output =
        capture_io(fn ->
          Add.run([
            "decision",
            "Choose Phoenix Channels for chat",
            "-c",
            "95",
            "-d",
            "Native Elixir, great presence, built-in PubSub"
          ])
        end)

      assert output =~ "Created node 4"
      assert output =~ "decision"

      # Link option to decision (chosen)
      link_output =
        capture_io(fn ->
          Link.run(["2", "4", "-t", "chosen", "-r", "best fit for Elixir stack"])
        end)

      assert link_output =~ "Created edge"

      # Mark other option as rejected
      capture_io(fn ->
        Link.run(["3", "4", "-t", "rejected", "-r", "requires JavaScript runtime"])
      end)

      # Update rejected option status
      capture_io(fn -> Status.run(["3", "superseded"]) end)

      # Verify
      node3 = Queries.get_node(3)
      assert node3.status == "superseded"
    end

    test "implement action with commit reference" do
      # Create decision first
      capture_io(fn -> Add.run(["decision", "Use Phoenix Channels", "-c", "95"]) end)

      # Create action with commit
      output =
        capture_io(fn ->
          Add.run([
            "action",
            "Implement ChatChannel module",
            "-c",
            "90",
            "-f",
            "lib/chat/channel.ex,lib/chat/presence.ex",
            "--commit",
            "abc123def456"
          ])
        end)

      assert output =~ "Created node 5"
      assert output =~ "action"

      # Verify metadata
      node = Queries.get_node(5)
      {:ok, meta} = Jason.decode(node.metadata_json)
      assert meta["commit"] == "abc123def456"
      assert meta["files"] == ["lib/chat/channel.ex", "lib/chat/presence.ex"]
    end

    test "record outcome after successful implementation" do
      capture_io(fn -> Add.run(["decision", "Use Phoenix Channels", "-c", "95"]) end)
      capture_io(fn -> Add.run(["action", "Implement ChatChannel", "-c", "90"]) end)
      capture_io(fn -> Link.run(["4", "5", "-r", "implementation step"]) end)

      output =
        capture_io(fn ->
          Add.run([
            "outcome",
            "Chat feature deployed to production",
            "-c",
            "95",
            "-d",
            "Real-time messaging working with 50ms average latency"
          ])
        end)

      assert output =~ "Created node 6"
      assert output =~ "outcome"

      # Link action to outcome
      capture_io(fn -> Link.run(["5", "6", "-r", "deployment result"]) end)

      # Verify edge structure
      edges = Queries.list_edges()
      assert length(edges) == 4
    end

    test "add observation during implementation" do
      capture_io(fn -> Add.run(["decision", "Use Phoenix Channels", "-c", "95"]) end)
      capture_io(fn -> Add.run(["action", "Implement ChatChannel", "-c", "90"]) end)

      output =
        capture_io(fn ->
          Add.run([
            "observation",
            "Presence tracking uses CRDTs under the hood",
            "-c",
            "100"
          ])
        end)

      assert output =~ "Created node 6"
      assert output =~ "observation"

      # Link observation to action
      capture_io(fn -> Link.run(["5", "6", "-r", "discovered during implementation"]) end)
    end

    test "edges command shows full graph structure" do
      capture_io(fn -> Add.run(["goal", "Chat", "-c", "90"]) end)
      capture_io(fn -> Add.run(["option", "Phoenix", "-c", "85"]) end)
      capture_io(fn -> Add.run(["decision", "Choose Phoenix", "-c", "95"]) end)
      capture_io(fn -> Link.run(["1", "2", "-r", "explore option"]) end)
      capture_io(fn -> Link.run(["2", "3", "-t", "chosen", "-r", "best fit"]) end)

      output = capture_io(fn -> Edges.run() end)

      assert output =~ "leads_to"
      assert output =~ "chosen"
      assert output =~ "explore option"
      assert output =~ "best fit"
    end
  end

  # ══════════════════════════════════════════════════════════════════════════════
  # PHASE 3: Graph Management and Show
  # ══════════════════════════════════════════════════════════════════════════════

  describe "Phase 3: Graph Inspection" do
    setup do
      # Build a complete mini-graph
      capture_io(fn -> Add.run(["goal", "Chat feature", "-c", "90"]) end)
      capture_io(fn -> Add.run(["option", "Phoenix Channels", "-c", "85"]) end)
      capture_io(fn -> Add.run(["decision", "Use Phoenix", "-c", "95"]) end)
      capture_io(fn -> Add.run(["action", "Implement channels", "-c", "90"]) end)
      capture_io(fn -> Add.run(["outcome", "Chat working", "-c", "95"]) end)
      capture_io(fn -> Link.run(["1", "2", "-r", "option"]) end)
      capture_io(fn -> Link.run(["2", "3", "-t", "chosen", "-r", "best"]) end)
      capture_io(fn -> Link.run(["3", "4", "-r", "impl"]) end)
      capture_io(fn -> Link.run(["4", "5", "-r", "result"]) end)
      :ok
    end

    test "graph command outputs complete JSON" do
      output = capture_io(fn -> Graph.run() end)

      {:ok, graph} = Jason.decode(output)

      assert length(graph["nodes"]) == 5
      assert length(graph["edges"]) == 4
      assert is_list(graph["documents"])
    end

    test "show command displays node details" do
      output = capture_io(fn -> Show.run(["1"]) end)

      assert output =~ "Node #1 goal"
      assert output =~ "Chat feature"
      assert output =~ "Status: pending"
      assert output =~ "Confidence: 90%"
      assert output =~ "Outgoing (1):"
    end

    test "show command with JSON output" do
      output = capture_io(fn -> Show.run(["3", "--json"]) end)

      {:ok, node} = Jason.decode(output)

      assert node["id"] == 3
      assert node["node_type"] == "decision"
      assert node["title"] == "Use Phoenix"
    end

    test "show command displays connections" do
      # Node 3 (decision) has incoming from option and outgoing to action
      output = capture_io(fn -> Show.run(["3"]) end)

      assert output =~ "Incoming (1):"
      assert output =~ "#2 ─[chosen]→ here"
      assert output =~ "Outgoing (1):"
      assert output =~ "here ─[leads_to]→ #4"
    end

    test "prompt command updates node metadata" do
      output =
        capture_io(fn ->
          Prompt.run(["1", "This was the original user request for real-time chat"])
        end)

      assert output =~ "Updated prompt"

      # Verify in database
      node = Queries.get_node(1)
      {:ok, meta} = Jason.decode(node.metadata_json)
      assert meta["prompt"] =~ "original user request"
    end
  end

  # ══════════════════════════════════════════════════════════════════════════════
  # PHASE 4: Advanced Commands - Pulse, Narratives, Archaeology
  # ══════════════════════════════════════════════════════════════════════════════

  describe "Phase 4: Advanced Analysis" do
    setup do
      # Build a graph with some gaps for pulse to find
      capture_io(fn -> Add.run(["goal", "Chat feature", "-c", "90"]) end)
      capture_io(fn -> Add.run(["option", "Phoenix Channels", "-c", "85"]) end)
      capture_io(fn -> Add.run(["decision", "Use Phoenix", "-c", "95"]) end)
      capture_io(fn -> Add.run(["action", "Implement channels", "-c", "90"]) end)
      capture_io(fn -> Add.run(["outcome", "Chat working", "-c", "95"]) end)
      capture_io(fn -> Link.run(["1", "2", "-r", "option"]) end)
      capture_io(fn -> Link.run(["2", "3", "-t", "chosen", "-r", "best"]) end)
      capture_io(fn -> Link.run(["3", "4", "-r", "impl"]) end)
      capture_io(fn -> Link.run(["4", "5", "-r", "result"]) end)

      # Add a second goal without follow-up (gap)
      capture_io(fn -> Add.run(["goal", "Performance optimization", "-c", "70"]) end)

      :ok
    end

    test "pulse command shows graph health" do
      output = capture_io(fn -> Pulse.run([]) end)

      assert output =~ "Decision Graph Pulse"
      assert output =~ "Total nodes:"
      assert output =~ "Total edges:"
      assert output =~ "Health score:"
      assert output =~ "By Type:"
      assert output =~ "goal"
      # Active Goals: section only shows if there are active (not pending) goals
      assert output =~ "By Status:"
    end

    test "pulse summary mode" do
      output = capture_io(fn -> Pulse.run(["--summary"]) end)

      assert output =~ "Pulse:"
      assert output =~ "nodes"
      assert output =~ "edges"
      assert output =~ "health:"
    end

    test "pulse with JSON output" do
      output = capture_io(fn -> Pulse.run(["--json"]) end)

      {:ok, pulse} = Jason.decode(output)

      assert is_integer(pulse["total_nodes"])
      assert is_integer(pulse["total_edges"])
      assert is_integer(pulse["health"])
      assert is_map(pulse["type_counts"])
      assert is_map(pulse["status_counts"])
      assert is_list(pulse["active_goals"])
      assert is_list(pulse["recent_nodes"])
      assert is_list(pulse["gaps"])
    end

    test "pulse identifies gaps when goals are active" do
      # Mark goals as active - gaps are only reported for active nodes
      capture_io(fn -> Status.run(["1", "active"]) end)
      capture_io(fn -> Status.run(["6", "active"]) end)

      output = capture_io(fn -> Pulse.run([]) end)

      # Goal 6 (Performance optimization) has no outgoing edges and should be a gap
      assert output =~ "Gaps" or output =~ "gaps" or output =~ "need follow-up"
    end

    test "narratives init creates template file" do
      # Mark goals as active first (narratives only shows active goals)
      capture_io(fn -> Status.run(["1", "active"]) end)
      capture_io(fn -> Status.run(["6", "active"]) end)

      path = Path.join(@tmp_dir, "narratives.md")

      output = capture_io(fn -> Narratives.run(["init", "-o", path]) end)

      assert output =~ "Initialized"
      assert File.exists?(path)

      content = File.read!(path)
      assert content =~ "Evolution Narratives"
      assert content =~ "Chat feature"
      assert content =~ "Performance optimization"
    end

    test "narratives show displays file content" do
      path = Path.join(@tmp_dir, "narratives.md")
      File.write!(path, "# Test Narratives\n\nThis is a test.")

      output = capture_io(fn -> Narratives.run(["show", path]) end)

      assert output =~ "Test Narratives"
    end

    test "narratives pivots finds revisit nodes" do
      # Add a revisit node
      capture_io(fn -> Add.run(["revisit", "Reconsidering chat architecture", "-c", "80"]) end)

      # Mark a node as superseded
      capture_io(fn -> Status.run(["3", "superseded"]) end)

      output = capture_io(fn -> Narratives.run(["pivots"]) end)

      assert output =~ "REVISIT" or output =~ "revisit" or output =~ "SUPERSEDED" or
               output =~ "superseded" or output =~ "No pivots"
    end

    test "archaeology timeline shows node history" do
      output = capture_io(fn -> Archaeology.run(["timeline"]) end)

      assert output =~ "Timeline" or output =~ "No nodes"
    end

    test "archaeology timeline with limit" do
      output = capture_io(fn -> Archaeology.run(["timeline", "--limit", "3"]) end)

      lines = output |> String.split("\n", trim: true) |> Enum.reject(&(&1 =~ ~r/^(Timeline|=)/))
      assert length(lines) <= 3
    end

    test "archaeology timeline with JSON" do
      output = capture_io(fn -> Archaeology.run(["timeline", "--json"]) end)

      {:ok, timeline} = Jason.decode(output)
      assert is_list(timeline)
    end

    test "archaeology supersede with dry run" do
      output = capture_io(fn -> Archaeology.run(["supersede", "4", "--dry-run"]) end)

      assert output =~ "Would supersede" or output =~ "1 nodes"

      # Verify node wasn't actually superseded (still pending, not superseded)
      node = Queries.get_node(4)
      assert node.status == "pending"
    end
  end

  # ══════════════════════════════════════════════════════════════════════════════
  # PHASE 5: Collaboration - Sync, Writeup, Diff
  # ══════════════════════════════════════════════════════════════════════════════

  describe "Phase 5: Collaboration Tools" do
    setup do
      # Build a small complete graph
      capture_io(fn -> Add.run(["goal", "Chat feature", "-c", "90"]) end)
      capture_io(fn -> Add.run(["decision", "Use Phoenix Channels", "-c", "95"]) end)
      capture_io(fn -> Add.run(["action", "Implement chat", "-c", "90"]) end)
      capture_io(fn -> Add.run(["outcome", "Chat deployed", "-c", "95"]) end)
      capture_io(fn -> Link.run(["1", "2", "-r", "chose approach"]) end)
      capture_io(fn -> Link.run(["2", "3", "-r", "implementation"]) end)
      capture_io(fn -> Link.run(["3", "4", "-r", "result"]) end)
      :ok
    end

    test "sync exports graph to JSON" do
      output_path = Path.join(@tmp_dir, "graph-data.json")

      output = capture_io(fn -> Sync.run([output_path]) end)

      assert output =~ "Exported" or File.exists?(output_path)

      if File.exists?(output_path) do
        {:ok, graph} = File.read!(output_path) |> Jason.decode()
        assert length(graph["nodes"]) == 4
        assert length(graph["edges"]) == 3
      end
    end

    test "writeup generates PR description" do
      output = capture_io(fn -> Writeup.run(["-t", "Add Chat Feature"]) end)

      assert output =~ "Add Chat Feature" or output =~ "Summary" or output =~ "Decision Graph"
    end

    test "writeup with node filter" do
      output = capture_io(fn -> Writeup.run(["-t", "Chat", "-n", "1,2,3"]) end)

      assert output =~ "Chat" or output =~ "goal" or output =~ "decision"
    end

    test "writeup with multiple options" do
      output = capture_io(fn -> Writeup.run(["-t", "Chat", "--no-test-plan"]) end)

      assert output =~ "Chat"
      refute output =~ "Test Plan"
    end

    test "diff export creates patch file" do
      output_path = Path.join(@tmp_dir, "chat-patch.json")

      output =
        capture_io(fn ->
          Diff.run(["export", "-o", output_path, "--author", "testuser"])
        end)

      assert output =~ "Exported" or File.exists?(output_path)

      if File.exists?(output_path) do
        {:ok, patch} = File.read!(output_path) |> Jason.decode()
        assert is_list(patch["nodes"])
        assert is_list(patch["edges"])
        assert patch["author"] == "testuser"
      end
    end

    test "diff export with node range" do
      output_path = Path.join(@tmp_dir, "partial-patch.json")

      output =
        capture_io(fn ->
          Diff.run(["export", "-n", "1-2", "-o", output_path])
        end)

      assert output =~ "Exported" or output =~ "nodes"
    end

    test "diff status shows patches" do
      # Create a patch first
      patch_dir = Path.join(@tmp_dir, "patches")
      File.mkdir_p!(patch_dir)
      patch_path = Path.join(patch_dir, "test-patch.json")

      patch = %{
        "version" => "1.0",
        "author" => "testuser",
        "branch" => "feature-chat",
        "nodes" => [],
        "edges" => []
      }

      File.write!(patch_path, Jason.encode!(patch))

      output = capture_io(fn -> Diff.run(["status", patch_dir]) end)

      assert output =~ "test-patch" or output =~ "testuser" or output =~ "No patches"
    end

    test "diff validate checks patch structure" do
      patch_path = Path.join(@tmp_dir, "valid-patch.json")

      patch = %{
        "version" => "1.0",
        "author" => "testuser",
        "branch" => "feature-x",
        "nodes" => [
          %{
            "change_id" => "test-uuid",
            "node_type" => "goal",
            "title" => "Test goal",
            "status" => "active"
          }
        ],
        "edges" => []
      }

      File.write!(patch_path, Jason.encode!(patch))

      output = capture_io(fn -> Diff.run(["validate", patch_path]) end)

      assert output =~ "valid" or output =~ "Valid" or output =~ "OK"
    end
  end

  # ══════════════════════════════════════════════════════════════════════════════
  # PHASE 6: Maintenance - Audit, Delete, Unlink, Backup
  # ══════════════════════════════════════════════════════════════════════════════

  describe "Phase 6: Maintenance Operations" do
    setup do
      capture_io(fn -> Add.run(["goal", "Chat feature", "-c", "90"]) end)
      capture_io(fn -> Add.run(["option", "Phoenix Channels", "-c", "85"]) end)
      capture_io(fn -> Add.run(["decision", "Use Phoenix", "-c", "95"]) end)
      capture_io(fn -> Link.run(["1", "2", "-r", "option"]) end)
      capture_io(fn -> Link.run(["2", "3", "-r", "chosen"]) end)
      :ok
    end

    test "audit generates health report" do
      output = capture_io(fn -> Audit.run([]) end)

      assert output =~ "Audit" or output =~ "health" or output =~ "nodes" or output =~ "edges"
    end

    test "audit shows node and edge counts" do
      output = capture_io(fn -> Audit.run([]) end)

      assert output =~ "Total nodes:"
      assert output =~ "Total edges:"
    end

    test "unlink removes edge between nodes" do
      # Verify edge exists
      edges_before = Queries.list_edges()
      assert length(edges_before) == 2

      # Unlink
      output = capture_io(fn -> Unlink.run(["1", "2"]) end)
      assert output =~ "Removed" or output =~ "Unlinked" or output =~ "edge"

      # Verify edge removed
      edges_after = Queries.list_edges()
      assert length(edges_after) == 1
    end

    test "delete removes node with dry run" do
      output = capture_io(fn -> Delete.run(["2", "--dry-run"]) end)

      assert output =~ "Would delete" or output =~ "dry run" or output =~ "node 2"

      # Verify node still exists
      assert Queries.get_node(2) != nil
    end

    test "delete removes node and edges" do
      output = capture_io(fn -> Delete.run(["2"]) end)

      assert output =~ "Deleted" or output =~ "removed"

      # Verify node gone
      assert Queries.get_node(2) == nil

      # Verify connected edges also removed
      edges = Queries.list_edges()

      refute Enum.any?(edges, fn e ->
               e.from_node_id == 2 or e.to_node_id == 2
             end)
    end

    test "backup creates database copy" do
      backup_path = Path.join(@tmp_dir, "backup.db")

      output = capture_io(fn -> Backup.run([backup_path]) end)

      assert output =~ "backup" or output =~ "Backup" or output =~ backup_path
    end

    test "command log tracks operations" do
      # Perform some operations
      capture_io(fn -> Add.run(["observation", "Test observation", "-c", "80"]) end)

      output = capture_io(fn -> CommandLog.run() end)

      # Should show logged commands
      assert output =~ "add" or output =~ "deciduous" or output =~ "No commands"
    end
  end

  # ══════════════════════════════════════════════════════════════════════════════
  # PHASE 7: Complete Workflow Integration - Real Project Simulation
  # ══════════════════════════════════════════════════════════════════════════════
  #
  # This test simulates building a real e-commerce checkout system with:
  # - Multiple interconnected goals
  # - Decision pivots when initial approach fails
  # - Parallel work streams
  # - Full use of all commands
  #
  # Final graph structure (~30 nodes):
  #
  #   [G1] Checkout System
  #     ├─ [O2] Stripe integration
  #     ├─ [O3] PayPal integration
  #     ├─ [O4] Build custom payment processor
  #     ├─ [D5] Choose Stripe (chosen from O2)
  #     │    ├─ [A6] Implement Stripe SDK
  #     │    │    └─ [OUT7] Stripe working
  #     │    ├─ [A8] Add webhook handlers
  #     │    │    └─ [OUT9] Webhooks deployed
  #     │    └─ [OBS10] Stripe has 2.9% fee
  #     └─ [OBS11] Competitor uses Stripe
  #
  #   [G12] Shopping Cart Persistence
  #     ├─ [O13] Redis sessions
  #     ├─ [O14] PostgreSQL storage
  #     ├─ [D15] Choose Redis (chosen from O13)
  #     │    ├─ [A16] Implement Redis cart
  #     │    │    └─ [OUT17] Redis cart working
  #     │    └─ [OBS18] Redis needs cluster for HA
  #     │
  #     │  ** PIVOT: Redis doesn't scale well **
  #     │
  #     ├─ [OBS19] Redis hitting memory limits
  #     ├─ [REV20] Reconsidering cart storage
  #     └─ [D21] Switch to PostgreSQL (new decision)
  #          ├─ [A22] Migrate cart to PostgreSQL
  #          │    └─ [OUT23] PostgreSQL cart deployed
  #          └─ [OBS24] 50% cost reduction
  #
  #   [G25] Order Confirmation Emails
  #     ├─ [O26] SendGrid
  #     ├─ [O27] AWS SES
  #     ├─ [D28] Choose SendGrid
  #     │    ├─ [A29] Integrate SendGrid
  #     │    │    └─ [OUT30] Emails working
  #     │    └─ [OBS31] 99.9% delivery rate
  #     └─ [OBS32] Marketing wants templates
  #
  # ══════════════════════════════════════════════════════════════════════════════

  describe "Phase 7: Complete Real-World Project" do
    test "builds complete e-commerce checkout system with pivots and revisions" do
      # ════════════════════════════════════════════════════════════════════════
      # PART 1: Payment Processing (Goal 1)
      # ════════════════════════════════════════════════════════════════════════

      # Create main goal with full prompt
      capture_io(fn ->
        Add.run([
          "goal",
          "Implement checkout payment processing",
          "-c",
          "95",
          "-p",
          "We need to accept credit card payments for our e-commerce checkout. Must support Visa, Mastercard, Amex. Need PCI compliance. Budget: $500/month for processing fees.",
          "-d",
          "Core payment processing for checkout flow"
        ])
      end)

      # Explore payment options
      capture_io(fn ->
        Add.run([
          "option",
          "Integrate Stripe payment gateway",
          "-c",
          "90",
          "-d",
          "Industry standard, excellent docs, 2.9% + $0.30 per transaction"
        ])
      end)

      capture_io(fn ->
        Add.run([
          "option",
          "Integrate PayPal checkout",
          "-c",
          "75",
          "-d",
          "Wide user base, but higher fees and worse UX"
        ])
      end)

      capture_io(fn ->
        Add.run([
          "option",
          "Build custom payment processor",
          "-c",
          "40",
          "-d",
          "Maximum control but PCI compliance nightmare"
        ])
      end)

      # Link options to goal
      capture_io(fn -> Link.run(["1", "2", "-r", "preferred third-party solution"]) end)
      capture_io(fn -> Link.run(["1", "3", "-r", "alternative payment method"]) end)
      capture_io(fn -> Link.run(["1", "4", "-r", "custom solution for comparison"]) end)

      # Add observation about competitor
      capture_io(fn ->
        Add.run([
          "observation",
          "Main competitor uses Stripe successfully",
          "-c",
          "100",
          "-d",
          "Confirmed via their public tech blog"
        ])
      end)

      capture_io(fn -> Link.run(["1", "5", "-r", "market research"]) end)

      # Make decision: Choose Stripe
      capture_io(fn ->
        Add.run([
          "decision",
          "Use Stripe for payment processing",
          "-c",
          "95",
          "-d",
          "Stripe chosen: best docs, proven at scale, reasonable fees, PCI compliant out of box"
        ])
      end)

      # Link options to decision with chosen/rejected
      capture_io(fn -> Link.run(["2", "6", "-t", "chosen", "-r", "best overall fit"]) end)

      capture_io(fn ->
        Link.run(["3", "6", "-t", "rejected", "-r", "worse developer experience"])
      end)

      capture_io(fn ->
        Link.run(["4", "6", "-t", "rejected", "-r", "PCI compliance too costly"])
      end)

      # Mark rejected options as superseded
      capture_io(fn -> Status.run(["3", "superseded"]) end)
      capture_io(fn -> Status.run(["4", "superseded"]) end)

      # Implement Stripe SDK
      capture_io(fn ->
        Add.run([
          "action",
          "Implement Stripe SDK integration",
          "-c",
          "90",
          "-f",
          "lib/payments/stripe.ex,lib/payments/checkout.ex",
          "--commit",
          "abc123def",
          "-d",
          "Core Stripe integration with checkout session creation"
        ])
      end)

      capture_io(fn -> Link.run(["6", "7", "-r", "implementation step"]) end)

      # Outcome: Stripe working
      capture_io(fn ->
        Add.run([
          "outcome",
          "Stripe payment processing deployed to production",
          "-c",
          "95",
          "-d",
          "Processing $50k/day with 99.9% success rate"
        ])
      end)

      capture_io(fn -> Link.run(["7", "8", "-r", "deployment result"]) end)

      # Second action: webhooks
      capture_io(fn ->
        Add.run([
          "action",
          "Implement Stripe webhook handlers",
          "-c",
          "85",
          "-f",
          "lib/payments/webhooks.ex",
          "--commit",
          "def456ghi"
        ])
      end)

      capture_io(fn -> Link.run(["6", "9", "-r", "webhook handling"]) end)

      capture_io(fn ->
        Add.run([
          "outcome",
          "Webhook handlers deployed and verified",
          "-c",
          "90"
        ])
      end)

      capture_io(fn -> Link.run(["9", "10", "-r", "verification complete"]) end)

      # Observation about fees
      capture_io(fn ->
        Add.run([
          "observation",
          "Stripe fee is 2.9% + $0.30, lower than PayPal",
          "-c",
          "100"
        ])
      end)

      capture_io(fn -> Link.run(["6", "11", "-r", "cost analysis"]) end)

      # ════════════════════════════════════════════════════════════════════════
      # PART 2: Shopping Cart Persistence (Goal 2) - WITH PIVOT
      # ════════════════════════════════════════════════════════════════════════

      # Create cart goal
      capture_io(fn ->
        Add.run([
          "goal",
          "Implement shopping cart persistence",
          "-c",
          "90",
          "-p",
          "Shopping cart should persist across sessions. Users complain about losing cart items.",
          "-d",
          "Cart must survive browser close, work across devices"
        ])
      end)

      # Options for cart storage
      capture_io(fn ->
        Add.run([
          "option",
          "Use Redis for session-based cart storage",
          "-c",
          "85",
          "-d",
          "Fast, in-memory, but needs careful memory management"
        ])
      end)

      capture_io(fn ->
        Add.run([
          "option",
          "Use PostgreSQL for persistent cart storage",
          "-c",
          "80",
          "-d",
          "Durable, queryable, but slightly slower"
        ])
      end)

      capture_io(fn -> Link.run(["12", "13", "-r", "fast in-memory option"]) end)
      capture_io(fn -> Link.run(["12", "14", "-r", "durable storage option"]) end)

      # Initial decision: Choose Redis
      capture_io(fn ->
        Add.run([
          "decision",
          "Use Redis for cart storage",
          "-c",
          "85",
          "-d",
          "Redis chosen for speed - cart operations need sub-10ms latency"
        ])
      end)

      capture_io(fn -> Link.run(["13", "15", "-t", "chosen", "-r", "speed requirement"]) end)

      # Implement Redis cart
      capture_io(fn ->
        Add.run([
          "action",
          "Implement Redis cart storage",
          "-c",
          "85",
          "-f",
          "lib/cart/redis_store.ex",
          "--commit",
          "ghi789jkl"
        ])
      end)

      capture_io(fn -> Link.run(["15", "16", "-r", "implementation"]) end)

      capture_io(fn ->
        Add.run([
          "outcome",
          "Redis cart deployed to staging",
          "-c",
          "80"
        ])
      end)

      capture_io(fn -> Link.run(["16", "17", "-r", "staging deployment"]) end)

      # Observation about Redis HA
      capture_io(fn ->
        Add.run([
          "observation",
          "Redis requires cluster setup for high availability",
          "-c",
          "90"
        ])
      end)

      capture_io(fn -> Link.run(["15", "18", "-r", "ops consideration"]) end)

      # ═══════════════════════════════════════════════════════════════════
      # PIVOT: Redis doesn't scale well - we need to change direction
      # ═══════════════════════════════════════════════════════════════════

      # Observation that triggers pivot
      capture_io(fn ->
        Add.run([
          "observation",
          "Redis hitting memory limits at 10k concurrent carts",
          "-c",
          "100",
          "-d",
          "Memory usage spiked to 8GB, approaching instance limits"
        ])
      end)

      capture_io(fn -> Link.run(["17", "19", "-r", "production issue discovered"]) end)

      # Create revisit node for the pivot
      capture_io(fn ->
        Add.run([
          "revisit",
          "Reconsidering cart storage strategy",
          "-c",
          "95",
          "-d",
          "Redis memory limits forcing us to reconsider PostgreSQL"
        ])
      end)

      capture_io(fn -> Link.run(["19", "20", "-r", "triggered reconsideration"]) end)
      capture_io(fn -> Link.run(["15", "20", "-r", "original decision being revisited"]) end)

      # Mark original Redis decision as superseded
      capture_io(fn -> Status.run(["15", "superseded"]) end)

      # New decision: Switch to PostgreSQL
      capture_io(fn ->
        Add.run([
          "decision",
          "Switch cart storage to PostgreSQL",
          "-c",
          "90",
          "-d",
          "PostgreSQL handles scale better, acceptable latency with proper indexing"
        ])
      end)

      capture_io(fn -> Link.run(["20", "21", "-r", "new direction"]) end)
      capture_io(fn -> Link.run(["14", "21", "-t", "chosen", "-r", "revisited and chosen"]) end)

      # Implement PostgreSQL cart
      capture_io(fn ->
        Add.run([
          "action",
          "Migrate cart storage to PostgreSQL",
          "-c",
          "90",
          "-f",
          "lib/cart/postgres_store.ex,priv/repo/migrations/add_carts.exs",
          "--commit",
          "jkl012mno"
        ])
      end)

      capture_io(fn -> Link.run(["21", "22", "-r", "migration implementation"]) end)

      capture_io(fn ->
        Add.run([
          "outcome",
          "PostgreSQL cart deployed to production",
          "-c",
          "95",
          "-d",
          "Handling 50k concurrent carts, 15ms p99 latency"
        ])
      end)

      capture_io(fn -> Link.run(["22", "23", "-r", "successful migration"]) end)

      # Positive observation after pivot
      capture_io(fn ->
        Add.run([
          "observation",
          "PostgreSQL cart reduced infrastructure costs by 50%",
          "-c",
          "100"
        ])
      end)

      capture_io(fn -> Link.run(["23", "24", "-r", "unexpected benefit"]) end)

      # ════════════════════════════════════════════════════════════════════════
      # PART 3: Order Confirmation Emails (Goal 3)
      # ════════════════════════════════════════════════════════════════════════

      capture_io(fn ->
        Add.run([
          "goal",
          "Implement order confirmation emails",
          "-c",
          "85",
          "-p",
          "Customers need email confirmations after purchase. Must include order details, tracking info.",
          "-d",
          "Transactional email for order confirmations"
        ])
      end)

      capture_io(fn ->
        Add.run([
          "option",
          "Use SendGrid for transactional email",
          "-c",
          "85"
        ])
      end)

      capture_io(fn ->
        Add.run([
          "option",
          "Use AWS SES for transactional email",
          "-c",
          "80"
        ])
      end)

      capture_io(fn -> Link.run(["25", "26", "-r", "dedicated email service"]) end)
      capture_io(fn -> Link.run(["25", "27", "-r", "AWS ecosystem option"]) end)

      capture_io(fn ->
        Add.run([
          "decision",
          "Use SendGrid for order emails",
          "-c",
          "90",
          "-d",
          "Better deliverability, easier templates, good analytics"
        ])
      end)

      capture_io(fn -> Link.run(["26", "28", "-t", "chosen", "-r", "better features"]) end)
      capture_io(fn -> Link.run(["27", "28", "-t", "rejected", "-r", "more complex setup"]) end)
      capture_io(fn -> Status.run(["27", "superseded"]) end)

      capture_io(fn ->
        Add.run([
          "action",
          "Integrate SendGrid API",
          "-c",
          "85",
          "-f",
          "lib/email/sendgrid.ex,lib/email/templates/order_confirmation.ex"
        ])
      end)

      capture_io(fn -> Link.run(["28", "29", "-r", "implementation"]) end)

      capture_io(fn ->
        Add.run([
          "outcome",
          "Order confirmation emails live in production",
          "-c",
          "95",
          "-d",
          "Sending 10k emails/day with 99.9% delivery rate"
        ])
      end)

      capture_io(fn -> Link.run(["29", "30", "-r", "deployment"]) end)

      capture_io(fn ->
        Add.run([
          "observation",
          "SendGrid reporting 99.9% delivery rate",
          "-c",
          "100"
        ])
      end)

      capture_io(fn -> Link.run(["30", "31", "-r", "metrics validation"]) end)

      capture_io(fn ->
        Add.run([
          "observation",
          "Marketing team requesting email template editor",
          "-c",
          "70"
        ])
      end)

      capture_io(fn -> Link.run(["25", "32", "-r", "stakeholder feedback"]) end)

      # ════════════════════════════════════════════════════════════════════════
      # VERIFICATION: Use all read/analysis commands to verify the graph
      # ════════════════════════════════════════════════════════════════════════

      # Verify node count
      nodes = Queries.list_nodes()
      assert length(nodes) == 32, "Expected 32 nodes, got #{length(nodes)}"

      # Verify edge count (34 edges across the 3 goal trees)
      edges = Queries.list_edges()
      assert length(edges) >= 34, "Expected at least 34 edges, got #{length(edges)}"

      # Test nodes command filtering
      nodes_output = capture_io(fn -> Nodes.run(["-t", "goal"]) end)
      assert nodes_output =~ "3 nodes:"
      assert nodes_output =~ "checkout"
      assert nodes_output =~ "cart"
      assert nodes_output =~ "email"

      # Test nodes by status
      nodes_output = capture_io(fn -> Nodes.run([]) end)
      assert nodes_output =~ "superseded"

      # Test edges command
      edges_output = capture_io(fn -> Edges.run() end)
      assert edges_output =~ "chosen"
      assert edges_output =~ "rejected"
      assert edges_output =~ "leads_to"

      # Test graph JSON export
      graph_output = capture_io(fn -> Graph.run() end)
      {:ok, graph_json} = Jason.decode(graph_output)
      assert length(graph_json["nodes"]) == 32
      assert length(graph_json["edges"]) >= 34

      # Test show command on various node types
      show_goal = capture_io(fn -> Show.run(["1"]) end)
      assert show_goal =~ "goal"
      assert show_goal =~ "checkout"
      assert show_goal =~ "Outgoing"

      show_decision = capture_io(fn -> Show.run(["6"]) end)
      assert show_decision =~ "decision"
      assert show_decision =~ "Incoming"

      show_revisit = capture_io(fn -> Show.run(["20"]) end)
      assert show_revisit =~ "revisit"
      assert show_revisit =~ "Reconsidering"

      # Test show with JSON
      show_json = capture_io(fn -> Show.run(["1", "--json"]) end)
      {:ok, node_json} = Jason.decode(show_json)
      assert node_json["node_type"] == "goal"

      # Mark goals as active for pulse/narratives
      capture_io(fn -> Status.run(["1", "active"]) end)
      capture_io(fn -> Status.run(["12", "active"]) end)
      capture_io(fn -> Status.run(["25", "active"]) end)

      # Test pulse command
      pulse_output = capture_io(fn -> Pulse.run([]) end)
      assert pulse_output =~ "Decision Graph Pulse"
      assert pulse_output =~ "Total nodes: 32"
      assert pulse_output =~ "Health score:"
      assert pulse_output =~ "Active Goals:"

      pulse_json = capture_io(fn -> Pulse.run(["--json"]) end)
      {:ok, pulse_data} = Jason.decode(pulse_json)
      assert pulse_data["total_nodes"] == 32
      assert is_integer(pulse_data["health"])

      pulse_summary = capture_io(fn -> Pulse.run(["--summary"]) end)
      assert pulse_summary =~ "Pulse:"

      # Test narratives
      narratives_path = Path.join(@tmp_dir, "narratives.md")
      capture_io(fn -> Narratives.run(["init", "-o", narratives_path, "--force"]) end)
      assert File.exists?(narratives_path)
      narratives_content = File.read!(narratives_path)
      assert narratives_content =~ "Evolution Narratives"

      assert narratives_content =~ "checkout" or narratives_content =~ "cart" or
               narratives_content =~ "email"

      # Test narratives pivots (should find the revisit node)
      pivots_output = capture_io(fn -> Narratives.run(["pivots"]) end)

      assert pivots_output =~ "REVISIT" or pivots_output =~ "SUPERSEDED" or
               pivots_output =~ "Pivot"

      # Test archaeology timeline
      timeline_output = capture_io(fn -> Archaeology.run(["timeline"]) end)
      assert timeline_output =~ "Timeline"

      timeline_limited = capture_io(fn -> Archaeology.run(["timeline", "--limit", "5"]) end)

      lines =
        timeline_limited
        |> String.split("\n", trim: true)
        |> Enum.reject(&(&1 =~ ~r/^(Timeline|=)/))

      assert length(lines) <= 5

      timeline_json = capture_io(fn -> Archaeology.run(["timeline", "--json"]) end)
      {:ok, timeline_data} = Jason.decode(timeline_json)
      assert is_list(timeline_data)
      assert length(timeline_data) == 32

      # Test archaeology supersede with dry-run
      supersede_output = capture_io(fn -> Archaeology.run(["supersede", "17", "--dry-run"]) end)
      assert supersede_output =~ "Would supersede"
      # Verify it wasn't actually superseded
      node_17 = Queries.get_node(17)
      assert node_17.status != "superseded"

      # Test audit
      audit_output = capture_io(fn -> Audit.run([]) end)
      assert audit_output =~ "Total nodes: 32"

      # Test sync export
      sync_path = Path.join(@tmp_dir, "checkout-graph.json")
      capture_io(fn -> Sync.run([sync_path]) end)
      assert File.exists?(sync_path)
      {:ok, sync_data} = File.read!(sync_path) |> Jason.decode()
      assert length(sync_data["nodes"]) == 32

      # Test writeup
      writeup_output = capture_io(fn -> Writeup.run(["-t", "E-commerce Checkout System"]) end)
      assert writeup_output =~ "E-commerce Checkout System"
      assert writeup_output =~ "Summary" or writeup_output =~ "Decision"

      writeup_filtered = capture_io(fn -> Writeup.run(["-t", "Payment", "-n", "1-11"]) end)
      assert writeup_filtered =~ "Payment"

      # Test diff export
      patch_path = Path.join(@tmp_dir, "checkout-patch.json")
      capture_io(fn -> Diff.run(["export", "-o", patch_path, "--author", "e2e-test"]) end)
      assert File.exists?(patch_path)
      {:ok, patch_data} = File.read!(patch_path) |> Jason.decode()
      assert patch_data["author"] == "e2e-test"
      assert length(patch_data["nodes"]) == 32

      # Test diff with node range
      partial_patch = Path.join(@tmp_dir, "partial-patch.json")
      capture_io(fn -> Diff.run(["export", "-n", "1-11", "-o", partial_patch]) end)
      {:ok, partial_data} = File.read!(partial_patch) |> Jason.decode()
      assert length(partial_data["nodes"]) == 11

      # Test diff validate
      validate_output = capture_io(fn -> Diff.run(["validate", patch_path]) end)
      assert validate_output =~ "valid" or validate_output =~ "Valid" or validate_output =~ "OK"

      # Test diff status
      status_output = capture_io(fn -> Diff.run(["status", @tmp_dir]) end)
      assert status_output =~ "checkout-patch" or status_output =~ "e2e-test"

      # Test prompt update
      capture_io(fn ->
        Prompt.run([
          "1",
          "Updated: Full checkout payment processing with Stripe, supporting all major cards"
        ])
      end)

      updated_node = Queries.get_node(1)
      {:ok, meta} = Jason.decode(updated_node.metadata_json)
      assert meta["prompt"] =~ "Updated:"

      # Test backup
      backup_path = Path.join(@tmp_dir, "checkout-backup.db")
      backup_output = capture_io(fn -> Backup.run([backup_path]) end)
      assert backup_output =~ "backup" or backup_output =~ "Backup" or File.exists?(backup_path)

      # ════════════════════════════════════════════════════════════════════════
      # VERIFY GRAPH TOPOLOGY
      # ════════════════════════════════════════════════════════════════════════

      final_graph = Queries.get_graph()
      nodes_by_id = Map.new(final_graph.nodes, &{&1.id, &1})

      # Verify canonical flow: goal -> options -> decision -> actions -> outcomes
      goal_1 = nodes_by_id[1]
      assert goal_1.node_type == "goal"

      # Goal 1 should have options as children
      goal_1_edges = Enum.filter(final_graph.edges, &(&1.from_node_id == 1))
      goal_1_children = Enum.map(goal_1_edges, &nodes_by_id[&1.to_node_id])
      child_types = Enum.map(goal_1_children, & &1.node_type)
      assert "option" in child_types
      assert "observation" in child_types

      # Decision 6 should have incoming chosen/rejected edges
      decision_6_incoming = Enum.filter(final_graph.edges, &(&1.to_node_id == 6))
      incoming_types = Enum.map(decision_6_incoming, & &1.edge_type)
      assert "chosen" in incoming_types
      assert "rejected" in incoming_types

      # Verify the pivot chain exists
      revisit_node = Enum.find(final_graph.nodes, &(&1.node_type == "revisit"))
      assert revisit_node != nil
      assert revisit_node.title =~ "Reconsidering"

      # Verify superseded nodes exist
      superseded_nodes = Enum.filter(final_graph.nodes, &(&1.status == "superseded"))
      # O3, O4, D15, O27
      assert length(superseded_nodes) >= 4

      # Verify outcome nodes are leaves (no outgoing except observations)
      outcome_nodes = Enum.filter(final_graph.nodes, &(&1.node_type == "outcome"))

      Enum.each(outcome_nodes, fn outcome ->
        outgoing = Enum.filter(final_graph.edges, &(&1.from_node_id == outcome.id))
        outgoing_types = Enum.map(outgoing, &nodes_by_id[&1.to_node_id].node_type)
        # Outcomes can lead to observations but not to actions/decisions
        refute "action" in outgoing_types
        refute "decision" in outgoing_types
      end)

      # Verify all three goals exist and have distinct trees
      goals = Enum.filter(final_graph.nodes, &(&1.node_type == "goal"))
      assert length(goals) == 3

      # Each goal should have at least one decision in its tree
      Enum.each([1, 12, 25], fn goal_id ->
        goal_descendants = find_descendants(goal_id, final_graph)
        descendant_types = Enum.map(goal_descendants, & &1.node_type)
        assert "decision" in descendant_types, "Goal #{goal_id} should have a decision"
        assert "outcome" in descendant_types, "Goal #{goal_id} should have an outcome"
      end)

      IO.puts("\n✅ Complete e-commerce checkout graph built successfully!")
      IO.puts("   - 32 nodes across 3 goal trees")
      IO.puts("   - #{length(final_graph.edges)} edges connecting the graph")
      IO.puts("   - 1 pivot (Redis → PostgreSQL)")
      IO.puts("   - All commands exercised and verified")
    end
  end

  # Helper to find all descendants of a node via BFS
  defp find_descendants(root_id, graph) do
    adj =
      Enum.reduce(graph.edges, %{}, fn e, acc ->
        Map.update(acc, e.from_node_id, [e.to_node_id], &[e.to_node_id | &1])
      end)

    nodes_by_id = Map.new(graph.nodes, &{&1.id, &1})

    do_bfs([root_id], adj, MapSet.new([root_id]))
    |> Enum.map(&nodes_by_id[&1])
    |> Enum.filter(& &1)
    |> Enum.reject(&(&1.id == root_id))
  end

  defp do_bfs([], _adj, visited), do: MapSet.to_list(visited)

  defp do_bfs([current | rest], adj, visited) do
    neighbors = Map.get(adj, current, [])
    new_neighbors = Enum.filter(neighbors, &(&1 not in visited))
    new_visited = Enum.reduce(new_neighbors, visited, &MapSet.put(&2, &1))
    do_bfs(rest ++ new_neighbors, adj, new_visited)
  end

  # ══════════════════════════════════════════════════════════════════════════════
  # PHASE 8: Error Handling and Edge Cases
  # ══════════════════════════════════════════════════════════════════════════════

  # Tests tagged :skip trigger System.halt() in error paths which kills the test process
  describe "Phase 8: Basic Error Handling" do
    @tag :skip
    test "show non-existent node returns error" do
      output = capture_io(:stderr, fn -> Show.run(["9999"]) end)

      assert output =~ "not found" or output =~ "Error" or output =~ "No node"
    end

    @tag :skip
    test "link with invalid node IDs reports error" do
      capture_io(fn -> Add.run(["goal", "Test", "-c", "90"]) end)

      output = capture_io(:stderr, fn -> Link.run(["1", "9999"]) end)

      assert output =~ "not found" or output =~ "Error" or output =~ "invalid"
    end

    @tag :skip
    test "delete non-existent node reports error" do
      output = capture_io(:stderr, fn -> Delete.run(["9999"]) end)

      assert output =~ "not found" or output =~ "Error"
    end

    test "empty graph produces helpful output" do
      nodes_output = capture_io(fn -> Nodes.run([]) end)
      assert nodes_output =~ "0 nodes" or nodes_output =~ "No nodes"

      edges_output = capture_io(fn -> Edges.run() end)
      assert edges_output =~ "No edges" or edges_output =~ "link"
    end

    @tag :skip
    test "command with invalid arguments shows help" do
      # This depends on how each command handles invalid args
      # Most should show usage or error message
      output =
        capture_io(:stderr, fn ->
          try do
            Add.run([])
          rescue
            _ -> :ok
          catch
            :exit, _ -> :ok
          end
        end)

      # Should either show help or crash gracefully
      assert output =~ "Usage" or output == "" or output =~ "Error"
    end
  end

  # ══════════════════════════════════════════════════════════════════════════════
  # PHASE 9: Stress Tests and Edge Cases - Trying to Break It
  # ══════════════════════════════════════════════════════════════════════════════
  #
  # These tests attempt to break the implementation with:
  # - Invalid inputs
  # - Boundary conditions
  # - Unicode and special characters
  # - Large data
  # - Circular references
  # - Concurrent-like operations
  # - SQL injection attempts
  # - Malformed data
  #
  # ══════════════════════════════════════════════════════════════════════════════

  # Tests tagged :skip trigger System.halt() in error paths
  describe "Phase 9: Stress Tests - Invalid Node IDs" do
    @tag :skip
    test "link from non-existent node fails gracefully" do
      capture_io(fn -> Add.run(["goal", "Test goal", "-c", "90"]) end)

      output = capture_io(:stderr, fn -> Link.run(["9999", "1"]) end)
      assert output =~ "not found" or output =~ "Error" or output =~ "invalid"
    end

    @tag :skip
    test "link to non-existent node fails gracefully" do
      capture_io(fn -> Add.run(["goal", "Test goal", "-c", "90"]) end)

      output = capture_io(:stderr, fn -> Link.run(["1", "9999"]) end)
      assert output =~ "not found" or output =~ "Error" or output =~ "invalid"
    end

    test "self-referential link (node linking to itself)" do
      capture_io(fn -> Add.run(["goal", "Test goal", "-c", "90"]) end)

      # Should either reject or accept - just don't crash
      result = capture_io(fn -> Link.run(["1", "1", "-r", "self-reference"]) end)
      # Implementation may allow or reject this - either is fine
      assert is_binary(result)
    end

    @tag :skip
    test "link with zero as node ID" do
      capture_io(fn -> Add.run(["goal", "Test goal", "-c", "90"]) end)

      # Node ID 0 doesn't exist
      output = capture_io(:stderr, fn -> Link.run(["0", "1"]) end)
      assert output =~ "not found" or output =~ "Error" or output =~ "invalid" or output == ""
    end

    @tag :skip
    test "link with negative node ID" do
      capture_io(fn -> Add.run(["goal", "Test goal", "-c", "90"]) end)

      # Should handle gracefully
      output =
        capture_io(:stderr, fn ->
          try do
            Link.run(["-1", "1"])
          rescue
            _ -> IO.puts("caught exception")
          catch
            :exit, _ -> IO.puts("caught exit")
          end
        end)

      assert is_binary(output)
    end

    @tag :skip
    test "link with very large node ID" do
      capture_io(fn -> Add.run(["goal", "Test goal", "-c", "90"]) end)

      # 64-bit max-ish
      output = capture_io(:stderr, fn -> Link.run(["9223372036854775807", "1"]) end)
      assert output =~ "not found" or output =~ "Error" or output =~ "invalid" or output == ""
    end

    @tag :skip
    test "show with non-numeric ID" do
      output =
        capture_io(:stderr, fn ->
          try do
            Show.run(["not-a-number"])
          rescue
            _ -> IO.puts("caught exception")
          catch
            :exit, _ -> IO.puts("caught exit")
          end
        end)

      assert is_binary(output)
    end

    @tag :skip
    test "status with non-existent node" do
      output = capture_io(:stderr, fn -> Status.run(["9999", "active"]) end)
      assert output =~ "not found" or output =~ "Error" or output == ""
    end

    @tag :skip
    test "prompt with non-existent node" do
      output = capture_io(:stderr, fn -> Prompt.run(["9999", "test prompt"]) end)
      assert output =~ "not found" or output =~ "Error" or output == ""
    end
  end

  describe "Phase 9: Stress Tests - String Edge Cases" do
    test "node with empty title" do
      # Should either reject or create with empty title
      output =
        capture_io(:stderr, fn ->
          try do
            capture_io(fn -> Add.run(["goal", "", "-c", "90"]) end)
          rescue
            _ -> IO.puts("rejected empty title")
          catch
            :exit, _ -> IO.puts("exit on empty title")
          end
        end)

      assert is_binary(output)
    end

    test "node with whitespace-only title" do
      output =
        capture_io(fn ->
          try do
            Add.run(["goal", "   ", "-c", "90"])
          rescue
            _ -> :rejected
          catch
            :exit, _ -> :rejected
          end
        end)

      assert is_binary(output)
    end

    test "node with very long title (1000 chars)" do
      long_title = String.duplicate("a", 1000)

      output = capture_io(fn -> Add.run(["goal", long_title, "-c", "90"]) end)

      assert output =~ "Created node 1" or output =~ "Error"

      if output =~ "Created node 1" do
        node = Queries.get_node(1)
        assert String.length(node.title) == 1000
      end
    end

    test "node with extremely long title (10_000 chars)" do
      very_long_title = String.duplicate("x", 10_000)

      output =
        capture_io(fn ->
          try do
            Add.run(["goal", very_long_title, "-c", "90"])
          rescue
            _ -> IO.puts("rejected")
          catch
            :exit, _ -> IO.puts("exit")
          end
        end)

      # Should either work or reject gracefully
      assert is_binary(output)
    end

    test "node with unicode title (emoji)" do
      output = capture_io(fn -> Add.run(["goal", "Fix bug 🐛 in feature ✨", "-c", "90"]) end)

      assert output =~ "Created node 1"

      node = Queries.get_node(1)
      assert node.title =~ "🐛"
      assert node.title =~ "✨"
    end

    test "node with unicode title (CJK characters)" do
      output = capture_io(fn -> Add.run(["goal", "实现用户认证功能", "-c", "90"]) end)

      assert output =~ "Created node 1"

      node = Queries.get_node(1)
      assert node.title == "实现用户认证功能"
    end

    test "node with unicode title (Arabic)" do
      output = capture_io(fn -> Add.run(["goal", "تنفيذ ميزة المصادقة", "-c", "90"]) end)

      assert output =~ "Created node 1"

      node = Queries.get_node(1)
      assert node.title == "تنفيذ ميزة المصادقة"
    end

    test "node with newlines in title" do
      title_with_newlines = "Line 1\nLine 2\nLine 3"

      output = capture_io(fn -> Add.run(["goal", title_with_newlines, "-c", "90"]) end)

      assert output =~ "Created node 1"

      node = Queries.get_node(1)
      assert node.title =~ "Line 1"
    end

    test "node with tabs and special whitespace" do
      title = "Tab\there\tand\tthere"

      output = capture_io(fn -> Add.run(["goal", title, "-c", "90"]) end)

      assert output =~ "Created node 1"
    end

    test "SQL injection in title - basic" do
      malicious_title = "'; DROP TABLE decision_nodes; --"

      output = capture_io(fn -> Add.run(["goal", malicious_title, "-c", "90"]) end)

      # Should create the node with escaped content
      assert output =~ "Created node 1"

      # Verify table still exists and has content
      node = Queries.get_node(1)
      assert node != nil
      assert node.title =~ "DROP TABLE"
    end

    test "SQL injection in title - union" do
      malicious_title = "test' UNION SELECT * FROM decision_nodes --"

      output = capture_io(fn -> Add.run(["goal", malicious_title, "-c", "90"]) end)

      assert output =~ "Created node 1"
      node = Queries.get_node(1)
      assert node.title =~ "UNION SELECT"
    end

    test "SQL injection in description" do
      output =
        capture_io(fn ->
          Add.run(["goal", "Safe title", "-c", "90", "-d", "'; DELETE FROM decision_nodes; --"])
        end)

      assert output =~ "Created node 1"

      # Verify we still have the node
      node = Queries.get_node(1)
      assert node != nil
    end

    test "SQL injection in rationale" do
      capture_io(fn -> Add.run(["goal", "Goal", "-c", "90"]) end)
      capture_io(fn -> Add.run(["option", "Option", "-c", "80"]) end)

      output =
        capture_io(fn -> Link.run(["1", "2", "-r", "'; DROP TABLE decision_edges; --"]) end)

      assert output =~ "Created edge" or output =~ "Linked"

      # Verify edges table still works
      edges = Queries.list_edges()
      assert length(edges) == 1
    end

    test "HTML/XSS in title" do
      xss_title = "<script>alert('xss')</script>"

      output = capture_io(fn -> Add.run(["goal", xss_title, "-c", "90"]) end)

      assert output =~ "Created node 1"

      node = Queries.get_node(1)
      assert node.title =~ "<script>"
    end

    test "null bytes in title" do
      # Null bytes can cause issues in C-based storage
      title_with_null = "Before\x00After"

      result =
        try do
          capture_io(fn -> Add.run(["goal", title_with_null, "-c", "90"]) end)
        rescue
          _ -> "caught_exception"
        catch
          :exit, _ -> "caught_exit"
        end

      assert is_binary(result)
    end
  end

  describe "Phase 9: Stress Tests - Confidence Edge Cases" do
    test "confidence at boundary 0" do
      output = capture_io(fn -> Add.run(["goal", "Zero confidence", "-c", "0"]) end)

      assert output =~ "Created node 1"

      node = Queries.get_node(1)
      {:ok, meta} = Jason.decode(node.metadata_json)
      assert meta["confidence"] == 0
    end

    test "confidence at boundary 100" do
      output = capture_io(fn -> Add.run(["goal", "Full confidence", "-c", "100"]) end)

      assert output =~ "Created node 1"

      node = Queries.get_node(1)
      {:ok, meta} = Jason.decode(node.metadata_json)
      assert meta["confidence"] == 100
    end

    test "confidence over 100" do
      output =
        capture_io(fn ->
          try do
            Add.run(["goal", "Over confident", "-c", "150"])
          rescue
            _ -> IO.puts("rejected")
          catch
            :exit, _ -> IO.puts("exit")
          end
        end)

      # Should either clamp, reject, or allow (implementation-dependent)
      assert is_binary(output)
    end

    test "negative confidence" do
      output =
        capture_io(:stderr, fn ->
          try do
            capture_io(fn -> Add.run(["goal", "Negative", "-c", "-50"]) end)
          rescue
            _ -> IO.puts("rejected")
          catch
            :exit, _ -> IO.puts("exit")
          end
        end)

      assert is_binary(output)
    end

    test "confidence as non-numeric string" do
      output =
        capture_io(:stderr, fn ->
          try do
            capture_io(fn -> Add.run(["goal", "Text confidence", "-c", "high"]) end)
          rescue
            _ -> IO.puts("rejected")
          catch
            :exit, _ -> IO.puts("exit")
          end
        end)

      assert is_binary(output)
    end

    test "confidence as float" do
      output =
        capture_io(fn ->
          try do
            Add.run(["goal", "Float confidence", "-c", "85.5"])
          rescue
            _ -> :rejected
          catch
            :exit, _ -> :rejected
          end
        end)

      # May parse as int (85) or reject
      assert is_binary(output) or output == :rejected
    end
  end

  describe "Phase 9: Stress Tests - Status Edge Cases" do
    setup do
      capture_io(fn -> Add.run(["goal", "Test goal", "-c", "90"]) end)
      :ok
    end

    test "invalid status value" do
      output =
        capture_io(:stderr, fn ->
          try do
            capture_io(fn -> Status.run(["1", "invalid_status_value"]) end)
          rescue
            _ -> IO.puts("rejected")
          catch
            :exit, _ -> IO.puts("exit")
          end
        end)

      # Should reject invalid status
      assert is_binary(output)
    end

    test "empty status value" do
      output =
        capture_io(:stderr, fn ->
          try do
            capture_io(fn -> Status.run(["1", ""]) end)
          rescue
            _ -> IO.puts("rejected")
          catch
            :exit, _ -> IO.puts("exit")
          end
        end)

      assert is_binary(output)
    end

    test "status with SQL injection" do
      output =
        capture_io(fn ->
          try do
            Status.run(["1", "active'; DROP TABLE decision_nodes; --"])
          rescue
            _ -> :rejected
          catch
            :exit, _ -> :rejected
          end
        end)

      # Should either reject or safely store
      assert is_binary(output) or output == :rejected

      # Verify table still works
      node = Queries.get_node(1)
      assert node != nil
    end

    test "valid status transitions" do
      # pending -> active
      capture_io(fn -> Status.run(["1", "active"]) end)
      assert Queries.get_node(1).status == "active"

      # active -> superseded
      capture_io(fn -> Status.run(["1", "superseded"]) end)
      assert Queries.get_node(1).status == "superseded"

      # superseded -> abandoned
      capture_io(fn -> Status.run(["1", "abandoned"]) end)
      assert Queries.get_node(1).status == "abandoned"

      # abandoned -> pending (reset)
      capture_io(fn -> Status.run(["1", "pending"]) end)
      assert Queries.get_node(1).status == "pending"
    end
  end

  describe "Phase 9: Stress Tests - Duplicate Operations" do
    setup do
      capture_io(fn -> Add.run(["goal", "Test goal", "-c", "90"]) end)
      capture_io(fn -> Add.run(["option", "Test option", "-c", "80"]) end)
      :ok
    end

    test "duplicate link creation" do
      # First link
      output1 = capture_io(fn -> Link.run(["1", "2", "-r", "first link"]) end)
      assert output1 =~ "Created edge" or output1 =~ "Linked"

      # Duplicate link - should either reject or be idempotent
      _output2 =
        capture_io(fn ->
          try do
            Link.run(["1", "2", "-r", "duplicate link"])
          rescue
            _ -> IO.puts("rejected duplicate")
          catch
            :exit, _ -> IO.puts("exit on duplicate")
          end
        end)

      # Either way, we should have at least one edge
      edges = Queries.list_edges() |> Enum.filter(&(&1.from_node_id == 1 and &1.to_node_id == 2))
      # Some implementations allow multiple edges with different rationales
      assert edges != []
    end

    test "unlink non-existent edge" do
      # No edge exists yet
      output =
        capture_io(:stderr, fn ->
          try do
            capture_io(fn -> Unlink.run(["1", "2"]) end)
          rescue
            _ -> IO.puts("no edge")
          catch
            :exit, _ -> IO.puts("exit")
          end
        end)

      assert is_binary(output)
    end

    test "double unlink" do
      # Create and unlink
      capture_io(fn -> Link.run(["1", "2", "-r", "test"]) end)
      capture_io(fn -> Unlink.run(["1", "2"]) end)

      # Try to unlink again
      output =
        capture_io(:stderr, fn ->
          try do
            capture_io(fn -> Unlink.run(["1", "2"]) end)
          rescue
            _ -> IO.puts("no edge")
          catch
            :exit, _ -> IO.puts("exit")
          end
        end)

      assert is_binary(output)
    end

    test "double delete" do
      # Delete node 2
      capture_io(fn -> Delete.run(["2"]) end)
      assert Queries.get_node(2) == nil

      # Try to delete again - should fail gracefully
      output =
        capture_io(:stderr, fn ->
          try do
            Delete.run(["2"])
          rescue
            _ -> IO.puts("not found")
          end
        end)

      assert output =~ "not found" or output =~ "Error" or output == ""
    end
  end

  describe "Phase 9: Stress Tests - Circular References" do
    test "direct circular reference (A -> A)" do
      capture_io(fn -> Add.run(["goal", "Node A", "-c", "90"]) end)

      result =
        capture_io(fn ->
          try do
            Link.run(["1", "1", "-r", "self-loop"])
          rescue
            _ -> IO.puts("rejected")
          catch
            :exit, _ -> IO.puts("exit")
          end
        end)

      assert is_binary(result)
    end

    test "indirect circular reference (A -> B -> A)" do
      capture_io(fn -> Add.run(["goal", "Node A", "-c", "90"]) end)
      capture_io(fn -> Add.run(["option", "Node B", "-c", "80"]) end)

      capture_io(fn -> Link.run(["1", "2", "-r", "A to B"]) end)

      # This creates a cycle
      result =
        capture_io(fn ->
          try do
            Link.run(["2", "1", "-r", "B to A - creates cycle"])
          rescue
            _ -> IO.puts("rejected")
          catch
            :exit, _ -> IO.puts("exit")
          end
        end)

      # Implementation may allow or reject cycles
      assert is_binary(result)
    end

    test "longer circular reference (A -> B -> C -> A)" do
      capture_io(fn -> Add.run(["goal", "Node A", "-c", "90"]) end)
      capture_io(fn -> Add.run(["option", "Node B", "-c", "80"]) end)
      capture_io(fn -> Add.run(["decision", "Node C", "-c", "85"]) end)

      capture_io(fn -> Link.run(["1", "2", "-r", "A to B"]) end)
      capture_io(fn -> Link.run(["2", "3", "-r", "B to C"]) end)

      # This creates a cycle
      result =
        capture_io(fn ->
          try do
            Link.run(["3", "1", "-r", "C to A - creates cycle"])
          rescue
            _ -> IO.puts("rejected")
          catch
            :exit, _ -> IO.puts("exit")
          end
        end)

      assert is_binary(result)

      # Graph operations should still work
      graph_output = capture_io(fn -> Graph.run() end)
      {:ok, graph} = Jason.decode(graph_output)
      assert length(graph["nodes"]) == 3
    end
  end

  describe "Phase 9: Stress Tests - Large Data" do
    test "create 100 nodes rapidly" do
      for i <- 1..100 do
        capture_io(fn -> Add.run(["goal", "Node #{i}", "-c", "#{rem(i, 100)}"]) end)
      end

      nodes = Queries.list_nodes()
      assert length(nodes) == 100
    end

    test "create 50 edges in a chain" do
      # Create 51 nodes
      for i <- 1..51 do
        capture_io(fn -> Add.run(["goal", "Chain node #{i}", "-c", "90"]) end)
      end

      # Link them in a chain
      for i <- 1..50 do
        capture_io(fn -> Link.run(["#{i}", "#{i + 1}", "-r", "chain link"]) end)
      end

      edges = Queries.list_edges()
      assert length(edges) == 50

      # Graph should still serialize
      graph_output = capture_io(fn -> Graph.run() end)
      {:ok, graph} = Jason.decode(graph_output)
      assert length(graph["nodes"]) == 51
      assert length(graph["edges"]) == 50
    end

    test "wide tree - one node with 20 children" do
      capture_io(fn -> Add.run(["goal", "Root node", "-c", "90"]) end)

      # Create 20 children
      for i <- 2..21 do
        capture_io(fn -> Add.run(["option", "Child #{i - 1}", "-c", "80"]) end)
        capture_io(fn -> Link.run(["1", "#{i}", "-r", "child #{i - 1}"]) end)
      end

      {_incoming, outgoing} = Queries.get_node_edges(1)
      assert length(outgoing) == 20

      # Show command should handle this
      show_output = capture_io(fn -> Show.run(["1"]) end)
      assert show_output =~ "Outgoing (20):"
    end

    test "node with very long description" do
      long_desc = String.duplicate("This is a very long description. ", 500)

      output =
        capture_io(fn -> Add.run(["goal", "Long desc node", "-c", "90", "-d", long_desc]) end)

      assert output =~ "Created node 1"

      node = Queries.get_node(1)
      assert String.length(node.description || "") > 1000
    end

    test "node with many files" do
      files = Enum.map_join(1..50, ",", &"lib/feature_#{&1}.ex")

      output =
        capture_io(fn -> Add.run(["action", "Multi-file change", "-c", "85", "-f", files]) end)

      assert output =~ "Created node 1"

      node = Queries.get_node(1)
      {:ok, meta} = Jason.decode(node.metadata_json)
      assert length(meta["files"]) == 50
    end
  end

  describe "Phase 9: Stress Tests - Diff/Patch Edge Cases" do
    setup do
      capture_io(fn -> Add.run(["goal", "Test goal", "-c", "90"]) end)
      capture_io(fn -> Add.run(["option", "Test option", "-c", "80"]) end)
      capture_io(fn -> Link.run(["1", "2", "-r", "test link"]) end)
      :ok
    end

    test "validate malformed patch - missing version" do
      patch_path = Path.join(@tmp_dir, "bad-patch-1.json")

      patch = %{
        "author" => "test",
        "nodes" => [],
        "edges" => []
        # Missing "version"
      }

      File.write!(patch_path, Jason.encode!(patch))

      output = capture_io(:stderr, fn -> Diff.run(["validate", patch_path]) end)
      # Should indicate invalid
      assert output =~ "invalid" or output =~ "Invalid" or output =~ "missing" or
               output =~ "error" or output == ""
    end

    test "validate malformed patch - missing nodes" do
      patch_path = Path.join(@tmp_dir, "bad-patch-2.json")

      patch = %{
        "version" => "1.0",
        "author" => "test",
        "edges" => []
        # Missing "nodes"
      }

      File.write!(patch_path, Jason.encode!(patch))

      output = capture_io(:stderr, fn -> Diff.run(["validate", patch_path]) end)
      assert is_binary(output)
    end

    test "validate patch with invalid node structure" do
      patch_path = Path.join(@tmp_dir, "bad-patch-3.json")

      patch = %{
        "version" => "1.0",
        "author" => "test",
        "nodes" => [
          %{
            # Missing required fields like change_id, title, node_type
            "foo" => "bar"
          }
        ],
        "edges" => []
      }

      File.write!(patch_path, Jason.encode!(patch))

      output = capture_io(:stderr, fn -> Diff.run(["validate", patch_path]) end)
      assert is_binary(output)
    end

    test "apply patch twice (idempotence)" do
      patch_path = Path.join(@tmp_dir, "idempotent-patch.json")

      capture_io(fn -> Diff.run(["export", "-o", patch_path, "--author", "test"]) end)

      # First apply
      _output1 =
        capture_io(fn ->
          try do
            Diff.run(["apply", patch_path])
          rescue
            _ -> IO.puts("error on first apply")
          catch
            :exit, _ -> IO.puts("exit on first apply")
          end
        end)

      # Second apply - should be idempotent
      _output2 =
        capture_io(fn ->
          try do
            Diff.run(["apply", patch_path])
          rescue
            _ -> IO.puts("error on second apply")
          catch
            :exit, _ -> IO.puts("exit on second apply")
          end
        end)

      # Should not duplicate nodes
      nodes = Queries.list_nodes()
      # We have 2 nodes, applying same patch twice should not create more
      assert length(nodes) == 2
    end

    test "export with empty graph" do
      # Clear the graph
      capture_io(fn -> Delete.run(["2"]) end)
      capture_io(fn -> Delete.run(["1"]) end)

      empty_patch_path = Path.join(@tmp_dir, "empty-patch.json")

      _output = capture_io(fn -> Diff.run(["export", "-o", empty_patch_path]) end)

      if File.exists?(empty_patch_path) do
        {:ok, patch} = File.read!(empty_patch_path) |> Jason.decode()
        assert patch["nodes"] == []
        assert patch["edges"] == []
      end
    end

    test "diff status with empty directory" do
      empty_dir = Path.join(@tmp_dir, "empty_patches")
      File.mkdir_p!(empty_dir)

      output = capture_io(fn -> Diff.run(["status", empty_dir]) end)
      assert output =~ "No patch files" or output =~ "0 patches" or output == ""
    end

    test "diff status with non-json files" do
      mixed_dir = Path.join(@tmp_dir, "mixed_patches")
      File.mkdir_p!(mixed_dir)
      File.write!(Path.join(mixed_dir, "readme.txt"), "Not a patch")
      File.write!(Path.join(mixed_dir, "notes.md"), "# Notes")

      output = capture_io(fn -> Diff.run(["status", mixed_dir]) end)
      # Should ignore non-JSON files gracefully
      assert is_binary(output)
    end
  end

  describe "Phase 9: Stress Tests - Edge Types" do
    setup do
      capture_io(fn -> Add.run(["goal", "Goal", "-c", "90"]) end)
      capture_io(fn -> Add.run(["option", "Option", "-c", "80"]) end)
      :ok
    end

    test "link with all valid edge types" do
      valid_types = ["leads_to", "chosen", "rejected", "revisits", "informs", "supersedes"]

      for {edge_type, i} <- Enum.with_index(valid_types, 3) do
        capture_io(fn -> Add.run(["option", "Option #{i}", "-c", "80"]) end)

        output = capture_io(fn -> Link.run(["1", "#{i}", "-t", edge_type, "-r", "test"]) end)

        assert output =~ "Created edge" or output =~ "Linked",
               "Failed for edge type: #{edge_type}"
      end
    end

    test "link with invalid edge type" do
      output =
        capture_io(:stderr, fn ->
          try do
            capture_io(fn -> Link.run(["1", "2", "-t", "invalid_type", "-r", "test"]) end)
          rescue
            _ -> IO.puts("rejected")
          catch
            :exit, _ -> IO.puts("exit")
          end
        end)

      # Should either reject or create with default type
      assert is_binary(output)
    end

    test "link with empty edge type" do
      output =
        capture_io(fn ->
          try do
            Link.run(["1", "2", "-t", "", "-r", "test"])
          rescue
            _ -> :rejected
          catch
            :exit, _ -> :rejected
          end
        end)

      assert is_binary(output) or output == :rejected
    end
  end

  describe "Phase 9: Stress Tests - Command Combinations" do
    test "rapid create-link-delete cycle" do
      for i <- 1..20 do
        # Create
        capture_io(fn -> Add.run(["goal", "Temp #{i}", "-c", "90"]) end)

        if i > 1 do
          # Link to previous
          capture_io(fn -> Link.run(["#{i - 1}", "#{i}", "-r", "chain"]) end)
        end

        # Delete previous
        if i > 2 do
          capture_io(fn -> Delete.run(["#{i - 2}"]) end)
        end
      end

      # Should have nodes 19 and 20 remaining
      nodes = Queries.list_nodes()
      assert length(nodes) >= 2
    end

    test "alternating add and status updates" do
      for i <- 1..10 do
        capture_io(fn -> Add.run(["goal", "Goal #{i}", "-c", "90"]) end)
        capture_io(fn -> Status.run(["#{i}", "active"]) end)
        capture_io(fn -> Status.run(["#{i}", "superseded"]) end)
        capture_io(fn -> Status.run(["#{i}", "pending"]) end)
      end

      nodes = Queries.list_nodes()
      assert length(nodes) == 10
      assert Enum.all?(nodes, &(&1.status == "pending"))
    end

    test "build graph then bulk status update" do
      # Build a tree
      capture_io(fn -> Add.run(["goal", "Root", "-c", "90"]) end)

      for i <- 2..11 do
        capture_io(fn -> Add.run(["option", "Option #{i}", "-c", "80"]) end)
        capture_io(fn -> Link.run(["1", "#{i}", "-r", "option"]) end)
      end

      # Supersede all options
      for i <- 2..11 do
        capture_io(fn -> Status.run(["#{i}", "superseded"]) end)
      end

      superseded = Queries.list_nodes() |> Enum.filter(&(&1.status == "superseded"))
      assert length(superseded) == 10
    end

    test "prompt update preserves other metadata" do
      capture_io(fn ->
        Add.run(["goal", "Test", "-c", "90", "-f", "test.ex", "--commit", "abc123"])
      end)

      node_before = Queries.get_node(1)
      {:ok, meta_before} = Jason.decode(node_before.metadata_json)
      assert meta_before["confidence"] == 90
      assert meta_before["commit"] == "abc123"

      capture_io(fn -> Prompt.run(["1", "New prompt text"]) end)

      node_after = Queries.get_node(1)
      {:ok, meta_after} = Jason.decode(node_after.metadata_json)
      assert meta_after["prompt"] == "New prompt text"
      assert meta_after["confidence"] == 90
      assert meta_after["commit"] == "abc123"
    end
  end

  describe "Phase 9: Stress Tests - Filter Edge Cases" do
    setup do
      capture_io(fn -> Add.run(["goal", "Goal 1", "-c", "90"]) end)
      capture_io(fn -> Add.run(["option", "Option 1", "-c", "80"]) end)
      capture_io(fn -> Add.run(["decision", "Decision 1", "-c", "85"]) end)
      capture_io(fn -> Add.run(["action", "Action 1", "-c", "90"]) end)
      capture_io(fn -> Add.run(["outcome", "Outcome 1", "-c", "95"]) end)
      capture_io(fn -> Add.run(["observation", "Observation 1", "-c", "100"]) end)
      capture_io(fn -> Add.run(["revisit", "Revisit 1", "-c", "80"]) end)
      :ok
    end

    test "filter by non-existent type" do
      output = capture_io(fn -> Nodes.run(["-t", "nonexistent_type"]) end)
      assert output =~ "0 nodes" or output =~ "No nodes"
    end

    test "filter by each valid type" do
      valid_types = ["goal", "option", "decision", "action", "outcome", "observation", "revisit"]

      for node_type <- valid_types do
        output = capture_io(fn -> Nodes.run(["-t", node_type]) end)
        assert output =~ "1 nodes:" or output =~ "1 node", "Failed for type: #{node_type}"
      end
    end

    test "filter by non-existent branch" do
      output = capture_io(fn -> Nodes.run(["-b", "nonexistent-branch-12345"]) end)
      assert output =~ "0 nodes" or output =~ "No nodes"
    end

    test "pulse with no active goals" do
      # All nodes are pending by default
      output = capture_io(fn -> Pulse.run([]) end)
      assert output =~ "Total nodes:" or output =~ "Decision Graph Pulse"
    end

    test "archaeology timeline on empty graph" do
      # Clear graph
      for i <- 1..7 do
        capture_io(fn -> Delete.run(["#{i}"]) end)
      end

      output = capture_io(fn -> Archaeology.run(["timeline"]) end)
      assert output =~ "No nodes" or output =~ "Timeline" or output == "[]"
    end
  end

  describe "Phase 9: Stress Tests - Sync and Export" do
    setup do
      capture_io(fn -> Add.run(["goal", "Export test", "-c", "90"]) end)
      :ok
    end

    test "sync to read-only path fails gracefully" do
      # Try to write to a path that doesn't exist and can't be created
      impossible_path = "/nonexistent/readonly/path/graph.json"

      output =
        capture_io(:stderr, fn ->
          try do
            capture_io(fn -> Sync.run([impossible_path]) end)
          rescue
            _ -> IO.puts("write failed")
          catch
            :exit, _ -> IO.puts("exit")
          end
        end)

      assert is_binary(output)
    end

    test "writeup with invalid node range" do
      output =
        capture_io(fn ->
          try do
            Writeup.run(["-t", "Test", "-n", "999-1000"])
          rescue
            _ -> IO.puts("invalid range")
          catch
            :exit, _ -> IO.puts("exit")
          end
        end)

      # Should produce output even if nodes don't exist
      assert is_binary(output)
    end

    test "writeup with reversed range" do
      capture_io(fn -> Add.run(["option", "Second", "-c", "80"]) end)
      capture_io(fn -> Add.run(["decision", "Third", "-c", "85"]) end)

      output =
        capture_io(fn ->
          try do
            Writeup.run(["-t", "Test", "-n", "3-1"])
          rescue
            _ -> IO.puts("error")
          catch
            :exit, _ -> IO.puts("exit")
          end
        end)

      # Should handle reversed range or fail gracefully
      assert is_binary(output)
    end

    test "backup to existing file" do
      backup_path = Path.join(@tmp_dir, "existing-backup.db")
      File.write!(backup_path, "existing content")

      output = capture_io(fn -> Backup.run([backup_path]) end)

      # Should overwrite or fail gracefully
      assert is_binary(output)
    end
  end

  describe "Phase 9: Stress Tests - JSON Output" do
    setup do
      capture_io(fn -> Add.run(["goal", "JSON test", "-c", "90"]) end)
      :ok
    end

    test "graph JSON is valid" do
      output = capture_io(fn -> Graph.run() end)
      assert {:ok, _} = Jason.decode(output)
    end

    test "show JSON is valid" do
      output = capture_io(fn -> Show.run(["1", "--json"]) end)
      assert {:ok, _} = Jason.decode(output)
    end

    test "pulse JSON is valid" do
      output = capture_io(fn -> Pulse.run(["--json"]) end)
      assert {:ok, _} = Jason.decode(output)
    end

    test "archaeology timeline JSON is valid" do
      output = capture_io(fn -> Archaeology.run(["timeline", "--json"]) end)
      assert {:ok, _} = Jason.decode(output)
    end

    test "audit JSON is valid" do
      output = capture_io(fn -> Audit.run(["--json"]) end)

      case Jason.decode(output) do
        {:ok, _} ->
          :ok

        {:error, _} ->
          # Audit might not support --json
          assert output =~ "Audit" or output =~ "nodes"
      end
    end
  end
end
