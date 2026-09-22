defmodule DeciduousMcp.Repo.Migrations.CreateCommandLog do
  use Ecto.Migration

  def change do
    create table(:command_log, primary_key: false) do
      add :id, :binary_id, primary_key: true
      add :workspace_id, references(:workspaces, type: :binary_id, on_delete: :delete_all),
        null: false
      add :decision_node_id,
          references(:decision_nodes, type: :binary_id, on_delete: :nilify_all)

      add :command, :text, null: false
      add :description, :text
      add :working_dir, :string
      add :exit_code, :integer
      add :stdout, :text
      add :stderr, :text
      add :duration_ms, :integer

      add :started_at, :utc_datetime_usec, null: false
      add :completed_at, :utc_datetime_usec

      timestamps(type: :utc_datetime_usec)
    end

    create index(:command_log, [:workspace_id, :started_at])
    create index(:command_log, [:decision_node_id])
  end
end
