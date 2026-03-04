defmodule Deciduex.Commands.CommandLog do
  @moduledoc """
  Implements `deciduex commands` — lists recent command log entries.

  Mirrors the output format of `deciduous commands` from the Rust CLI.
  """

  alias Deciduex.Queries

  @default_limit 20

  def run(args \\ []) do
    limit = parse_limit(args)

    commands = Queries.list_recent_commands(limit)

    if commands == [] do
      IO.puts("No commands logged.")
    else
      Enum.each(commands, fn cmd ->
        IO.puts(
          "[#{cmd.started_at}] #{truncate(cmd.command, 60)} (exit: #{format_exit_code(cmd.exit_code)})"
        )
      end)
    end
  end

  defp format_exit_code(nil), do: "running"
  defp format_exit_code(code), do: to_string(code)

  defp parse_limit([flag, value | _]) when flag in ["-l", "--limit"] do
    parse_int(value, @default_limit)
  end

  defp parse_limit(_args), do: @default_limit

  defp parse_int(str, default) do
    case Integer.parse(str) do
      {n, ""} -> n
      _ -> default
    end
  end

  defp truncate(str, max_len) do
    if String.length(str) > max_len do
      String.slice(str, 0, max_len - 3) <> "..."
    else
      str
    end
  end
end
