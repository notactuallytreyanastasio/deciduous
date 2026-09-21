defmodule DeciduousMcp.MCP.Tools.QueryNodes do
  @moduledoc "MCP Tool: search and filter decision graph nodes."
  use Hermes.Server.Component, type: :tool

  alias DeciduousMcp.Graph.Nodes
  alias DeciduousMcp.Schema.Node

  @impl true
  def definition do
    %{
      name: "query_nodes",
      description:
        "Search and filter decision graph nodes. Equivalent to `deciduous nodes` CLI command.",
      input_schema: %{
        type: "object",
        properties: %{
          type: %{
            type: "string",
            enum: ["goal", "decision", "option", "action", "outcome", "observation", "revisit"],
            description: "Filter by node type"
          },
          status: %{
            type: "string",
            enum: ["pending", "active", "completed", "rejected", "superseded", "abandoned"],
            description: "Filter by status"
          },
          branch: %{type: "string", description: "Filter by git branch"},
          search: %{type: "string", description: "Text search in title and description"},
          limit: %{type: "integer", description: "Max results (default: 100)"}
        }
      }
    }
  end

  @impl true
  def call(%{arguments: args, server: frame}) do
    workspace_id = frame.assigns.workspace_id

    opts =
      []
      |> maybe_opt(:type, args["type"])
      |> maybe_opt(:status, args["status"])
      |> maybe_opt(:branch, args["branch"])
      |> maybe_opt(:search, args["search"])
      |> maybe_opt(:limit, args["limit"])

    nodes = Nodes.list_nodes(workspace_id, opts)

    result = %{
      count: length(nodes),
      nodes:
        Enum.map(nodes, fn n ->
          %{
            id: n.id,
            change_id: n.change_id,
            node_type: n.node_type,
            title: n.title,
            status: n.status,
            branch: Node.branch(n),
            confidence: Node.confidence(n),
            created_at: DateTime.to_iso8601(n.inserted_at)
          }
        end)
    }

    {:ok, Jason.encode!(result)}
  end

  defp maybe_opt(opts, _key, nil), do: opts
  defp maybe_opt(opts, key, value), do: Keyword.put(opts, key, value)
end
