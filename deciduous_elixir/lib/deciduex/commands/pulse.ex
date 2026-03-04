defmodule Deciduex.Commands.Pulse do
  @moduledoc """
  Implements the `pulse` command to show decision graph health and activity.

  Usage: deciduex pulse [options]

  Options:
    -b, --branch BRANCH    Filter by git branch
    -r, --recent N         Number of recent nodes to show (default 10)
    --json                 Output as JSON
    --summary              Show summary only
  """

  alias Deciduex.Queries

  def run(args) do
    opts = parse_args(args)
    generate_pulse(opts)
  end

  defp generate_pulse(opts) do
    graph = Queries.get_graph()

    # Apply branch filter if specified
    filtered =
      if opts[:branch] do
        filter_by_branch(graph, opts[:branch])
      else
        graph
      end

    pulse = build_pulse(filtered, opts)

    if opts[:json] do
      IO.puts(Jason.encode!(pulse, pretty: true))
    else
      print_pulse(pulse, opts)
    end
  end

  defp filter_by_branch(graph, branch) do
    nodes = Enum.filter(graph.nodes, &node_matches_branch?(&1, branch))
    node_ids = MapSet.new(nodes, & &1.id)

    edges =
      Enum.filter(graph.edges, &(&1.from_node_id in node_ids and &1.to_node_id in node_ids))

    %{graph | nodes: nodes, edges: edges}
  end

  defp node_matches_branch?(%{metadata_json: nil}, _branch), do: false

  defp node_matches_branch?(%{metadata_json: json}, branch) do
    case Jason.decode(json) do
      {:ok, %{"branch" => node_branch}} -> node_branch == branch
      _ -> false
    end
  end

  defp build_pulse(graph, opts) do
    recent_count = opts[:recent] || 10
    nodes = graph.nodes
    edges = graph.edges

    # Count by type
    type_counts =
      nodes
      |> Enum.group_by(& &1.node_type)
      |> Enum.map(fn {type, list} -> {type, length(list)} end)
      |> Enum.into(%{})

    # Count by status
    status_counts =
      nodes
      |> Enum.group_by(& &1.status)
      |> Enum.map(fn {status, list} -> {status, length(list)} end)
      |> Enum.into(%{})

    # Find active goals
    active_goals =
      nodes
      |> Enum.filter(&(&1.node_type == "goal" and &1.status == "active"))
      |> Enum.sort_by(& &1.created_at, :desc)

    # Find recent nodes
    recent_nodes =
      nodes
      |> Enum.sort_by(& &1.created_at, :desc)
      |> Enum.take(recent_count)

    # Find gaps (nodes with no outgoing edges that should have them)
    gap_nodes = find_gaps(nodes, edges)

    # Calculate health score
    health = calculate_health(nodes, edges, gap_nodes)

    %{
      total_nodes: length(nodes),
      total_edges: length(edges),
      type_counts: type_counts,
      status_counts: status_counts,
      active_goals: Enum.map(active_goals, &node_summary/1),
      recent_nodes: Enum.map(recent_nodes, &node_summary/1),
      gaps: Enum.map(gap_nodes, &node_summary/1),
      health: health
    }
  end

  defp node_summary(node) do
    %{
      id: node.id,
      type: node.node_type,
      title: node.title,
      status: node.status,
      created_at: node.created_at
    }
  end

  defp find_gaps(nodes, edges) do
    # Nodes that should lead to something but don't
    outgoing_ids = MapSet.new(edges, & &1.from_node_id)

    Enum.filter(nodes, fn n ->
      should_have_outgoing?(n) and n.id not in outgoing_ids and n.status == "active"
    end)
  end

  defp should_have_outgoing?(%{node_type: type}) when type in ["goal", "decision"], do: true
  defp should_have_outgoing?(_), do: false

  defp calculate_health([], _edges, _gaps), do: 100

  defp calculate_health(nodes, edges, gaps) do
    gap_penalty = min(length(gaps) * 10, 50)
    orphan_penalty = calculate_orphan_penalty(nodes, edges)
    max(0, 100 - gap_penalty - orphan_penalty)
  end

  defp calculate_orphan_penalty(nodes, edges) do
    connected_ids =
      edges
      |> Enum.flat_map(&[&1.from_node_id, &1.to_node_id])
      |> MapSet.new()

    orphans =
      nodes
      |> Enum.filter(&(&1.node_type != "goal" and &1.id not in connected_ids))
      |> length()

    min(orphans * 5, 30)
  end

  defp print_pulse(pulse, opts) do
    if opts[:summary] do
      print_summary(pulse)
    else
      print_full(pulse)
    end
  end

  defp print_summary(pulse) do
    IO.puts(
      "Pulse: #{pulse.total_nodes} nodes, #{pulse.total_edges} edges, health: #{pulse.health}%"
    )

    IO.puts("Active goals: #{length(pulse.active_goals)}, Gaps: #{length(pulse.gaps)}")
  end

  defp print_full(pulse) do
    IO.puts("Decision Graph Pulse")
    IO.puts("====================")
    IO.puts("")
    IO.puts("Overview:")
    IO.puts("  Total nodes: #{pulse.total_nodes}")
    IO.puts("  Total edges: #{pulse.total_edges}")
    IO.puts("  Health score: #{pulse.health}%")
    IO.puts("")

    IO.puts("By Type:")

    Enum.each(pulse.type_counts, fn {type, count} ->
      IO.puts("  #{type}: #{count}")
    end)

    IO.puts("")

    IO.puts("By Status:")

    Enum.each(pulse.status_counts, fn {status, count} ->
      IO.puts("  #{status}: #{count}")
    end)

    IO.puts("")

    if Enum.any?(pulse.active_goals) do
      IO.puts("Active Goals:")

      Enum.each(pulse.active_goals, fn g ->
        IO.puts("  [#{g.id}] #{g.title}")
      end)

      IO.puts("")
    end

    if Enum.any?(pulse.gaps) do
      IO.puts("Gaps (need follow-up):")

      Enum.each(pulse.gaps, fn g ->
        IO.puts("  [#{g.id}] #{g.type}: #{g.title}")
      end)

      IO.puts("")
    end

    if Enum.any?(pulse.recent_nodes) do
      IO.puts("Recent Activity:")

      Enum.each(pulse.recent_nodes, fn n ->
        IO.puts("  [#{n.id}] #{n.type}: #{n.title}")
      end)
    end
  end

  defp parse_args(args), do: parse_args(args, %{})

  defp parse_args([], opts), do: opts

  defp parse_args(["-b", branch | rest], opts),
    do: parse_args(rest, Map.put(opts, :branch, branch))

  defp parse_args(["--branch", branch | rest], opts),
    do: parse_args(rest, Map.put(opts, :branch, branch))

  defp parse_args(["-r", count | rest], opts) do
    case Integer.parse(count) do
      {n, ""} -> parse_args(rest, Map.put(opts, :recent, n))
      _ -> parse_args(rest, opts)
    end
  end

  defp parse_args(["--recent", count | rest], opts) do
    case Integer.parse(count) do
      {n, ""} -> parse_args(rest, Map.put(opts, :recent, n))
      _ -> parse_args(rest, opts)
    end
  end

  defp parse_args(["--json" | rest], opts), do: parse_args(rest, Map.put(opts, :json, true))
  defp parse_args(["--summary" | rest], opts), do: parse_args(rest, Map.put(opts, :summary, true))
  defp parse_args([_ | rest], opts), do: parse_args(rest, opts)
end
