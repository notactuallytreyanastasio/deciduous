defmodule DeciduousMcp.Repo.Migrations.CreateQaInteractions do
  use Ecto.Migration

  def change do
    create table(:qa_interactions, primary_key: false) do
      add :id, :binary_id, primary_key: true
      add :workspace_id, references(:workspaces, type: :binary_id, on_delete: :delete_all),
        null: false

      add :user_prompt, :text, null: false
      add :total_prompt, :text, null: false
      add :response, :text, null: false
      add :context, :map

      add :deleted_at, :utc_datetime_usec

      timestamps(type: :utc_datetime_usec)
    end

    create index(:qa_interactions, [:workspace_id, :inserted_at])
    create index(:qa_interactions, [:deleted_at])

    # Full-text search using Postgres tsvector
    execute(
      """
      ALTER TABLE qa_interactions
      ADD COLUMN search_vector tsvector
      GENERATED ALWAYS AS (
        setweight(to_tsvector('english', coalesce(user_prompt, '')), 'A') ||
        setweight(to_tsvector('english', coalesce(response, '')), 'B')
      ) STORED
      """,
      "ALTER TABLE qa_interactions DROP COLUMN search_vector"
    )

    execute(
      "CREATE INDEX idx_qa_search ON qa_interactions USING GIN (search_vector)",
      "DROP INDEX idx_qa_search"
    )
  end
end
