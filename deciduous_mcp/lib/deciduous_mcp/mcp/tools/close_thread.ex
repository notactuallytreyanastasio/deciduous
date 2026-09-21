defmodule DeciduousMcp.MCP.Tools.CloseThread do
  @moduledoc """
  MCP Tool: close_thread

  Wraps up a line of reasoning by creating an outcome node and linking it
  to the action or decision that produced it. Optionally marks the entire
  chain as completed.

  Call this at the end of a conversation when work has concluded, or when
  a particular line of investigation reaches a conclusion.
  """
  use Hermes.Server.Component, type: :tool

  alias DeciduousMcp.Graph.{Nodes, Edges}

  @impl true
  def definition do
    %{
      name: "close_thread",
      description:
        "Close out a reasoning thread with an outcome. Creates an outcome node linked to " <>
          "the parent action/decision, and optionally marks the goal as completed. " <>
          "Call this when a line of work reaches a conclusion.",
      input_schema: %{
        type: "object",
        properties: %{
          title: %{type: "string", description: "Summary of the outcome"},
          description: %{
            type: "string",
            description: "Detailed explanation of what happened and what was learned"
          },
          success: %{
            type: "boolean",
            description: "Did this thread succeed? (true = completed, false = failed/abandoned)"
          },
          parent_node_id: %{
            type: "string",
            description: "UUID of the action or decision that led to this outcome"
          },
          goal_node_id: %{
            type: "string",
            description: "UUID of the root goal to mark as completed (optional)"
          },
          lessons_learned: %{
            type: "array",
            description: "Key takeaways to log as observations",
            items: %{type: "string"}
          },
          next_steps: %{
            type: "array",
            description: "Follow-up goals to create",
            items: %{
              type: "object",
              properties: %{
                title: %{type: "string"},
                description: %{type: "string"}
              },
              required: ["title"]
            }
          },
          branch: %{type: "string"}
        },
        required: ["title"]
      }
    }
  end

  @impl true
  def call(%{arguments: args, server: frame}) do
    workspace_id = frame.assigns.workspace_id
    meta = %{} |> maybe_put("branch", args["branch"])
    status = if args["success"] != false, do: "completed", else: "rejected"

    # Create the outcome node
    {:ok, outcome} =
      Nodes.create_node(workspace_id, %{
        node_type: "outcome",
        title: args["title"],
        description: args["description"],
        status: status,
        metadata: meta
      })

    # Link to parent
    if args["parent_node_id"] do
      Edges.create_edge(workspace_id, %{
        from_node_id: args["parent_node_id"],
        to_node_id: outcome.id,
        edge_type: "leads_to",
        rationale: "Result"
      })
    end

    # Mark goal as completed if requested
    if args["goal_node_id"] && args["success"] != false do
      Nodes.update_node(args["goal_node_id"], %{status: "completed"})
    end

    # Log lessons learned as observations
    lesson_nodes =
      Enum.map(args["lessons_learned"] || [], fn lesson ->
        {:ok, obs} =
          Nodes.create_node(workspace_id, %{
            node_type: "observation",
            title: lesson,
            status: "active",
            metadata: meta
          })

        Edges.create_edge(workspace_id, %{
          from_node_id: outcome.id,
          to_node_id: obs.id,
          edge_type: "leads_to",
          rationale: "Lesson learned"
        })

        %{id: obs.id, title: lesson}
      end)

    # Create follow-up goals
    next_goal_nodes =
      Enum.map(args["next_steps"] || [], fn step ->
        {:ok, goal} =
          Nodes.create_node(workspace_id, %{
            node_type: "goal",
            title: step["title"],
            description: step["description"],
            status: "pending",
            metadata: meta
          })

        Edges.create_edge(workspace_id, %{
          from_node_id: outcome.id,
          to_node_id: goal.id,
          edge_type: "leads_to",
          rationale: "Follow-up from outcome"
        })

        %{id: goal.id, title: step["title"]}
      end)

    {:ok,
     Jason.encode!(%{
       outcome_id: outcome.id,
       status: status,
       lessons_logged: length(lesson_nodes),
       next_goals_created: length(next_goal_nodes),
       next_goals: next_goal_nodes,
       message: "Thread closed: #{args["title"]}"
     })}
  end

  defp maybe_put(map, _key, nil), do: map
  defp maybe_put(map, key, value), do: Map.put(map, key, value)
end
