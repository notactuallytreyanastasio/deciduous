defmodule DeciduousMcp.Schema.NodeTheme do
  @moduledoc """
  Join table between nodes and themes.
  Tracks whether the tag was applied manually or by AI.
  """
  use Ecto.Schema
  import Ecto.Changeset

  @primary_key false
  @foreign_key_type :binary_id

  schema "node_themes" do
    field :source, :string, default: "manual"

    belongs_to :node, DeciduousMcp.Schema.Node
    belongs_to :theme, DeciduousMcp.Schema.Theme

    timestamps(type: :utc_datetime_usec, updated_at: false)
  end

  def changeset(node_theme, attrs) do
    node_theme
    |> cast(attrs, [:node_id, :theme_id, :source])
    |> validate_required([:node_id, :theme_id])
    |> validate_inclusion(:source, ~w(manual ai))
    |> unique_constraint([:node_id, :theme_id])
  end
end
