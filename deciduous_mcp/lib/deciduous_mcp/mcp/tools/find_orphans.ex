defmodule DeciduousMcp.MCP.Tools.FindOrphans do
  @moduledoc """
  MCP Tool: find disconnected non-goal nodes in the graph.

  With `suggest_parents: true`, each of the first #{25} orphans also carries
  up to #{3} ranked parent guesses from `DeciduousMcp.Graph.Candidates`, the
  same ranking add_node shows. Off by default: every suggested orphan costs
  one bounded candidate read (at most #{DeciduousMcp.Graph.Candidates.pool_bound()} rows) plus a
  descendant walk, so the cap keeps a call on a graph with hundreds of
  orphans from doing hundreds of them.
  """
  use DeciduousMcp.MCP.Component, type: :tool

  alias DeciduousMcp.Graph.{Candidates, Query}
  alias DeciduousMcp.MCP.Scope

  @suggest_max_orphans 25
  @suggest_per_orphan 3

  def definition do
    %{
      name: "find_orphans",
      description:
        "Find nodes with no incoming edges that aren't goals. " <>
          "These indicate missing connections in the decision graph. " <>
          "suggest_parents: true adds ranked parent guesses to the first " <>
          "#{@suggest_max_orphans} orphans (never linked for you).",
      input_schema: %{
        type: "object",
        properties: %{
          suggest_parents: %{
            type: "boolean",
            description:
              "Also rank up to #{@suggest_per_orphan} likely parents for each of the first " <>
                "#{@suggest_max_orphans} orphans (default false)"
          }
        }
      }
    }
    |> Scope.with_workspace_arg(global?: true)
  end

  def call(%{arguments: args, server: frame}) do
    case Scope.read_scope(frame, args) do
      {:ok, workspace_id} -> do_call(workspace_id, args["suggest_parents"] == true)
      {:error, message} -> {:error, %{code: -1, message: message}}
    end
  end

  defp do_call(workspace_id, suggest?) do
    orphans = Query.find_orphans(workspace_id)

    listed =
      orphans
      |> Enum.with_index()
      |> Enum.map(fn {n, i} ->
        base = %{id: n.id, change_id: n.change_id, node_type: n.node_type, title: n.title}

        if suggest? and i < @suggest_max_orphans,
          do: Map.put(base, :suggested_parents, suggest(n)),
          else: base
      end)

    result =
      %{count: length(orphans), orphans: listed}
      |> then(fn r ->
        if suggest?,
          do:
            Map.put(
              r,
              :suggested_for,
              min(length(orphans), @suggest_max_orphans)
            ),
          else: r
      end)

    {:ok, Jason.encode!(result)}
  end

  # An orphan may already have children; one of them as its parent would
  # close a cycle, so the orphan's descendants are excluded.
  defp suggest(n) do
    {descendants, _truncated} = Query.descendants_bounded(n.id)

    Candidates.suggest_parents(
      n.workspace_id,
      %{
        node_type: n.node_type,
        title: n.title,
        description: n.description,
        branch: n.metadata["branch"],
        files: n.metadata["files"]
      },
      exclude: Enum.map(descendants, & &1.id),
      # Ranked as of when the orphan was written: its parent existed then,
      # and a node written months later is not where it belonged. The
      # second of slack keeps rows a bulk import stamped with the same
      # instant. Without this, 120-hour-old options outranked the
      # months-old context of a months-old decision on the dev DB.
      as_of: DateTime.add(n.inserted_at, 1, :second),
      limit: @suggest_per_orphan
    )
  end
end
