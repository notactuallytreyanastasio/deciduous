defmodule DeciduousMcp.Schema.Document do
  @moduledoc """
  A file attachment on a decision node.
  Documents are stored with content-hash naming for deduplication.
  Supports soft-delete via `detached_at`.
  """
  use Ecto.Schema
  import Ecto.Changeset

  @primary_key {:id, :binary_id, autogenerate: true}
  @foreign_key_type :binary_id

  @description_sources ~w(none user ai)

  schema "node_documents" do
    field :change_id, :string
    field :content_hash, :string
    field :original_filename, :string
    field :storage_filename, :string
    field :mime_type, :string
    field :file_size, :integer
    field :description, :string
    field :description_source, :string, default: "none"
    field :attached_by, :string
    field :detached_at, :utc_datetime_usec

    belongs_to :workspace, DeciduousMcp.Schema.Workspace
    belongs_to :node, DeciduousMcp.Schema.Node

    timestamps(type: :utc_datetime_usec)
  end

  def changeset(document, attrs) do
    document
    |> cast(attrs, [
      :change_id, :content_hash, :original_filename, :storage_filename,
      :mime_type, :file_size, :description, :description_source,
      :attached_by, :node_id, :workspace_id
    ])
    |> validate_required([
      :change_id, :content_hash, :original_filename, :storage_filename,
      :mime_type, :file_size, :node_id, :workspace_id
    ])
    |> validate_inclusion(:description_source, @description_sources)
    |> validate_number(:file_size, greater_than: 0)
    |> unique_constraint([:workspace_id, :change_id])
  end
end
