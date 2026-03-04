defmodule Deciduex.Commands.Archaeology do
  @moduledoc """
  Implements the `archaeology` command for retroactive graph building.

  Usage: deciduex archaeology <subcommand> [options]

  Subcommands:
    pivot       Create a pivot chain (observation -> revisit -> new decision)
    timeline    Show timeline of graph evolution
    supersede   Mark a node and optionally its children as superseded
  """

  alias Deciduex.Queries
  alias Deciduex.Repo
  alias Ecto.Adapters.SQL

  def run(["pivot" | args]) do
    opts = parse_pivot_args(args)
    create_pivot(opts)
  end

  def run(["timeline" | args]) do
    opts = parse_timeline_args(args)
    show_timeline(opts)
  end

  def run(["supersede" | args]) do
    opts = parse_supersede_args(args)
    supersede_node(opts)
  end

  def run([]) do
    IO.puts(:stderr, "Usage: deciduex archaeology <subcommand>")
    IO.puts(:stderr, "")
    IO.puts(:stderr, "Subcommands:")
    IO.puts(:stderr, "  pivot       Create a pivot chain atomically")
    IO.puts(:stderr, "  timeline    Show timeline of graph evolution")
    IO.puts(:stderr, "  supersede   Mark a node as superseded")
    System.halt(1)
  end

  def run([unknown | _]) do
    IO.puts(:stderr, "Unknown archaeology subcommand: #{unknown}")
    System.halt(1)
  end

  # Pivot subcommand

  defp create_pivot(opts) do
    from_id = opts[:from_id]
    observation = opts[:observation]
    new_approach = opts[:new_approach]

    unless from_id && observation && new_approach do
      IO.puts(:stderr, "Error: --from, --observation, and --new-approach are required")
      System.halt(1)
    end

    confidence = opts[:confidence] || 80
    reason = opts[:reason] || "Reconsidering approach"

    # Verify the from node exists
    case get_node(from_id) do
      nil ->
        IO.puts(:stderr, "Error: Node #{from_id} not found")
        System.halt(1)

      from_node ->
        IO.puts("Creating pivot chain from [#{from_id}] #{from_node.title}...")

        # Create observation node
        obs_id = create_node("observation", observation, confidence)
        IO.puts("  Created observation [#{obs_id}]: #{observation}")

        # Create revisit node
        revisit_title = "Reconsidering: #{from_node.title}"
        revisit_id = create_node("revisit", revisit_title, confidence)
        IO.puts("  Created revisit [#{revisit_id}]: #{revisit_title}")

        # Create new decision node
        decision_id = create_node("decision", new_approach, confidence)
        IO.puts("  Created decision [#{decision_id}]: #{new_approach}")

        # Create edges
        create_edge(from_id, obs_id, "leads_to", "Observed issue")
        create_edge(obs_id, revisit_id, "leads_to", reason)
        create_edge(revisit_id, decision_id, "leads_to", "New approach")

        # Mark old node as superseded
        update_status(from_id, "superseded")
        IO.puts("  Marked [#{from_id}] as superseded")

        IO.puts("")
        IO.puts("Pivot chain complete:")
        IO.puts("  [#{from_id}] -> [#{obs_id}] -> [#{revisit_id}] -> [#{decision_id}]")
    end
  end

  # Timeline subcommand

  defp show_timeline(opts) do
    graph = Queries.get_graph()

    filtered =
      graph.nodes
      |> filter_by_type(opts[:type])
      |> filter_by_branch_nodes(opts[:branch])
      |> Enum.sort_by(& &1.created_at)
      |> maybe_limit(opts[:limit])

    if opts[:json] do
      timeline =
        Enum.map(filtered, fn n ->
          %{
            id: n.id,
            type: n.node_type,
            title: n.title,
            status: n.status,
            created_at: n.created_at
          }
        end)

      IO.puts(Jason.encode!(timeline, pretty: true))
    else
      print_timeline(filtered)
    end
  end

  defp filter_by_type(nodes, nil), do: nodes
  defp filter_by_type(nodes, type), do: Enum.filter(nodes, &(&1.node_type == type))

  defp filter_by_branch_nodes(nodes, nil), do: nodes

  defp filter_by_branch_nodes(nodes, branch) do
    Enum.filter(nodes, &node_matches_branch?(&1, branch))
  end

  defp node_matches_branch?(%{metadata_json: nil}, _branch), do: false

  defp node_matches_branch?(%{metadata_json: json}, branch) do
    case Jason.decode(json) do
      {:ok, %{"branch" => node_branch}} -> node_branch == branch
      _ -> false
    end
  end

  defp maybe_limit(nodes, nil), do: nodes
  defp maybe_limit(nodes, limit), do: Enum.take(nodes, limit)

  defp print_timeline([]) do
    IO.puts("No nodes found")
  end

  defp print_timeline(nodes) do
    IO.puts("Timeline")
    IO.puts("========")
    IO.puts("")

    Enum.each(nodes, fn n ->
      status_marker = if n.status != "active", do: " (#{n.status})", else: ""
      IO.puts("#{n.created_at} [#{n.id}] #{n.node_type}: #{n.title}#{status_marker}")
    end)
  end

  # Supersede subcommand

  defp supersede_node(opts) do
    id = opts[:id]

    unless id do
      IO.puts(:stderr, "Error: Node ID required")
      System.halt(1)
    end

    case get_node(id) do
      nil ->
        IO.puts(:stderr, "Error: Node #{id} not found")
        System.halt(1)

      node ->
        dry_run = opts[:dry_run] || false
        cascade = opts[:cascade] || false

        nodes_to_supersede =
          if cascade do
            find_descendants(id, Queries.get_graph())
          else
            [node]
          end

        if dry_run do
          IO.puts("Would supersede #{length(nodes_to_supersede)} nodes:")

          Enum.each(nodes_to_supersede, fn n ->
            IO.puts("  [#{n.id}] #{n.node_type}: #{n.title}")
          end)
        else
          Enum.each(nodes_to_supersede, fn n ->
            update_status(n.id, "superseded")
            IO.puts("Superseded [#{n.id}] #{n.title}")
          end)

          IO.puts("")
          IO.puts("Total: #{length(nodes_to_supersede)} nodes superseded")
        end
    end
  end

  defp find_descendants(root_id, graph) do
    root = Enum.find(graph.nodes, &(&1.id == root_id))

    adj =
      graph.edges
      |> Enum.reduce(%{}, fn e, acc ->
        Map.update(acc, e.from_node_id, [e.to_node_id], &[e.to_node_id | &1])
      end)

    do_bfs([root_id], adj, MapSet.new([root_id]))
    |> Enum.map(fn id -> Enum.find(graph.nodes, &(&1.id == id)) end)
    |> Enum.filter(& &1)
    |> Enum.sort_by(& &1.id)
    |> then(fn nodes -> if root, do: [root | Enum.reject(nodes, &(&1.id == root_id))], else: nodes end)
  end

  defp do_bfs([], _adj, visited), do: MapSet.to_list(visited)

  defp do_bfs([current | rest], adj, visited) do
    neighbors = Map.get(adj, current, [])
    new_neighbors = Enum.filter(neighbors, &(&1 not in visited))
    new_visited = Enum.reduce(new_neighbors, visited, &MapSet.put(&2, &1))
    do_bfs(rest ++ new_neighbors, adj, new_visited)
  end

  # Helper functions

  defp get_node(id) do
    Enum.find(Queries.list_nodes(), &(&1.id == id))
  end

  defp create_node(type, title, confidence) do
    now = DateTime.utc_now() |> DateTime.to_iso8601()
    change_id = UUID.uuid4()

    metadata =
      Jason.encode!(%{
        "confidence" => confidence,
        "branch" => get_current_branch()
      })

    result =
      SQL.query!(
        Repo,
        """
        INSERT INTO decision_nodes (change_id, node_type, title, status, metadata_json, created_at, updated_at)
        VALUES (?, ?, ?, 'active', ?, ?, ?)
        RETURNING id
        """,
        [change_id, type, title, metadata, now, now]
      )

    [[id]] = result.rows
    id
  end

  defp create_edge(from_id, to_id, edge_type, rationale) do
    now = DateTime.utc_now() |> DateTime.to_iso8601()

    SQL.query!(
      Repo,
      """
      INSERT INTO decision_edges (from_node_id, to_node_id, edge_type, rationale, created_at)
      VALUES (?, ?, ?, ?, ?)
      """,
      [from_id, to_id, edge_type, rationale, now]
    )
  end

  defp update_status(id, status) do
    now = DateTime.utc_now() |> DateTime.to_iso8601()

    SQL.query!(
      Repo,
      "UPDATE decision_nodes SET status = ?, updated_at = ? WHERE id = ?",
      [status, now, id]
    )
  end

  defp get_current_branch do
    case System.cmd("git", ["rev-parse", "--abbrev-ref", "HEAD"], stderr_to_stdout: true) do
      {branch, 0} -> String.trim(branch)
      _ -> nil
    end
  end

  # Argument parsing

  defp parse_pivot_args(args), do: parse_pivot_args(args, %{})

  defp parse_pivot_args([], opts), do: opts

  defp parse_pivot_args(["--from", id | rest], opts) do
    case Integer.parse(id) do
      {n, ""} -> parse_pivot_args(rest, Map.put(opts, :from_id, n))
      _ -> parse_pivot_args(rest, opts)
    end
  end

  defp parse_pivot_args(["--observation", text | rest], opts),
    do: parse_pivot_args(rest, Map.put(opts, :observation, text))

  defp parse_pivot_args(["--new-approach", text | rest], opts),
    do: parse_pivot_args(rest, Map.put(opts, :new_approach, text))

  defp parse_pivot_args(["-c", conf | rest], opts) do
    case Integer.parse(conf) do
      {n, ""} -> parse_pivot_args(rest, Map.put(opts, :confidence, n))
      _ -> parse_pivot_args(rest, opts)
    end
  end

  defp parse_pivot_args(["--confidence", conf | rest], opts) do
    case Integer.parse(conf) do
      {n, ""} -> parse_pivot_args(rest, Map.put(opts, :confidence, n))
      _ -> parse_pivot_args(rest, opts)
    end
  end

  defp parse_pivot_args(["--reason", text | rest], opts),
    do: parse_pivot_args(rest, Map.put(opts, :reason, text))

  defp parse_pivot_args([_ | rest], opts), do: parse_pivot_args(rest, opts)

  defp parse_timeline_args(args), do: parse_timeline_args(args, %{})

  defp parse_timeline_args([], opts), do: opts

  defp parse_timeline_args(["--limit", n | rest], opts) do
    case Integer.parse(n) do
      {num, ""} -> parse_timeline_args(rest, Map.put(opts, :limit, num))
      _ -> parse_timeline_args(rest, opts)
    end
  end

  defp parse_timeline_args(["--type", t | rest], opts),
    do: parse_timeline_args(rest, Map.put(opts, :type, t))

  defp parse_timeline_args(["-b", branch | rest], opts),
    do: parse_timeline_args(rest, Map.put(opts, :branch, branch))

  defp parse_timeline_args(["--branch", branch | rest], opts),
    do: parse_timeline_args(rest, Map.put(opts, :branch, branch))

  defp parse_timeline_args(["--json" | rest], opts),
    do: parse_timeline_args(rest, Map.put(opts, :json, true))

  defp parse_timeline_args([_ | rest], opts), do: parse_timeline_args(rest, opts)

  defp parse_supersede_args(args), do: parse_supersede_args(args, %{})

  defp parse_supersede_args([], opts), do: opts

  defp parse_supersede_args(["--cascade" | rest], opts),
    do: parse_supersede_args(rest, Map.put(opts, :cascade, true))

  defp parse_supersede_args(["--dry-run" | rest], opts),
    do: parse_supersede_args(rest, Map.put(opts, :dry_run, true))

  defp parse_supersede_args(["-" <> _ | rest], opts), do: parse_supersede_args(rest, opts)

  defp parse_supersede_args([id | rest], opts) do
    case Integer.parse(id) do
      {n, ""} -> parse_supersede_args(rest, Map.put(opts, :id, n))
      _ -> parse_supersede_args(rest, opts)
    end
  end
end
