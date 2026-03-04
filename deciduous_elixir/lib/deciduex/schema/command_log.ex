defmodule Deciduex.Schema.CommandLog do
  use Ecto.Schema

  @primary_key {:id, :id, autogenerate: false}
  @timestamps_opts false

  schema "command_log" do
    field :command, :string
    field :description, :string
    field :working_dir, :string
    field :exit_code, :integer
    field :stdout, :string
    field :stderr, :string
    field :started_at, :string
    field :completed_at, :string
    field :duration_ms, :integer
    field :decision_node_id, :integer
  end
end
