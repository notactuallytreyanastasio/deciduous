defmodule DeciduousMcp.MCP.Tools.QueryNodes do
  @moduledoc "MCP Tool: search and filter decision graph nodes."
  use DeciduousMcp.MCP.Component, type: :tool

  alias DeciduousMcp.MCP.Scope
  alias DeciduousMcp.Graph.Nodes
  alias DeciduousMcp.Schema.Node

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
            enum: [
              "goal", "decision", "option", "action", "outcome",
              "observation", "revisit", "feedback"
            ],
            description: "Filter by node type"
          },
          status: %{
            type: "string",
            enum: [
              "pending", "active", "completed", "rejected",
              "superseded", "abandoned", "done"
            ],
            description: "Filter by status"
          },
          branch: %{type: "string", description: "Filter by git branch"},
          search: %{type: "string", description: "Text search in title and description"},
          limit: %{type: "integer", description: "Max results (default: 100)"}
        }
      }
    }
    |> Scope.with_workspace_arg(global?: true)
  end

  # Postgres refuses a negative LIMIT by raising, which reached the client as
  # an inspected Postgrex struct. Said here in one line instead.
  def call(%{arguments: %{"limit" => limit}}) when is_integer(limit) and limit < 1 do
    {:error, %{code: -1, message: "limit must be at least 1, got #{limit}"}}
  end

  def call(%{arguments: args, server: frame}) do
    case Scope.read_scope(frame, args) do
      {:ok, workspace_id} -> do_call(workspace_id, args)
      {:error, message} -> {:error, %{code: -1, message: message}}
    end
  end

  defp do_call(workspace_id, args) do
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
            # Without this the global view is unreadable: 40k nodes from 91
            # projects, none of them saying which project they came from.
            workspace: n.workspace.name,
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
