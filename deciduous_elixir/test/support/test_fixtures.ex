defmodule Deciduex.TestFixtures do
  @moduledoc """
  Shared test fixtures for creating deciduous database tables and test data.

  Since the schemas are read-only (Rust manages writes), we use raw SQL
  to create tables and insert test data.
  """

  alias Deciduex.Repo
  alias Ecto.Adapters.SQL

  def create_tables! do
    # Drop and recreate to ensure clean state between tests
    SQL.query!(Repo, "DROP TABLE IF EXISTS command_log")
    SQL.query!(Repo, "DROP TABLE IF EXISTS decision_edges")
    SQL.query!(Repo, "DROP TABLE IF EXISTS decision_nodes")

    SQL.query!(Repo, """
    CREATE TABLE decision_nodes (
      id INTEGER PRIMARY KEY,
      change_id TEXT NOT NULL,
      node_type TEXT NOT NULL,
      title TEXT NOT NULL,
      description TEXT,
      status TEXT NOT NULL DEFAULT 'active',
      created_at TEXT NOT NULL,
      updated_at TEXT NOT NULL,
      metadata_json TEXT
    )
    """)

    SQL.query!(Repo, """
    CREATE TABLE decision_edges (
      id INTEGER PRIMARY KEY,
      from_node_id INTEGER NOT NULL,
      to_node_id INTEGER NOT NULL,
      from_change_id TEXT,
      to_change_id TEXT,
      edge_type TEXT NOT NULL DEFAULT 'leads_to',
      weight REAL,
      rationale TEXT,
      created_at TEXT NOT NULL
    )
    """)

    SQL.query!(Repo, """
    CREATE TABLE command_log (
      id INTEGER PRIMARY KEY,
      command TEXT NOT NULL,
      description TEXT,
      working_dir TEXT,
      exit_code INTEGER,
      stdout TEXT,
      stderr TEXT,
      started_at TEXT NOT NULL,
      completed_at TEXT,
      duration_ms INTEGER,
      decision_node_id INTEGER
    )
    """)
  end

  def insert_node!(attrs) do
    defaults = %{
      change_id: "test-#{attrs[:id]}",
      description: nil,
      status: "active",
      updated_at: attrs[:created_at],
      metadata_json: nil
    }

    row = Map.merge(defaults, attrs)

    SQL.query!(
      Repo,
      """
      INSERT INTO decision_nodes (id, change_id, node_type, title, description, status, created_at, updated_at, metadata_json)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
      """,
      [
        row.id,
        row.change_id,
        row.node_type,
        row.title,
        row.description,
        row.status,
        row.created_at,
        row.updated_at,
        row.metadata_json
      ]
    )
  end

  def insert_edge!(attrs) do
    defaults = %{
      from_change_id: nil,
      to_change_id: nil,
      edge_type: "leads_to",
      weight: nil,
      rationale: nil,
      created_at: "2024-01-01T00:00:00Z"
    }

    row = Map.merge(defaults, attrs)

    SQL.query!(
      Repo,
      """
      INSERT INTO decision_edges (id, from_node_id, to_node_id, from_change_id, to_change_id, edge_type, weight, rationale, created_at)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
      """,
      [
        row.id,
        row.from_node_id,
        row.to_node_id,
        row.from_change_id,
        row.to_change_id,
        row.edge_type,
        row.weight,
        row.rationale,
        row.created_at
      ]
    )
  end

  def insert_command!(attrs) do
    defaults = %{
      description: nil,
      working_dir: nil,
      exit_code: 0,
      stdout: nil,
      stderr: nil,
      completed_at: nil,
      duration_ms: nil,
      decision_node_id: nil
    }

    row = Map.merge(defaults, attrs)

    SQL.query!(
      Repo,
      """
      INSERT INTO command_log (id, command, description, working_dir, exit_code, stdout, stderr, started_at, completed_at, duration_ms, decision_node_id)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
      """,
      [
        row.id,
        row.command,
        row.description,
        row.working_dir,
        row.exit_code,
        row.stdout,
        row.stderr,
        row.started_at,
        row.completed_at,
        row.duration_ms,
        row.decision_node_id
      ]
    )
  end

  def seed_sample_data! do
    insert_node!(%{
      id: 1,
      node_type: "goal",
      title: "Add auth",
      created_at: "2024-01-01T00:00:00Z",
      metadata_json: Jason.encode!(%{"branch" => "main", "confidence" => 90})
    })

    insert_node!(%{
      id: 2,
      node_type: "option",
      title: "Use JWT",
      created_at: "2024-01-02T00:00:00Z",
      metadata_json: Jason.encode!(%{"branch" => "feature-auth"})
    })

    insert_node!(%{
      id: 3,
      node_type: "decision",
      title: "Choose JWT",
      created_at: "2024-01-03T00:00:00Z",
      metadata_json: Jason.encode!(%{"branch" => "feature-auth"})
    })

    insert_edge!(%{
      id: 1,
      from_node_id: 1,
      to_node_id: 2,
      edge_type: "leads_to",
      rationale: "possible approach",
      created_at: "2024-01-01T00:00:00Z"
    })

    insert_edge!(%{
      id: 2,
      from_node_id: 2,
      to_node_id: 3,
      edge_type: "chosen",
      rationale: "JWT is standard",
      created_at: "2024-01-02T00:00:00Z"
    })

    insert_command!(%{
      id: 1,
      command: ~s(deciduous add goal "Add auth" -c 90),
      started_at: "2024-01-01T10:30:00Z",
      exit_code: 0
    })

    insert_command!(%{
      id: 2,
      command: ~s(deciduous add option "Use JWT"),
      started_at: "2024-01-02T10:30:00Z",
      exit_code: 0
    })
  end
end
