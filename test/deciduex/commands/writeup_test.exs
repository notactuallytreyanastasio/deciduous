defmodule Deciduex.Commands.WriteupTest do
  use ExUnit.Case

  import ExUnit.CaptureIO
  import Deciduex.TestFixtures

  alias Deciduex.Commands.Writeup

  @test_output_dir "/tmp/deciduex_writeup_test"

  setup do
    create_tables!()
    seed_sample_data!()

    # Add more nodes for a richer test
    insert_node!(%{
      id: 4,
      node_type: "action",
      title: "Implement JWT auth",
      created_at: "2024-01-04T00:00:00Z"
    })

    insert_node!(%{
      id: 5,
      node_type: "outcome",
      title: "Auth system working",
      created_at: "2024-01-05T00:00:00Z"
    })

    insert_edge!(%{
      id: 3,
      from_node_id: 3,
      to_node_id: 4,
      edge_type: "leads_to"
    })

    insert_edge!(%{
      id: 4,
      from_node_id: 4,
      to_node_id: 5,
      edge_type: "leads_to"
    })

    # Clean up test output directory
    File.rm_rf!(@test_output_dir)
    File.mkdir_p!(@test_output_dir)

    on_exit(fn ->
      File.rm_rf!(@test_output_dir)
    end)

    :ok
  end

  describe "writeup command" do
    test "generates writeup to stdout" do
      output =
        capture_io(fn ->
          Writeup.run([])
        end)

      assert output =~ "## Pull Request"
      assert output =~ "Add auth"
      assert output =~ "Choose JWT"
    end

    test "generates writeup with custom title" do
      output =
        capture_io(fn ->
          Writeup.run(["-t", "My PR Title"])
        end)

      assert output =~ "## My PR Title"
    end

    test "includes goals in summary" do
      output =
        capture_io(fn ->
          Writeup.run([])
        end)

      assert output =~ "## Summary"
      assert output =~ "**Goal:** Add auth"
    end

    test "includes decisions section" do
      output =
        capture_io(fn ->
          Writeup.run([])
        end)

      assert output =~ "## Key Decisions"
      assert output =~ "Choose JWT"
    end

    test "includes actions section" do
      output =
        capture_io(fn ->
          Writeup.run([])
        end)

      assert output =~ "## Implementation"
      assert output =~ "Implement JWT auth"
    end

    test "includes outcomes section" do
      output =
        capture_io(fn ->
          Writeup.run([])
        end)

      assert output =~ "## Results"
      assert output =~ "Auth system working"
    end

    test "includes test plan by default" do
      output =
        capture_io(fn ->
          Writeup.run([])
        end)

      assert output =~ "## Test Plan"
      assert output =~ "Unit tests pass"
    end

    test "skips test plan with --no-test-plan" do
      output =
        capture_io(fn ->
          Writeup.run(["--no-test-plan"])
        end)

      refute output =~ "## Test Plan"
    end

    test "filters by node IDs" do
      output =
        capture_io(fn ->
          Writeup.run(["-n", "1,3"])
        end)

      # Node 1 (goal) appears in Summary
      assert output =~ "Add auth"
      # Node 3 (decision) appears in Key Decisions
      assert output =~ "Choose JWT"
      # Node 4 (action) is filtered out
      refute output =~ "Implement JWT auth"
    end

    test "writes to file with -o" do
      output_path = Path.join(@test_output_dir, "writeup.md")

      output =
        capture_io(fn ->
          Writeup.run(["-o", output_path])
        end)

      assert output =~ "Generated PR writeup to #{output_path}"
      assert File.exists?(output_path)

      {:ok, content} = File.read(output_path)
      assert content =~ "## Pull Request"
    end
  end
end
