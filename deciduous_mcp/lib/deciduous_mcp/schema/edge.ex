defmodule DeciduousMcp.Schema.Edge do
  @moduledoc """
  A directed edge in the decision graph, connecting two nodes.

  Edge types convey relationship semantics:
  - `leads_to` — natural progression (default)
  - `chosen` — option that was selected
  - `rejected` — option that was not selected
  - `requires` — dependency
  - `blocks` — prevents progress
  - `enables` — makes something possible
  - `took_from` — a borrow. `from` is the node you took the idea from, `to`
    is your node that used it. This edge crosses branches on purpose: in a
    run with several agents on their own branches it is the only edge that
    does, and it is what turns "who borrowed what" from a regular expression
    over titles into a graph query.
  """
  use Ecto.Schema
  import Ecto.Changeset

  @primary_key {:id, :binary_id, autogenerate: true}
  @foreign_key_type :binary_id

  @edge_types ~w(leads_to requires chosen rejected blocks enables took_from)

  schema "decision_edges" do
    field :edge_type, :string, default: "leads_to"
    field :weight, :float, default: 1.0
    field :rationale, :string

    # For sync compatibility with Rust CLI
    field :from_change_id, :string
    field :to_change_id, :string

    belongs_to :workspace, DeciduousMcp.Schema.Workspace
    belongs_to :from_node, DeciduousMcp.Schema.Node
    belongs_to :to_node, DeciduousMcp.Schema.Node

    timestamps(type: :utc_datetime_usec)
  end

  def changeset(edge, attrs) do
    edge
    |> cast(attrs, [
      :edge_type,
      :weight,
      :rationale,
      :from_node_id,
      :to_node_id,
      :from_change_id,
      :to_change_id,
      :workspace_id
    ])
    |> validate_required([:from_node_id, :to_node_id, :workspace_id])
    |> validate_inclusion(:edge_type, @edge_types)
    |> validate_number(:weight, greater_than_or_equal_to: 0)
    |> foreign_key_constraint(:from_node_id)
    |> foreign_key_constraint(:to_node_id)
    |> unique_constraint([:from_node_id, :to_node_id, :edge_type])
    |> validate_no_self_loop()
  end

  defp validate_no_self_loop(changeset) do
    from = get_field(changeset, :from_node_id)
    to = get_field(changeset, :to_node_id)

    if from && to && from == to do
      add_error(changeset, :to_node_id, "cannot create edge from a node to itself")
    else
      changeset
    end
  end

  def edge_types, do: @edge_types
end
