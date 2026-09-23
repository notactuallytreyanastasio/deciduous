defmodule DeciduousMcp.Repo.Migrations.CreateAppliedOps do
  use Ecto.Migration

  @moduledoc """
  The ids of CLI operations this server has already applied.

  The CLI keeps a local log of every write and replays the unacknowledged
  tail to `POST /ops`. An ack can be lost after the server committed (the
  connection drops, the laptop sleeps), and the CLI then sends the op again.
  Re-applying "set status to completed" an hour later would undo whatever an
  agent set in between, so the op id is recorded in the same transaction as
  the change and a second arrival is answered "duplicate".

  Kept forever. A row is a uuid and a timestamp; pruning would reopen the
  window in which a very late replay re-applies an old edit, and there is no
  point at which the server can know a CLI will never replay again.
  """

  def change do
    create table(:applied_ops, primary_key: false) do
      add :workspace_id, references(:workspaces, type: :binary_id, on_delete: :delete_all),
        null: false,
        primary_key: true

      add :op_id, :string, null: false, primary_key: true
      add :kind, :string, null: false
      add :applied_at, :utc_datetime_usec, null: false
    end
  end
end
