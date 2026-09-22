defmodule DeciduousMcp.Schema.Theme do
  @moduledoc """
  A tag/theme that can be applied to nodes for organization.
  Each theme has a name and color for visual identification.
  """
  use Ecto.Schema
  import Ecto.Changeset

  @primary_key {:id, :binary_id, autogenerate: true}
  @foreign_key_type :binary_id

  schema "themes" do
    field :change_id, :string
    field :name, :string
    field :color, :string, default: "#6b7280"
    field :description, :string

    belongs_to :workspace, DeciduousMcp.Schema.Workspace

    many_to_many :nodes, DeciduousMcp.Schema.Node,
      join_through: DeciduousMcp.Schema.NodeTheme

    timestamps(type: :utc_datetime_usec)
  end

  def changeset(theme, attrs) do
    theme
    |> cast(attrs, [:change_id, :name, :color, :description, :workspace_id])
    |> validate_required([:change_id, :name, :workspace_id])
    |> validate_format(:color, ~r/^#[0-9a-fA-F]{6}$/, message: "must be a hex color like #6b7280")
    |> unique_constraint([:workspace_id, :name])
    |> unique_constraint([:workspace_id, :change_id])
  end
end
