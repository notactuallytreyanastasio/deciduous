defmodule DeciduousMcp.MCP.Tools.GetGraph do
  @moduledoc "MCP Tool: export the full decision graph."
  use DeciduousMcp.MCP.Component, type: :tool

  import Ecto.Query
  alias DeciduousMcp.MCP.Scope
  alias DeciduousMcp.Graph.Query
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.Node

  # Above this the answer is not something an LLM reads; it is a file. The
  # epstein workspace (7,805 nodes, 51,156 edges) is 31MB as text and takes
  # 4s on production, during which every other session's tool call queues
  # behind it in the one Hermes.Server.Base process; the global view (30,201
  # nodes) never returns at all because the transport gives up at 5s.
  @default_max_nodes 2_000
  @hard_max_nodes 20_000

  def definition do
    %{
      name: "get_graph",
      description:
        "Get the decision graph (nodes, edges, themes, documents) for one workspace. " <>
          "Returns titles, types, status and branch by default; pass include_details " <>
          "for descriptions, metadata and edge rationales. Refuses graphs over " <>
          "max_nodes (default #{@default_max_nodes}) — use query_nodes, or GET /export " <>
          "for the whole thing as a file.",
      input_schema: %{
        type: "object",
        properties: %{
          branch: %{type: "string", description: "Filter to a specific git branch"},
          include_details: %{
            type: "boolean",
            description:
              "Include description, metadata and edge rationale (default false; ~3x the bytes)"
          },
          max_nodes: %{
            type: "integer",
            minimum: 1,
            maximum: @hard_max_nodes,
            description:
              "Refuse rather than return a graph with more live nodes than this (default #{@default_max_nodes})"
          }
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

  defp do_call(scope, args) do
    max_nodes = args["max_nodes"] || @default_max_nodes
    branch = args["branch"]
    count = count_nodes(scope, branch)

    if count > max_nodes do
      {:error,
       %{
         code: -1,
         message:
           "graph has #{count} live nodes, over max_nodes=#{max_nodes}. " <>
             "Narrow with `branch`, raise max_nodes (up to #{@hard_max_nodes}), use query_nodes " <>
             "with filters, or fetch GET /export?workspace=... as a file."
       }}
    else
      opts = [details: args["include_details"] == true]
      opts = if branch, do: [{:branch, branch} | opts], else: opts
      {:ok, Jason.encode!(Query.get_full_graph(scope, opts))}
    end
  end

  # ~10ms on production for either scope; cheap enough to pay before every call.
  defp count_nodes(scope, branch) do
    Node
    |> then(fn q -> if scope == :global, do: q, else: where(q, [n], n.workspace_id == ^scope) end)
    |> where([n], is_nil(n.deleted_at))
    |> then(fn q ->
      if branch, do: where(q, [n], fragment("? ->> 'branch' = ?", n.metadata, ^branch)), else: q
    end)
    |> select([n], count(n.id))
    |> Repo.one!()
  end
end
