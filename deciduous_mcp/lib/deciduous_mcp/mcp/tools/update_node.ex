defmodule DeciduousMcp.MCP.Tools.UpdateNode do
  @moduledoc "MCP Tool: update an existing decision graph node."
  use DeciduousMcp.MCP.Component, type: :tool

  alias DeciduousMcp.Graph.Nodes
  alias DeciduousMcp.MCP.Scope

  def definition do
    %{
      name: "update_node",
      description: "Update an existing decision graph node's title, description, or status.",
      input_schema: %{
        type: "object",
        properties: %{
          node_id: %{type: "string", description: "UUID of the node to update"},
          title: %{type: "string", description: "New title"},
          description: %{type: "string", description: "New description"},
          status: %{
            type: "string",
            enum: ["pending", "active", "completed", "rejected", "superseded", "abandoned"],
            description: "New status"
          },
          # The keys add_node writes, with add_node's types. Other keys are
          # still accepted; every string is bounded, and so is the call. A
          # key sent as null passes the type check and removes the key.
          metadata: %{
            type: "object",
            description:
              "Metadata keys to change. Merged into the node's existing metadata: keys " <>
                "sent are set, a key sent as null is removed, keys not sent are kept " <>
                "(branch, prompt, commit, files). confidence must be a number 0-100.",
            properties: %{
              confidence: %{type: "integer", minimum: 0, maximum: 100},
              commit: %{type: "string"},
              prompt: %{type: "string"},
              branch: %{type: "string"},
              files: %{type: "array", items: %{type: "string"}}
            }
          },
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

  def call(%{arguments: args, server: frame}) do
    case Scope.write_scope_for_node(frame, args["node_id"], args) do
      {:ok, _workspace_id} -> do_call(args)
      {:error, message} -> {:error, %{code: -1, message: message}}
    end
  end

  defp do_call(args) do
    attrs =
      %{}
      |> maybe_put(:title, args["title"])
      |> maybe_put(:description, args["description"])
      |> maybe_put(:status, args["status"])
      |> maybe_put(:metadata, args["metadata"])

    case Nodes.update_node(args["node_id"], attrs, merge_metadata: true) do
      {:ok, node} ->
        {:ok,
         Jason.encode!(%{
           id: node.id,
           title: node.title,
           status: node.status,
           message: "Node updated"
         })}

      {:error, :not_found} ->
        {:error, %{code: -1, message: "Node not found: #{args["node_id"]}"}}

      {:error, :deleted} ->
        {:error, %{code: -1, message: "node #{args["node_id"]} was deleted"}}

      {:error, %Ecto.Changeset{} = changeset} ->
        {:error, %{code: -1, message: "Update failed: #{changeset_errors(changeset)}"}}

      {:error, reason} ->
        {:error,
         %{
           code: -1,
           message: "Update failed: #{DeciduousMcp.MCP.Component.describe_error(reason)}"
         }}
    end
  end

  defp maybe_put(map, _key, nil), do: map
  defp maybe_put(map, key, value), do: Map.put(map, key, value)

  defp changeset_errors(changeset) do
    changeset.errors
    |> Enum.map_join("; ", fn {field, {message, _}} -> "#{field} #{message}" end)
  end
end
