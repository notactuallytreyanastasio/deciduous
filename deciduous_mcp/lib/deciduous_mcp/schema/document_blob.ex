defmodule DeciduousMcp.Schema.DocumentBlob do
  @moduledoc """
  Raw content for a document, keyed by its sha256.

  Deliberately separate from `DeciduousMcp.Schema.Document`: several
  attachments across several workspaces can share one blob, and a document row
  can exist with no blob at all when the file was lost before import.
  """
  use Ecto.Schema

  @primary_key {:content_hash, :string, autogenerate: false}

  schema "document_blobs" do
    field :content, :binary
    field :byte_size, :integer
    field :mime_type, :string

    timestamps(type: :utc_datetime_usec, updated_at: false)
  end
end
