defmodule DeciduousMcp.Schema.User do
  @moduledoc """
  Maps git identities to workspace members.
  When the CLI syncs events, the author field is matched to a user via git_name.
  """
  use Ecto.Schema
  import Ecto.Changeset

  @primary_key {:id, :binary_id, autogenerate: true}
  @foreign_key_type :binary_id

  schema "users" do
    field :git_name, :string
    field :email, :string
    field :display_name, :string

    belongs_to :workspace, DeciduousMcp.Schema.Workspace

    timestamps(type: :utc_datetime_usec)
  end

  def changeset(user, attrs) do
    user
    |> cast(attrs, [:git_name, :email, :display_name, :workspace_id])
    |> validate_required([:git_name, :workspace_id])
    |> unique_constraint([:workspace_id, :git_name])
  end
end
