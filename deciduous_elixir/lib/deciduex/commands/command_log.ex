defmodule Deciduex.Commands.CommandLog do
  @moduledoc """
  Implements `deciduex commands` — lists recent command log entries.

  Mirrors the output format of `deciduous commands` from the Rust CLI.
  """

  @default_limit 20

  def run(args \\ []) do
    limit = parse_limit(args)

    commands = Deciduex.Queries.list_recent_commands(limit)

    if commands == [] do
      IO.puts("No commands logged.")
    else
      Enum.each(commands, fn cmd ->
        exit_str =
          if cmd.exit_code do
            to_string(cmd.exit_code)
          else
            "running"
          end

        IO.puts("[#{cmd.started_at}] #{truncate(cmd.command, 60)} (exit: #{exit_str})")
      end)
    end
  end

  defp parse_limit(args) do
    case args do
      [flag, value | _] when flag in ["-l", "--limit"] ->
        case Integer.parse(value) do
          {n, ""} -> n
          _ -> @default_limit
        end

      _ ->
        @default_limit
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
