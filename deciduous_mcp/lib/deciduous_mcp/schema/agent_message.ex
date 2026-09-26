defmodule DeciduousMcp.Schema.AgentMessage do
  @moduledoc """
  One post on a workspace's message board (see `DeciduousMcp.Board`).
  Not part of the graph: never exported, imported or synced.
  """
  use Ecto.Schema

  @primary_key {:id, :id, autogenerate: true}
  @foreign_key_type :binary_id

  schema "agent_messages" do
    field :branch, :string
    field :author, :string
    field :subject, :string
    field :body, :string
    field :mentions, {:array, :string}, default: []
    field :reply_to, :integer
    field :created_at, :utc_datetime_usec, read_after_writes: true

    belongs_to :workspace, DeciduousMcp.Schema.Workspace
  end
end
