defmodule DeciduousMcp.Repo.Migrations.CreateDocumentBlobs do
  use Ecto.Migration

  @moduledoc """
  Document content, stored in Postgres.

  Blobs live in their own table keyed by content hash rather than in a column
  on `node_documents`, for two reasons:

    * The CLI already names files by content hash, and the same file really is
      attached in several places — `deep-squishing-sparrow.md` is referenced by
      two projects. Keying on the hash stores those bytes once across all 91
      workspaces instead of once per attachment.
    * A row in `node_documents` can outlive its content. Five documents on this
      machine are referenced by live rows whose files are already gone. A
      separate table lets the metadata import without inventing bytes for it.

  There is no foreign key from node_documents.content_hash to here, precisely
  so those five rows can exist without content.
  """

  def change do
    create table(:document_blobs, primary_key: false) do
      # sha256 of the content, 64 hex characters — verified server-side on
      # upload rather than taken on trust from the client.
      add :content_hash, :string, size: 64, primary_key: true
      add :content, :binary, null: false
      add :byte_size, :bigint, null: false
      add :mime_type, :string

      timestamps(type: :utc_datetime_usec, updated_at: false)
    end

    alter table(:node_documents) do
      # Set at import when no bytes could be found for this row's hash. Kept
      # explicit so a fetch can answer "this is gone" instead of "not found",
      # which are different problems.
      add :content_missing, :boolean, null: false, default: false

      # Which backend holds the bytes. Only "postgres" today; object storage
      # would add a value here rather than a migration.
      add :storage, :string, null: false, default: "postgres"
    end

    create index(:node_documents, [:content_missing])
  end
end
