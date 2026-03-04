defmodule Deciduex.Schema.DecisionNode do
  use Ecto.Schema

  @primary_key {:id, :id, autogenerate: false}
  @timestamps_opts false
  @derive {Jason.Encoder,
           only: [
             :id,
             :change_id,
             :node_type,
             :title,
             :description,
             :status,
             :created_at,
             :updated_at,
             :metadata_json
           ]}

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
