defmodule Deciduex.CLI do
  @moduledoc """
  CLI entry point. Dispatches subcommands.
  """

  # These functions intentionally never return (they halt or raise)
  @dialyzer {:nowarn_function, exit_with_error: 0}
  @dialyzer {:nowarn_function, exit_with_error: 1}

  @doc """
  Exit with error code. When `:deciduex, :raise_on_exit` is true (test mode),
  raises instead of halting the VM.
  """
  @spec exit_with_error(integer()) :: no_return()
  def exit_with_error(code \\ 1) do
    if Application.get_env(:deciduex, :raise_on_exit, false) do
      raise "CLI exit with code #{code}"
    else
      System.halt(code)
    end
  end

  alias Deciduex.Commands.Add
  alias Deciduex.Commands.Archaeology
  alias Deciduex.Commands.Audit
  alias Deciduex.Commands.Backup
  alias Deciduex.Commands.CommandLog
  alias Deciduex.Commands.Delete
  alias Deciduex.Commands.Diff
  alias Deciduex.Commands.Doc
  alias Deciduex.Commands.Edges
  alias Deciduex.Commands.Graph
  alias Deciduex.Commands.Init
  alias Deciduex.Commands.Link
  alias Deciduex.Commands.Narratives
  alias Deciduex.Commands.Nodes
  alias Deciduex.Commands.Prompt
  alias Deciduex.Commands.Pulse
  alias Deciduex.Commands.Show
  alias Deciduex.Commands.Status
  alias Deciduex.Commands.Sync
  alias Deciduex.Commands.Unlink
  alias Deciduex.Commands.Update
  alias Deciduex.Commands.Writeup
  alias Deciduex.DB
  alias Deciduex.Repo

  def main(args) do
    # Handle commands that don't require a database first
    case args do
      ["init" | rest] ->
        dispatch_no_db(["init" | rest])

      ["update" | rest] ->
        dispatch_no_db(["update" | rest])

      ["check-update" | _rest] ->
        dispatch_no_db(["check-update"])

      _ ->
        # Commands that require database
        require_db_and_dispatch(args)
    end
  end

  defp require_db_and_dispatch(args) do
    # When called via OTP release `eval`, the application tree isn't started.
    # Ensure Ecto and its dependencies are running before we open the DB.
    {:ok, _} = Application.ensure_all_started(:ecto_sqlite3)

    case DB.find_db_path() do
      {:ok, db_path} ->
        {:ok, _} = Repo.start_link(database: db_path)
        # Ensure all required tables exist (auto-migration)
        :ok = DB.ensure_schema(Repo)
        dispatch(args)

      :error ->
        IO.puts(:stderr, "No .deciduous/deciduous.db found in current or parent directories.")
        IO.puts(:stderr, "Run 'deciduous init' first.")
        exit_with_error()
    end
  end

  # Commands that don't require a database
  defp dispatch_no_db(args) do
    case args do
      ["init" | rest] ->
        opts = parse_init_opts(rest)
        Init.run(opts)

      ["update" | rest] ->
        opts = parse_update_opts(rest)
        Update.run(opts)

      ["check-update"] ->
        check_update()

      _ ->
        print_usage()
    end
  end

  defp parse_init_opts(args), do: parse_init_opts(args, claude: true)

  defp parse_init_opts([], opts), do: opts

  defp parse_init_opts(["--claude" | rest], opts),
    do: parse_init_opts(rest, Keyword.put(opts, :claude, true))

  defp parse_init_opts(["--opencode" | rest], opts),
    do: parse_init_opts(rest, Keyword.put(opts, :opencode, true))

  defp parse_init_opts(["--windsurf" | rest], opts),
    do: parse_init_opts(rest, Keyword.put(opts, :windsurf, true))

  defp parse_init_opts(["--no-workflows" | rest], opts),
    do: parse_init_opts(rest, Keyword.put(opts, :workflows, false))

  defp parse_init_opts([_ | rest], opts), do: parse_init_opts(rest, opts)

  defp parse_update_opts(args), do: parse_update_opts(args, [])

  defp parse_update_opts([], opts), do: opts

  defp parse_update_opts(["--force" | rest], opts),
    do: parse_update_opts(rest, Keyword.put(opts, :force, true))

  defp parse_update_opts([_ | rest], opts), do: parse_update_opts(rest, opts)

  defp check_update do
    version = Application.spec(:deciduex, :vsn) |> to_string()
    IO.puts("Current version: #{version}")
    IO.puts("Check https://hex.pm/packages/deciduex for updates.")
  end

  # Command dispatch table: command name -> {module, arity}
  # :noargs means command takes no arguments, :args means it takes rest of args
  @commands %{
    "add" => {Add, :args},
    "link" => {Link, :args},
    "unlink" => {Unlink, :args},
    "status" => {Status, :args},
    "prompt" => {Prompt, :args},
    "delete" => {Delete, :args},
    "backup" => {Backup, :args},
    "nodes" => {Nodes, :args},
    "edges" => {Edges, :noargs},
    "graph" => {Graph, :noargs},
    "show" => {Show, :args},
    "commands" => {CommandLog, :args},
    "doc" => {Doc, :args},
    "sync" => {Sync, :args},
    "writeup" => {Writeup, :args},
    "diff" => {Diff, :args},
    "audit" => {Audit, :args},
    "pulse" => {Pulse, :args},
    "narratives" => {Narratives, :args},
    "archaeology" => {Archaeology, :args}
  }

  defp dispatch([]), do: print_usage()

  defp dispatch([cmd | rest]) do
    case Map.get(@commands, cmd) do
      {module, :args} -> module.run(rest)
      {module, :noargs} -> module.run()
      nil -> unknown_command(cmd)
    end
  end

  @dialyzer {:nowarn_function, unknown_command: 1}
  defp unknown_command(cmd) do
    IO.puts(:stderr, "Unknown command: #{cmd}")
    exit_with_error()
  end

  defp print_usage do
    IO.puts("Usage: deciduex <command> [options]")
    IO.puts("")
    IO.puts("Commands:")
    IO.puts("  init [options]        Initialize deciduous in current directory")
    IO.puts("  update [options]      Update integration files to latest version")
    IO.puts("  check-update          Check for available updates")
    IO.puts("  add <type> <title>    Create a new decision node")
    IO.puts("  link <from> <to>      Create edge between nodes")
    IO.puts("  unlink <from> <to>    Remove edge between nodes")
    IO.puts("  status <id> <status>  Update node status")
    IO.puts("  prompt <id> [text]    Update node prompt")
    IO.puts("  delete <id>           Delete node and its edges")
    IO.puts("  backup [path]         Create database backup")
    IO.puts("  nodes                 List all decision graph nodes")
    IO.puts("  edges                 List all decision graph edges")
    IO.puts("  graph                 Output full graph as JSON")
    IO.puts("  show <id>             Show detailed node information")
    IO.puts("  commands              Show recent command log")
    IO.puts("  doc <subcommand>      Manage document attachments")
    IO.puts("  sync [output]         Export graph to JSON for static hosting")
    IO.puts("  writeup [options]     Generate PR writeup from graph")
    IO.puts("  diff <subcommand>     Export/apply graph patches for multi-user sync")
    IO.puts("  audit [options]       Audit and maintain graph data quality")
    IO.puts("  pulse [options]       Show graph health and activity")
    IO.puts("  narratives <sub>      Manage evolution narratives")
    IO.puts("  archaeology <sub>     Retroactive graph building")
    IO.puts("")
    IO.puts("Init options:")
    IO.puts("  --claude              Enable Claude Code integration (default)")
    IO.puts("  --opencode            Enable OpenCode integration")
    IO.puts("  --windsurf            Enable Windsurf integration")
    IO.puts("  --no-workflows        Skip GitHub workflow files")
  end
end
