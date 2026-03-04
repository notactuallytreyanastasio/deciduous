defmodule Deciduex.Schema.DecisionNode do
  use Ecto.Schema

  @primary_key {:id, :id, autogenerate: false}
  @timestamps_opts false

  schema "decision_nodes" do
    field(:change_id, :string)
    field(:node_type, :string)
    field(:title, :string)
    field(:description, :string)
    field(:status, :string)
    field(:created_at, :string)
    field(:updated_at, :string)
    field(:metadata_json, :string)
  end
end
