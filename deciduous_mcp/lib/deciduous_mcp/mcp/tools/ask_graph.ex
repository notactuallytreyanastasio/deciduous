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

  The tool combines full-text search, type/status filtering, and graph
  traversal to find relevant nodes and their context.
  """
  use DeciduousMcp.MCP.Component, type: :tool

  import Ecto.Query
  alias DeciduousMcp.MCP.Scope
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.{Node, Edge}

  def definition do
    %{
      name: "ask_graph",
      description:
        "Ask a natural language question about the decision graph. Searches across all nodes, " <>
          "descriptions, and relationships to answer questions like 'What did we decide about auth?', " <>
          "'What goals are still pending?', or 'Trace the history of our caching approach'.",
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

    # Extract search terms from the question
    search_terms = extract_search_terms(question)

    # Run searches in parallel conceptually — find matching nodes
    text_matches = search_by_text(workspace_id, search_terms, scope)
    type_matches = search_by_implied_type(workspace_id, question, scope)

    # Merge and deduplicate results
    all_matches =
      (text_matches ++ type_matches)
      |> Enum.uniq_by(& &1.id)
      |> Enum.take(25)

    # Optionally enrich with graph context
    results =
      if include_context do
        Enum.map(all_matches, fn node ->
          edges_from = edges_from_node(node.id)
          edges_to = edges_to_node(node.id)

          %{
            id: node.id,
            change_id: node.change_id,
            node_type: node.node_type,
            title: node.title,
            description: node.description,
            status: node.status,
            metadata: node.metadata,
            created_at: DateTime.to_iso8601(node.inserted_at),
            connects_to:
              Enum.map(edges_from, fn e ->
                target = Repo.get(Node, e.to_node_id)

                %{
                  node_id: e.to_node_id,
                  edge_type: e.edge_type,
                  rationale: e.rationale,
                  title: target && target.title,
                  node_type: target && target.node_type
                }
              end),
            connected_from:
              Enum.map(edges_to, fn e ->
                source = Repo.get(Node, e.from_node_id)

                %{
                  node_id: e.from_node_id,
                  edge_type: e.edge_type,
                  rationale: e.rationale,
                  title: source && source.title,
                  node_type: source && source.node_type
                }
              end)
          }
        end)
      else
        Enum.map(all_matches, fn node ->
          %{
            id: node.id,
            change_id: node.change_id,
            node_type: node.node_type,
            title: node.title,
            description: node.description,
            status: node.status,
            created_at: DateTime.to_iso8601(node.inserted_at)
          }
        end)
      end

    # Build a summary
    type_counts =
      results
      |> Enum.group_by(& &1.node_type)
      |> Enum.map(fn {type, nodes} -> {type, length(nodes)} end)
      |> Map.new()

    {:ok,
     Jason.encode!(%{
       question: question,
       result_count: length(results),
       type_breakdown: type_counts,
       results: results,
       search_terms: search_terms,
       scope: scope
     })}
  end

  # --- Search strategies ---

  defp scope_ws(query, :global), do: query
  defp scope_ws(query, workspace_id), do: where(query, [n], n.workspace_id == ^workspace_id)

  defp search_by_text(workspace_id, terms, scope) when terms != [] do
    base_query =
      Node
      |> scope_ws(workspace_id)
      |> where([n], is_nil(n.deleted_at))
      |> apply_scope(scope)

    # The terms are OR'd with each other and then AND'd onto the scope as one
    # group. This used `or_where` per term, which Ecto renders as
    # `(workspace AND not deleted AND scope) OR term1 OR term2`: every term
    # matched every workspace on the server, deleted nodes included.
    any_term =
      Enum.reduce(terms, dynamic(false), fn term, acc ->
        pattern = "%#{term}%"

        dynamic(
          [n],
          ^acc or ilike(n.title, ^pattern) or ilike(n.description, ^pattern) or
            ilike(fragment("?::text", n.metadata), ^pattern)
        )
      end)

    base_query
    |> where(^any_term)
    |> order_by([n], desc: n.inserted_at)
    |> limit(25)
    |> Repo.all()
  end

  defp search_by_text(_workspace_id, [], _scope), do: []

  defp search_by_implied_type(workspace_id, question, scope) do
    # Detect if the question implies a specific node type
    q = String.downcase(question)

    type_filter =
      cond do
        String.contains?(q, ["decision", "decided", "chose", "choice"]) -> "decision"
        String.contains?(q, ["goal", "objective", "target", "aim"]) -> "goal"
        String.contains?(q, ["observation", "noticed", "learned", "insight"]) -> "observation"
        String.contains?(q, ["action", "implemented", "built", "created", "did"]) -> "action"
        String.contains?(q, ["outcome", "result", "succeeded", "failed"]) -> "outcome"
        String.contains?(q, ["option", "approach", "alternative", "considered"]) -> "option"
        String.contains?(q, ["pivot", "revisit", "reconsidered", "changed"]) -> "revisit"
        true -> nil
      end

    status_filter =
      cond do
        String.contains?(q, ["pending", "open", "todo", "remaining", "still"]) -> "pending"
        String.contains?(q, ["completed", "done", "finished"]) -> "completed"
        String.contains?(q, ["rejected", "abandoned", "dropped"]) -> "rejected"
        String.contains?(q, ["active", "current", "in progress"]) -> "active"
        true -> nil
      end

    query =
      Node
      |> scope_ws(workspace_id)
      |> where([n], is_nil(n.deleted_at))
      |> apply_scope(scope)

    query = if type_filter, do: where(query, [n], n.node_type == ^type_filter), else: query
    query = if status_filter, do: where(query, [n], n.status == ^status_filter), else: query

    # Only return results if we actually matched a type or status
    if type_filter || status_filter do
      query
      |> order_by([n], desc: n.inserted_at)
      |> limit(15)
      |> Repo.all()
    else
      []
    end
  end

  # --- Scope filters ---

  defp apply_scope(query, "active") do
    where(query, [n], n.status in ["pending", "active"])
  end

  defp apply_scope(query, "decisions") do
    where(query, [n], n.node_type == "decision")
  end

  defp apply_scope(query, "goals") do
    where(query, [n], n.node_type == "goal")
  end

  defp apply_scope(query, "observations") do
    where(query, [n], n.node_type == "observation")
  end

  defp apply_scope(query, "recent") do
    query |> order_by([n], desc: n.inserted_at) |> limit(50)
  end

  defp apply_scope(query, _), do: query

  # --- Helpers ---

  defp extract_search_terms(question) do
    # Remove common question words and extract meaningful terms
    stop_words =
      ~w(what which how when where why who is are was were do does did
         the a an in on at to for of and or but not with from by about
         have has had been being will would could should can may might
         this that these those it its my our your their we they you i
         me tell show find get list give search look all any some every
         please help can)

    question
    |> String.downcase()
    |> String.replace(~r/[^\w\s-]/, "")
    |> String.split()
    |> Enum.reject(&(&1 in stop_words))
    |> Enum.reject(&(String.length(&1) < 3))
    |> Enum.take(8)
  end

  # Only edges whose other end is live: a deleted neighbour was listed under
  # connects_to with its title, as if it were still part of the graph.
  defp edges_from_node(node_id) do
    Edge
    |> join(:inner, [e], n in Node, on: n.id == e.to_node_id and is_nil(n.deleted_at))
    |> where([e], e.from_node_id == ^node_id)
    |> Repo.all()
  end

  defp edges_to_node(node_id) do
    Edge
    |> join(:inner, [e], n in Node, on: n.id == e.from_node_id and is_nil(n.deleted_at))
    |> where([e], e.to_node_id == ^node_id)
    |> Repo.all()
  end
end
