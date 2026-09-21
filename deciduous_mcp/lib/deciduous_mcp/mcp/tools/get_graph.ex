defmodule DeciduousMcp.MCP.Tools.GetGraph do
  @moduledoc "MCP Tool: export the full decision graph."
  use DeciduousMcp.MCP.Component, type: :tool

  alias DeciduousMcp.MCP.Scope
  alias DeciduousMcp.Graph.Query

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
    |> Scope.with_workspace_arg(global?: true)
  end

  def call(%{arguments: args, server: frame}) do
    case Scope.read_scope(frame, args) do
      {:ok, workspace_id} -> do_call(workspace_id, args)
      {:error, message} -> {:error, %{code: -1, message: message}}
    end
  end

  defp do_call(workspace_id, args) do
    opts = if args["branch"], do: [branch: args["branch"]], else: []
    graph = Query.get_full_graph(workspace_id, opts)
    {:ok, Jason.encode!(graph)}
  end
end
