defmodule DeciduousMcp.MCP.Tools.QueryNodes do
  @moduledoc "MCP Tool: search and filter decision graph nodes."
  use DeciduousMcp.MCP.Component, type: :tool

  alias DeciduousMcp.MCP.{ReadDefaults, Scope}
  alias DeciduousMcp.Graph.{Nodes, Related}
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
              "goal",
              "decision",
              "option",
              "action",
              "outcome",
              "observation",
              "revisit",
              "feedback"
            ],
            description: "Filter by node type"
          },
          status: %{
            type: "string",
            enum: [
              "pending",
              "active",
              "completed",
              "rejected",
              "superseded",
              "abandoned",
              "done"
            ],
            description: "Filter by status"
          },
          exclude_retired: %{
            type: "boolean",
            description:
              "If true, leave out retired notes (observations whose status is superseded or " <>
                "abandoned) before limit. Decisions, options, goals, actions, outcomes and " <>
                "revisits are kept whatever their status. Ignored when status is given: an " <>
                "explicit status always wins. Default: false unless the server says otherwise."
          },
          branch: %{type: "string", description: "Filter by git branch"},
          search: %{type: "string", description: "Text search in title and description"},
          file: %{
            type: "string",
            description:
              "Only nodes whose metadata.files names this path (what was decided about a file). " <>
                "Compared after trimming, dropping a leading ./ and collapsing //. A path ending in / " <>
                "also matches every path under it; a stored directory that contains the path matches too. " <>
                "Each node then carries file_match: exact, under_dir or dir_contains, exact first; " <>
                "decisions_above lists (up to 20) decisions within 3 edges above any node naming the file, " <>
                "with via = the matching node ids; file_node_count counts every node naming the file, " <>
                "before type/status/limit. " <>
                "Needs one workspace, not *."
          },
          limit: %{
            type: "integer",
            minimum: 1,
            maximum: 10_000,
            description: "Max results (default: 100)"
          }
        }
      }
    }
    |> Scope.with_workspace_arg(global?: true)
  end

  # Postgres refuses a negative LIMIT by raising, which reached the client as
  # an inspected Postgrex struct. Said here in one line instead.
  @decisions_above_max 20

  def call(%{arguments: %{"limit" => limit}}) when is_integer(limit) and limit < 1 do
    {:error, %{code: -1, message: "limit must be at least 1, got #{limit}"}}
  end

  def call(%{arguments: args, server: frame}) do
    case Scope.read_scope(frame, args) do
      {:ok, workspace_id} -> do_call(workspace_id, args, exclude_retired?(args))
      {:error, message} -> {:error, %{code: -1, message: message}}
    end
  end

  defp exclude_retired?(%{"exclude_retired" => value}) when is_boolean(value), do: value
  defp exclude_retired?(_args), do: ReadDefaults.hide_retired?()

  defp do_call(workspace_id, %{"file" => file} = args, exclude_retired) do
    case Related.nodes_for_file(workspace_id, file) do
      {:ok, matches} ->
        match_of = Map.new(matches, &{&1.node.id, &1.match})
        rank = Map.new(Enum.with_index(matches), fn {m, i} -> {m.node.id, i} end)

        # Every node naming the file passes through the other filters, then
        # the limit is taken in match order (exact first), not by recency.
        nodes =
          Nodes.list_nodes(
            workspace_id,
            filter_opts(args, exclude_retired)
            |> Keyword.put(:ids, Map.keys(match_of))
            |> Keyword.put(:limit, max(map_size(match_of), 1))
          )
          |> Enum.sort_by(&Map.fetch!(rank, &1.id))
          |> Enum.take(args["limit"] || 100)

        # The decisions those nodes were carried out under, over every node
        # naming the file (not only the page returned): decisions rarely
        # carry files themselves, the actions under them do.
        above =
          match_of
          |> Map.keys()
          |> Related.decisions_above()
          |> Enum.take(@decisions_above_max)
          |> Enum.map(fn %{decision: d, via: via} ->
            %{id: d.id, title: d.title, status: d.status, via: via}
          end)

        result =
          nodes
          |> encode(fn n, map -> Map.put(map, :file_match, Map.fetch!(match_of, n.id)) end)
          |> Map.put(:decisions_above, above)
          |> Map.put(:file_node_count, map_size(match_of))
          |> Map.put(:exclude_retired, exclude_retired)

        {:ok, Jason.encode!(result)}

      {:error, message} ->
        {:error, %{code: -1, message: message}}
    end
  end

  defp do_call(workspace_id, args, exclude_retired) do
    opts = filter_opts(args, exclude_retired) |> maybe_opt(:limit, args["limit"])

    result =
      Nodes.list_nodes(workspace_id, opts)
      |> encode(fn _n, map -> map end)
      |> Map.put(:exclude_retired, exclude_retired)

    {:ok, Jason.encode!(result)}
  end

  # An explicit status always wins, so retired notes are excluded only when the
  # caller named no status.
  defp filter_opts(args, exclude_retired) do
    []
    |> maybe_opt(:type, args["type"])
    |> maybe_opt(:status, args["status"])
    |> maybe_opt(:branch, args["branch"])
    |> maybe_opt(:search, args["search"])
    |> maybe_opt(
      :exclude_retired_notes,
      if(exclude_retired and is_nil(args["status"]), do: true)
    )
  end

  defp encode(nodes, extra) do
    %{
      count: length(nodes),
      nodes:
        Enum.map(nodes, fn n ->
          extra.(n, %{
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
          })
        end)
    }
  end

  defp maybe_opt(opts, _key, nil), do: opts
  defp maybe_opt(opts, key, value), do: Keyword.put(opts, key, value)
end
