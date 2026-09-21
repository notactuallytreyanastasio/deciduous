defmodule DeciduousMcp.MCP.Tools.AddNode do
  @moduledoc """
  MCP Tool: add_node

  Creates a new node in the decision graph. Equivalent to `deciduous add <type> "title"`.

  Node types follow the decision flow:
    goal → option → decision → action → outcome
  With observations and revisits attaching anywhere.
  """
  use Hermes.Server.Component, type: :tool

  alias DeciduousMcp.Graph.Nodes

  @impl true
  def definition do
    %{
      name: "add_node",
      description:
        "Add a new node to the decision graph. Types: goal (objective), " <>
          "decision (choice point), option (approach), action (implementation), " <>
          "outcome (result), observation (insight), revisit (pivot point).",
      input_schema: %{
        type: "object",
        properties: %{
          node_type: %{
            type: "string",
            enum: ["goal", "decision", "option", "action", "outcome", "observation", "revisit"],
            description: "The type of decision graph node"
          },
          title: %{type: "string", description: "Short title for the node"},
          description: %{type: "string", description: "Detailed description"},
          status: %{
            type: "string",
            enum: ["pending", "active", "completed", "rejected", "superseded", "abandoned"],
            description: "Node status (default: pending)"
          },
          confidence: %{
            type: "integer",
            minimum: 0,
            maximum: 100,
            description: "Confidence level 0-100"
          },
          commit: %{type: "string", description: "Git commit hash to link"},
          prompt: %{type: "string", description: "Verbatim user prompt that triggered this work"},
          files: %{type: "array", items: %{type: "string"}, description: "Associated file paths"},
          branch: %{type: "string", description: "Git branch name"}
        },
        required: ["node_type", "title"]
      }
    }
  end

  @impl true
  def call(%{arguments: args, server: frame}) do
    workspace_id = frame.assigns.workspace_id

    metadata =
      %{}
      |> maybe_put("confidence", args["confidence"])
      |> maybe_put("commit", args["commit"])
      |> maybe_put("prompt", args["prompt"])
      |> maybe_put("files", args["files"])
      |> maybe_put("branch", args["branch"])

    attrs = %{
      node_type: args["node_type"],
      title: args["title"],
      description: args["description"],
      status: args["status"] || "pending",
      metadata: metadata
    }

    case Nodes.create_node(workspace_id, attrs) do
      {:ok, node} ->
        {:ok,
         Jason.encode!(%{
           id: node.id,
           change_id: node.change_id,
           node_type: node.node_type,
           title: node.title,
           status: node.status,
           message: "Node created successfully"
         })}

      {:error, reason} ->
        {:error, %{code: -1, message: "Failed to create node: #{inspect(reason)}"}}
    end
  end

  defp maybe_put(map, _key, nil), do: map
  defp maybe_put(map, key, value), do: Map.put(map, key, value)
end
