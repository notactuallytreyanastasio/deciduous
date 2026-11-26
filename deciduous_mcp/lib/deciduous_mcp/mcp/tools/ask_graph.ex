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
  budgeted expansion along the graph with an explicit stop reason, plus
  `DeciduousMcp.Graph.Related`'s shared file/commit route. This
  module only reads the arguments, calls it, and renders the JSON, loading
  each result's one-hop context in two queries for all results together.

  Two optional arguments make the answer smaller, both applied while
  rendering so retrieval and its ranking never change: `detail: "lean"`
  shortens each result, and `include_retired: false` leaves out retired notes
  (observations whose status is superseded or abandoned), never decisions or
  any other type, except on a history question. Without them (and without
  the deployment settings in `DeciduousMcp.MCP.ReadDefaults`) the answer is
  what it always was, plus `detail` and `retired_omitted`.
  """
  use DeciduousMcp.MCP.Component, type: :tool

  import Ecto.Query
  alias DeciduousMcp.MCP.{ReadDefaults, Scope}
  alias DeciduousMcp.Graph.{Nodes, Related, Retrieval}
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.{Node, Edge}

  # A lean read hides retired notes only: observations whose status is
  # superseded or abandoned (`Nodes.retired_note?/2`). A superseded decision or
  # an abandoned action is still part of why the code is the way it is, and a
  # rejected option is a path not taken, which "why" questions look for.
  @lean_description_chars 400
  @lean_edges 5

  def definition do
    %{
      name: "ask_graph",
      description:
        "Ask a natural language question about the decision graph. Finds entry nodes by text " <>
          "(trigram and full-text, fused), then follows edges the question calls for: why/decided " <>
          "questions follow choices and revisits, history/pivot questions follow revisits and " <>
          "superseded nodes, blocked/depends questions follow requires/blocks. Each result says " <>
          "how it was reached (reached_by, path); unmatched_terms lists question words nothing " <>
          "in the graph contains. When one of those is a name or not a common English word " <>
          "(distinctive_unmatched_terms), the graph has nothing on the question's subject: " <>
          "stop_reason is distinctive_term_unmatched and at most 3 nearest matches come back.",
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
          },
          detail: %{
            type: "string",
            enum: ["full", "lean"],
            description:
              "How much each result carries. 'full' (the default unless the server says otherwise) is " <>
                "everything. 'lean' returns the first 400 characters of each description, no metadata " <>
                "and at most five edges each way, with connects_to_total and connected_from_total " <>
                "giving the full counts."
          },
          include_retired: %{
            type: "boolean",
            description:
              "If false, retired notes (observations whose status is superseded or abandoned) are " <>
                "left out of the results and of each result's edges, except for history questions " <>
                "(pivot, superseded, replaced, trace), which always keep them. Decisions and every " <>
                "other type are kept whatever their status. retired_omitted says how many results " <>
                "were left out. Default: true unless the server says otherwise."
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

    view = %{
      detail: args["detail"] || ReadDefaults.detail(),
      include_retired:
        if(is_boolean(args["include_retired"]),
          do: args["include_retired"],
          else: not ReadDefaults.hide_retired?()
        )
    }

    # Related.route/1 adds the shared_identifier route: neighbours that
    # share a file path or commit with the frontier, read from metadata
    # rather than stored as edges.
    case Retrieval.run(workspace_id, question, scope: scope, extra_routes: [Related.route()]) do
      {:ok, r} -> {:ok, Jason.encode!(render(question, scope, include_context, view, r))}
      {:error, message} -> {:error, %{code: -1, message: message}}
    end
  end

  defp render(question, scope, include_context, view, r) do
    # A history question is asking for exactly what retiring hides.
    keep_retired? = view.include_retired or history?(r)

    {hits, dropped} =
      Retrieval.ordered(r)
      |> Enum.split_with(
        &(keep_retired? or not Nodes.retired_note?(&1.node.node_type, &1.node.status))
      )

    context =
      if include_context,
        do: context_for(Enum.map(hits, & &1.node.id), keep_retired?),
        else: %{}

    lean? = view.detail == "lean"

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

        base = if hit[:ranks] && not lean?, do: Map.put(base, :ranks, hit.ranks), else: base
        base = if lean?, do: lean_description(base), else: base

        cond do
          not include_context ->
            base

          lean? ->
            {out, inc} = Map.get(context, node.id, {[], []})

            base
            |> Map.put(:connects_to, Enum.take(out, @lean_edges))
            |> Map.put(:connected_from, Enum.take(inc, @lean_edges))
            |> Map.put(:connects_to_total, length(out))
            |> Map.put(:connected_from_total, length(inc))

          true ->
            {out, inc} = Map.get(context, node.id, {[], []})

            base
            |> Map.put(:metadata, node.metadata)
            |> Map.put(:connects_to, out)
            |> Map.put(:connected_from, inc)
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
      distinctive_unmatched_terms: r.distinctive_unmatched_terms,
      routes: r.routes,
      depth_cap: r.depth_cap,
      rounds: r.rounds,
      stop_reason: r.stop_reason,
      truncated_rounds: r.truncated_rounds,
      scope: scope,
      detail: view.detail,
      retired_omitted: length(dropped)
    }
  end

  defp history?(r), do: Enum.any?(r.routes, &(&1.name == "history"))

  defp lean_description(%{description: d} = base)
       when is_binary(d) and byte_size(d) > @lean_description_chars do
    if String.length(d) > @lean_description_chars do
      base
      |> Map.put(:description, String.slice(d, 0, @lean_description_chars))
      |> Map.put(:description_truncated, true)
    else
      base
    end
  end

  defp lean_description(base), do: base

  # One hop around every result, in two queries for all of them: edges out
  # (with the target) and edges in (with the source). Only edges whose other
  # end is live: a deleted neighbour was listed under connects_to with its
  # title, as if it were still part of the graph.
  defp context_for([], _keep_retired?), do: %{}

  defp context_for(ids, keep_retired?) do
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
             node_type: n.node_type,
             status: n.status
           }}
      )
      |> Repo.all()
      |> neighbours(keep_retired?)

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
             node_type: n.node_type,
             status: n.status
           }}
      )
      |> Repo.all()
      |> neighbours(keep_retired?)

    Map.new(ids, &{&1, {Map.get(out, &1, []), Map.get(inc, &1, [])}})
  end

  # Groups the rows by result. The neighbour's status is selected only so a
  # retired note can be left out; it is not part of the answer.
  defp neighbours(rows, keep_retired?) do
    rows
    |> Enum.reject(fn {_id, n} ->
      not keep_retired? and Nodes.retired_note?(n.node_type, n.status)
    end)
    |> Enum.group_by(&elem(&1, 0), fn {_id, n} -> Map.delete(n, :status) end)
  end
end
