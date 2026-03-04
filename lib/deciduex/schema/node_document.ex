defmodule Deciduex.Schema.NodeDocument do
  @moduledoc """
  Schema for node_documents table.

  Stores document attachments linked to decision graph nodes.
  Files are stored in .deciduous/documents/ with content-hash naming.
  """

  use Ecto.Schema

  @primary_key {:id, :integer, autogenerate: false}
  @derive {Jason.Encoder,
           only: [
             :id,
             :change_id,
             :node_id,
             :node_change_id,
             :content_hash,
             :original_filename,
             :storage_filename,
             :mime_type,
             :file_size,
             :description,
             :description_source,
             :attached_at,
             :attached_by,
             :detached_at
           ]}

  schema "node_documents" do
    field(:change_id, :string)
    field(:node_id, :integer)
    field(:node_change_id, :string)
    field(:content_hash, :string)
    field(:original_filename, :string)
    field(:storage_filename, :string)
    field(:mime_type, :string)
    field(:file_size, :integer)
    field(:description, :string)
    field(:description_source, :string)
    field(:attached_at, :string)
    field(:attached_by, :string)
    field(:detached_at, :string)
  end
end
