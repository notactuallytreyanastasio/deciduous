defmodule Deciduex.Commands.SyncTest do
  use ExUnit.Case

  import ExUnit.CaptureIO
  import Deciduex.TestFixtures

  alias Deciduex.Commands.Sync

  @test_output_dir "/tmp/deciduex_sync_test"

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

  describe "sync command" do
    test "exports graph to specified path" do
      output_path = Path.join(@test_output_dir, "graph-data.json")

      output =
        capture_io(fn ->
          Sync.run([output_path])
        end)

      assert output =~ "Exported graph to #{output_path}"
      assert output =~ "3 nodes"
      assert output =~ "2 edges"

      # Verify file was created
      assert File.exists?(output_path)

      # Verify JSON content
      {:ok, content} = File.read(output_path)
      data = Jason.decode!(content)
      assert is_list(data["nodes"])
      assert is_list(data["edges"])
      assert length(data["nodes"]) == 3
    end

    test "exports to default path when none specified" do
      # Create docs directory
      docs_dir = Path.join(@test_output_dir, "docs")
      File.mkdir_p!(docs_dir)

      # Change to test directory temporarily
      original_dir = File.cwd!()
      File.cd!(@test_output_dir)

      output =
        capture_io(fn ->
          Sync.run([])
        end)

      File.cd!(original_dir)

      assert output =~ "Exported graph to docs/graph-data.json"
      assert File.exists?(Path.join(docs_dir, "graph-data.json"))
    end
  end
end
