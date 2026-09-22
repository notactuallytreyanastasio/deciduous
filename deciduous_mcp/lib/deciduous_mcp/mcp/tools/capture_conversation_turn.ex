defmodule DeciduousMcp.MCP.Tools.CaptureConversationTurn do
  @moduledoc """
  MCP Tool: capture_conversation_turn

  The primary "always-on" capture tool. Called by the AI assistant after each
  meaningful exchange in a conversation to atomically record the reasoning
  into the decision graph.

  Takes a structured summary of what happened and creates the full node chain
  with edges in a single call. This is the high-level alternative to manually
  calling add_node + add_edge repeatedly.

  ## What it captures

  A conversation turn might produce any combination of:
  - A goal (user asked for something new)
  - Observations (things noticed or learned)
  - Options considered
  - A decision (choice between options)
  - An action taken
  - An outcome (result of the action)

  The tool creates all relevant nodes and connects them with proper edges,
  following the Deciduous graph flow: goal → options → decision → action → outcome.

  ## When to call

  The AI should call this after each substantive exchange — not for every single
  message, but whenever reasoning, decisions, or meaningful work happened.
  """
  use DeciduousMcp.MCP.Component, type: :tool

  alias DeciduousMcp.MCP.Scope
  alias DeciduousMcp.Graph.{Nodes, Edges}

  def definition do
    %{
      name: "capture_conversation_turn",
      description:
        "Capture the reasoning from a conversation exchange into the decision graph. " <>
          "Creates nodes and edges atomically for goals, observations, decisions, actions, " <>
          "and outcomes discussed in the conversation. Call this after each meaningful exchange.",
      input_schema: %{
        type: "object",
        properties: %{
          summary: %{
            type: "string",
            description: "Brief summary of what happened in this conversation turn (1-2 sentences)"
          },
          goal: %{
            type: "object",
            description: "A new goal or objective that emerged (if any)",
            properties: %{
              title: %{type: "string"},
              description: %{type: "string"},
              prompt: %{type: "string", description: "The user's verbatim request"}
            }
          },
          observations: %{
            type: "array",
            description: "Insights, learnings, or things noticed",
            items: %{
              type: "object",
              properties: %{
                title: %{type: "string"},
                description: %{type: "string"}
              },
              required: ["title"]
            }
          },
          options_considered: %{
            type: "array",
            description: "Approaches that were considered",
            items: %{
              type: "object",
              properties: %{
                title: %{type: "string"},
                description: %{type: "string"},
                chosen: %{type: "boolean", description: "Was this option selected?"}
              },
              required: ["title"]
            }
          },
          decision: %{
            type: "object",
            description: "A decision that was made (choosing between options)",
            properties: %{
              title: %{type: "string"},
              rationale: %{type: "string", description: "Why this choice was made"}
            }
          },
          action: %{
            type: "object",
            description: "Something that was done or implemented",
            properties: %{
              title: %{type: "string"},
              description: %{type: "string"},
              files: %{type: "array", items: %{type: "string"}},
              commit: %{type: "string"}
            }
          },
          outcome: %{
            type: "object",
            description: "The result of an action",
            properties: %{
              title: %{type: "string"},
              description: %{type: "string"},
              success: %{type: "boolean"}
            }
          },
          parent_node_id: %{
            type: "string",
            description: "UUID of an existing node to connect this turn to (continues a thread)"
          },
          branch: %{type: "string", description: "Git branch name"},
          confidence: %{type: "integer", minimum: 0, maximum: 100}
        },
        required: ["summary"]
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
    branch = args["branch"]
    confidence = args["confidence"]
    parent_id = args["parent_node_id"]

    created_nodes = []
    # Track the "previous" node for chaining edges
    prev_node_id = parent_id

    try do
      # 1. Create goal if present
      {created_nodes, prev_node_id} =
        if args["goal"] do
          {:ok, node} =
            Nodes.create_node(workspace_id, %{
              node_type: "goal",
              title: args["goal"]["title"],
              description: args["goal"]["description"],
              status: "active",
              metadata: build_metadata(args["goal"]["prompt"], branch, confidence, nil)
            })

          if prev_node_id do
            Edges.create_edge(workspace_id, %{
              from_node_id: prev_node_id,
              to_node_id: node.id,
              edge_type: "leads_to",
              rationale: "Continuation from previous context"
            })
          end

          {[%{id: node.id, type: "goal", title: node.title} | created_nodes], node.id}
        else
          {created_nodes, prev_node_id}
        end

      # 2. Create observations
      {created_nodes, prev_node_id} =
        Enum.reduce(args["observations"] || [], {created_nodes, prev_node_id}, fn obs,
                                                                                    {nodes_acc,
                                                                                     prev} ->
          {:ok, node} =
            Nodes.create_node(workspace_id, %{
              node_type: "observation",
              title: obs["title"],
              description: obs["description"],
              status: "active",
              metadata: build_metadata(nil, branch, confidence, nil)
            })

          if prev do
            Edges.create_edge(workspace_id, %{
              from_node_id: prev,
              to_node_id: node.id,
              edge_type: "leads_to",
              rationale: "Observed during work"
            })
          end

          {[%{id: node.id, type: "observation", title: node.title} | nodes_acc], prev || node.id}
        end)

      # 3. Create options
      option_ids =
        Enum.map(args["options_considered"] || [], fn opt ->
          {:ok, node} =
            Nodes.create_node(workspace_id, %{
              node_type: "option",
              title: opt["title"],
              description: opt["description"],
              status: if(opt["chosen"], do: "completed", else: "rejected"),
              metadata: build_metadata(nil, branch, confidence, nil)
            })

          if prev_node_id do
            Edges.create_edge(workspace_id, %{
              from_node_id: prev_node_id,
              to_node_id: node.id,
              edge_type: "leads_to",
              rationale: "Option considered"
            })
          end

          %{id: node.id, type: "option", title: node.title, chosen: opt["chosen"]}
        end)

      created_nodes = option_ids ++ created_nodes

      # 4. Create decision if present
      {created_nodes, prev_node_id} =
        if args["decision"] do
          {:ok, node} =
            Nodes.create_node(workspace_id, %{
              node_type: "decision",
              title: args["decision"]["title"],
              description: args["decision"]["rationale"],
              status: "completed",
              metadata: build_metadata(nil, branch, confidence, nil)
            })

          # Link options to decision with chosen/rejected edges
          Enum.each(option_ids, fn opt ->
            edge_type = if opt.chosen, do: "chosen", else: "rejected"

            Edges.create_edge(workspace_id, %{
              from_node_id: node.id,
              to_node_id: opt.id,
              edge_type: edge_type,
              rationale: args["decision"]["rationale"]
            })
          end)

          # Link from previous context to decision
          if prev_node_id && Enum.empty?(option_ids) do
            Edges.create_edge(workspace_id, %{
              from_node_id: prev_node_id,
              to_node_id: node.id,
              edge_type: "leads_to"
            })
          end

          {[%{id: node.id, type: "decision", title: node.title} | created_nodes], node.id}
        else
          {created_nodes, prev_node_id}
        end

      # 5. Create action if present
      {created_nodes, prev_node_id} =
        if args["action"] do
          {:ok, node} =
            Nodes.create_node(workspace_id, %{
              node_type: "action",
              title: args["action"]["title"],
              description: args["action"]["description"],
              status: "completed",
              metadata:
                build_metadata(nil, branch, confidence, args["action"]["commit"])
                |> maybe_put("files", args["action"]["files"])
            })

          if prev_node_id do
            Edges.create_edge(workspace_id, %{
              from_node_id: prev_node_id,
              to_node_id: node.id,
              edge_type: "leads_to",
              rationale: "Implementation"
            })
          end

          {[%{id: node.id, type: "action", title: node.title} | created_nodes], node.id}
        else
          {created_nodes, prev_node_id}
        end

      # 6. Create outcome if present
      created_nodes =
        if args["outcome"] do
          status = if args["outcome"]["success"] != false, do: "completed", else: "rejected"

          {:ok, node} =
            Nodes.create_node(workspace_id, %{
              node_type: "outcome",
              title: args["outcome"]["title"],
              description: args["outcome"]["description"],
              status: status,
              metadata: build_metadata(nil, branch, confidence, nil)
            })

          if prev_node_id do
            Edges.create_edge(workspace_id, %{
              from_node_id: prev_node_id,
              to_node_id: node.id,
              edge_type: "leads_to",
              rationale: "Result"
            })
          end

          [%{id: node.id, type: "outcome", title: node.title} | created_nodes]
        else
          created_nodes
        end

      created_nodes = Enum.reverse(created_nodes)

      {:ok,
       Jason.encode!(%{
         summary: args["summary"],
         nodes_created: length(created_nodes),
         nodes: created_nodes,
         message: "Conversation turn captured successfully"
       })}
    rescue
      e ->
        {:error, %{code: -1, message: "Failed to capture turn: #{Exception.message(e)}"}}
    end
  end

  defp build_metadata(prompt, branch, confidence, commit) do
    %{}
    |> maybe_put("prompt", prompt)
    |> maybe_put("branch", branch)
    |> maybe_put("confidence", confidence)
    |> maybe_put("commit", commit)
  end

  defp maybe_put(map, _key, nil), do: map
  defp maybe_put(map, key, value), do: Map.put(map, key, value)
end
