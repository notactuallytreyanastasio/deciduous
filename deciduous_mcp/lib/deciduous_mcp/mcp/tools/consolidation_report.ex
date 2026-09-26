defmodule DeciduousMcp.MCP.Tools.ConsolidationReport do
  @moduledoc """
  MCP Tool: what in this workspace looks redundant, superseded without a
  revisit, stalled or unattached. Read-only; see
  `DeciduousMcp.Graph.Consolidation`.
  """
  use DeciduousMcp.MCP.Component, type: :tool

  alias DeciduousMcp.Graph.Consolidation
  alias DeciduousMcp.MCP.Scope

  def definition do
    d = Consolidation.defaults()
    l = Consolidation.limits()

    int = fn key, what ->
      {lo, hi} = l[key]
      %{type: "integer", minimum: lo, maximum: hi, description: "#{what} (default #{d[key]})."}
    end

    {slo, shi} = l.similarity

    %{
      name: "consolidation_report",
      description:
        "Proposes consolidation for one workspace and changes nothing. Four lists, each " <>
          "finding with node ids and the update_node/add_edge that would act on it: " <>
          "duplicate_goals (trigram-similar goal titles, labelled merge, keep_separate or " <>
          "uncertain with the evidence), competing_decisions (similar live decisions under " <>
          "the same option or goal with no revisit between them: a missing revisit), " <>
          "stale_actions (pending actions older than stale_days with no outcome), and " <>
          "parentless actions and outcomes (the first 10 with a ranked suggested_parent, never " <>
          "linked). Bounded; every cap and what it cut is in the result.",
      input_schema: %{
        type: "object",
        properties: %{
          similarity: %{
            type: "number",
            minimum: slo,
            maximum: shi,
            description:
              "pg_trgm title similarity a goal or decision pair needs to be examined (default #{d.similarity})."
          },
          stale_days:
            int.(:stale_days, "An action with no outcome is stale after this many days"),
          max_pairs: int.(:max_pairs, "Most similar pairs examined per section"),
          max_items: int.(:max_items, "Most stale or parentless nodes listed"),
          max_nodes:
            int.(:max_nodes, "Most goals, and most decisions, compared pairwise, newest first")
        }
      }
    }
    |> Scope.with_workspace_arg()
  end

  def call(%{arguments: args, server: frame}) do
    case Scope.read_scope(frame, args) do
      {:ok, :global} ->
        {:error,
         %{code: -1, message: "consolidation_report looks at one workspace; pass its name."}}

      {:ok, workspace_id} ->
        opts = Map.take(args, ~w(similarity stale_days max_pairs max_items max_nodes))

        case Consolidation.report(workspace_id, opts) do
          {:ok, report} -> {:ok, Jason.encode!(report)}
          {:error, message} -> {:error, %{code: -1, message: message}}
        end

      {:error, message} ->
        {:error, %{code: -1, message: message}}
    end
  end
end
