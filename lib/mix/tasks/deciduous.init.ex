defmodule Mix.Tasks.Deciduous.Init do
  @shortdoc "Initialize deciduous in current project"
  @moduledoc """
  Initialize deciduous decision graph in the current project.

  ## Usage

      mix deciduous.init [OPTIONS]

  ## Options

      --force    Overwrite existing files
      --minimal  Skip optional files (just database + config)

  ## What it creates

      .deciduous/
      ├── deciduous.db       # SQLite database
      ├── config.toml        # Configuration
      └── documents/         # Attached files

      .claude/commands/      # Claude Code integration (optional)
      CLAUDE.md updates      # Decision workflow docs (optional)

  ## Example

      cd my-project
      mix deciduous.init

  """

  use Mix.Task

  @impl Mix.Task
  def run(args) do
    Mix.Task.run("app.start")
    Deciduex.CLI.main(["init" | args])
  end
end
