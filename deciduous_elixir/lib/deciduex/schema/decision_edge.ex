defmodule Deciduex.Schema.DecisionEdge do
  use Ecto.Schema

  @primary_key {:id, :id, autogenerate: false}
  @timestamps_opts false

  schema "decision_edges" do
    field(:from_node_id, :integer)
    field(:to_node_id, :integer)
    field(:from_change_id, :string)
    field(:to_change_id, :string)
    field(:edge_type, :string)
    field(:weight, :float)
    field(:rationale, :string)
    field(:created_at, :string)
  end
end
