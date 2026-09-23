defmodule DeciduousMcp.MCP.Tools.CloseThread do
  @moduledoc """
  MCP Tool: close_thread

  Wraps up a line of reasoning by creating an outcome node and linking it
  to the action or decision that produced it. Optionally marks the entire
  chain as completed.

  Call this at the end of a conversation when work has concluded, or when
  a particular line of investigation reaches a conclusion.
  """
  use DeciduousMcp.MCP.Component, type: :tool

  alias DeciduousMcp.MCP.Scope
  alias DeciduousMcp.Graph.{Nodes, Edges}
  alias DeciduousMcp.Repo

  def definition do
    %{
      name: "close_thread",
      description:
        "Close out a reasoning thread with an outcome. Creates an outcome node linked to " <>
          "the parent action/decision, and optionally marks the goal as completed. " <>
          "Call this when a line of work reaches a conclusion.",
      input_schema: %{
        type: "object",
        properties: %{
          title: %{type: "string", description: "Summary of the outcome"},
          description: %{
            type: "string",
            description: "Detailed explanation of what happened and what was learned"
          },
          success: %{
            type: "boolean",
            description: "Did this thread succeed? (true = completed, false = failed/abandoned)"
          },
          parent_node_id: %{
            type: "string",
            description: "UUID of the action or decision that led to this outcome"
          },
          goal_node_id: %{
            type: "string",
            description: "UUID of the root goal to mark as completed (optional)"
          },
          lessons_learned: %{
            type: "array",
            description: "Key takeaways to log as observations",
            items: %{type: "string"}
          },
          next_steps: %{
            type: "array",
            description: "Follow-up goals to create",
            items: %{
              type: "object",
              properties: %{
                title: %{type: "string"},
                description: %{type: "string"}
              },
              required: ["title"]
            }
          },
          branch: %{type: "string"}
        },
        required: ["title"]
      }
    }
    |> Scope.with_workspace_arg()
  end

  def call(%{arguments: args, server: frame}) do
    with :ok <- validate(args),
         {:ok, workspace_id} <- Scope.write_workspace_id(frame, args),
         :ok <- check_goal(workspace_id, args["goal_node_id"]) do
      write(workspace_id, args)
    else
      {:error, message} -> {:error, %{code: -1, message: message}}
    end
  end

  # Peri checks arrays as :any, so the item schemas above are advisory. An
  # item of the wrong shape used to crash the handler after the outcome and
  # every earlier item were already committed.
  defp validate(args) do
    cond do
      not list_or_nil?(args["lessons_learned"]) ->
        {:error, "lessons_learned must be a list of strings; nothing was written"}

      bad = Enum.find(args["lessons_learned"] || [], &(not is_binary(&1))) ->
        {:error,
         "lessons_learned items must be strings, got #{inspect(bad)}; nothing was written"}

      not list_or_nil?(args["next_steps"]) ->
        {:error, "next_steps must be a list of {title, description} objects; nothing was written"}

      bad = Enum.find(args["next_steps"] || [], &(not step?(&1))) ->
        {:error,
         "next_steps items must be objects with a string title, got #{inspect(bad)}; " <>
           "nothing was written"}

      true ->
        :ok
    end
  end

  defp list_or_nil?(v), do: is_nil(v) or is_list(v)

  defp step?(%{"title" => t} = step) when is_binary(t),
    do: is_nil(step["description"]) or is_binary(step["description"])

  defp step?(_), do: false

  # The goal is the one row close_thread edits rather than creates, and it
  # arrives by id. The workspace the call writes to is already settled (the
  # pin, else the argument), so the goal has to be in it: resolving the
  # goal by id alone completed a goal in any workspace on the server, from a
  # client pinned to a different one.
  defp check_goal(_workspace_id, nil), do: :ok

  defp check_goal(workspace_id, goal_id) do
    case Nodes.get_node(goal_id) do
      {:ok, %{workspace_id: ^workspace_id, deleted_at: nil}} ->
        :ok

      {:ok, %{workspace_id: ^workspace_id}} ->
        {:error, "goal_node_id #{goal_id} was deleted; nothing was written"}

      _ ->
        {:error, "goal_node_id #{goal_id} is not a node in this workspace; nothing was written"}
    end
  end

  # One transaction for the outcome, its edge, the goal and every lesson and
  # follow-up. Each step's result is checked: create_edge's used to be thrown
  # away, so a parent id that named no node answered OK with an unlinked
  # outcome, and the first failure after the outcome left it committed.
  defp write(workspace_id, args) do
    Repo.transaction(fn -> build(workspace_id, args) end)
    |> case do
      {:ok, result} -> {:ok, Jason.encode!(result)}
      {:error, message} when is_binary(message) -> {:error, message}
      {:error, other} -> {:error, "Thread not closed, nothing was written: #{inspect(other)}"}
    end
  end

  defp build(workspace_id, args) do
    meta = %{} |> maybe_put("branch", args["branch"])
    status = if args["success"] != false, do: "completed", else: "rejected"

    outcome =
      insert!(workspace_id, %{
        node_type: "outcome",
        title: args["title"],
        description: args["description"],
        status: status,
        metadata: meta
      })

    if args["parent_node_id"] do
      link!(workspace_id, args["parent_node_id"], outcome.id, "Result", "parent_node_id")
    end

    if args["goal_node_id"] && args["success"] != false do
      case Nodes.update_node(args["goal_node_id"], %{status: "completed"}) do
        {:ok, _} ->
          :ok

        {:error, reason} ->
          Repo.rollback(
            "goal_node_id #{args["goal_node_id"]} could not be completed (#{inspect(reason)}); " <>
              "nothing was written"
          )
      end
    end

    lesson_nodes =
      Enum.map(args["lessons_learned"] || [], fn lesson ->
        obs =
          insert!(workspace_id, %{
            node_type: "observation",
            title: lesson,
            status: "active",
            metadata: meta
          })

        link!(workspace_id, outcome.id, obs.id, "Lesson learned", "lessons_learned")
        %{id: obs.id, title: lesson}
      end)

    next_goal_nodes =
      Enum.map(args["next_steps"] || [], fn step ->
        goal =
          insert!(workspace_id, %{
            node_type: "goal",
            title: step["title"],
            description: step["description"],
            status: "pending",
            metadata: meta
          })

        link!(workspace_id, outcome.id, goal.id, "Follow-up from outcome", "next_steps")
        %{id: goal.id, title: step["title"]}
      end)

    %{
      outcome_id: outcome.id,
      status: status,
      lessons_logged: length(lesson_nodes),
      next_goals_created: length(next_goal_nodes),
      next_goals: next_goal_nodes,
      message: "Thread closed: #{args["title"]}"
    }
  end

  defp insert!(workspace_id, attrs) do
    case Nodes.create_node(workspace_id, attrs) do
      {:ok, node} ->
        node

      {:error, reason} ->
        Repo.rollback("Thread not closed, nothing was written: #{inspect(reason)}")
    end
  end

  defp link!(workspace_id, from, to, rationale, what) do
    attrs = %{from_node_id: from, to_node_id: to, edge_type: "leads_to", rationale: rationale}

    case Edges.create_edge(workspace_id, attrs) do
      {:ok, _edge} ->
        :ok

      {:error, {:node_not_found, id}} ->
        Repo.rollback("#{what}: #{id} is not a live node in this workspace; nothing was written")

      {:error, reason} ->
        Repo.rollback("#{what}: edge not written (#{inspect(reason)}); nothing was written")
    end
  end

  defp maybe_put(map, _key, nil), do: map
  defp maybe_put(map, key, value), do: Map.put(map, key, value)
end
