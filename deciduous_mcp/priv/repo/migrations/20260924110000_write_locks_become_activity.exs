defmodule DeciduousMcp.Repo.Migrations.WriteLocksBecomeActivity do
  use Ecto.Migration

  @moduledoc """
  The branch write lock becomes a record of who wrote where (team probe
  T2, T10; see `DeciduousMcp.Activity`).

  One row per (workspace, branch, session) instead of one per (workspace,
  branch): several sessions may now write a branch at once, and each is
  listed. `expires_at` was the lease's end, ten seconds after the last
  write; it becomes `last_seen_at`, the last write itself.
  """

  def up do
    rename table(:write_locks), to: table(:write_activity)
    rename table(:write_activity), :lock_key, to: :branch
    rename table(:write_activity), :acquired_at, to: :first_seen_at
    rename table(:write_activity), :expires_at, to: :last_seen_at

    execute("UPDATE write_activity SET last_seen_at = last_seen_at - interval '10 seconds'")
    execute("ALTER TABLE write_activity DROP CONSTRAINT write_locks_pkey")
    execute("ALTER TABLE write_activity ADD PRIMARY KEY (workspace_id, branch, session_id)")

    drop index(:write_activity, [:expires_at], name: :write_locks_expires_at_index)
    create index(:write_activity, [:workspace_id, :last_seen_at])
  end

  def down do
    drop index(:write_activity, [:workspace_id, :last_seen_at])

    # One row per branch again: keep each branch's latest writer.
    execute("""
    DELETE FROM write_activity a USING write_activity b
     WHERE a.workspace_id = b.workspace_id AND a.branch = b.branch
       AND (a.last_seen_at, a.session_id) < (b.last_seen_at, b.session_id)
    """)

    execute("ALTER TABLE write_activity DROP CONSTRAINT write_activity_pkey")

    execute(
      "ALTER TABLE write_activity ADD CONSTRAINT write_locks_pkey PRIMARY KEY (workspace_id, branch)"
    )

    execute("UPDATE write_activity SET last_seen_at = last_seen_at + interval '10 seconds'")

    rename table(:write_activity), :last_seen_at, to: :expires_at
    rename table(:write_activity), :first_seen_at, to: :acquired_at
    rename table(:write_activity), :branch, to: :lock_key
    rename table(:write_activity), to: table(:write_locks)
    create index(:write_locks, [:expires_at])
  end
end
