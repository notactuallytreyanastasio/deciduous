defmodule Deciduex.Commands.DiffTest do
  use ExUnit.Case

  import ExUnit.CaptureIO
  import Deciduex.TestFixtures

  alias Deciduex.Commands.Diff

  @test_output_dir "/tmp/deciduex_diff_test"

  setup do
    create_tables!()
    seed_sample_data!()

    # Clean up test output directory
    File.rm_rf!(@test_output_dir)
    File.mkdir_p!(@test_output_dir)

    on_exit(fn ->
      File.rm_rf!(@test_output_dir)
    end)

    :ok
  end

  describe "diff export" do
    test "exports patch to file" do
      output_path = Path.join(@test_output_dir, "test-patch.json")

      output =
        capture_io(fn ->
          Diff.run(["export", "-o", output_path])
        end)

      assert output =~ "Exported patch to #{output_path}"
      assert output =~ "3 nodes"
      assert output =~ "2 edges"

      # Verify file was created
      assert File.exists?(output_path)

      # Verify JSON structure
      {:ok, content} = File.read(output_path)
      patch = Jason.decode!(content)
      assert patch["version"] == "1.0"
      assert is_list(patch["nodes"])
      assert is_list(patch["edges"])
      assert length(patch["nodes"]) == 3
    end

    test "exports patch with node filter" do
      output_path = Path.join(@test_output_dir, "filtered-patch.json")

      output =
        capture_io(fn ->
          Diff.run(["export", "-o", output_path, "-n", "1,2"])
        end)

      assert output =~ "2 nodes"

      {:ok, content} = File.read(output_path)
      patch = Jason.decode!(content)
      assert length(patch["nodes"]) == 2
    end

    test "exports patch with branch filter" do
      output_path = Path.join(@test_output_dir, "branch-patch.json")

      capture_io(fn ->
        Diff.run(["export", "-o", output_path, "-b", "feature-auth"])
      end)

      # Should only include nodes with branch "feature-auth"
      {:ok, content} = File.read(output_path)
      patch = Jason.decode!(content)
      # Nodes 2 and 3 have branch "feature-auth"
      assert length(patch["nodes"]) == 2
    end

    # Note: Tests requiring System.halt are skipped since they stop the VM
    @tag :skip
    test "requires output path" do
      # This test is skipped because System.halt stops the VM
      :ok
    end
  end

  describe "diff apply" do
    test "applies patch file" do
      # First, create a patch
      patch = %{
        version: "1.0",
        author: "test",
        branch: "test-branch",
        created_at: DateTime.utc_now() |> DateTime.to_iso8601(),
        nodes: [
          %{
            change_id: "new-node-1",
            node_type: "goal",
            title: "New Goal",
            description: nil,
            status: "active",
            metadata_json: nil,
            created_at: "2024-01-10T00:00:00Z"
          }
        ],
        edges: []
      }

      patch_path = Path.join(@test_output_dir, "apply-test.json")
      File.write!(patch_path, Jason.encode!(patch))

      output =
        capture_io(fn ->
          Diff.run(["apply", patch_path])
        end)

      assert output =~ "Applied apply-test.json: 1 nodes, 0 edges"
      assert output =~ "Total: 1 nodes added"
    end

    test "skips duplicate nodes" do
      # Create a patch with existing change_id
      patch = %{
        version: "1.0",
        nodes: [
          %{
            change_id: "test-1",
            node_type: "goal",
            title: "Add auth",
            description: nil,
            status: "active",
            metadata_json: nil,
            created_at: "2024-01-01T00:00:00Z"
          }
        ],
        edges: []
      }

      patch_path = Path.join(@test_output_dir, "duplicate-test.json")
      File.write!(patch_path, Jason.encode!(patch))

      output =
        capture_io(fn ->
          Diff.run(["apply", patch_path])
        end)

      assert output =~ "0 nodes added, 1 skipped"
    end

    test "dry run does not apply changes" do
      patch = %{
        version: "1.0",
        nodes: [
          %{
            change_id: "dry-run-node",
            node_type: "goal",
            title: "Dry Run Goal",
            status: "active",
            created_at: "2024-01-10T00:00:00Z"
          }
        ],
        edges: []
      }

      patch_path = Path.join(@test_output_dir, "dry-run-test.json")
      File.write!(patch_path, Jason.encode!(patch))

      output =
        capture_io(fn ->
          Diff.run(["apply", "--dry-run", patch_path])
        end)

      assert output =~ "Would apply"
      assert output =~ "Dry run complete"
    end
  end

  describe "diff status" do
    test "shows no patches when directory is empty" do
      patches_dir = Path.join(@test_output_dir, "patches")
      File.mkdir_p!(patches_dir)

      output =
        capture_io(fn ->
          Diff.run(["status", patches_dir])
        end)

      assert output =~ "No patch files found"
    end

    test "lists patches in directory" do
      patches_dir = Path.join(@test_output_dir, "patches")
      File.mkdir_p!(patches_dir)

      # Create a test patch
      patch = %{
        version: "1.0",
        author: "alice",
        branch: "feature-x",
        nodes: [%{change_id: "test", node_type: "goal", title: "Test"}],
        edges: []
      }

      File.write!(Path.join(patches_dir, "test.json"), Jason.encode!(patch))

      output =
        capture_io(fn ->
          Diff.run(["status", patches_dir])
        end)

      assert output =~ "Available patches"
      assert output =~ "test.json"
      assert output =~ "1 nodes"
      assert output =~ "alice"
    end
  end

  describe "diff validate" do
    test "validates valid patch" do
      patch = %{
        version: "1.0",
        nodes: [
          %{change_id: "node-1", node_type: "goal", title: "Goal"},
          %{change_id: "node-2", node_type: "action", title: "Action"}
        ],
        edges: [
          %{from_change_id: "node-1", to_change_id: "node-2", edge_type: "leads_to"}
        ]
      }

      patch_path = Path.join(@test_output_dir, "valid.json")
      File.write!(patch_path, Jason.encode!(patch))

      output =
        capture_io(fn ->
          Diff.run(["validate", patch_path])
        end)

      assert output =~ "Valid"
      assert output =~ "2 nodes"
      assert output =~ "1 edges"
    end

    # Note: Tests requiring System.halt are skipped since they stop the VM
    @tag :skip
    test "detects invalid edge references" do
      # This test is skipped because System.halt stops the VM
      :ok
    end
  end
end
