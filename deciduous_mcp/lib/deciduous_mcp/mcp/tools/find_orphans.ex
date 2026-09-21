defmodule DeciduousMcp.MCP.Tools.FindOrphans do
  @moduledoc "MCP Tool: find disconnected non-goal nodes in the graph."
  use DeciduousMcp.MCP.Component, type: :tool

  alias DeciduousMcp.Graph.Query
  alias DeciduousMcp.MCP.Scope

  def definition do
    %{
      name: "find_orphans",
      description:
        "Find nodes with no incoming edges that aren't goals. " <>
          "These indicate missing connections in the decision graph.",
      input_schema: %{type: "object", properties: %{}}
    }
    |> Scope.with_workspace_arg(global?: true)
  end

  def call(%{arguments: args, server: frame}) do
    case Scope.read_scope(frame, args) do
      {:ok, workspace_id} -> do_call(workspace_id)
      {:error, message} -> {:error, %{code: -1, message: message}}
    end
  end

  defp do_call(workspace_id) do
    orphans = Query.find_orphans(workspace_id)

    result = %{
      count: length(orphans),
      orphans:
        Enum.map(orphans, fn n ->
          %{id: n.id, change_id: n.change_id, node_type: n.node_type, title: n.title}
        end)
    }

    {:ok, Jason.encode!(result)}
  end
end
