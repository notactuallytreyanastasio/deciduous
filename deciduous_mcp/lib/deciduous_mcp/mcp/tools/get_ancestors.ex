defmodule DeciduousMcp.MCP.Tools.GetAncestors do
  @moduledoc "MCP Tool: walk the graph backward from a node to find all ancestors."
  use DeciduousMcp.MCP.Component, type: :tool

  alias DeciduousMcp.Graph.Query
  alias DeciduousMcp.MCP.Scope

  def definition do
    %{
      name: "get_ancestors",
      description: "Walk the graph backward from a node to find all ancestor nodes.",
      input_schema: %{
        type: "object",
        properties: %{
          node_id: %{type: "string", description: "UUID of the starting node"},
          max_depth: %{
            type: "integer",
            minimum: 1,
            maximum: 200,
            description: "Levels to walk (default 50)"
          },
          max_nodes: %{
            type: "integer",
            minimum: 1,
            maximum: 5000,
            description: "Stop after this many nodes (default 1000)"
          }
        },
        required: ["node_id"]
      }
    }
  end

  def call(%{arguments: %{"node_id" => node_id} = args, server: frame}) do
    case Scope.read_node(frame, node_id) do
      {:ok, _node} -> walk(node_id, args)
      {:error, message} -> {:error, %{code: -1, message: message}}
    end
  end

  defp walk(node_id, args) do
    {nodes, truncated?} =
      Query.ancestors_bounded(node_id, max_depth: args["max_depth"], max_nodes: args["max_nodes"])

    result = %{
      count: length(nodes),
      # Without this the caller cannot tell a small subtree from a cut one.
      truncated: truncated?,
      nodes:
        Enum.map(nodes, fn n ->
          %{id: n.id, node_type: n.node_type, title: n.title}
        end)
    }

    {:ok, Jason.encode!(result)}
  end
end
