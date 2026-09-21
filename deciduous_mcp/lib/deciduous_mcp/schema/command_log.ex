defmodule DeciduousMcp.Schema.CommandLog do
  @moduledoc """
  Audit trail of shell commands executed during development.
  Can be linked to decision nodes for traceability.
  """
  use Ecto.Schema
  import Ecto.Changeset

  @primary_key {:id, :binary_id, autogenerate: true}
  @foreign_key_type :binary_id

  schema "command_log" do
    field :command, :string
    field :description, :string
    field :working_dir, :string
    field :exit_code, :integer
    field :stdout, :string
    field :stderr, :string
    field :duration_ms, :integer
    field :started_at, :utc_datetime_usec
    field :completed_at, :utc_datetime_usec

    belongs_to :workspace, DeciduousMcp.Schema.Workspace
    belongs_to :decision_node, DeciduousMcp.Schema.Node

    timestamps(type: :utc_datetime_usec)
  end

  def changeset(log, attrs) do
    log
    |> cast(attrs, [
      :command, :description, :working_dir, :exit_code, :stdout, :stderr,
      :duration_ms, :started_at, :completed_at, :workspace_id, :decision_node_id
    ])
    |> validate_required([:command, :started_at, :workspace_id])
  end
end
