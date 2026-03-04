defmodule Deciduex.Commands.Update do
  @moduledoc """
  Update deciduous integration files to the latest version.

  Overwrites existing command/skill files while preserving user customizations
  in CLAUDE.md and config files.
  """

  alias Deciduex.Templates

  @doc """
  Update deciduous files in the current directory.

  ## Options
    * `:claude` - Update Claude Code files (default: auto-detect)
    * `:opencode` - Update OpenCode files (default: auto-detect)
    * `:windsurf` - Update Windsurf files (default: auto-detect)
    * `:force` - Overwrite all files without prompting (default: false)
  """
  def run(opts \\ []) do
    cwd = File.cwd!()
    deciduous_dir = Path.join(cwd, ".deciduous")

    unless File.exists?(deciduous_dir) do
      IO.puts("\e[31mError:\e[0m No .deciduous directory found.")
      IO.puts("Run 'deciduex init' first to initialize deciduous.")
      {:error, :not_initialized}
    else
      do_update(cwd, opts)
    end
  end

  defp do_update(cwd, opts) do
    # Auto-detect which integrations are present
    has_claude = File.exists?(Path.join(cwd, ".claude"))
    has_opencode = File.exists?(Path.join(cwd, ".opencode"))
    has_windsurf = File.exists?(Path.join(cwd, ".windsurf"))

    update_claude = Keyword.get(opts, :claude, has_claude)
    update_opencode = Keyword.get(opts, :opencode, has_opencode)
    update_windsurf = Keyword.get(opts, :windsurf, has_windsurf)

    IO.puts("\n\e[36m\e[1mUpdating Deciduous integration files...\e[0m\n")

    with :ok <- maybe_update_claude(cwd, update_claude),
         :ok <- maybe_update_opencode(cwd, update_opencode),
         :ok <- maybe_update_windsurf(cwd, update_windsurf),
         :ok <- update_version_file(cwd) do
      IO.puts("\n\e[32m\e[1mDone!\e[0m Files updated to latest version.")
      :ok
    end
  end

  defp maybe_update_claude(_cwd, false), do: :ok

  defp maybe_update_claude(cwd, true) do
    IO.puts("Updating Claude Code files...")

    # Update commands (always overwrite - these are managed by deciduous)
    for {path, template} <- Templates.claude_commands() do
      full_path = Path.join(cwd, path)
      update_file(full_path, Templates.get(template), path)
    end

    # Update skills
    for {path, template} <- Templates.claude_skills() do
      full_path = Path.join(cwd, path)
      update_file(full_path, Templates.get(template), path)
    end

    :ok
  end

  defp maybe_update_opencode(_cwd, false), do: :ok

  defp maybe_update_opencode(cwd, true) do
    IO.puts("Updating OpenCode files...")

    # Update commands
    for {path, template} <- Templates.opencode_commands() do
      full_path = Path.join(cwd, path)
      update_file(full_path, Templates.get(template), path)
    end

    # Update skills
    for {path, template} <- Templates.opencode_skills() do
      full_path = Path.join(cwd, path)
      update_file(full_path, Templates.get(template), path)
    end

    # Update plugins
    plugins = [
      {".opencode/plugins/require-action-node.ts", :opencode_plugin_action},
      {".opencode/plugins/post-commit-reminder.ts", :opencode_plugin_commit}
    ]

    for {path, template} <- plugins do
      full_path = Path.join(cwd, path)
      update_file(full_path, Templates.get(template), path)
    end

    :ok
  end

  defp maybe_update_windsurf(_cwd, false), do: :ok

  defp maybe_update_windsurf(cwd, true) do
    IO.puts("Updating Windsurf files...")

    for {path, template} <- Templates.windsurf_files() do
      full_path = Path.join(cwd, path)

      if String.ends_with?(path, ".sh") do
        update_executable(full_path, Templates.get(template), path)
      else
        update_file(full_path, Templates.get(template), path)
      end
    end

    :ok
  end

  defp update_version_file(cwd) do
    version = Application.spec(:deciduex, :vsn) |> to_string()
    version_path = Path.join([cwd, ".deciduous", ".version"])
    File.write!(version_path, version)
    IO.puts("   \e[32mUpdated\e[0m .deciduous/.version (#{version})")
    :ok
  end

  defp update_file(path, content, display_path) do
    dir = Path.dirname(path)
    File.mkdir_p!(dir)
    File.write!(path, content)
    IO.puts("   \e[32mUpdated\e[0m #{display_path}")
  end

  defp update_executable(path, content, display_path) do
    dir = Path.dirname(path)
    File.mkdir_p!(dir)
    File.write!(path, content)
    File.chmod!(path, 0o755)
    IO.puts("   \e[32mUpdated\e[0m #{display_path}")
  end
end
