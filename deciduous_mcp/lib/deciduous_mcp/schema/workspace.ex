defmodule DeciduousMcp.Schema.Workspace do
  @moduledoc """
  A workspace isolates a team's decision graph.
  Each team/project gets its own workspace with independent nodes, edges, and metadata.
  """
  use Ecto.Schema
  import Ecto.Changeset

  @primary_key {:id, :binary_id, autogenerate: true}
  @foreign_key_type :binary_id

  schema "workspaces" do
    field :name, :string
    field :description, :string
    field :settings, :map, default: %{}

    has_many :users, DeciduousMcp.Schema.User
    has_many :nodes, DeciduousMcp.Schema.Node
    has_many :themes, DeciduousMcp.Schema.Theme

    timestamps(type: :utc_datetime_usec)
  end

  def changeset(workspace, attrs) do
    workspace
    |> cast(attrs, [:name, :description, :settings])
    |> validate_required([:name])
    |> unique_constraint(:name)
  end
end
