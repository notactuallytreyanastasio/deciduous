defmodule DeciduousMcp.Repo.Migrations.CreateUsers do
  use Ecto.Migration

  def change do
    create table(:users, primary_key: false) do
      add :id, :binary_id, primary_key: true
      add :workspace_id, references(:workspaces, type: :binary_id, on_delete: :delete_all),
        null: false
      add :git_name, :string, null: false
      add :email, :string
      add :display_name, :string

      timestamps(type: :utc_datetime_usec)
    end

    create index(:users, [:workspace_id])
    create unique_index(:users, [:workspace_id, :git_name])
    create index(:users, [:email])
  end
end
