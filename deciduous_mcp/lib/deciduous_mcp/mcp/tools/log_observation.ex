defmodule DeciduousMcp.MCP.Tools.LogObservation do
  @moduledoc """
  MCP Tool: log_observation

  Quick fire-and-forget tool for capturing insights during a conversation.
  Creates an observation node and optionally links it to a related node.

  `took_from` is the borrow case. When an agent takes an idea from a node on
  another branch — another agent's rotation table, a cleaner loop — it says
  so in one call: the observation is written and a `took_from` edge is drawn
  from the source node to it. The first arena run had 170 borrows recorded
  in observation titles and zero edges crossing a branch, because the tool
  offered nowhere to put the source. Now it does.

  Both `related_to` and `took_from` accept a node UUID or a `change_id`; the
  latter is what agents see in each other's node listings, and asking them
  to translate it first was one more step for the borrow to get lost in.
  """
  use DeciduousMcp.MCP.Component, type: :tool

  alias DeciduousMcp.MCP.Scope
  alias DeciduousMcp.Graph.{Nodes, Edges}
  alias DeciduousMcp.Repo

  def definition do
    %{
      name: "log_observation",
      description:
        "Quickly log an insight, learning, or discovery as an observation node. " <>
          "Use this liberally — observations are the connective tissue of the decision graph. " <>
          "Took an idea from another agent's node? Pass its id or change_id as took_from and " <>
          "the borrow becomes a took_from edge, not just a sentence.",
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
            description:
              "UUID or change_id of a related node (goal, action, outcome) to link this observation to"
          },
          took_from: %{
            type: "string",
            description:
              "UUID or change_id of the node this observation borrows from, on any branch. " <>
                "Draws a took_from edge from that node to this observation."
          },
          why: %{
            type: "string",
            description:
              "Why the borrowed idea was better than yours, and what you changed. " <>
                "Recorded as the took_from edge's rationale."
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
    with {:ok, related} <- resolve_optional(workspace_id, args["related_to"], "related_to"),
         {:ok, source} <- resolve_optional(workspace_id, args["took_from"], "took_from"),
         {:ok, {node, took_from}} <- write(workspace_id, args, related, source) do
      {:ok,
       Jason.encode!(
         %{
           id: node.id,
           change_id: node.change_id,
           title: node.title,
           message: "Observation logged"
         }
         |> maybe_put(:took_from, took_from)
       )}
    else
      {:error, message} -> {:error, %{code: -1, message: message}}
    end
  end

  @doc false
  # The observation and its edges are one transaction. Resolving the source
  # and inserting the edge are two steps, and between them another agent can
  # soft-delete the source: this is the tool for borrowing from nodes other
  # agents are still working on. If the edge cannot be written the
  # observation is not left behind without it, and the caller gets an error
  # rather than a crash. Public only so a test can drive the failure path
  # with an already-resolved source that no longer exists.
  def write(workspace_id, args, related, source) do
    Repo.transaction(fn ->
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

      with :ok <- link_related(workspace_id, related, node),
           {:ok, took_from} <- link_source(workspace_id, source, node, args) do
        {node, took_from}
      else
        {:error, message} -> Repo.rollback(message)
      end
    end)
  end

  defp link_related(_workspace_id, nil, _node), do: :ok

  defp link_related(workspace_id, related, node) do
    case Edges.create_edge(workspace_id, %{
           from_node_id: related.id,
           to_node_id: node.id,
           edge_type: "leads_to",
           rationale: "Observation from context"
         }) do
      {:ok, _edge} -> :ok
      {:error, reason} -> {:error, "related_to: #{edge_failure(related, reason)}"}
    end
  end

  defp link_source(_workspace_id, nil, _node, _args), do: {:ok, nil}

  defp link_source(workspace_id, source, node, args) do
    case Edges.create_edge(workspace_id, %{
           from_node_id: source.id,
           to_node_id: node.id,
           edge_type: "took_from",
           rationale: args["why"] || args["description"] || "Borrowed"
         }) do
      {:ok, edge} ->
        {:ok, %{edge_id: edge.id, from_node_id: source.id, from_change_id: source.change_id}}

      {:error, reason} ->
        {:error, "took_from: #{edge_failure(source, reason)}"}
    end
  end

  defp edge_failure(node, {:node_not_found, _}),
    do: "node #{node.id} was deleted before the edge could be written; nothing was logged"

  defp edge_failure(_node, reason),
    do: "could not write the edge (#{inspect(reason)}); nothing was logged"

  defp resolve_optional(_workspace_id, nil, _field), do: {:ok, nil}

  # A change_id is a UUID too, so "does it parse as a UUID" cannot tell the
  # two apart. Try it as a node id in this workspace first, then as a
  # change_id; both are unique, so the first hit is the right one.
  defp resolve_optional(workspace_id, ref, field) do
    with {:error, :not_found} <- by_id(workspace_id, ref),
         {:error, :not_found} <- Nodes.get_node_by_change_id(workspace_id, ref) do
      {:error, "#{field}: no node #{ref} in this workspace"}
    end
  end

  defp by_id(workspace_id, ref) do
    with {:ok, id} <- Ecto.UUID.cast(ref),
         {:ok, %{workspace_id: ^workspace_id, deleted_at: nil} = node} <- Nodes.get_node(id) do
      {:ok, node}
    else
      _ -> {:error, :not_found}
    end
  end

  defp maybe_put(map, _key, nil), do: map
  defp maybe_put(map, key, value), do: Map.put(map, key, value)
end
