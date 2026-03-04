defmodule Deciduex.Commands.Prompt do
  @moduledoc """
  Implements the `prompt` command to update a node's prompt in metadata.

  Usage: deciduex prompt <id> [prompt_text]

  If prompt_text is not provided, reads from stdin.
  """

  alias Deciduex.Mutations
  alias Deciduex.Queries

  def run(args) do
    case parse_args(args) do
      {:ok, id, prompt} ->
        update_prompt(id, prompt)

      {:error, reason} ->
        IO.puts(:stderr, "Error: #{reason}")
        print_usage()
        Deciduex.CLI.exit_with_error()
    end
  end

  defp update_prompt(id, prompt) do
    case Queries.get_node(id) do
      nil ->
        IO.puts(:stderr, "Error: Node ##{id} not found")
        Deciduex.CLI.exit_with_error()

      _node ->
        do_update_prompt(id, prompt)
    end
  end

  defp do_update_prompt(id, prompt) do
    case Mutations.update_prompt(id, prompt) do
      :ok ->
        IO.puts("Updated prompt for node #{id} (#{byte_size(prompt)} chars)")
        Mutations.log_command("prompt", [to_string(id)], 0)

      {:error, reason} ->
        IO.puts(:stderr, "Error: #{inspect(reason)}")
        Deciduex.CLI.exit_with_error()
    end
  end

  defp parse_args([]), do: {:error, "Missing node ID"}

  defp parse_args([id_str]) do
    with {id, ""} <- Integer.parse(id_str),
         prompt when prompt != "" <- read_stdin() do
      {:ok, id, prompt}
    else
      :error -> {:error, "Invalid node ID: #{id_str}"}
      {_, _} -> {:error, "Invalid node ID: #{id_str}"}
      "" -> {:error, "No prompt provided (stdin was empty)"}
    end
  end

  defp parse_args([id_str | rest]) do
    case Integer.parse(id_str) do
      {id, ""} -> {:ok, id, Enum.join(rest, " ")}
      _ -> {:error, "Invalid node ID: #{id_str}"}
    end
  end

  defp read_stdin do
    case IO.read(:stdio, :eof) do
      {:error, _} -> ""
      :eof -> ""
      data -> String.trim(data)
    end
  end

  defp print_usage do
    IO.puts(:stderr, """
    Usage: deciduex prompt <id> [prompt_text]

    Updates the prompt stored in the node's metadata.
    If prompt_text is not provided, reads from stdin.

    Example:
      deciduex prompt 42 "The full user request..."
      echo "Multi-line prompt" | deciduex prompt 42
    """)
  end
end
