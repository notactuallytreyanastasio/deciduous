defmodule Deciduex.Commands.Narratives do
  @moduledoc """
  Implements the `narratives` command for managing evolution narratives.

  Usage: deciduex narratives <subcommand> [options]

  Subcommands:
    init      Initialize narratives.md with active goal titles
    show      Display current narratives
    pivots    Find pivot points in the graph
  """

  alias Deciduex.Queries

  @default_path ".deciduous/narratives.md"

  def run(["init" | args]) do
    opts = parse_init_args(args)
    init_narratives(opts)
  end

  def run(["show" | args]) do
    path = List.first(args) || @default_path
    show_narratives(path)
  end

  def run(["pivots" | args]) do
    opts = parse_pivots_args(args)
    find_pivots(opts)
  end

  def run([]) do
    IO.puts(:stderr, "Usage: deciduex narratives <subcommand>")
    IO.puts(:stderr, "")
    IO.puts(:stderr, "Subcommands:")
    IO.puts(:stderr, "  init      Initialize narratives.md with active goal titles")
    IO.puts(:stderr, "  show      Display current narratives")
    IO.puts(:stderr, "  pivots    Find pivot points in the graph")
    Deciduex.CLI.exit_with_error()
  end

  def run([unknown | _]) do
    IO.puts(:stderr, "Unknown narratives subcommand: #{unknown}")
    Deciduex.CLI.exit_with_error()
  end

  # Init subcommand

  defp init_narratives(opts) do
    path = opts[:output] || @default_path

    with :ok <- check_file_exists(path, opts[:force]) do
      do_init_narratives(path)
    end
  end

  defp check_file_exists(path, force) do
    if File.exists?(path) and not force do
      IO.puts(:stderr, "Error: #{path} already exists. Use --force to overwrite.")
      Deciduex.CLI.exit_with_error()
    else
      :ok
    end
  end

  defp do_init_narratives(path) do
    graph = Queries.get_graph()

    active_goals =
      graph.nodes
      |> Enum.filter(&(&1.node_type == "goal" and &1.status == "active"))
      |> Enum.sort_by(& &1.created_at)

    content = build_narratives_template(active_goals)

    path |> Path.dirname() |> File.mkdir_p!()

    case File.write(path, content) do
      :ok ->
        IO.puts("Initialized #{path} with #{length(active_goals)} goal sections")

      {:error, reason} ->
        IO.puts(:stderr, "Error writing file: #{inspect(reason)}")
        Deciduex.CLI.exit_with_error()
    end
  end

  defp build_narratives_template(goals) do
    header = """
    # Evolution Narratives

    This document captures the evolution stories of this project.
    Each section represents a major goal and its journey.

    """

    sections = Enum.map_join(goals, "\n\n", &build_goal_section/1)
    header <> sections
  end

  defp build_goal_section(goal) do
    """
    ## #{goal.title}

    **Status:** #{goal.status}
    **Created:** #{goal.created_at}

    ### Context

    _Why was this needed?_

    ### Evolution

    _How did this evolve over time?_

    ### Pivots

    _Any major direction changes?_

    ### Outcome

    _What was the result?_
    """
  end

  # Show subcommand

  defp show_narratives(path) do
    case File.read(path) do
      {:ok, content} ->
        IO.puts(content)

      {:error, :enoent} ->
        IO.puts(:stderr, "No narratives file found at #{path}")
        IO.puts(:stderr, "Run 'deciduex narratives init' to create one")
        Deciduex.CLI.exit_with_error()

      {:error, reason} ->
        IO.puts(:stderr, "Error reading file: #{inspect(reason)}")
        Deciduex.CLI.exit_with_error()
    end
  end

  # Pivots subcommand

  defp find_pivots(opts) do
    graph = Queries.get_graph()
    filtered = maybe_filter_by_branch(graph, opts[:branch])
    pivots = identify_pivots(filtered)
    output_pivots(pivots, opts[:json])
  end

  defp maybe_filter_by_branch(graph, nil), do: graph
  defp maybe_filter_by_branch(graph, branch), do: filter_by_branch(graph, branch)

  defp output_pivots(pivots, true), do: IO.puts(Jason.encode!(pivots, pretty: true))
  defp output_pivots(pivots, _), do: print_pivots(pivots)

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

  defp identify_pivots(graph) do
    # Find revisit nodes (explicit pivots)
    revisit_nodes = Enum.filter(graph.nodes, &(&1.node_type == "revisit"))

    # Find superseded nodes (implicit pivots)
    superseded_nodes = Enum.filter(graph.nodes, &(&1.status == "superseded"))

    # Build pivot entries
    pivots =
      Enum.map(revisit_nodes, fn node ->
        %{
          type: "revisit",
          id: node.id,
          title: node.title,
          created_at: node.created_at,
          related: find_related_nodes(node, graph)
        }
      end)

    superseded_pivots =
      Enum.map(superseded_nodes, fn node ->
        %{
          type: "superseded",
          id: node.id,
          title: node.title,
          created_at: node.created_at,
          superseded_by: find_superseding_node(node, graph)
        }
      end)

    pivots ++ superseded_pivots
  end

  defp find_related_nodes(node, graph) do
    incoming =
      graph.edges
      |> Enum.filter(&(&1.to_node_id == node.id))
      |> Enum.map(& &1.from_node_id)

    outgoing =
      graph.edges
      |> Enum.filter(&(&1.from_node_id == node.id))
      |> Enum.map(& &1.to_node_id)

    related_ids = incoming ++ outgoing

    graph.nodes
    |> Enum.filter(&(&1.id in related_ids))
    |> Enum.map(&%{id: &1.id, type: &1.node_type, title: &1.title})
  end

  defp find_superseding_node(node, graph) do
    graph.edges
    |> Enum.find(&(&1.to_node_id == node.id and &1.edge_type == "supersedes"))
    |> lookup_superseding_node(graph)
  end

  defp lookup_superseding_node(nil, _graph), do: nil

  defp lookup_superseding_node(%{from_node_id: id}, graph) do
    case Enum.find(graph.nodes, &(&1.id == id)) do
      nil -> nil
      found_node -> %{id: found_node.id, type: found_node.node_type, title: found_node.title}
    end
  end

  defp print_pivots([]) do
    IO.puts("No pivots found in the graph")
  end

  defp print_pivots(pivots) do
    IO.puts("Pivot Points")
    IO.puts("============")
    IO.puts("")

    Enum.each(pivots, fn p ->
      print_pivot(p)
      IO.puts("")
    end)
  end

  defp print_pivot(%{type: "revisit"} = p) do
    IO.puts("[#{p.id}] REVISIT: #{p.title}")
    IO.puts("  Created: #{p.created_at}")
    print_related(p.related)
  end

  defp print_pivot(%{type: "superseded"} = p) do
    IO.puts("[#{p.id}] SUPERSEDED: #{p.title}")
    print_superseded_by(p.superseded_by)
  end

  defp print_related([]), do: :ok

  defp print_related(related) do
    IO.puts("  Related:")
    Enum.each(related, fn r -> IO.puts("    - [#{r.id}] #{r.type}: #{r.title}") end)
  end

  defp print_superseded_by(nil), do: :ok

  defp print_superseded_by(superseded_by) do
    IO.puts("  Replaced by: [#{superseded_by.id}] #{superseded_by.title}")
  end

  defp parse_init_args(args), do: parse_init_args(args, %{})

  defp parse_init_args([], opts), do: opts

  defp parse_init_args(["-o", path | rest], opts),
    do: parse_init_args(rest, Map.put(opts, :output, path))

  defp parse_init_args(["--output", path | rest], opts),
    do: parse_init_args(rest, Map.put(opts, :output, path))

  defp parse_init_args(["--force" | rest], opts),
    do: parse_init_args(rest, Map.put(opts, :force, true))

  defp parse_init_args([_ | rest], opts), do: parse_init_args(rest, opts)

  defp parse_pivots_args(args), do: parse_pivots_args(args, %{})

  defp parse_pivots_args([], opts), do: opts

  defp parse_pivots_args(["-b", branch | rest], opts),
    do: parse_pivots_args(rest, Map.put(opts, :branch, branch))

  defp parse_pivots_args(["--branch", branch | rest], opts),
    do: parse_pivots_args(rest, Map.put(opts, :branch, branch))

  defp parse_pivots_args(["--json" | rest], opts),
    do: parse_pivots_args(rest, Map.put(opts, :json, true))

  defp parse_pivots_args([_ | rest], opts), do: parse_pivots_args(rest, opts)
end
