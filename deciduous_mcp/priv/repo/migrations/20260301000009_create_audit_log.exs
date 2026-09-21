defmodule DeciduousMcp.Repo.Migrations.CreateAuditLog do
  use Ecto.Migration

  def change do
    create table(:audit_log, primary_key: false) do
      add :id, :binary_id, primary_key: true
      add :workspace_id, references(:workspaces, type: :binary_id, on_delete: :delete_all),
        null: false
      add :user_id, references(:users, type: :binary_id, on_delete: :nilify_all)

      # What entity was changed
      add :entity_type, :string, null: false
      add :entity_id, :binary_id, null: false
      add :entity_change_id, :string

      # What happened
      add :action, :string, null: false
      add :changes, :map, default: %{}

      # Source of the change (mcp, cli_sync, manual)
      add :source, :string, null: false, default: "mcp"

      timestamps(type: :utc_datetime_usec, updated_at: false)
    end

    create index(:audit_log, [:workspace_id, :inserted_at])
    create index(:audit_log, [:entity_type, :entity_id])
    create index(:audit_log, [:user_id])
  end
end
