defmodule Deciduex.Commands.Writeup do
  @moduledoc """
  Implements the `writeup` command to generate a PR writeup.

  Usage: deciduex writeup [options]

  Options:
    -t, --title TITLE    PR title
    -n, --nodes SPEC     Node IDs or ranges (e.g., "1-11" or "1,3,5-10")
    -r, --roots IDS      Root node IDs (comma-separated, traverses children)
    -o, --output FILE    Output file (default: stdout)
    --no-test-plan       Skip test plan section
  """

  alias Deciduex.Queries

  def run(args) do
    opts = parse_args(args)
    generate_writeup(opts)
  end

  defp generate_writeup(opts) do
    graph = Queries.get_graph()
    filtered = apply_filters(graph, opts)
    writeup = build_writeup(filtered, opts)
    output_writeup(writeup, opts[:output])
  end

  defp apply_filters(graph, opts) do
    cond do
      opts[:nodes] -> filter_by_ids(graph, parse_node_range(opts[:nodes]))
      opts[:roots] -> filter_from_roots(graph, parse_comma_ids(opts[:roots]))
      true -> graph
    end
  end

  defp output_writeup(writeup, nil), do: IO.puts(writeup)

  defp output_writeup(writeup, path) do
    case File.write(path, writeup) do
      :ok ->
        IO.puts("Generated PR writeup to #{path}")

      {:error, reason} ->
        IO.puts(:stderr, "Error writing file: #{inspect(reason)}")
        System.halt(1)
    end
  end

  defp build_writeup(graph, opts) do
    title = opts[:title] || "Pull Request"
    include_test_plan = opts[:no_test_plan] != true

    sections = [
      build_summary(graph),
      build_decisions(graph),
      build_actions(graph),
      build_outcomes(graph),
      if(include_test_plan, do: build_test_plan(), else: nil),
      build_footer()
    ]

    ["## #{title}\n" | sections]
    |> Enum.filter(&(&1 != nil))
    |> Enum.join("\n")
  end

  defp build_summary(graph) do
    goals = Enum.filter(graph.nodes, &(&1.node_type == "goal"))

    if Enum.empty?(goals) do
      nil
    else
      goal_text = Enum.map_join(goals, "\n", &format_goal/1)
      "## Summary\n\n#{goal_text}\n"
    end
  end

  defp format_goal(goal) do
    desc = if goal.description, do: "\n#{goal.description}\n", else: ""
    "**Goal:** #{goal.title}#{desc}"
  end

  defp build_decisions(graph) do
    decisions = Enum.filter(graph.nodes, &(&1.node_type == "decision"))

    if Enum.empty?(decisions) do
      nil
    else
      decision_text = Enum.map_join(decisions, "\n", &format_decision(&1, graph))
      "## Key Decisions\n\n#{decision_text}"
    end
  end

  defp format_decision(decision, graph) do
    options = find_options_for_decision(decision, graph)
    observations = find_observations_for_decision(decision, graph)

    parts = ["### #{decision.title}\n"]
    parts = maybe_add_options(parts, options, decision, graph)
    parts = maybe_add_observations(parts, observations)

    Enum.join(parts, "\n")
  end

  defp maybe_add_options(parts, [], _decision, _graph), do: parts

  defp maybe_add_options(parts, options, decision, graph) do
    option_list = Enum.map_join(options, "\n", &format_option(&1, decision, graph))
    parts ++ ["**Options considered:**\n\n#{option_list}\n"]
  end

  defp format_option(opt, decision, graph) do
    marker = if chosen?(decision, opt, graph), do: "[x]", else: "[ ]"
    "- #{marker} #{opt.title}"
  end

  defp maybe_add_observations(parts, []), do: parts

  defp maybe_add_observations(parts, observations) do
    obs_list = Enum.map_join(observations, "\n", &"- #{&1.title}")
    parts ++ ["**Observations:**\n\n#{obs_list}\n"]
  end

  defp find_options_for_decision(decision, graph) do
    option_ids =
      graph.edges
      |> Enum.filter(&(&1.from_node_id == decision.id))
      |> Enum.map(& &1.to_node_id)

    graph.nodes
    |> Enum.filter(&(&1.node_type == "option" and &1.id in option_ids))
  end

  defp find_observations_for_decision(decision, graph) do
    related_ids =
      graph.edges
      |> Enum.filter(&(&1.from_node_id == decision.id or &1.to_node_id == decision.id))
      |> Enum.flat_map(&[&1.from_node_id, &1.to_node_id])
      |> MapSet.new()

    graph.nodes
    |> Enum.filter(&(&1.node_type == "observation" and &1.id in related_ids))
  end

  defp chosen?(decision, option, graph) do
    Enum.any?(graph.edges, fn e ->
      e.from_node_id == decision.id and e.to_node_id == option.id and e.edge_type == "chosen"
    end)
  end

  defp build_actions(graph) do
    actions = Enum.filter(graph.nodes, &(&1.node_type == "action"))

    if Enum.empty?(actions) do
      nil
    else
      action_list = Enum.map_join(actions, "\n", &"- #{&1.title}")
      "## Implementation\n\n#{action_list}\n"
    end
  end

  defp build_outcomes(graph) do
    outcomes = Enum.filter(graph.nodes, &(&1.node_type == "outcome"))

    if Enum.empty?(outcomes) do
      nil
    else
      outcome_list = Enum.map_join(outcomes, "\n", &"- #{&1.title}")
      "## Results\n\n#{outcome_list}\n"
    end
  end

  defp build_test_plan do
    """
    ## Test Plan

    - [ ] Unit tests pass
    - [ ] Integration tests pass
    - [ ] Manual testing completed
    """
  end

  defp build_footer do
    "\n---\n_Generated with [Claude Code](https://claude.com/claude-code)_\n"
  end

  # Graph filtering helpers

  defp filter_by_ids(graph, ids) do
    id_set = MapSet.new(ids)

    nodes = Enum.filter(graph.nodes, &(&1.id in id_set))

    edges =
      Enum.filter(graph.edges, fn e ->
        e.from_node_id in id_set and e.to_node_id in id_set
      end)

    %{graph | nodes: nodes, edges: edges}
  end

  defp filter_from_roots(graph, root_ids) do
    # BFS to find all reachable nodes
    reachable = traverse_from_roots(graph, root_ids)
    filter_by_ids(graph, MapSet.to_list(reachable))
  end

  defp traverse_from_roots(graph, root_ids) do
    # Build adjacency map
    adj =
      graph.edges
      |> Enum.reduce(%{}, fn edge, acc ->
        Map.update(acc, edge.from_node_id, [edge.to_node_id], &[edge.to_node_id | &1])
      end)

    # BFS
    do_bfs(root_ids, adj, MapSet.new(root_ids))
  end

  defp do_bfs([], _adj, visited), do: visited

  defp do_bfs([current | rest], adj, visited) do
    neighbors = Map.get(adj, current, [])
    new_neighbors = Enum.filter(neighbors, &(&1 not in visited))
    new_visited = Enum.reduce(new_neighbors, visited, &MapSet.put(&2, &1))
    do_bfs(rest ++ new_neighbors, adj, new_visited)
  end

  # Argument parsing

  defp parse_args(args), do: parse_args(args, %{})

  defp parse_args([], opts), do: opts

  defp parse_args(["-t", title | rest], opts), do: parse_args(rest, Map.put(opts, :title, title))

  defp parse_args(["--title", title | rest], opts),
    do: parse_args(rest, Map.put(opts, :title, title))

  defp parse_args(["-n", nodes | rest], opts), do: parse_args(rest, Map.put(opts, :nodes, nodes))

  defp parse_args(["--nodes", nodes | rest], opts),
    do: parse_args(rest, Map.put(opts, :nodes, nodes))

  defp parse_args(["-r", roots | rest], opts), do: parse_args(rest, Map.put(opts, :roots, roots))

  defp parse_args(["--roots", roots | rest], opts),
    do: parse_args(rest, Map.put(opts, :roots, roots))

  defp parse_args(["-o", output | rest], opts),
    do: parse_args(rest, Map.put(opts, :output, output))

  defp parse_args(["--output", output | rest], opts),
    do: parse_args(rest, Map.put(opts, :output, output))

  defp parse_args(["--no-test-plan" | rest], opts),
    do: parse_args(rest, Map.put(opts, :no_test_plan, true))

  defp parse_args([_ | rest], opts), do: parse_args(rest, opts)

  defp parse_node_range(spec) do
    spec
    |> String.split(",")
    |> Enum.flat_map(&parse_range_part(String.trim(&1)))
  end

  defp parse_range_part(part) do
    if String.contains?(part, "-") do
      parse_range(part)
    else
      parse_single_id(part)
    end
  end

  defp parse_range(part) do
    case String.split(part, "-") do
      [start_str, end_str] -> expand_range(start_str, end_str)
      _ -> []
    end
  end

  defp expand_range(start_str, end_str) do
    with {start_val, ""} <- Integer.parse(start_str),
         {end_val, ""} <- Integer.parse(end_str) do
      Enum.to_list(start_val..end_val)
    else
      _ -> []
    end
  end

  defp parse_single_id(part) do
    case Integer.parse(part) do
      {id, ""} -> [id]
      _ -> []
    end
  end

  defp parse_comma_ids(spec) do
    spec
    |> String.split(",")
    |> Enum.map(&String.trim/1)
    |> Enum.filter(&(&1 != ""))
    |> Enum.map(&Integer.parse/1)
    |> Enum.filter(&match?({_, ""}, &1))
    |> Enum.map(fn {id, ""} -> id end)
  end
end
