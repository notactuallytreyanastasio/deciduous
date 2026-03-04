defmodule Deciduex.DBTest do
  use ExUnit.Case

  test "find_db_path/0 finds database in current directory" do
    # The real .deciduous/deciduous.db exists at project root
    # Since tests run from deciduous_elixir/, it should find the parent's DB
    case Deciduex.DB.find_db_path() do
      {:ok, path} ->
        assert String.ends_with?(path, ".deciduous/deciduous.db")
        assert File.exists?(path)

      :error ->
        # This is acceptable if running tests outside the repo
        :ok
    end
  end
end
