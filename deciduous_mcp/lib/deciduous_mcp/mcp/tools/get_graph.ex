defmodule DeciduousMcp.MCP.Tools.GetGraph do
  @moduledoc "MCP Tool: export the full decision graph."
  use Hermes.Server.Component, type: :tool

  alias DeciduousMcp.Graph.Query

  @impl true
  def definition do
    %{
      name: "get_graph",
      description:
        "Get the full decision graph (all nodes, edges, themes, documents). " <>
          "Equivalent to `deciduous graph`. Optionally filter by git branch.",
      input_schema: %{
        type: "object",
        properties: %{
          branch: %{type: "string", description: "Filter to a specific git branch"}
        }
      }
    }
  end

  @impl true
  def call(%{arguments: args, server: frame}) do
    workspace_id = frame.assigns.workspace_id
    opts = if args["branch"], do: [branch: args["branch"]], else: []
    graph = Query.get_full_graph(workspace_id, opts)
    {:ok, Jason.encode!(graph)}
  end
end
