defmodule Deciduex.Commands.Status do
  @moduledoc """
  Implements the `status` command to update a node's status.

  Usage: deciduex status <id> <status>

  Valid statuses: pending, active, superseded, abandoned
  """

  alias Deciduex.Mutations
  alias Deciduex.Queries

  @valid_statuses ~w(pending active superseded abandoned)

  def run(args) do
    case parse_args(args) do
      {:ok, id, status} ->
        update_status(id, status)

      {:error, reason} ->
        IO.puts(:stderr, "Error: #{reason}")
        print_usage()
        Deciduex.CLI.exit_with_error()
    end
  end

  defp update_status(id, status) do
    case Queries.get_node(id) do
      nil ->
        IO.puts(:stderr, "Error: Node ##{id} not found")
        Deciduex.CLI.exit_with_error()

      node ->
        do_update_status(id, status, node)
    end
  end

  defp do_update_status(id, status, node) do
    case Mutations.update_status(id, status) do
      :ok ->
        IO.puts("Updated node #{id} status: #{node.status} -> #{status}")
        Mutations.log_command("status", [to_string(id), status], 0)

      {:error, reason} ->
        IO.puts(:stderr, "Error: #{inspect(reason)}")
        Deciduex.CLI.exit_with_error()
    end
  end

  defp parse_args([]), do: {:error, "Missing node ID and status"}
  defp parse_args([_id]), do: {:error, "Missing status"}

  defp parse_args([id_str, status | _rest]) do
    with {id, ""} <- Integer.parse(id_str),
         true <- status in @valid_statuses do
      {:ok, id, status}
    else
      :error -> {:error, "Invalid node ID: #{id_str}"}
      {_, _} -> {:error, "Invalid node ID: #{id_str}"}
      false -> {:error, "Invalid status: #{status}. Valid: #{Enum.join(@valid_statuses, ", ")}"}
    end
  end

  defp print_usage do
    IO.puts(:stderr, """
    Usage: deciduex status <id> <status>

    Valid statuses: #{Enum.join(@valid_statuses, ", ")}
    """)
  end
end
