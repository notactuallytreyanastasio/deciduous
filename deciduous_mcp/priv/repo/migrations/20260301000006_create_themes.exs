defmodule DeciduousMcp.Repo.Migrations.CreateThemes do
  use Ecto.Migration

  def change do
    create table(:themes, primary_key: false) do
      add :id, :binary_id, primary_key: true
      add :change_id, :string, null: false
      add :workspace_id, references(:workspaces, type: :binary_id, on_delete: :delete_all),
        null: false

      add :name, :string, null: false
      add :color, :string, null: false, default: "#6b7280"
      add :description, :text

      timestamps(type: :utc_datetime_usec)
    end

    create unique_index(:themes, [:workspace_id, :name])
    create unique_index(:themes, [:workspace_id, :change_id])

    create table(:node_themes, primary_key: false) do
      add :node_id, references(:decision_nodes, type: :binary_id, on_delete: :delete_all),
        null: false
      add :theme_id, references(:themes, type: :binary_id, on_delete: :delete_all),
        null: false
      add :source, :string, null: false, default: "manual"

      timestamps(type: :utc_datetime_usec, updated_at: false)
    end

    create unique_index(:node_themes, [:node_id, :theme_id])
    create index(:node_themes, [:theme_id])
  end
end
