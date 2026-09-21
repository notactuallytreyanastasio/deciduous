defmodule DeciduousMcp.MCP.Tools.ShowNode do
  @moduledoc "MCP Tool: get detailed info about a single node with its connections."
  use Hermes.Server.Component, type: :tool

  alias DeciduousMcp.Graph.Nodes

  @impl true
  def definition do
    %{
      name: "show_node",
      description:
        "Get detailed information about a single node, including connected edges, documents, and themes.",
      input_schema: %{
        type: "object",
        properties: %{
          node_id: %{type: "string", description: "UUID of the node to show"}
        },
        required: ["node_id"]
      }
    }
  end

  @impl true
  def call(%{arguments: %{"node_id" => node_id}}) do
    case Nodes.get_node(node_id, [:edges_from, :edges_to, :documents, :themes]) do
      {:ok, node} ->
        result = %{
          id: node.id,
          change_id: node.change_id,
          node_type: node.node_type,
          title: node.title,
          description: node.description,
          status: node.status,
          metadata: node.metadata,
          edges_from:
            Enum.map(node.edges_from, fn e ->
              %{id: e.id, to: e.to_node_id, type: e.edge_type, rationale: e.rationale}
            end),
          edges_to:
            Enum.map(node.edges_to, fn e ->
              %{id: e.id, from: e.from_node_id, type: e.edge_type, rationale: e.rationale}
            end),
          documents:
            Enum.map(node.documents, fn d ->
              %{id: d.id, filename: d.original_filename, mime_type: d.mime_type}
            end),
          themes:
            Enum.map(node.themes, fn t ->
              %{id: t.id, name: t.name, color: t.color}
            end),
          created_at: DateTime.to_iso8601(node.inserted_at)
        }

        {:ok, Jason.encode!(result)}

      {:error, :not_found} ->
        {:error, %{code: -1, message: "Node not found: #{node_id}"}}
    end
  end
end
