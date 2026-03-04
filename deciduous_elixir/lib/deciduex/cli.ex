defmodule Deciduex.CLI do
  @moduledoc """
  CLI entry point. Dispatches subcommands.
  """

  alias Deciduex.Commands.Add
  alias Deciduex.Commands.CommandLog
  alias Deciduex.Commands.Delete
  alias Deciduex.Commands.Edges
  alias Deciduex.Commands.Graph
  alias Deciduex.Commands.Link
  alias Deciduex.Commands.Nodes
  alias Deciduex.Commands.Prompt
  alias Deciduex.Commands.Show
  alias Deciduex.Commands.Status
  alias Deciduex.Commands.Unlink
  alias Deciduex.DB
  alias Deciduex.Repo

  def main(args) do
    # When called via OTP release `eval`, the application tree isn't started.
    # Ensure Ecto and its dependencies are running before we open the DB.
    {:ok, _} = Application.ensure_all_started(:ecto_sqlite3)

    case DB.find_db_path() do
      {:ok, db_path} ->
        {:ok, _} = Repo.start_link(database: db_path)
        dispatch(args)

      :error ->
        IO.puts(:stderr, "No .deciduous/deciduous.db found in current or parent directories.")
        IO.puts(:stderr, "Run 'deciduous init' first.")
        System.halt(1)
    end
  end

  defp dispatch(args) do
    case args do
      ["add" | rest] ->
        Add.run(rest)

      ["link" | rest] ->
        Link.run(rest)

      ["unlink" | rest] ->
        Unlink.run(rest)

      ["status" | rest] ->
        Status.run(rest)

      ["prompt" | rest] ->
        Prompt.run(rest)

      ["delete" | rest] ->
        Delete.run(rest)

      ["nodes" | rest] ->
        Nodes.run(rest)

      ["edges" | _rest] ->
        Edges.run()

      ["graph" | _rest] ->
        Graph.run()

      ["show" | rest] ->
        Show.run(rest)

      ["commands" | rest] ->
        CommandLog.run(rest)

      [] ->
        print_usage()

      [unknown | _] ->
        IO.puts(:stderr, "Unknown command: #{unknown}")
        System.halt(1)
    end
  end

  defp print_usage do
    IO.puts("Usage: deciduex <command> [options]")
    IO.puts("")
    IO.puts("Commands:")
    IO.puts("  add <type> <title>    Create a new decision node")
    IO.puts("  link <from> <to>      Create edge between nodes")
    IO.puts("  unlink <from> <to>    Remove edge between nodes")
    IO.puts("  status <id> <status>  Update node status")
    IO.puts("  prompt <id> [text]    Update node prompt")
    IO.puts("  delete <id>           Delete node and its edges")
    IO.puts("  nodes                 List all decision graph nodes")
    IO.puts("  edges                 List all decision graph edges")
    IO.puts("  graph                 Output full graph as JSON")
    IO.puts("  show <id>             Show detailed node information")
    IO.puts("  commands              Show recent command log")
  end
end
