defmodule DeciduousMcp.MCP.Tools.LogDecision do
  @moduledoc """
  MCP Tool: log_decision

  Purpose-built for capturing a decision point. Creates the full decision
  structure atomically: the decision node, all option nodes, and the
  chosen/rejected edges connecting them.

  Call this whenever you choose between approaches, technologies, patterns,
  or any other fork in the road.
  """
  use DeciduousMcp.MCP.Component, type: :tool

  alias DeciduousMcp.MCP.Scope
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Graph.{Nodes, Edges}

  def definition do
    %{
      name: "log_decision",
      description:
        "Record a decision point with the options that were considered and which was chosen. " <>
          "Creates a decision node, option nodes, and chosen/rejected edges atomically.",
      input_schema: %{
        type: "object",
        properties: %{
          title: %{
            type: "string",
            description: "What was being decided (e.g., 'Choose auth strategy')"
          },
          rationale: %{type: "string", description: "Why the chosen option was selected"},
          chosen_option: %{
            type: ["object", "string"],
            description: "The option that was selected: an object, or just its title",
            properties: %{
              title: %{type: "string"},
              description: %{type: "string"}
            },
            required: ["title"]
          },
          rejected_options: %{
            type: "array",
            description: "Options that were considered but not selected",
            items: %{
              type: ["object", "string"],
              properties: %{
                title: %{type: "string"},
                description: %{type: "string"},
                reason: %{type: "string", description: "Why this was rejected"}
              },
              required: ["title"]
            }
          },
          parent_node_id: %{
            type: "string",
            description: "UUID of the goal or context node this decision belongs to"
          },
          confidence: %{type: "integer", minimum: 0, maximum: 100},
          branch: %{type: "string"}
        },
        required: ["title", "chosen_option"]
      }
    }
    |> Scope.with_workspace_arg()
  end

  def call(%{arguments: args, server: frame}) do
    case Scope.write_workspace_id(frame, args) do
      {:ok, workspace_id} -> do_call(workspace_id, args)
      {:error, message} -> {:error, %{code: -1, message: message}}
    end
  end

  # Everything is validated before the first insert and written in one
  # transaction. Before this, options given as strings (the schema asks for
  # objects, but Peri checks arrays as :any) crashed mid-write after the
  # decision node was already in, and every create_edge result was thrown
  # away, so a parent_node_id that is not a node left the decision orphaned
  # while the call reported success. The description said "atomically".
  defp do_call(workspace_id, args) do
    meta =
      %{} |> maybe_put("confidence", args["confidence"]) |> maybe_put("branch", args["branch"])

    with {:ok, chosen_opt} <- option(args["chosen_option"], "chosen_option"),
         {:ok, rejected_opts} <- options(args["rejected_options"] || []) do
      Repo.transaction(fn ->
        decision =
          insert!(workspace_id, %{
            node_type: "decision",
            title: args["title"],
            description: args["rationale"],
            status: "completed",
            metadata: meta
          })

        if args["parent_node_id"] do
          link!(workspace_id, args["parent_node_id"], decision.id, "leads_to", "Decision point")
        end

        chosen =
          insert!(workspace_id, %{
            node_type: "option",
            title: chosen_opt["title"],
            description: chosen_opt["description"],
            status: "completed",
            metadata: meta
          })

        link!(workspace_id, decision.id, chosen.id, "chosen", args["rationale"])

        rejected =
          Enum.map(rejected_opts, fn opt ->
            node =
              insert!(workspace_id, %{
                node_type: "option",
                title: opt["title"],
                description: opt["description"],
                status: "rejected",
                metadata: meta
              })

            link!(workspace_id, decision.id, node.id, "rejected", opt["reason"])
            %{id: node.id, title: node.title}
          end)

        %{
          decision_id: decision.id,
          chosen: %{id: chosen.id, title: chosen.title},
          rejected: rejected,
          message: "Decision logged: #{args["title"]}"
        }
      end)
      |> case do
        {:ok, result} ->
          {:ok, Jason.encode!(result)}

        {:error, message} when is_binary(message) ->
          {:error, %{code: -1, message: message}}

        {:error, other} ->
          {:error, %{code: -1, message: "Decision not logged: #{inspect(other)}"}}
      end
    else
      {:error, message} -> {:error, %{code: -1, message: message}}
    end
  end

  # An option is a string (its title) or an object with a title.
  defp option(title, _field) when is_binary(title) and title != "", do: {:ok, %{"title" => title}}

  defp option(%{"title" => title} = opt, _field) when is_binary(title) and title != "",
    do: {:ok, opt}

  defp option(other, field),
    do:
      {:error,
       "#{field} must be a title string or an object with a title, got: #{inspect(other)}; nothing was written"}

  defp options(list) when is_list(list) do
    list
    |> Enum.with_index()
    |> Enum.reduce_while({:ok, []}, fn {opt, i}, {:ok, acc} ->
      case option(opt, "rejected_options[#{i}]") do
        {:ok, o} -> {:cont, {:ok, [o | acc]}}
        error -> {:halt, error}
      end
    end)
    |> case do
      {:ok, acc} -> {:ok, Enum.reverse(acc)}
      error -> error
    end
  end

  defp options(other),
    do: {:error, "rejected_options must be a list, got: #{inspect(other)}; nothing was written"}

  defp insert!(workspace_id, attrs) do
    case Nodes.create_node(workspace_id, attrs) do
      {:ok, node} ->
        node

      {:error, reason} ->
        Repo.rollback("Decision not logged, nothing was written: #{inspect(reason)}")
    end
  end

  defp link!(workspace_id, from, to, type, rationale) do
    case Edges.create_edge(workspace_id, %{
           from_node_id: from,
           to_node_id: to,
           edge_type: type,
           rationale: rationale
         }) do
      {:ok, _edge} ->
        :ok

      {:error, {:node_not_found, id}} ->
        Repo.rollback("#{inspect(id)} is not a node in this workspace; nothing was written")

      {:error, reason} ->
        Repo.rollback("Decision not logged, nothing was written: #{inspect(reason)}")
    end
  end

  defp maybe_put(map, _key, nil), do: map
  defp maybe_put(map, key, value), do: Map.put(map, key, value)
end
