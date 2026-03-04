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
        System.halt(1)
    end
  end

  defp update_status(id, status) do
    # Verify node exists first
    case Queries.get_node(id) do
      nil ->
        IO.puts(:stderr, "Error: Node ##{id} not found")
        System.halt(1)

      node ->
        case Mutations.update_status(id, status) do
          :ok ->
            IO.puts("Updated node #{id} status: #{node.status} -> #{status}")
            Mutations.log_command("status", [to_string(id), status], 0)

          {:error, reason} ->
            IO.puts(:stderr, "Error: #{inspect(reason)}")
            System.halt(1)
        end
    end
  end

  defp parse_args([]), do: {:error, "Missing node ID and status"}
  defp parse_args([_id]), do: {:error, "Missing status"}

  defp parse_args([id_str, status | _rest]) do
    case Integer.parse(id_str) do
      {id, ""} ->
        if status in @valid_statuses do
          {:ok, id, status}
        else
          {:error, "Invalid status: #{status}. Valid: #{Enum.join(@valid_statuses, ", ")}"}
        end

      _ ->
        {:error, "Invalid node ID: #{id_str}"}
    end
  end

  defp print_usage do
    IO.puts(:stderr, """
    Usage: deciduex status <id> <status>

    Valid statuses: #{Enum.join(@valid_statuses, ", ")}
    """)
  end
end
