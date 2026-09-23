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
          },
          change_id: %{
            type: "string",
            minLength: 1,
            maxLength: 255,
            description:
              "Optional identity for this node, the same on every surface (the CLI prints " <>
                "one for every node it writes). If this workspace already has a node with " <>
                "it, of the same type and title, that node is the answer (created: false) " <>
                "and no second one is made; so a retry, or the same write sent through the " <>
                "CLI and MCP, makes one node. A different node under it is refused."
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

    case create_once(workspace_id, attrs, args) do
      {:ok, {node, edge, created?}} ->
        message =
          cond do
            not created? ->
              "Node already exists with change_id #{node.change_id}; nothing new was created" <>
                if(edge, do: " (linked under parent_id)", else: "")

            edge ->
              "Node created and linked"

            true ->
              "Node created successfully"
          end

        {:ok,
         Jason.encode!(
           %{
             id: node.id,
             change_id: node.change_id,
             node_type: node.node_type,
             title: node.title,
             status: node.status,
             created: created?,
             message: message
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

      {:error, {:change_id_taken, cid, node}} ->
        {:error,
         %{
           code: -1,
           message:
             "change_id #{cid} is already node #{node.id}, #{node.node_type} " <>
               "#{inspect(node.title)}; this call names a #{attrs.node_type} " <>
               "#{inspect(attrs.title)}. Nothing was written. Use another change_id, or " <>
               "update_node to change that node."
         }}

      {:error, {:other_parent, node, [], parent_id}} ->
        {:error,
         %{
           code: -1,
           message:
             "change_id #{node.change_id} is already node #{node.id}, and #{parent_id} " <>
               "hangs under it, so it cannot also be its parent (the two would be each " <>
               "other's parent). Nothing was written."
         }}

      {:error, {:other_parent, node, parents, parent_id}} ->
        {:error,
         %{
           code: -1,
           message:
             "change_id #{node.change_id} is already node #{node.id}, under " <>
               "#{Enum.map_join(parents, ", ", & &1.from_node_id)}; this call names " <>
               "parent_id #{parent_id}. Nothing was written. A retry names the parent the " <>
               "first call did; to give the node another parent on purpose, add_edge " <>
               "#{parent_id} -> #{node.id}."
         }}

      {:error, {:change_id_other_description, cid, node}} ->
        {:error,
         %{
           code: -1,
           message:
             "change_id #{cid} is already node #{node.id}, #{node.node_type} " <>
               "#{inspect(node.title)}, with another description; a retry sends the one " <>
               "the first call did. Nothing was written. To change it, update_node " <>
               "#{node.id} with the new description."
         }}

      {:error, {:change_id_deleted, cid, node}} ->
        {:error,
         %{
           code: -1,
           message:
             "change_id #{cid} is node #{node.id}, deleted at " <>
               "#{DateTime.to_iso8601(node.deleted_at)}; it is not recreated. Nothing was written."
         }}

      {:error, reason} ->
        {:error,
         %{
           code: -1,
           message: "Failed to create node: #{DeciduousMcp.MCP.Component.describe_error(reason)}"
         }}
    end
  end

  # With a change_id: one node per change_id, whichever surface or retry
  # gets there first (team probe T3: the same write through the CLI and
  # MCP at the same moment made two nodes). Under the same per-change_id
  # lock /ops takes, so the two paths cannot both miss each other.
  defp create_once(workspace_id, attrs, %{"change_id" => cid} = args) when is_binary(cid) do
    Repo.transaction(fn ->
      :ok = Nodes.lock_change_id(workspace_id, cid)

      case Nodes.any_by_change_id(workspace_id, cid) do
        nil ->
          case create(workspace_id, Map.put(attrs, :change_id, cid), args["parent_id"], args) do
            {:ok, {node, edge}} -> {node, edge, true}
            {:error, reason} -> Repo.rollback(reason)
          end

        %{deleted_at: %DateTime{}} = node ->
          Repo.rollback({:change_id_deleted, cid, node})

        # A description the call sends is part of the write: a retry sends
        # the same one, and a different one used to be dropped under
        # created: false (T3). None sent is not compared.
        %{node_type: type, title: title, description: description} = node
        when type == attrs.node_type and title == attrs.title and
               is_binary(attrs.description) and description != attrs.description ->
          Repo.rollback({:change_id_other_description, cid, node})

        %{node_type: type, title: title} = node
        when type == attrs.node_type and title == attrs.title ->
          case link_existing(workspace_id, node, args) do
            {:ok, edge} -> {node, edge, false}
            {:error, reason} -> Repo.rollback(reason)
          end

        node ->
          Repo.rollback({:change_id_taken, cid, node})
      end
    end)
  end

  defp create_once(workspace_id, attrs, args) do
    with {:ok, {node, edge}} <- create(workspace_id, attrs, args["parent_id"], args),
         do: {:ok, {node, edge, true}}
  end

  # A retry of a call with parent_id finds the edge the first call made; a
  # node the CLI made, with no parent yet, gets the edge the call asked for.
  # A node that already hangs under another parent is refused, not given a
  # second one: a retry names the same parent, so a different one is a
  # different write (T3 verification: the retry "linked under parent_id"
  # quietly added a second parent, and naming the node's own child made a
  # 2-cycle). A second parent is add_edge's to make, on purpose.
  defp link_existing(_workspace_id, _node, %{"parent_id" => nil}), do: {:ok, nil}

  defp link_existing(_workspace_id, _node, args) when not is_map_key(args, "parent_id"),
    do: {:ok, nil}

  defp link_existing(workspace_id, node, %{"parent_id" => parent_id} = args) do
    type = args["edge_type"] || "leads_to"
    parents = Enum.reject(Edges.edges_to(node.id), &(&1.edge_type == "took_from"))

    case Enum.find(parents, &(&1.from_node_id == parent_id and &1.edge_type == type)) do
      %{} = edge ->
        {:ok, edge}

      nil when parents != [] ->
        {:error, {:other_parent, node, parents, parent_id}}

      nil ->
        case Edges.create_edge(workspace_id, %{
               from_node_id: parent_id,
               to_node_id: node.id,
               edge_type: type,
               rationale: args["rationale"]
             }) do
          {:ok, edge} -> {:ok, edge}
          {:error, {:node_not_found, _}} -> {:error, {:node_not_found, parent_id}}
          {:error, {:reverse_exists, _}} -> {:error, {:other_parent, node, [], parent_id}}
          {:error, reason} -> {:error, reason}
        end
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
