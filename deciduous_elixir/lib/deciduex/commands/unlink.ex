defmodule Deciduex.Commands.Unlink do
  @moduledoc """
  Implements the `unlink` command to remove edges between nodes.

  Usage: deciduex unlink <from_id> <to_id>
  """

  alias Deciduex.Mutations

  def run(args) do
    case parse_args(args) do
      {:ok, from_id, to_id} ->
        delete_link(from_id, to_id)

      {:error, reason} ->
        IO.puts(:stderr, "Error: #{reason}")
        print_usage()
        Deciduex.CLI.exit_with_error()
    end
  end

  defp delete_link(from_id, to_id) do
    case Mutations.delete_edge(from_id, to_id) do
      {:ok, _count} ->
        IO.puts("Deleted edge (#{from_id} -> #{to_id})")
        Mutations.log_command("unlink", [to_string(from_id), to_string(to_id)], 0)

      {:error, :not_found} ->
        IO.puts(:stderr, "Error: No edge found from #{from_id} to #{to_id}")
        Deciduex.CLI.exit_with_error()

      {:error, reason} ->
        IO.puts(:stderr, "Error: #{inspect(reason)}")
        Deciduex.CLI.exit_with_error()
    end
  end

  defp parse_args([]), do: {:error, "Missing from and to node IDs"}
  defp parse_args([_from]), do: {:error, "Missing to node ID"}

  defp parse_args([from_str, to_str | _rest]) do
    with {from_id, ""} <- Integer.parse(from_str),
         {to_id, ""} <- Integer.parse(to_str) do
      {:ok, from_id, to_id}
    else
      :error -> {:error, "Invalid node ID"}
      {_, _} -> {:error, "Invalid node ID format"}
    end
  end

  defp print_usage do
    IO.puts(:stderr, """
    Usage: deciduex unlink <from_id> <to_id>

    Removes the edge between the specified nodes.
    """)
  end
end
