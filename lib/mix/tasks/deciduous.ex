defmodule Mix.Tasks.Deciduous do
  @shortdoc "Decision graph tooling for AI-assisted development"
  @moduledoc """
  Decision graph CLI.

  ## Usage

      mix deciduous COMMAND [ARGS]

  ## Commands

      mix deciduous init          Initialize deciduous in current project
      mix deciduous add TYPE TITLE  Add a node (goal/decision/action/outcome/etc)
      mix deciduous link FROM TO    Create edge between nodes
      mix deciduous nodes           List all nodes
      mix deciduous edges           List all edges
      mix deciduous graph           Export full graph as JSON
      mix deciduous show ID         Show node details
      mix deciduous serve           Start web viewer

  ## Examples

      mix deciduous init
      mix deciduous add goal "Implement feature X" -c 90
      mix deciduous link 1 2 -r "leads to"
      mix deciduous nodes --type goal
      mix deciduous serve --port 4000

  ## Options

  Run `mix deciduous COMMAND --help` for command-specific options.
  """

  use Mix.Task

  @impl Mix.Task
  def run(args) do
    # Start the application (needed for Ecto/Repo)
    Mix.Task.run("app.start")

    # Delegate to CLI
    Deciduex.CLI.main(args)
  end
end
