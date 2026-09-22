defmodule DeciduousMcp.MCP.Tools.AddEdge do
  @moduledoc "MCP Tool: create a directed edge between two nodes."
  use DeciduousMcp.MCP.Component, type: :tool

  alias DeciduousMcp.MCP.Scope
  alias DeciduousMcp.Graph.Edges

  def definition do
    %{
      name: "add_edge",
      description:
        "Create a directed edge between two nodes. Equivalent to `deciduous link FROM TO`. " <>
          "Edge types: leads_to (default), chosen, rejected, requires, blocks, enables.",
      input_schema: %{
        type: "object",
        properties: %{
          from_node_id: %{type: "string", description: "UUID of the source node"},
          to_node_id: %{type: "string", description: "UUID of the target node"},
          edge_type: %{
            type: "string",
            enum: ["leads_to", "chosen", "rejected", "requires", "blocks", "enables"],
            description: "Relationship type (default: leads_to)"
          },
          rationale: %{type: "string", description: "Why this connection exists"}
        },
        required: ["from_node_id", "to_node_id"]
      }
    }
    |> Scope.with_workspace_arg()
  end

  def call(%{arguments: args, server: frame}) do
    case Scope.write_workspace_id(frame, args) do
      {:ok, workspace_id} -> do_call(workspace_id, args)
      {:error, message} -> {:error, %{code: -1, message: message}}
    end
  end

  defp do_call(workspace_id, args) do

    attrs = %{
      from_node_id: args["from_node_id"],
      to_node_id: args["to_node_id"],
      edge_type: args["edge_type"] || "leads_to",
      rationale: args["rationale"]
    }

    case Edges.create_edge(workspace_id, attrs) do
      {:ok, edge} ->
        {:ok,
         Jason.encode!(%{
           id: edge.id,
           from_node_id: edge.from_node_id,
           to_node_id: edge.to_node_id,
           edge_type: edge.edge_type,
           message: "Edge created successfully"
         })}

      {:error, {:node_not_found, node_id}} ->
        {:error, %{code: -1, message: "Node not found: #{node_id}"}}

      {:error, reason} ->
        {:error, %{code: -1, message: "Failed to create edge: #{inspect(reason)}"}}
    end
  end
end
