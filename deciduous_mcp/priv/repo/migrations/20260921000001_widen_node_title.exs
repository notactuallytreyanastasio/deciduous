defmodule DeciduousMcp.Repo.Migrations.WidenNodeTitle do
  use Ecto.Migration

  @moduledoc """
  `add :title, :string` gave the column varchar(255). SQLite imposes no such
  limit, and the graphs on disk have made full use of that: 1,796 of 30,062
  nodes carry a title longer than 255 characters, the longest 4,629.

  Importing them into varchar(255) fails the whole batch with
  `22001 string_data_right_truncation`. Truncating instead would silently
  shorten 6% of every title in the archive, so the column is widened.
  """

  def up do
    alter table(:decision_nodes) do
      modify :title, :text
    end
  end

  def down do
    # Irreversible in practice: going back to varchar(255) would fail on, or
    # silently cut, every title this migration exists to preserve.
    alter table(:decision_nodes) do
      modify :title, :string
    end
  end
end
