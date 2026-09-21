defmodule DeciduousMcp.MCP.Tools.LogDecision do
  @moduledoc """
  MCP Tool: log_decision

  Purpose-built for capturing a decision point. Creates the full decision
  structure atomically: the decision node, all option nodes, and the
  chosen/rejected edges connecting them.

  Call this whenever you choose between approaches, technologies, patterns,
  or any other fork in the road.
  """
  use Hermes.Server.Component, type: :tool

  alias DeciduousMcp.Graph.{Nodes, Edges}

  @impl true
  def definition do
    %{
      name: "log_decision",
      description:
        "Record a decision point with the options that were considered and which was chosen. " <>
          "Creates a decision node, option nodes, and chosen/rejected edges atomically.",
      input_schema: %{
        type: "object",
        properties: %{
          title: %{type: "string", description: "What was being decided (e.g., 'Choose auth strategy')"},
          rationale: %{type: "string", description: "Why the chosen option was selected"},
          chosen_option: %{
            type: "object",
            description: "The option that was selected",
            properties: %{
              title: %{type: "string"},
              description: %{type: "string"}
            },
            required: ["title"]
          },
          rejected_options: %{
            type: "array",
            description: "Options that were considered but not selected",
            items: %{
              type: "object",
              properties: %{
                title: %{type: "string"},
                description: %{type: "string"},
                reason: %{type: "string", description: "Why this was rejected"}
              },
              required: ["title"]
            }
          },
          parent_node_id: %{
            type: "string",
            description: "UUID of the goal or context node this decision belongs to"
          },
          confidence: %{type: "integer", minimum: 0, maximum: 100},
          branch: %{type: "string"}
        },
        required: ["title", "chosen_option"]
      }
    }
  end

  @impl true
  def call(%{arguments: args, server: frame}) do
    workspace_id = frame.assigns.workspace_id
    meta = %{} |> maybe_put("confidence", args["confidence"]) |> maybe_put("branch", args["branch"])

    # Create the decision node
    {:ok, decision} =
      Nodes.create_node(workspace_id, %{
        node_type: "decision",
        title: args["title"],
        description: args["rationale"],
        status: "completed",
        metadata: meta
      })

    # Link to parent if provided
    if args["parent_node_id"] do
      Edges.create_edge(workspace_id, %{
        from_node_id: args["parent_node_id"],
        to_node_id: decision.id,
        edge_type: "leads_to",
        rationale: "Decision point"
      })
    end

    # Create the chosen option
    {:ok, chosen} =
      Nodes.create_node(workspace_id, %{
        node_type: "option",
        title: args["chosen_option"]["title"],
        description: args["chosen_option"]["description"],
        status: "completed",
        metadata: meta
      })

    Edges.create_edge(workspace_id, %{
      from_node_id: decision.id,
      to_node_id: chosen.id,
      edge_type: "chosen",
      rationale: args["rationale"]
    })

    # Create rejected options
    rejected =
      Enum.map(args["rejected_options"] || [], fn opt ->
        {:ok, node} =
          Nodes.create_node(workspace_id, %{
            node_type: "option",
            title: opt["title"],
            description: opt["description"],
            status: "rejected",
            metadata: meta
          })

        Edges.create_edge(workspace_id, %{
          from_node_id: decision.id,
          to_node_id: node.id,
          edge_type: "rejected",
          rationale: opt["reason"]
        })

        %{id: node.id, title: node.title}
      end)

    {:ok,
     Jason.encode!(%{
       decision_id: decision.id,
       chosen: %{id: chosen.id, title: chosen.title},
       rejected: rejected,
       message: "Decision logged: #{args["title"]}"
     })}
  end

  defp maybe_put(map, _key, nil), do: map
  defp maybe_put(map, key, value), do: Map.put(map, key, value)
end
