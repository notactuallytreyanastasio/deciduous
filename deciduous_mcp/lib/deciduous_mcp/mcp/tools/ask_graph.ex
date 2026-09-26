defmodule DeciduousMcp.MCP.Tools.AskGraph do
  @moduledoc """
  MCP Tool: ask_graph

  Natural language query interface over the decision graph. Instead of
  requiring structured queries, this tool accepts plain questions and
  searches across nodes, descriptions, metadata, and relationships to
  build a comprehensive answer.

  Examples:
  - "What decisions have we made about authentication?"
  - "Show me all the times we pivoted on caching"
  - "What's the current status of the rate limiting work?"
  - "What observations came out of the database migration?"
  - "What goals are still pending?"
  - "Trace the history of how we got to the current auth approach"

  The retrieval itself is `DeciduousMcp.Graph.Retrieval`: anchors from a
  reciprocal-rank fusion of trigram and full-text rankings, then routed,
  budgeted expansion along the graph with an explicit stop reason. This
  module only reads the arguments, calls it, and renders the JSON, loading
  each result's one-hop context in two queries for all results together.
  """
  use DeciduousMcp.MCP.Component, type: :tool

  import Ecto.Query
  alias DeciduousMcp.MCP.Scope
  alias DeciduousMcp.Graph.Retrieval
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.{Node, Edge}

  def definition do
    %{
      name: "ask_graph",
      description:
        "Ask a natural language question about the decision graph. Finds entry nodes by text " <>
          "(trigram and full-text, fused), then follows edges the question calls for: why/decided " <>
          "questions follow choices and revisits, history/pivot questions follow revisits and " <>
          "superseded nodes, blocked/depends questions follow requires/blocks. Each result says " <>
          "how it was reached (reached_by, path); unmatched_terms lists question words nothing " <>
          "in the graph contains.",
      input_schema: %{
        type: "object",
        properties: %{
          question: %{
            type: "string",
            description: "Your question about the decision graph in natural language"
          },
          scope: %{
            type: "string",
            enum: ["all", "active", "decisions", "goals", "observations", "recent"],
            description:
              "Narrow the search scope (optional). 'all' searches everything, 'active' only non-completed, " <>
                "'decisions' only decision nodes, 'goals' only goals, 'observations' only observations, " <>
                "'recent' only last 50 nodes."
          },
          include_context: %{
            type: "boolean",
            description:
              "If true, include connected nodes (ancestors/descendants) for each result. Default: true"
          }
        },
        required: ["question"]
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
    question = args["question"]
    scope = args["scope"] || "all"
    include_context = args["include_context"] != false

    case Retrieval.run(workspace_id, question, scope: scope) do
      {:ok, r} -> {:ok, Jason.encode!(render(question, scope, include_context, r))}
      {:error, message} -> {:error, %{code: -1, message: message}}
    end
  end

  defp render(question, scope, include_context, r) do
    hits = Retrieval.ordered(r)
    context = if include_context, do: context_for(Enum.map(hits, & &1.node.id)), else: %{}

    results =
      Enum.map(hits, fn hit ->
        node = hit.node

        base = %{
          id: node.id,
          change_id: node.change_id,
          node_type: node.node_type,
          title: node.title,
          description: node.description,
          status: node.status,
          created_at: DateTime.to_iso8601(node.inserted_at),
          reached_by: hit.reached_by,
          path: hit.path,
          score: hit.score
        }

        base = if hit[:ranks], do: Map.put(base, :ranks, hit.ranks), else: base

        if include_context do
          {out, inc} = Map.get(context, node.id, {[], []})

          base
          |> Map.put(:metadata, node.metadata)
          |> Map.put(:connects_to, out)
          |> Map.put(:connected_from, inc)
        else
          base
        end
      end)

    %{
      question: question,
      result_count: length(results),
      type_breakdown: results |> Enum.frequencies_by(& &1.node_type),
      results: results,
      search_terms: r.terms,
      route_terms: r.route_terms,
      term_hits: r.term_hits,
      unmatched_terms: r.unmatched_terms,
      routes: r.routes,
      depth_cap: r.depth_cap,
      rounds: r.rounds,
      stop_reason: r.stop_reason,
      truncated_rounds: r.truncated_rounds,
      scope: scope
    }
  end

  # One hop around every result, in two queries for all of them: edges out
  # (with the target) and edges in (with the source). Only edges whose other
  # end is live: a deleted neighbour was listed under connects_to with its
  # title, as if it were still part of the graph.
  defp context_for([]), do: %{}

  defp context_for(ids) do
    out =
      from(e in Edge,
        join: n in Node,
        on: n.id == e.to_node_id and is_nil(n.deleted_at),
        where: e.from_node_id in type(^ids, {:array, :binary_id}),
        order_by: [asc: e.inserted_at, asc: e.id],
        select:
          {e.from_node_id,
           %{
             node_id: e.to_node_id,
             edge_type: e.edge_type,
             rationale: e.rationale,
             title: n.title,
             node_type: n.node_type
           }}
      )
      |> Repo.all()
      |> Enum.group_by(&elem(&1, 0), &elem(&1, 1))

    inc =
      from(e in Edge,
        join: n in Node,
        on: n.id == e.from_node_id and is_nil(n.deleted_at),
        where: e.to_node_id in type(^ids, {:array, :binary_id}),
        order_by: [asc: e.inserted_at, asc: e.id],
        select:
          {e.to_node_id,
           %{
             node_id: e.from_node_id,
             edge_type: e.edge_type,
             rationale: e.rationale,
             title: n.title,
             node_type: n.node_type
           }}
      )
      |> Repo.all()
      |> Enum.group_by(&elem(&1, 0), &elem(&1, 1))

    Map.new(ids, &{&1, {Map.get(out, &1, []), Map.get(inc, &1, [])}})
  end
end
