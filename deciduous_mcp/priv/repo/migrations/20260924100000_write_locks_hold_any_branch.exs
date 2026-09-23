defmodule DeciduousMcp.Repo.Migrations.WriteLocksHoldAnyBranch do
  use Ecto.Migration

  @moduledoc """
  `lock_key` (the branch), `client_name` and `client_version` were
  varchar(255). A 256-character branch, or a client whose clientInfo name
  was that long, made the lock upsert fail, and every write from it with
  "add_node failed (MatchError)" (SERVER-N2). The branch is bounded where
  it enters (512 characters, `DeciduousMcp.MCP.ArgCheck`) and so is
  clientInfo (255, at initialize); these columns no longer impose a second,
  smaller limit of their own.
  """

  def up do
    alter table(:write_locks) do
      modify :lock_key, :text, from: :string
      modify :client_name, :text, from: :string
      modify :client_version, :text, from: :string
    end
  end

  def down do
    alter table(:write_locks) do
      modify :lock_key, :string, from: :text
      modify :client_name, :string, from: :text
      modify :client_version, :string, from: :text
    end
  end
end
