defmodule DeciduousMcp.MCP.Tools.FindOrphans do
  @moduledoc "MCP Tool: find disconnected non-goal nodes in the graph."
  use Hermes.Server.Component, type: :tool

  alias DeciduousMcp.Graph.Query

  @impl true
  def definition do
    %{
      name: "find_orphans",
      description:
        "Find nodes with no incoming edges that aren't goals. " <>
          "These indicate missing connections in the decision graph.",
      input_schema: %{type: "object", properties: %{}}
    }
  end

  @impl true
  def call(%{server: frame}) do
    orphans = Query.find_orphans(frame.assigns.workspace_id)

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
