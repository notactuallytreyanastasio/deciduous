defmodule DeciduousMcp.MCP.Tools.AddEdge do
  @moduledoc "MCP Tool: create a directed edge between two nodes."
  use DeciduousMcp.MCP.Component, type: :tool

  alias DeciduousMcp.MCP.Scope
  alias DeciduousMcp.Graph.Edges

  def definition do
    %{
      name: "add_edge",
      description:
        "Create a directed edge between two nodes. Equivalent to `deciduous link FROM TO`. " <>
          "Edge types: leads_to (default), chosen, rejected, requires, blocks, enables, took_from.",
      input_schema: %{
        type: "object",
        properties: %{
          from_node_id: %{type: "string", description: "UUID of the source node"},
          to_node_id: %{type: "string", description: "UUID of the target node"},
          edge_type: %{
            type: "string",
            enum: DeciduousMcp.Schema.Edge.edge_types(),
            description:
              "Relationship type (default: leads_to). took_from records a borrow: " <>
                "from = the node you took the idea from (any branch), to = your node that used it."
          },
          rationale: %{type: "string", description: "Why this connection exists"},
          branch: %{
            type: "string",
            description:
              "Git branch this write belongs to; check_activity shows who else is writing it"
          }
        },
        required: ["from_node_id", "to_node_id"]
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

  defp do_call(workspace_id, args), do: create(workspace_id, args)

  defp create(workspace_id, args) do
    attrs = %{
      from_node_id: args["from_node_id"],
      to_node_id: args["to_node_id"],
      edge_type: args["edge_type"] || "leads_to",
      rationale: args["rationale"]
    }

    case Edges.create_edge(workspace_id, attrs) do
      {:ok, edge} ->
        {:ok,
         Jason.encode!(%{
           id: edge.id,
           from_node_id: edge.from_node_id,
           to_node_id: edge.to_node_id,
           edge_type: edge.edge_type,
           created: true,
           message: "Edge created successfully"
         })}

      # A retry, or a race lost to the same edge from another session or
      # the CLI: the edge the call asked for is there. Answered, not
      # failed; it used to fail on the unique index as
      # "from_node_id: has already been taken".
      {:error, {:edge_exists, edge}} ->
        {:ok,
         Jason.encode!(%{
           id: edge.id,
           from_node_id: edge.from_node_id,
           to_node_id: edge.to_node_id,
           edge_type: edge.edge_type,
           created: false,
           message: "Edge already exists; nothing new was created"
         })}

      # A 2-cycle (team probe T6): an agent linked option -> decision to
      # make a decision reachable, beside the decision -> option edge
      # capture_conversation_turn had drawn. Refused in Edges.create_edge,
      # under the lock every edge create takes, so two sessions adding the
      # two directions at once cannot both win.
      {:error, {:reverse_exists, edge}} ->
        {:error,
         %{
           code: -1,
           message:
             "#{edge.from_node_id} -> #{edge.to_node_id} (#{edge.edge_type}) already exists; " <>
               "#{args["from_node_id"]} -> #{args["to_node_id"]} would make the two nodes " <>
               "each other's parent, and nothing was written. A decision's options hang " <>
               "under it with chosen/rejected edges (decision -> option); to put a decision " <>
               "under its goal, link goal -> decision."
         }}

      {:error, {:node_not_found, node_id}} ->
        {:error, %{code: -1, message: "Node not found: #{node_id}"}}

      {:error, reason} ->
        {:error, %{code: -1, message: "Failed to create edge: #{DeciduousMcp.MCP.Component.describe_error(reason)}"}}
    end
  end
end
