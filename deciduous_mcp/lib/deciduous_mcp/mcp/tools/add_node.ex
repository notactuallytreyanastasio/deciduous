defmodule DeciduousMcp.MCP.Tools.AddNode do
  @moduledoc """
  MCP Tool: add_node

  Creates a new node in the decision graph. Equivalent to `deciduous add <type> "title"`.

  Node types follow the decision flow:
    goal → option → decision → action → outcome
  With observations and revisits attaching anywhere.

  `parent_id` links the new node under an existing one in the same call, in
  one transaction: if the parent is not a node in this workspace, nothing is
  created. The two-step form (add_node, then add_edge with the new id) needs
  the first answer before the second call can be written; agents that send
  both at once put a placeholder where the id goes. Seen five times in one
  night on production, two of them crashing the handler before the 16-byte
  id guard.
  """
  use DeciduousMcp.MCP.Component, type: :tool

  alias DeciduousMcp.MCP.Scope
  alias DeciduousMcp.Graph.{Edges, Nodes}
  alias DeciduousMcp.Repo

  def definition do
    %{
      name: "add_node",
      description:
        "Add a new node to the decision graph. Types: goal (objective), " <>
          "decision (choice point), option (approach), action (implementation), " <>
          "outcome (result), observation (insight), revisit (pivot point). " <>
          "Pass parent_id to link it under an existing node in the same call; " <>
          "do not send add_edge in the same batch with an id you have not received yet.",
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
          branch: %{type: "string", description: "Git branch name"},
          parent_id: %{
            type: "string",
            description:
              "UUID of an existing node to link this one under (edge parent -> new node), " <>
                "created in the same transaction"
          },
          edge_type: %{
            type: "string",
            enum: DeciduousMcp.Schema.Edge.edge_types(),
            description: "Type of the parent edge (default: leads_to). Only with parent_id."
          },
          rationale: %{
            type: "string",
            description: "Why the parent leads to this node. Only with parent_id."
          }
        },
        required: ["node_type", "title"]
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

    case create(workspace_id, attrs, args["parent_id"], args) do
      {:ok, {node, edge}} ->
        {:ok,
         Jason.encode!(
           %{
             id: node.id,
             change_id: node.change_id,
             node_type: node.node_type,
             title: node.title,
             status: node.status,
             message: if(edge, do: "Node created and linked", else: "Node created successfully")
           }
           |> maybe_put(:edge_id, edge && edge.id)
           |> maybe_put(:parent_id, edge && edge.from_node_id)
         )}

      {:error, {:node_not_found, parent}} ->
        {:error,
         %{
           code: -1,
           message:
             "parent_id #{parent} is not a node in this workspace; nothing was created. " <>
               "Check the id (query_nodes or show_node), or leave parent_id out and link later."
         }}

      {:error, reason} ->
        {:error, %{code: -1, message: "Failed to create node: #{inspect(reason)}"}}
    end
  end

  defp create(workspace_id, attrs, nil, _args) do
    with {:ok, node} <- Nodes.create_node(workspace_id, attrs), do: {:ok, {node, nil}}
  end

  defp create(workspace_id, attrs, parent_id, args) do
    Repo.transaction(fn ->
      with {:ok, node} <- Nodes.create_node(workspace_id, attrs),
           {:ok, edge} <-
             Edges.create_edge(workspace_id, %{
               from_node_id: parent_id,
               to_node_id: node.id,
               edge_type: args["edge_type"] || "leads_to",
               rationale: args["rationale"]
             }) do
        {node, edge}
      else
        {:error, :not_found} -> Repo.rollback({:node_not_found, parent_id})
        {:error, {:node_not_found, _}} -> Repo.rollback({:node_not_found, parent_id})
        {:error, reason} -> Repo.rollback(reason)
      end
    end)
  end

  defp maybe_put(map, _key, nil), do: map
  defp maybe_put(map, key, value), do: Map.put(map, key, value)
end
