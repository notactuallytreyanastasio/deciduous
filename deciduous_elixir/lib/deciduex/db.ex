defmodule Deciduex.DB do
  @moduledoc """
  Database path discovery for the deciduous SQLite database.

  Searches from cwd upward for `.deciduous/deciduous.db`.
  """

  @db_filename ".deciduous/deciduous.db"

  @doc """
  Finds the path to the deciduous SQLite database by walking up from cwd.
  Returns `{:ok, path}` or `:error` if not found.
  """
  def find_db_path do
    find_db_path(File.cwd!())
  end

  defp find_db_path("/") do
    :error
  end

  defp find_db_path(dir) do
    candidate = Path.join(dir, @db_filename)

    if File.exists?(candidate) do
      {:ok, candidate}
    else
      find_db_path(Path.dirname(dir))
    end
  end
end
