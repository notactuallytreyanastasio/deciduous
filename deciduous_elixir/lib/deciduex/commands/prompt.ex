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
        System.halt(1)
    end
  end

  defp update_prompt(id, prompt) do
    # Verify node exists first
    case Queries.get_node(id) do
      nil ->
        IO.puts(:stderr, "Error: Node ##{id} not found")
        System.halt(1)

      _node ->
        case Mutations.update_prompt(id, prompt) do
          :ok ->
            IO.puts("Updated prompt for node #{id} (#{byte_size(prompt)} chars)")
            Mutations.log_command("prompt", [to_string(id)], 0)

          {:error, reason} ->
            IO.puts(:stderr, "Error: #{inspect(reason)}")
            System.halt(1)
        end
    end
  end

  defp parse_args([]), do: {:error, "Missing node ID"}

  defp parse_args([id_str]) do
    # Read prompt from stdin
    case Integer.parse(id_str) do
      {id, ""} ->
        prompt = read_stdin()

        if prompt == "" do
          {:error, "No prompt provided (stdin was empty)"}
        else
          {:ok, id, prompt}
        end

      _ ->
        {:error, "Invalid node ID: #{id_str}"}
    end
  end

  defp parse_args([id_str | rest]) do
    case Integer.parse(id_str) do
      {id, ""} ->
        prompt = Enum.join(rest, " ")
        {:ok, id, prompt}

      _ ->
        {:error, "Invalid node ID: #{id_str}"}
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
