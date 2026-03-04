defmodule Deciduex.Commands.Show do
  @moduledoc """
  Implements `deciduex show <id>` — displays detailed node information.

  Mirrors the output format of `deciduous show` from the Rust CLI.
  Supports `--json` flag for JSON output.
  """

  alias Deciduex.Queries

  def run(args) do
    case parse_args(args) do
      {:ok, id, %{json: true}} -> show_node(id, :json)
      {:ok, id, _opts} -> show_node(id, :formatted)
      :error -> usage_error()
    end
  end

  defp show_node(id, format) do
    case Queries.get_node(id) do
      nil -> node_not_found(id)
      node -> render(node, format)
    end
  end

  defp render(node, :json), do: render_json(node)
  defp render(node, :formatted), do: render_formatted(node)

  @spec node_not_found(integer()) :: no_return()
  defp node_not_found(id) do
    IO.puts(:stderr, "Error: Node ##{id} not found")
    Deciduex.CLI.exit_with_error()
  end

  @spec usage_error() :: no_return()
  defp usage_error do
    IO.puts(:stderr, "Usage: deciduex show <id> [--json]")
    Deciduex.CLI.exit_with_error()
  end

  defp parse_args(args) do
    {id_str, rest} =
      case args do
        [id | rest] -> {id, rest}
        [] -> {nil, []}
      end

    with id_str when is_binary(id_str) <- id_str,
         {id, ""} <- Integer.parse(id_str) do
      json? = "--json" in rest
      {:ok, id, %{json: json?}}
    else
      _ -> :error
    end
  end

  defp render_json(node) do
    map = %{
      "id" => node.id,
      "change_id" => node.change_id,
      "node_type" => node.node_type,
      "title" => node.title,
      "description" => node.description,
      "status" => node.status,
      "created_at" => node.created_at,
      "updated_at" => node.updated_at,
      "metadata_json" => node.metadata_json
    }

    IO.puts(Jason.encode!(map, pretty: true))
  end

  defp render_formatted(node) do
    IO.puts("")
    IO.puts("Node ##{node.id} #{node.node_type}")
    IO.puts(String.duplicate("─", 60))
    IO.puts("Title: #{node.title}")

    if node.description do
      IO.puts("Description: #{node.description}")
    end

    IO.puts("Status: #{node.status}")
    IO.puts("Created: #{node.created_at}")
    IO.puts("Updated: #{node.updated_at}")

    render_metadata(node.metadata_json)
    render_connections(node.id)

    IO.puts("")
  end

  defp render_metadata(nil), do: :ok

  defp render_metadata(metadata_json) do
    case Jason.decode(metadata_json) do
      {:ok, meta} when meta != %{} -> render_meta_fields(meta)
      _ -> :ok
    end
  end

  defp render_meta_fields(meta) do
    IO.puts("")
    IO.puts("Metadata")
    render_meta_field("Confidence", meta["confidence"], &"#{&1}%")
    render_meta_field("Branch", meta["branch"])
    render_meta_field("Commit", meta["commit"])
    render_meta_files(meta["files"])
    render_prompt(meta["prompt"])
  end

  defp render_meta_field(_label, nil), do: :ok
  defp render_meta_field(label, value), do: IO.puts("  #{label}: #{value}")

  defp render_meta_field(_label, nil, _formatter), do: :ok
  defp render_meta_field(label, value, formatter), do: IO.puts("  #{label}: #{formatter.(value)}")

  defp render_meta_files(nil), do: :ok

  defp render_meta_files(files) do
    case Enum.join(files, ", ") do
      "" -> :ok
      file_list -> IO.puts("  Files: #{file_list}")
    end
  end

  defp render_prompt(nil), do: :ok

  defp render_prompt(prompt) do
    IO.puts("")
    IO.puts("Prompt")
    prompt |> String.split("\n") |> Enum.each(&IO.puts("  #{&1}"))
  end

  defp render_connections(node_id) do
    {incoming, outgoing} = Queries.get_node_edges(node_id)

    if incoming != [] or outgoing != [] do
      IO.puts("")
      IO.puts("Connections")
    end

    render_edge_group("Incoming", incoming, &format_incoming_edge/1)
    render_edge_group("Outgoing", outgoing, &format_outgoing_edge/1)
  end

  defp render_edge_group(_label, [], _formatter), do: :ok

  defp render_edge_group(label, edges, formatter) do
    IO.puts("  #{label} (#{length(edges)}):")
    Enum.each(edges, fn edge -> IO.puts("    #{formatter.(edge)}") end)
  end

  defp format_incoming_edge(edge) do
    base = "##{edge.from_node_id} ─[#{edge.edge_type}]→ here"
    append_rationale(base, edge.rationale)
  end

  defp format_outgoing_edge(edge) do
    base = "here ─[#{edge.edge_type}]→ ##{edge.to_node_id}"
    append_rationale(base, edge.rationale)
  end

  defp append_rationale(base, nil), do: base
  defp append_rationale(base, ""), do: base
  defp append_rationale(base, rationale), do: "#{base}: #{rationale}"
end
