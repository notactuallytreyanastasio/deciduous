defmodule DeciduousMcp.MCP.Tools.ListWorkspaces do
  @moduledoc """
  MCP Tool: list every project in the shared graph.

  This is the entry point to the global view. A client that has only ever
  written to one workspace can use this to find out what else is there, and
  what the right `workspace` argument is for a project whose directory name it
  does not know.
  """
  use DeciduousMcp.MCP.Component, type: :tool

  alias DeciduousMcp.Graph.Workspaces
  alias DeciduousMcp.MCP.Scope

  def definition do
    %{
      name: "list_workspaces",
      description:
        "List every project in the shared decision graph, with node and edge " <>
          "counts, most populated first. Use this to discover what the " <>
          "`workspace` argument should be for other tools, or pass " <>
          "workspace: \"#{Scope.global_token()}\" to those tools to query " <>
          "across all of them at once.",
      input_schema: %{type: "object", properties: %{}}
    }
  end

  def call(%{server: frame}) do
    # A pinned client sees its own workspace only: the list is every
    # project's name and size, which is exactly the neighbour-reading a pin
    # exists to rule out.
    workspaces =
      case Scope.pinned_workspace_id(frame) do
        nil -> Workspaces.list_with_counts()
        pinned -> Enum.filter(Workspaces.list_with_counts(), &(&1.id == pinned))
      end

    result = %{
      count: length(workspaces),
      total_nodes: Enum.sum(Enum.map(workspaces, & &1.node_count)),
      total_edges: Enum.sum(Enum.map(workspaces, & &1.edge_count)),
      workspaces:
        Enum.map(workspaces, fn w ->
          %{
            name: w.name,
            node_count: w.node_count,
            edge_count: w.edge_count,
            updated_at: w.updated_at && DateTime.to_iso8601(w.updated_at)
          }
        end)
    }

    {:ok, Jason.encode!(result)}
  end
end
