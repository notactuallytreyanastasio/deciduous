defmodule DeciduousMcp.MCP.Tools.DeleteNode do
  @moduledoc "MCP Tool: soft-delete a decision graph node."
  use DeciduousMcp.MCP.Component, type: :tool

  alias DeciduousMcp.Graph.Nodes
  alias DeciduousMcp.MCP.Scope

  def definition do
    %{
      name: "delete_node",
      description:
        "Soft-delete a decision graph node. The node is marked as deleted but preserved for audit.",
      input_schema: %{
        type: "object",
        properties: %{
          node_id: %{type: "string", description: "UUID of the node to delete"},
          branch: %{
            type: "string",
            description:
              "Git branch name, so this write is locked against others on the same branch (see check_activity)"
          }
        },
        required: ["node_id"]
      }
    }
  end

  def call(%{arguments: %{"node_id" => node_id} = args, server: frame}) do
    case Scope.write_scope_for_node(frame, node_id, args) do
      {:ok, _workspace_id} -> do_call(node_id)
      {:error, message} -> {:error, %{code: -1, message: message}}
    end
  end

  defp do_call(node_id) do
    case Nodes.delete_node(node_id) do
      {:ok, node} ->
        {:ok, Jason.encode!(%{id: node.id, message: "Node soft-deleted"})}

      {:error, :not_found} ->
        {:error, %{code: -1, message: "Node not found: #{node_id}"}}
    end
  end
end
