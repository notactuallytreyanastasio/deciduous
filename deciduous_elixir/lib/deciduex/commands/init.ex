defmodule Deciduex.Commands.Init do
  @moduledoc """
  Initialize deciduous in a project directory.

  Creates .deciduous/ directory, config files, and AI assistant integrations
  for Claude Code, OpenCode, and/or Windsurf.
  """

  alias Deciduex.Templates

  @doc """
  Initialize deciduous in the current directory.

  ## Options
    * `:claude` - Enable Claude Code integration (default: true)
    * `:opencode` - Enable OpenCode integration (default: false)
    * `:windsurf` - Enable Windsurf integration (default: false)
    * `:workflows` - Enable GitHub workflows (default: true)
  """
  def run(opts \\ []) do
    setup_claude = Keyword.get(opts, :claude, true)
    setup_opencode = Keyword.get(opts, :opencode, false)
    setup_windsurf = Keyword.get(opts, :windsurf, false)
    setup_workflows = Keyword.get(opts, :workflows, true)

    cwd = File.cwd!()

    assistant_name = assistant_name(setup_claude, setup_opencode, setup_windsurf)
    IO.puts("\n\e[36m\e[1mInitializing Deciduous for #{assistant_name}...\e[0m")
    IO.puts("   Directory: #{cwd}\n")

    with :ok <- create_deciduous_dir(cwd),
         :ok <- maybe_setup_claude(cwd, setup_claude),
         :ok <- maybe_setup_opencode(cwd, setup_opencode),
         :ok <- maybe_setup_windsurf(cwd, setup_windsurf),
         :ok <- maybe_setup_workflows(cwd, setup_workflows) do
      IO.puts("\n\e[32m\e[1mDone!\e[0m Deciduous initialized successfully.")
      IO.puts("\nNext steps:")
      IO.puts("  1. Run 'deciduous serve' to start the graph viewer")
      IO.puts("  2. Use '/decision add goal \"Your goal\"' to start logging")
      :ok
    end
  end

  defp assistant_name(claude, opencode, windsurf) do
    names =
      [
        if(claude, do: "Claude Code"),
        if(opencode, do: "OpenCode"),
        if(windsurf, do: "Windsurf")
      ]
      |> Enum.filter(& &1)

    case names do
      [] -> "standalone"
      [name] -> name
      _ -> Enum.join(names, " + ")
    end
  end

  # Create .deciduous directory structure
  defp create_deciduous_dir(cwd) do
    deciduous_dir = Path.join(cwd, ".deciduous")
    documents_dir = Path.join(deciduous_dir, "documents")

    create_dir_if_missing(deciduous_dir)
    create_dir_if_missing(documents_dir)

    # Create database
    db_path = Path.join(deciduous_dir, "deciduous.db")
    create_database(db_path)

    # Write config.toml
    config_path = Path.join(deciduous_dir, "config.toml")
    write_file_if_missing(config_path, Templates.get(:default_config), ".deciduous/config.toml")

    # Write version file
    version = Application.spec(:deciduex, :vsn) |> to_string()
    version_path = Path.join(deciduous_dir, ".version")
    File.write!(version_path, version)
    IO.puts("   \e[32mCreating\e[0m .deciduous/.version (#{version})")

    :ok
  end

  defp create_database(db_path) do
    if File.exists?(db_path) do
      IO.puts("   \e[33mSkipping\e[0m .deciduous/deciduous.db (already exists)")
    else
      # Get schema SQL from priv
      schema_path = Path.join(:code.priv_dir(:deciduex), "repo/schema.sql")

      schema_sql =
        if File.exists?(schema_path) do
          File.read!(schema_path)
        else
          # Embedded schema as fallback
          embedded_schema()
        end

      # Create database and run schema
      {:ok, conn} = Exqlite.Sqlite3.open(db_path)

      case Exqlite.Sqlite3.execute(conn, schema_sql) do
        :ok ->
          IO.puts("   \e[32mCreating\e[0m .deciduous/deciduous.db")

        {:error, reason} ->
          IO.puts(:stderr, "   \e[31mError\e[0m creating database: #{inspect(reason)}")
      end

      Exqlite.Sqlite3.close(conn)
    end
  end

  defp embedded_schema do
    """
    CREATE TABLE IF NOT EXISTS decision_nodes (
        id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
        change_id TEXT UNIQUE,
        node_type TEXT NOT NULL,
        title TEXT NOT NULL,
        description TEXT,
        status TEXT NOT NULL DEFAULT 'pending',
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        metadata_json TEXT
    );

    CREATE TABLE IF NOT EXISTS decision_edges (
        id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
        change_id TEXT UNIQUE,
        from_node_id INTEGER NOT NULL,
        to_node_id INTEGER NOT NULL,
        from_change_id TEXT,
        to_change_id TEXT,
        edge_type TEXT NOT NULL,
        weight REAL DEFAULT 1.0,
        rationale TEXT,
        created_at TEXT NOT NULL,
        FOREIGN KEY (from_node_id) REFERENCES decision_nodes(id),
        FOREIGN KEY (to_node_id) REFERENCES decision_nodes(id),
        UNIQUE(from_node_id, to_node_id, edge_type)
    );

    CREATE TABLE IF NOT EXISTS command_log (
        id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
        command TEXT NOT NULL,
        description TEXT,
        working_dir TEXT,
        exit_code INTEGER,
        stdout TEXT,
        stderr TEXT,
        started_at TEXT NOT NULL,
        completed_at TEXT,
        duration_ms INTEGER,
        decision_node_id INTEGER,
        FOREIGN KEY (decision_node_id) REFERENCES decision_nodes(id)
    );

    CREATE TABLE IF NOT EXISTS node_documents (
        id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
        change_id TEXT NOT NULL UNIQUE,
        node_id INTEGER NOT NULL,
        node_change_id TEXT NOT NULL,
        content_hash TEXT NOT NULL,
        original_filename TEXT NOT NULL,
        storage_filename TEXT NOT NULL,
        mime_type TEXT NOT NULL,
        file_size INTEGER NOT NULL,
        description TEXT,
        description_source TEXT NOT NULL DEFAULT 'none',
        attached_at TEXT NOT NULL,
        attached_by TEXT,
        detached_at TEXT,
        FOREIGN KEY (node_id) REFERENCES decision_nodes(id)
    );

    CREATE INDEX IF NOT EXISTS idx_nodes_type ON decision_nodes(node_type);
    CREATE INDEX IF NOT EXISTS idx_nodes_status ON decision_nodes(status);
    CREATE INDEX IF NOT EXISTS idx_nodes_change_id ON decision_nodes(change_id);
    CREATE INDEX IF NOT EXISTS idx_edges_from ON decision_edges(from_node_id);
    CREATE INDEX IF NOT EXISTS idx_edges_to ON decision_edges(to_node_id);
    CREATE INDEX IF NOT EXISTS idx_edges_change_id ON decision_edges(change_id);
    CREATE INDEX IF NOT EXISTS idx_command_started_at ON command_log(started_at);
    CREATE INDEX IF NOT EXISTS idx_docs_node_id ON node_documents(node_id);
    CREATE INDEX IF NOT EXISTS idx_docs_change_id ON node_documents(change_id);
    """
  end

  # Claude Code setup
  defp maybe_setup_claude(_cwd, false), do: :ok

  defp maybe_setup_claude(cwd, true) do
    claude_dir = Path.join([cwd, ".claude", "commands"])
    skills_dir = Path.join([cwd, ".claude", "skills"])
    hooks_dir = Path.join([cwd, ".claude", "hooks"])

    create_dir_if_missing(claude_dir)
    create_dir_if_missing(skills_dir)
    create_dir_if_missing(hooks_dir)

    # Write commands
    for {path, template} <- Templates.claude_commands() do
      full_path = Path.join(cwd, path)
      write_file_if_missing(full_path, Templates.get(template), path)
    end

    # Write skills
    for {path, template} <- Templates.claude_skills() do
      full_path = Path.join(cwd, path)
      write_file_if_missing(full_path, Templates.get(template), path)
    end

    # Write hooks
    for {path, template} <- Templates.claude_hooks() do
      full_path = Path.join(cwd, path)
      write_executable_if_missing(full_path, Templates.get(template), path)
    end

    # Write settings.json
    settings_path = Path.join([cwd, ".claude", "settings.json"])
    write_file_if_missing(settings_path, Templates.get(:settings_json), ".claude/settings.json")

    # Write agents.toml
    agents_path = Path.join([cwd, ".claude", "agents.toml"])
    write_file_if_missing(agents_path, Templates.get(:agents_toml), ".claude/agents.toml")

    # Append to CLAUDE.md
    claude_md_path = Path.join(cwd, "CLAUDE.md")
    append_config_md(claude_md_path, Templates.get(:claude_md_section), "CLAUDE.md")

    :ok
  end

  # OpenCode setup
  defp maybe_setup_opencode(_cwd, false), do: :ok

  defp maybe_setup_opencode(cwd, true) do
    commands_dir = Path.join([cwd, ".opencode", "commands"])
    skills_dir = Path.join([cwd, ".opencode", "skills"])
    plugins_dir = Path.join([cwd, ".opencode", "plugins"])
    agents_dir = Path.join([cwd, ".opencode", "agents"])
    tools_dir = Path.join([cwd, ".opencode", "tools"])

    create_dir_if_missing(commands_dir)
    create_dir_if_missing(skills_dir)
    create_dir_if_missing(plugins_dir)
    create_dir_if_missing(agents_dir)
    create_dir_if_missing(tools_dir)

    # Write commands
    for {path, template} <- Templates.opencode_commands() do
      full_path = Path.join(cwd, path)
      write_file_if_missing(full_path, Templates.get(template), path)
    end

    # Write skills
    for {path, template} <- Templates.opencode_skills() do
      full_path = Path.join(cwd, path)
      write_file_if_missing(full_path, Templates.get(template), path)
    end

    # Write plugins
    plugins = [
      {".opencode/plugins/require-action-node.ts", :opencode_plugin_action},
      {".opencode/plugins/post-commit-reminder.ts", :opencode_plugin_commit}
    ]

    for {path, template} <- plugins do
      full_path = Path.join(cwd, path)
      write_file_if_missing(full_path, Templates.get(template), path)
    end

    # Write agent
    agent_path = Path.join(cwd, ".opencode/agents/deciduous.md")
    write_file_if_missing(agent_path, Templates.get(:opencode_agent), ".opencode/agents/deciduous.md")

    # Write tool
    tool_path = Path.join(cwd, ".opencode/tools/deciduous.ts")
    write_file_if_missing(tool_path, Templates.get(:opencode_tool), ".opencode/tools/deciduous.ts")

    :ok
  end

  # Windsurf setup
  defp maybe_setup_windsurf(_cwd, false), do: :ok

  defp maybe_setup_windsurf(cwd, true) do
    rules_dir = Path.join([cwd, ".windsurf", "rules"])
    hooks_dir = Path.join([cwd, ".windsurf", "hooks"])

    create_dir_if_missing(rules_dir)
    create_dir_if_missing(hooks_dir)

    # Write files
    for {path, template} <- Templates.windsurf_files() do
      full_path = Path.join(cwd, path)

      if String.ends_with?(path, ".sh") do
        write_executable_if_missing(full_path, Templates.get(template), path)
      else
        write_file_if_missing(full_path, Templates.get(template), path)
      end
    end

    :ok
  end

  # GitHub workflows setup
  defp maybe_setup_workflows(_cwd, false), do: :ok

  defp maybe_setup_workflows(cwd, true) do
    workflows_dir = Path.join([cwd, ".github", "workflows"])
    create_dir_if_missing(workflows_dir)

    for {path, template} <- Templates.workflow_files() do
      full_path = Path.join(cwd, path)
      write_file_if_missing(full_path, Templates.get(template), path)
    end

    :ok
  end

  # Helper functions
  defp create_dir_if_missing(path) do
    unless File.exists?(path) do
      File.mkdir_p!(path)
      IO.puts("   \e[32mCreating\e[0m #{Path.relative_to_cwd(path)}/")
    end
  end

  defp write_file_if_missing(path, content, display_path) do
    dir = Path.dirname(path)
    File.mkdir_p!(dir)

    if File.exists?(path) do
      IO.puts("   \e[33mSkipping\e[0m #{display_path} (already exists)")
    else
      File.write!(path, content)
      IO.puts("   \e[32mCreating\e[0m #{display_path}")
    end
  end

  defp write_executable_if_missing(path, content, display_path) do
    dir = Path.dirname(path)
    File.mkdir_p!(dir)

    if File.exists?(path) do
      IO.puts("   \e[33mSkipping\e[0m #{display_path} (already exists)")
    else
      File.write!(path, content)
      File.chmod!(path, 0o755)
      IO.puts("   \e[32mCreating\e[0m #{display_path}")
    end
  end

  defp append_config_md(path, content, display_path) do
    marker_start = "<!-- deciduous:start -->"

    if File.exists?(path) do
      existing = File.read!(path)

      if String.contains?(existing, marker_start) do
        IO.puts("   \e[33mSkipping\e[0m #{display_path} (deciduous section exists)")
      else
        File.write!(path, existing <> "\n" <> content)
        IO.puts("   \e[32mAppending\e[0m deciduous section to #{display_path}")
      end
    else
      File.write!(path, content)
      IO.puts("   \e[32mCreating\e[0m #{display_path}")
    end
  end
end
