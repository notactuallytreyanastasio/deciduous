defmodule Deciduex.Commands.Delete do
  @moduledoc """
  Implements the `delete` command to remove a node and its edges.

  Usage: deciduex delete <id> [--dry-run]
  """

  alias Deciduex.Mutations
  alias Deciduex.Queries

  def run(args) do
    case parse_args(args) do
      {:ok, id, opts} ->
        delete_node(id, opts)

      {:error, reason} ->
        IO.puts(:stderr, "Error: #{reason}")
        print_usage()
        System.halt(1)
    end
  end

  defp delete_node(id, opts) do
    # Verify node exists first
    case Queries.get_node(id) do
      nil ->
        IO.puts(:stderr, "Error: Node ##{id} not found")
        System.halt(1)

      node ->
        if opts[:dry_run] do
          print_dry_run(id, node)
        else
          do_delete(id, node)
        end
    end
  end

  defp print_dry_run(id, node) do
    # Count edges that would be deleted
    edges = Queries.list_edges()
    incoming = Enum.count(edges, &(&1.to_node_id == id))
    outgoing = Enum.count(edges, &(&1.from_node_id == id))

    IO.puts("Would delete node ##{id}: #{node.title}")
    IO.puts("  Type: #{node.node_type}")
    IO.puts("  Status: #{node.status}")
    IO.puts("  Incoming edges: #{incoming}")
    IO.puts("  Outgoing edges: #{outgoing}")
    IO.puts("")
    IO.puts("Run without --dry-run to actually delete.")
  end

  defp do_delete(id, node) do
    case Mutations.delete_node(id) do
      :ok ->
        IO.puts("Deleted node ##{id}: #{node.title}")
        Mutations.log_command("delete", [to_string(id)], 0)

      {:error, reason} ->
        IO.puts(:stderr, "Error: #{inspect(reason)}")
        System.halt(1)
    end
  end

  defp parse_args([]), do: {:error, "Missing node ID"}

  defp parse_args([id_str | rest]) do
    case Integer.parse(id_str) do
      {id, ""} ->
        opts = parse_opts(rest, %{})
        {:ok, id, opts}

      _ ->
        {:error, "Invalid node ID: #{id_str}"}
    end
  end

  defp parse_opts([], opts), do: opts
  defp parse_opts(["--dry-run" | rest], opts), do: parse_opts(rest, Map.put(opts, :dry_run, true))
  defp parse_opts([_ | rest], opts), do: parse_opts(rest, opts)

  defp print_usage do
    IO.puts(:stderr, """
    Usage: deciduex delete <id> [--dry-run]

    Deletes a node and all its edges.

    Options:
      --dry-run  Show what would be deleted without actually deleting
    """)
  end
end
