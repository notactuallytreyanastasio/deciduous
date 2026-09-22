defmodule DeciduousMcp.Schema.AuditLog do
  @moduledoc """
  Tracks all changes to the decision graph for auditability.
  Records who changed what, when, and from which source (MCP, CLI sync, manual).
  """
  use Ecto.Schema
  import Ecto.Changeset

  @primary_key {:id, :binary_id, autogenerate: true}
  @foreign_key_type :binary_id

  schema "audit_log" do
    field :entity_type, :string
    field :entity_id, :binary_id
    field :entity_change_id, :string
    field :action, :string
    field :changes, :map, default: %{}
    field :source, :string, default: "mcp"

    belongs_to :workspace, DeciduousMcp.Schema.Workspace
    belongs_to :user, DeciduousMcp.Schema.User

    timestamps(type: :utc_datetime_usec, updated_at: false)
  end

  def changeset(log, attrs) do
    log
    |> cast(attrs, [
      :entity_type, :entity_id, :entity_change_id, :action,
      :changes, :source, :workspace_id, :user_id
    ])
    |> validate_required([:entity_type, :entity_id, :action, :workspace_id])
    |> validate_inclusion(:source, ~w(mcp cli_sync manual))
  end
end
