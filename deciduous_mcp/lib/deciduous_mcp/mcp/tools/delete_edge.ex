defmodule DeciduousMcp.MCP.Tools.DeleteEdge do
  @moduledoc "MCP Tool: remove an edge between two nodes."
  use DeciduousMcp.MCP.Component, type: :tool

  alias DeciduousMcp.Graph.Edges

  def definition do
    %{
      name: "delete_edge",
      description: "Remove an edge between two nodes. Equivalent to `deciduous unlink FROM TO`.",
      input_schema: %{
        type: "object",
        properties: %{
          from_node_id: %{type: "string", description: "UUID of the source node"},
          to_node_id: %{type: "string", description: "UUID of the target node"},
          edge_type: %{
            type: "string",
            enum: ["leads_to", "chosen", "rejected", "requires", "blocks", "enables"],
            description: "Edge type to remove (default: leads_to)"
          }
        },
        required: ["from_node_id", "to_node_id"]
      }
    }
  end

  def call(%{arguments: args}) do
    edge_type = args["edge_type"] || "leads_to"

    case Edges.delete_edge(args["from_node_id"], args["to_node_id"], edge_type) do
      {:ok, _} -> {:ok, Jason.encode!(%{message: "Edge deleted"})}
      {:error, :not_found} -> {:error, %{code: -1, message: "Edge not found"}}
    end
  end
end
