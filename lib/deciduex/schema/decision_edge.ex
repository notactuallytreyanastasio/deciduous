defmodule Deciduex.Schema.DecisionEdge do
  use Ecto.Schema

  @primary_key {:id, :id, autogenerate: false}
  @timestamps_opts false
  @derive {Jason.Encoder,
           only: [
             :id,
             :from_node_id,
             :to_node_id,
             :from_change_id,
             :to_change_id,
             :edge_type,
             :weight,
             :rationale,
             :created_at
           ]}

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
