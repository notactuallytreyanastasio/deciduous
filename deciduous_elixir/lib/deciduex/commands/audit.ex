defmodule Deciduex.Commands.Audit do
  @moduledoc """
  Implements the `audit` command to maintain graph data quality.

  Usage: deciduex audit [options]

  Options:
    --associate-commits    Match commits to nodes by title keywords
    --min-score N          Minimum keyword match score (0-100, default 50)
    --dry-run              Show what would be done without applying
    --yes                  Skip confirmation prompts
  """

  alias Deciduex.Queries
  alias Deciduex.Repo

  alias Ecto.Adapters.SQL

  def run(args) do
    args
    |> parse_args()
    |> execute_command()
  end

  defp execute_command(%{associate_commits: true} = opts), do: associate_commits(opts)
  defp execute_command(_opts), do: show_graph_health()

  defp show_graph_health do
    graph = Queries.get_graph()

    # Find orphan nodes (no incoming or outgoing edges)
    connected_ids = get_connected_node_ids(graph.edges)

    orphans =
      Enum.filter(graph.nodes, fn n -> n.id not in connected_ids and n.node_type != "goal" end)

    # Find nodes missing change_id
    missing_change_id = Enum.filter(graph.nodes, &(is_nil(&1.change_id) or &1.change_id == ""))

    # Find nodes without commits
    nodes_without_commits = Enum.filter(graph.nodes, &node_missing_commit?/1)

    IO.puts("Graph Health Report")
    IO.puts("==================")
    IO.puts("")
    IO.puts("Total nodes: #{length(graph.nodes)}")
    IO.puts("Total edges: #{length(graph.edges)}")
    IO.puts("")

    if Enum.any?(orphans) do
      IO.puts("Orphan nodes (no connections, excluding goals): #{length(orphans)}")
      Enum.each(orphans, fn n -> IO.puts("  - [#{n.id}] #{n.title}") end)
      IO.puts("")
    end

    if Enum.any?(missing_change_id) do
      IO.puts("Nodes missing change_id: #{length(missing_change_id)}")
      Enum.each(missing_change_id, fn n -> IO.puts("  - [#{n.id}] #{n.title}") end)
      IO.puts("")
    end

    action_outcome_count = Enum.count(graph.nodes, &(&1.node_type in ["action", "outcome"]))

    if Enum.any?(nodes_without_commits) do
      IO.puts(
        "Action/outcome nodes without commits: #{length(nodes_without_commits)}/#{action_outcome_count}"
      )

      IO.puts("  Run with --associate-commits to auto-link")
    else
      IO.puts("All action/outcome nodes have commits linked")
    end
  end

  defp get_connected_node_ids(edges) do
    edges
    |> Enum.flat_map(fn e -> [e.from_node_id, e.to_node_id] end)
    |> MapSet.new()
  end

  defp node_missing_commit?(%{node_type: type, metadata_json: json})
       when type in ["action", "outcome"] do
    case Jason.decode(json || "{}") do
      {:ok, %{"commit" => commit}} when is_binary(commit) and commit != "" -> false
      _ -> true
    end
  end

  defp node_missing_commit?(_), do: false

  defp associate_commits(opts) do
    min_score = (opts[:min_score] || 50) / 100.0
    graph = Queries.get_graph()
    commits = get_git_commits()

    nodes_to_check = find_nodes_needing_commits(graph.nodes)

    case nodes_to_check do
      [] ->
        IO.puts("All action/outcome nodes already have commits linked.")

      nodes ->
        matches = find_commit_matches(nodes, commits, min_score)
        process_matches(matches, min_score, opts)
    end
  end

  defp find_nodes_needing_commits(nodes) do
    Enum.filter(nodes, fn node ->
      node.node_type in ["action", "outcome"] and node_missing_commit?(node)
    end)
  end

  defp find_commit_matches(nodes, commits, min_score) do
    IO.puts("Finding commit matches for #{length(nodes)} nodes...")
    IO.puts("")

    Enum.flat_map(nodes, fn node ->
      find_best_match(node, commits, min_score)
    end)
  end

  defp find_best_match(node, commits, min_score) do
    best_match =
      commits
      |> Enum.map(fn commit -> {commit, keyword_match_score(node.title, commit.message)} end)
      |> Enum.filter(fn {_, score} -> score >= min_score end)
      |> Enum.max_by(fn {_, score} -> score end, fn -> nil end)

    case best_match do
      {commit, score} -> [%{node: node, commit: commit, score: score}]
      nil -> []
    end
  end

  defp process_matches([], min_score, _opts) do
    IO.puts("No matches found above threshold (#{round(min_score * 100)}%)")
  end

  defp process_matches(matches, _min_score, opts) do
    print_matches(matches)
    maybe_apply_matches(matches, opts)
  end

  defp print_matches(matches) do
    IO.puts("Found #{length(matches)} potential matches:")
    IO.puts("")

    Enum.each(matches, fn m ->
      IO.puts("  [#{m.node.id}] #{m.node.title}")
      IO.puts("    -> #{String.slice(m.commit.hash, 0, 7)} #{m.commit.message}")
      IO.puts("    Score: #{round(m.score * 100)}%")
      IO.puts("")
    end)
  end

  defp maybe_apply_matches(_matches, %{dry_run: true}) do
    IO.puts("Dry run - no changes applied")
  end

  defp maybe_apply_matches(matches, opts) do
    if opts[:yes] || confirm?("Apply #{length(matches)} matches?") do
      apply_matches(matches)
      IO.puts("Applied #{length(matches)} commit associations")
    else
      IO.puts("Aborted")
    end
  end

  defp get_git_commits do
    case System.cmd("git", ["log", "--format=%H|%s", "--since=2024-11-01"],
           stderr_to_stdout: true
         ) do
      {output, 0} ->
        output
        |> String.trim()
        |> String.split("\n")
        |> Enum.flat_map(&parse_commit_line/1)

      _ ->
        []
    end
  end

  defp parse_commit_line(line) do
    case String.split(line, "|", parts: 2) do
      [hash, message] -> [%{hash: hash, message: message}]
      _ -> []
    end
  end

  defp keyword_match_score(title, message) do
    title_words = extract_keywords(title)
    message_words = extract_keywords(message)

    if Enum.empty?(title_words) do
      0.0
    else
      matches = Enum.count(title_words, &(&1 in message_words))
      matches / length(title_words)
    end
  end

  defp extract_keywords(text) do
    text
    |> String.downcase()
    |> String.replace(~r/[^a-z0-9\s]/, "")
    |> String.split()
    |> Enum.filter(&(String.length(&1) > 2))
    |> Enum.uniq()
  end

  defp apply_matches(matches) do
    Enum.each(matches, fn m ->
      update_node_commit(m.node.id, m.commit.hash)
    end)
  end

  defp update_node_commit(node_id, commit_hash) do
    result = SQL.query!(Repo, "SELECT metadata_json FROM decision_nodes WHERE id = ?", [node_id])

    case result.rows do
      [[json]] ->
        metadata = Jason.decode!(json || "{}")
        new_metadata = Map.put(metadata, "commit", commit_hash)
        new_json = Jason.encode!(new_metadata)

        SQL.query!(
          Repo,
          "UPDATE decision_nodes SET metadata_json = ?, updated_at = ? WHERE id = ?",
          [new_json, DateTime.utc_now() |> DateTime.to_iso8601(), node_id]
        )

      _ ->
        :ok
    end
  end

  defp confirm?(prompt) do
    IO.write("#{prompt} [y/N] ")

    case IO.gets("") do
      input when is_binary(input) -> String.trim(input) in ["y", "Y", "yes", "Yes"]
      _ -> false
    end
  end

  defp parse_args(args), do: parse_args(args, %{})

  defp parse_args([], opts), do: opts

  defp parse_args(["--associate-commits" | rest], opts),
    do: parse_args(rest, Map.put(opts, :associate_commits, true))

  defp parse_args(["--min-score", score | rest], opts) do
    case Integer.parse(score) do
      {n, ""} -> parse_args(rest, Map.put(opts, :min_score, n))
      _ -> parse_args(rest, opts)
    end
  end

  defp parse_args(["--dry-run" | rest], opts), do: parse_args(rest, Map.put(opts, :dry_run, true))
  defp parse_args(["--yes" | rest], opts), do: parse_args(rest, Map.put(opts, :yes, true))
  defp parse_args([_ | rest], opts), do: parse_args(rest, opts)
end
