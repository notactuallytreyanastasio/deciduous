defmodule Deciduex.Commands.AddTest do
  use ExUnit.Case

  import ExUnit.CaptureIO
  import Deciduex.TestFixtures

  alias Deciduex.Commands.Add
  alias Deciduex.Queries
  alias Deciduex.Repo
  alias Ecto.Adapters.SQL

  setup do
    create_tables!()
    :ok
  end

  describe "add command" do
    test "creates a goal node with just type and title" do
      output =
        capture_io(fn ->
          Add.run(["goal", "Test goal"])
        end)

      assert output =~ "Created node"
      assert output =~ "type: goal"
      assert output =~ "title: Test goal"

      # Verify node exists in database
      nodes = Queries.list_nodes()
      assert length(nodes) == 1
      assert hd(nodes).node_type == "goal"
      assert hd(nodes).title == "Test goal"
      assert hd(nodes).status == "pending"
    end

    test "creates node with confidence flag" do
      output =
        capture_io(fn ->
          Add.run(["action", "Test action", "-c", "85"])
        end)

      assert output =~ "[confidence: 85%]"

      node = hd(Queries.list_nodes())
      {:ok, meta} = Jason.decode(node.metadata_json)
      assert meta["confidence"] == 85
    end

    test "creates node with branch flag" do
      output =
        capture_io(fn ->
          Add.run(["decision", "Test decision", "-b", "feature-x", "--no-branch"])
        end)

      # --no-branch should override -b
      refute output =~ "[branch:"
    end

    test "creates node with explicit branch" do
      output =
        capture_io(fn ->
          Add.run(["option", "Test option", "-b", "my-branch"])
        end)

      assert output =~ "[branch: my-branch]"

      node = hd(Queries.list_nodes())
      {:ok, meta} = Jason.decode(node.metadata_json)
      assert meta["branch"] == "my-branch"
    end

    test "creates node with description" do
      capture_io(fn ->
        Add.run(["observation", "Test obs", "-d", "A detailed description"])
      end)

      node = hd(Queries.list_nodes())
      assert node.description == "A detailed description"
    end

    test "creates node with files" do
      output =
        capture_io(fn ->
          Add.run(["action", "Test files", "-f", "foo.ex,bar.ex"])
        end)

      assert output =~ "[files: foo.ex,bar.ex]"

      node = hd(Queries.list_nodes())
      {:ok, meta} = Jason.decode(node.metadata_json)
      assert meta["files"] == ["foo.ex", "bar.ex"]
    end

    test "creates node with prompt" do
      output =
        capture_io(fn ->
          Add.run([
            "goal",
            "Test prompt",
            "-p",
            "This is a long prompt that should be captured verbatim for context recovery and should be at least 200 characters to avoid the warning message about short prompts being summaries instead of full prompts"
          ])
        end)

      assert output =~ "[prompt:"

      node = hd(Queries.list_nodes())
      {:ok, meta} = Jason.decode(node.metadata_json)
      assert meta["prompt"] =~ "This is a long prompt"
    end

    @tag :skip
    test "warns on short prompt" do
      output =
        capture_io(:stderr, fn ->
          capture_io(fn ->
            Add.run(["goal", "Short prompt test", "-p", "Short"])
          end)
        end)

      assert output =~ "Warning:"
      assert output =~ "looks like a summary"
    end

    test "creates all valid node types" do
      types = ~w(goal option decision action outcome observation revisit)

      for type <- types do
        capture_io(fn ->
          Add.run([type, "Test #{type}"])
        end)
      end

      nodes = Queries.list_nodes()
      assert length(nodes) == 7

      created_types = Enum.map(nodes, & &1.node_type) |> Enum.sort()
      assert created_types == Enum.sort(types)
    end

    @tag :skip
    test "rejects invalid node type" do
      output =
        capture_io(:stderr, fn ->
          catch_exit(Add.run(["invalid", "Bad type"]))
        end)

      assert output =~ "Invalid node type"
    end

    test "logs command to command_log" do
      capture_io(fn ->
        Add.run(["goal", "Logged goal", "-c", "90"])
      end)

      # Check command was logged
      result = SQL.query!(Repo, "SELECT command FROM command_log ORDER BY id DESC LIMIT 1")
      assert length(result.rows) == 1
      [[cmd]] = result.rows
      assert cmd =~ "add"
      assert cmd =~ "goal"
    end

    test "creates node with date flag" do
      output =
        capture_io(fn ->
          Add.run(["goal", "Backdated goal", "--date", "2024-01-15"])
        end)

      assert output =~ "[date:"

      node = hd(Queries.list_nodes())
      assert node.created_at =~ "2024-01-15"
    end
  end
end
