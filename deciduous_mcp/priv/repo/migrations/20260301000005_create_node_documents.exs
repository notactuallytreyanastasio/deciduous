defmodule DeciduousMcp.Repo.Migrations.CreateNodeDocuments do
  use Ecto.Migration

  def change do
    create table(:node_documents, primary_key: false) do
      add :id, :binary_id, primary_key: true
      add :change_id, :string, null: false
      add :workspace_id, references(:workspaces, type: :binary_id, on_delete: :delete_all),
        null: false
      add :node_id, references(:decision_nodes, type: :binary_id, on_delete: :delete_all),
        null: false

      add :content_hash, :string, null: false
      add :original_filename, :string, null: false
      add :storage_filename, :string, null: false
      add :mime_type, :string, null: false
      add :file_size, :integer, null: false

      add :description, :text
      add :description_source, :string, null: false, default: "none"

      add :attached_by, :string
      add :detached_at, :utc_datetime_usec

      timestamps(type: :utc_datetime_usec)
    end

    create unique_index(:node_documents, [:workspace_id, :change_id])
    create index(:node_documents, [:node_id])
    create index(:node_documents, [:content_hash])
    create index(:node_documents, [:detached_at])
  end
end
