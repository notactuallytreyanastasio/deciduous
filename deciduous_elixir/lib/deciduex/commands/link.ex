defmodule Deciduex.Commands.Link do
  @moduledoc """
  Implements the `link` command to create edges between nodes.

  Usage: deciduex link <from_id> <to_id> [options]

  Options:
    -r, --rationale <text>  Reason for the connection
    -t, --type <type>       Edge type (default: leads_to)
  """

  alias Deciduex.Mutations
  alias Deciduex.Queries

  @valid_edge_types ~w(leads_to chosen rejected)

  def run(args) do
    case parse_args(args) do
      {:ok, from_id, to_id, opts} ->
        create_link(from_id, to_id, opts)

      {:error, reason} ->
        IO.puts(:stderr, "Error: #{reason}")
        print_usage()
        System.halt(1)
    end
  end

  defp create_link(from_id, to_id, opts) do
    edge_type = opts[:type] || "leads_to"
    rationale = opts[:rationale]

    # Validate nodes exist
    case validate_nodes(from_id, to_id) do
      :ok ->
        case Mutations.create_edge(from_id, to_id, edge_type, rationale) do
          {:ok, id} ->
            IO.puts("Created edge #{id} (#{from_id} -> #{to_id} via #{edge_type})")
            log_command(from_id, to_id, opts)

          {:error, reason} ->
            IO.puts(:stderr, "Error: #{inspect(reason)}")
            System.halt(1)
        end

      {:error, reason} ->
        IO.puts(:stderr, "Error: #{reason}")
        System.halt(1)
    end
  end

  defp validate_nodes(from_id, to_id) do
    from_node = Queries.get_node(from_id)
    to_node = Queries.get_node(to_id)

    cond do
      is_nil(from_node) -> {:error, "Node ##{from_id} not found"}
      is_nil(to_node) -> {:error, "Node ##{to_id} not found"}
      true -> :ok
    end
  end

  defp parse_args([]), do: {:error, "Missing from and to node IDs"}
  defp parse_args([_from]), do: {:error, "Missing to node ID"}

  defp parse_args([from_str, to_str | rest]) do
    with {from_id, ""} <- Integer.parse(from_str),
         {to_id, ""} <- Integer.parse(to_str),
         {:ok, opts} <- parse_opts(rest, %{}) do
      {:ok, from_id, to_id, opts}
    else
      :error -> {:error, "Invalid node ID"}
      {_num, _rest} -> {:error, "Invalid node ID format"}
      {:error, reason} -> {:error, reason}
    end
  end

  defp parse_opts([], opts), do: {:ok, opts}

  defp parse_opts(["-r", val | rest], opts) do
    parse_opts(rest, Map.put(opts, :rationale, val))
  end

  defp parse_opts(["--rationale", val | rest], opts) do
    parse_opts(rest, Map.put(opts, :rationale, val))
  end

  defp parse_opts(["-t", val | rest], opts) do
    if val in @valid_edge_types do
      parse_opts(rest, Map.put(opts, :type, val))
    else
      {:error, "Invalid edge type: #{val}. Valid types: #{Enum.join(@valid_edge_types, ", ")}"}
    end
  end

  defp parse_opts(["--type", val | rest], opts) do
    if val in @valid_edge_types do
      parse_opts(rest, Map.put(opts, :type, val))
    else
      {:error, "Invalid edge type: #{val}. Valid types: #{Enum.join(@valid_edge_types, ", ")}"}
    end
  end

  defp parse_opts([unknown | _], _opts) do
    {:error, "Unknown option: #{unknown}"}
  end

  defp log_command(from_id, to_id, opts) do
    args = [to_string(from_id), to_string(to_id)]
    args = if opts[:rationale], do: args ++ ["-r", opts[:rationale]], else: args
    args = if opts[:type], do: args ++ ["-t", opts[:type]], else: args
    Mutations.log_command("link", args, 0)
  end

  defp print_usage do
    IO.puts(:stderr, """
    Usage: deciduex link <from_id> <to_id> [options]

    Options:
      -r, --rationale <text>  Reason for the connection
      -t, --type <type>       Edge type: #{Enum.join(@valid_edge_types, ", ")} (default: leads_to)
    """)
  end
end
