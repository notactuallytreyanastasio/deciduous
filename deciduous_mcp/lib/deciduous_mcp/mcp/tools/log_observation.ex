defmodule DeciduousMcp.MCP.Tools.LogObservation do
  @moduledoc """
  MCP Tool: log_observation

  Quick fire-and-forget tool for capturing insights during a conversation.
  Creates an observation node and optionally links it to a related node.

  Call this whenever you notice something interesting, learn something new,
  or discover a constraint or opportunity.
  """
  use DeciduousMcp.MCP.Component, type: :tool

  alias DeciduousMcp.MCP.Scope
  alias DeciduousMcp.Graph.{Nodes, Edges}

  def definition do
    %{
      name: "log_observation",
      description:
        "Quickly log an insight, learning, or discovery as an observation node. " <>
          "Use this liberally — observations are the connective tissue of the decision graph.",
      input_schema: %{
        type: "object",
        properties: %{
          title: %{type: "string", description: "Short description of what was observed"},
          description: %{
            type: "string",
            description: "Detailed explanation of the observation and its implications"
          },
          related_to: %{
            type: "string",
            description: "UUID of a related node (goal, action, outcome) to link this observation to"
          },
          tags: %{
            type: "array",
            items: %{type: "string"},
            description: "Optional tags for categorization"
          },
          branch: %{type: "string"}
        },
        required: ["title"]
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

    {:ok, node} =
      Nodes.create_node(workspace_id, %{
        node_type: "observation",
        title: args["title"],
        description: args["description"],
        status: "active",
        metadata:
          %{}
          |> maybe_put("branch", args["branch"])
          |> maybe_put("tags", args["tags"])
      })

    if args["related_to"] do
      Edges.create_edge(workspace_id, %{
        from_node_id: args["related_to"],
        to_node_id: node.id,
        edge_type: "leads_to",
        rationale: "Observation from context"
      })
    end

    {:ok,
     Jason.encode!(%{
       id: node.id,
       change_id: node.change_id,
       title: node.title,
       message: "Observation logged"
     })}
  end

  defp maybe_put(map, _key, nil), do: map
  defp maybe_put(map, key, value), do: Map.put(map, key, value)
end
