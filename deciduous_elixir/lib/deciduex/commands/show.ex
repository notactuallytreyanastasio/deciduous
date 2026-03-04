defmodule Deciduex.Commands.Show do
  @moduledoc """
  Implements `deciduex show <id>` — displays detailed node information.

  Mirrors the output format of `deciduous show` from the Rust CLI.
  Supports `--json` flag for JSON output.
  """

  def run(args) do
    case parse_args(args) do
      {:ok, id, opts} ->
        case Deciduex.Queries.get_node(id) do
          nil ->
            IO.puts(:stderr, "Error: Node ##{id} not found")
            System.halt(1)

          node ->
            if opts[:json] do
              render_json(node)
            else
              render_formatted(node)
            end
        end

      :error ->
        IO.puts(:stderr, "Usage: deciduex show <id> [--json]")
        System.halt(1)
    end
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
      {:ok, meta} when meta != %{} ->
        IO.puts("")
        IO.puts("Metadata")

        if confidence = meta["confidence"] do
          IO.puts("  Confidence: #{confidence}%")
        end

        if branch = meta["branch"] do
          IO.puts("  Branch: #{branch}")
        end

        if commit = meta["commit"] do
          IO.puts("  Commit: #{commit}")
        end

        if files = meta["files"] do
          file_list = Enum.join(files, ", ")

          if file_list != "" do
            IO.puts("  Files: #{file_list}")
          end
        end

        if prompt = meta["prompt"] do
          IO.puts("")
          IO.puts("Prompt")

          prompt
          |> String.split("\n")
          |> Enum.each(fn line -> IO.puts("  #{line}") end)
        end

      _ ->
        :ok
    end
  end

  defp render_connections(node_id) do
    {incoming, outgoing} = Deciduex.Queries.get_node_edges(node_id)

    if incoming != [] or outgoing != [] do
      IO.puts("")
      IO.puts("Connections")
    end

    if incoming != [] do
      IO.puts("  Incoming (#{length(incoming)}):")

      Enum.each(incoming, fn edge ->
        rationale = edge.rationale || ""

        if rationale == "" do
          IO.puts("    ##{edge.from_node_id} ─[#{edge.edge_type}]→ here")
        else
          IO.puts("    ##{edge.from_node_id} ─[#{edge.edge_type}]→ here: #{rationale}")
        end
      end)
    end

    if outgoing != [] do
      IO.puts("  Outgoing (#{length(outgoing)}):")

      Enum.each(outgoing, fn edge ->
        rationale = edge.rationale || ""

        if rationale == "" do
          IO.puts("    here ─[#{edge.edge_type}]→ ##{edge.to_node_id}")
        else
          IO.puts("    here ─[#{edge.edge_type}]→ ##{edge.to_node_id}: #{rationale}")
        end
      end)
    end
  end
end
