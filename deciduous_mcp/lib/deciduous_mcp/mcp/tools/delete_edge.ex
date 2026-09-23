defmodule DeciduousMcp.MCP.Tools.DeleteEdge do
  @moduledoc "MCP Tool: remove an edge between two nodes."
  use DeciduousMcp.MCP.Component, type: :tool

  alias DeciduousMcp.Graph.Edges
  alias DeciduousMcp.MCP.Scope

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
            enum: DeciduousMcp.Schema.Edge.edge_types(),
            description: "Edge type to remove (default: leads_to)"
          },
          branch: %{
            type: "string",
            description:
              "Git branch name, so this write is locked against others on the same branch (see check_activity)"
          }
        },
        required: ["from_node_id", "to_node_id"]
      }
    }
  end

  def call(%{arguments: args, server: frame}) do
    with :ok <- Scope.check_node(frame, args["to_node_id"]),
         {:ok, _workspace_id} <- Scope.write_scope_for_node(frame, args["from_node_id"], args) do
      do_call(args)
    else
      {:error, message} -> {:error, %{code: -1, message: message}}
    end
  end

  defp do_call(args) do
    edge_type = args["edge_type"] || "leads_to"

    case Edges.delete_edge(args["from_node_id"], args["to_node_id"], edge_type) do
      {:ok, _} -> {:ok, Jason.encode!(%{message: "Edge deleted"})}
      {:error, :not_found} -> {:error, %{code: -1, message: "Edge not found"}}
    end
  end
end
