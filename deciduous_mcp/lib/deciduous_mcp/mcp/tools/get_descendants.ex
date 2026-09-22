defmodule DeciduousMcp.MCP.Tools.GetDescendants do
  @moduledoc "MCP Tool: walk the graph forward from a node to find all descendants."
  use DeciduousMcp.MCP.Component, type: :tool

  alias DeciduousMcp.Graph.Query

  def definition do
    %{
      name: "get_descendants",
      description: "Walk the graph forward from a node to find all descendant nodes.",
      input_schema: %{
        type: "object",
        properties: %{
          node_id: %{type: "string", description: "UUID of the starting node"}
        },
        required: ["node_id"]
      }
    }
  end

  def call(%{arguments: %{"node_id" => node_id}}) do
    nodes = Query.descendants(node_id)

    result = %{
      count: length(nodes),
      nodes:
        Enum.map(nodes, fn n ->
          %{id: n.id, node_type: n.node_type, title: n.title}
        end)
    }

    {:ok, Jason.encode!(result)}
  end
end
