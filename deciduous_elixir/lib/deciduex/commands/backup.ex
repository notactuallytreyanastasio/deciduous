defmodule Deciduex.Commands.Backup do
  @moduledoc """
  Implements the `backup` command to create a database backup.

  Usage: deciduex backup [output_path]

  Default output: deciduous_backup_YYYYMMDD_HHMMSS.db
  """

  alias Deciduex.DB

  def run(args) do
    case DB.find_db_path() do
      {:ok, db_path} ->
        output_path = get_output_path(args)
        create_backup(db_path, output_path)

      :error ->
        IO.puts(:stderr, "Error: No .deciduous/deciduous.db found")
        System.halt(1)
    end
  end

  defp get_output_path([]), do: default_backup_name()
  defp get_output_path([path | _]), do: path

  defp default_backup_name do
    {{year, month, day}, {hour, minute, second}} = :calendar.local_time()

    timestamp =
      :io_lib.format("~4..0B~2..0B~2..0B_~2..0B~2..0B~2..0B", [
        year,
        month,
        day,
        hour,
        minute,
        second
      ])
      |> IO.iodata_to_binary()

    "deciduous_backup_#{timestamp}.db"
  end

  defp create_backup(source, dest) do
    case File.cp(source, dest) do
      :ok ->
        {:ok, stat} = File.stat(dest)
        size_kb = div(stat.size, 1024)
        IO.puts("Created backup: #{dest} (#{size_kb} KB)")

      {:error, reason} ->
        IO.puts(:stderr, "Error creating backup: #{inspect(reason)}")
        System.halt(1)
    end
  end
end
