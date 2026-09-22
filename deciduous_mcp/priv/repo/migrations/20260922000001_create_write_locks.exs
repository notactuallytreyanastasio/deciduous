defmodule DeciduousMcp.Repo.Migrations.CreateWriteLocks do
  use Ecto.Migration

  @moduledoc """
  A short-lived advisory lock so two agents writing to the same workspace
  don't interleave nodes that don't know about each other.

  One row per (workspace, lock_key), composite primary key so acquiring is a
  single indexed upsert — no surrogate id, no second index to maintain.
  `lock_key` is the branch name by default, or the literal `"*"` when a
  workspace is configured for workspace-wide locking instead of per-branch
  (`workspaces.settings["lock_scope"] == "workspace"`) — so the default,
  multi-branch-sessions-anywhere, is two agents on different branches never
  contending, while a workspace that wants stricter isolation can opt in
  without a schema change.

  No cleanup job: `expires_at` is checked on every acquire, so a stale row is
  simply overwritten by the next claimant. The table is bounded by the number
  of workspace/branch pairs that have ever been written to, which for this
  deployment is in the hundreds, not something that needs sweeping.
  """

  def change do
    create table(:write_locks, primary_key: false) do
      add :workspace_id, references(:workspaces, type: :binary_id, on_delete: :delete_all),
        null: false,
        primary_key: true

      add :lock_key, :string, null: false, primary_key: true

      add :session_id, :string, null: false
      add :client_name, :string
      add :client_version, :string

      add :acquired_at, :utc_datetime_usec, null: false
      add :expires_at, :utc_datetime_usec, null: false
    end

    create index(:write_locks, [:expires_at])

    # The one query pattern every write tool now runs, and the one a
    # concurrent-session check runs too: filter a workspace's nodes by branch.
    # GIN jsonb_path_ops (idx_nodes_metadata) accelerates `@>` containment,
    # not `->>'branch' = value` text extraction — this is a different index
    # for a different operator, not a duplicate of it.
    execute(
      "CREATE INDEX idx_nodes_branch ON decision_nodes ((metadata ->> 'branch'))",
      "DROP INDEX idx_nodes_branch"
    )
  end
end
