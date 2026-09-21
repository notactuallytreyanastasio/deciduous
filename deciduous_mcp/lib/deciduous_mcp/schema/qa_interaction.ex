defmodule DeciduousMcp.Schema.QaInteraction do
  @moduledoc """
  Stores Q&A interactions with Claude for searchable history.
  Uses Postgres generated tsvector column for full-text search.
  """
  use Ecto.Schema
  import Ecto.Changeset

  @primary_key {:id, :binary_id, autogenerate: true}
  @foreign_key_type :binary_id

  schema "qa_interactions" do
    field :user_prompt, :string
    field :total_prompt, :string
    field :response, :string
    field :context, :map
    field :deleted_at, :utc_datetime_usec

    belongs_to :workspace, DeciduousMcp.Schema.Workspace

    timestamps(type: :utc_datetime_usec)
  end

  def changeset(qa, attrs) do
    qa
    |> cast(attrs, [:user_prompt, :total_prompt, :response, :context, :workspace_id])
    |> validate_required([:user_prompt, :total_prompt, :response, :workspace_id])
  end
end
