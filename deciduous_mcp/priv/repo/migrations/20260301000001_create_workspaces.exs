defmodule DeciduousMcp.Repo.Migrations.CreateWorkspaces do
  use Ecto.Migration

  def change do
    create table(:workspaces, primary_key: false) do
      add :id, :binary_id, primary_key: true
      add :name, :string, null: false
      add :description, :text
      add :settings, :map, default: %{}

      timestamps(type: :utc_datetime_usec)
    end

    create unique_index(:workspaces, [:name])
  end
end
