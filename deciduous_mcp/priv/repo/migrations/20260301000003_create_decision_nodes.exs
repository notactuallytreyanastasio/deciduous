defmodule DeciduousMcp.Repo.Migrations.CreateDecisionNodes do
  use Ecto.Migration

  def change do
    # Enum type for node_type
    execute(
      "CREATE TYPE node_type AS ENUM ('goal', 'decision', 'option', 'action', 'outcome', 'observation', 'revisit')",
      "DROP TYPE node_type"
    )

    # Enum type for node_status
    execute(
      "CREATE TYPE node_status AS ENUM ('pending', 'active', 'completed', 'rejected', 'superseded', 'abandoned')",
      "DROP TYPE node_status"
    )

    create table(:decision_nodes, primary_key: false) do
      add :id, :binary_id, primary_key: true

      # The change_id from the Rust CLI — preserved for sync compatibility
      add :change_id, :string, null: false
      add :workspace_id, references(:workspaces, type: :binary_id, on_delete: :delete_all),
        null: false
      add :created_by_id, references(:users, type: :binary_id, on_delete: :nilify_all)

      add :node_type, :string, null: false
      add :title, :string, null: false
      add :description, :text
      add :status, :string, null: false, default: "pending"

      # JSONB metadata: confidence, commit, prompt, files, branch
      add :metadata, :map, default: %{}

      # Soft delete
      add :deleted_at, :utc_datetime_usec

      timestamps(type: :utc_datetime_usec)
    end

    create unique_index(:decision_nodes, [:workspace_id, :change_id])
    create index(:decision_nodes, [:workspace_id, :node_type])
    create index(:decision_nodes, [:workspace_id, :status])
    create index(:decision_nodes, [:workspace_id, :inserted_at])
    create index(:decision_nodes, [:deleted_at])

    # GIN index on metadata JSONB for querying by branch, commit, etc.
    execute(
      "CREATE INDEX idx_nodes_metadata ON decision_nodes USING GIN (metadata jsonb_path_ops)",
      "DROP INDEX idx_nodes_metadata"
    )
  end
end
