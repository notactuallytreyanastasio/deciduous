defmodule Deciduex.CLI do
  @moduledoc """
  CLI entry point. Dispatches subcommands.
  """

  def main(args) do
    # When called via OTP release `eval`, the application tree isn't started.
    # Ensure Ecto and its dependencies are running before we open the DB.
    {:ok, _} = Application.ensure_all_started(:ecto_sqlite3)

    case Deciduex.DB.find_db_path() do
      {:ok, db_path} ->
        {:ok, _} = Deciduex.Repo.start_link(database: db_path)
        dispatch(args)

      :error ->
        IO.puts(:stderr, "No .deciduous/deciduous.db found in current or parent directories.")
        IO.puts(:stderr, "Run 'deciduous init' first.")
        System.halt(1)
    end
  end

  defp dispatch(args) do
    case args do
      ["nodes" | rest] ->
        Deciduex.Commands.Nodes.run(rest)

      ["edges" | _rest] ->
        Deciduex.Commands.Edges.run()

      ["graph" | _rest] ->
        Deciduex.Commands.Graph.run()

      ["show" | rest] ->
        Deciduex.Commands.Show.run(rest)

      ["commands" | rest] ->
        Deciduex.Commands.CommandLog.run(rest)

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
    IO.puts("  nodes      List all decision graph nodes")
    IO.puts("  edges      List all decision graph edges")
    IO.puts("  graph      Output full graph as JSON")
    IO.puts("  show <id>  Show detailed node information")
    IO.puts("  commands   Show recent command log")
  end
end
