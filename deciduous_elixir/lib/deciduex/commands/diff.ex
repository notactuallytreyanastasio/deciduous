defmodule Deciduex.Commands.Diff do
  @moduledoc """
  Implements the `diff` command for multi-user graph sync.

  Usage: deciduex diff <subcommand> [options]

  Subcommands:
    export    Export nodes as a patch file
    apply     Apply patch files (idempotent)
    status    List available patches
    validate  Validate patch files
  """

  alias Deciduex.Queries
  alias Deciduex.Repo
  alias Ecto.Adapters.SQL

  @patch_version "1.0"

  def run(["export" | args]) do
    opts = parse_export_args(args)
    export_patch(opts)
  end

  def run(["apply" | args]) do
    opts = parse_apply_args(args)
    apply_patches(opts)
  end

  def run(["status" | args]) do
    path = List.first(args) || ".deciduous/patches"
    show_status(path)
  end

  def run(["validate" | args]) do
    validate_patches(args)
  end

  def run([]) do
    IO.puts(:stderr, "Usage: deciduex diff <subcommand>")
    IO.puts(:stderr, "")
    IO.puts(:stderr, "Subcommands:")
    IO.puts(:stderr, "  export    Export nodes as a patch file")
    IO.puts(:stderr, "  apply     Apply patch files (idempotent)")
    IO.puts(:stderr, "  status    List available patches")
    IO.puts(:stderr, "  validate  Validate patch files")
    System.halt(1)
  end

  def run([unknown | _]) do
    IO.puts(:stderr, "Unknown diff subcommand: #{unknown}")
    System.halt(1)
  end

  # Export subcommand

  defp export_patch(opts) do
    unless opts[:output] do
      IO.puts(:stderr, "Error: --output/-o is required")
      System.halt(1)
    end

    graph = Queries.get_graph()
    filtered = apply_export_filters(graph, opts)

    patch = build_patch(filtered, opts)
    json = Jason.encode!(patch, pretty: true)

    output_path = opts[:output]
    output_path |> Path.dirname() |> File.mkdir_p!()

    case File.write(output_path, json) do
      :ok ->
        IO.puts("Exported patch to #{output_path}")
        IO.puts("  #{length(patch.nodes)} nodes, #{length(patch.edges)} edges")

      {:error, reason} ->
        IO.puts(:stderr, "Error writing file: #{inspect(reason)}")
        System.halt(1)
    end
  end

  defp apply_export_filters(graph, opts) do
    graph
    |> filter_by_node_ids(opts[:nodes])
    |> filter_by_branch(opts[:branch])
  end

  defp filter_by_node_ids(graph, nil), do: graph

  defp filter_by_node_ids(graph, node_spec) do
    ids = parse_node_range(node_spec) |> MapSet.new()
    nodes = Enum.filter(graph.nodes, &(&1.id in ids))
    edges = Enum.filter(graph.edges, &(&1.from_node_id in ids and &1.to_node_id in ids))
    %{graph | nodes: nodes, edges: edges}
  end

  defp filter_by_branch(graph, nil), do: graph

  defp filter_by_branch(graph, branch) do
    nodes = Enum.filter(graph.nodes, &node_matches_branch?(&1, branch))
    node_ids = MapSet.new(nodes, & &1.id)
    edges = Enum.filter(graph.edges, &(&1.from_node_id in node_ids and &1.to_node_id in node_ids))
    %{graph | nodes: nodes, edges: edges}
  end

  defp node_matches_branch?(%{metadata_json: nil}, _branch), do: false

  defp node_matches_branch?(%{metadata_json: json}, branch) do
    case Jason.decode(json) do
      {:ok, %{"branch" => node_branch}} -> node_branch == branch
      _ -> false
    end
  end

  defp build_patch(graph, opts) do
    %{
      version: @patch_version,
      author: opts[:author],
      branch: opts[:branch] || get_current_branch(),
      created_at: DateTime.utc_now() |> DateTime.to_iso8601(),
      base_commit: opts[:base_commit] || get_head_commit(),
      nodes: Enum.map(graph.nodes, &node_to_patch_node/1),
      edges: Enum.map(graph.edges, &edge_to_patch_edge(graph.nodes, &1))
    }
  end

  defp node_to_patch_node(node) do
    %{
      change_id: node.change_id,
      node_type: node.node_type,
      title: node.title,
      description: node.description,
      status: node.status,
      metadata_json: node.metadata_json,
      created_at: node.created_at
    }
  end

  defp edge_to_patch_edge(nodes, edge) do
    from_node = Enum.find(nodes, &(&1.id == edge.from_node_id))
    to_node = Enum.find(nodes, &(&1.id == edge.to_node_id))

    %{
      from_change_id: from_node && from_node.change_id,
      to_change_id: to_node && to_node.change_id,
      edge_type: edge.edge_type,
      rationale: edge.rationale
    }
  end

  # Apply subcommand

  defp apply_patches(opts) do
    files = opts[:files] || []

    if Enum.empty?(files) do
      IO.puts(:stderr, "Error: No patch files specified")
      System.halt(1)
    end

    dry_run = opts[:dry_run] || false

    {total_nodes_added, total_nodes_skipped, total_edges_added, total_edges_skipped} =
      Enum.reduce(files, {0, 0, 0, 0}, fn file, {na, ns, ea, es} ->
        case apply_patch_file(file, dry_run) do
          {:ok, result} ->
            {na + result.nodes_added, ns + result.nodes_skipped,
             ea + result.edges_added, es + result.edges_skipped}

          {:error, reason} ->
            IO.puts(:stderr, "Error processing #{file}: #{reason}")
            {na, ns, ea, es}
        end
      end)

    if dry_run do
      IO.puts("\nDry run complete (no changes made)")
    end

    IO.puts(
      "\nTotal: #{total_nodes_added} nodes added, #{total_nodes_skipped} skipped; " <>
        "#{total_edges_added} edges added, #{total_edges_skipped} skipped"
    )
  end

  defp apply_patch_file(file, dry_run) do
    with {:ok, content} <- File.read(file),
         {:ok, patch} <- Jason.decode(content) do
      apply_patch(patch, file, dry_run)
    else
      {:error, %Jason.DecodeError{} = reason} ->
        {:error, "Invalid JSON: #{inspect(reason)}"}

      {:error, reason} ->
        {:error, "Cannot read file: #{inspect(reason)}"}
    end
  end

  defp apply_patch(patch, file, dry_run) do
    nodes = patch["nodes"] || []
    edges = patch["edges"] || []

    # Get existing change_ids to skip duplicates
    existing_node_ids = get_existing_node_change_ids()
    existing_edge_pairs = get_existing_edge_pairs()

    {nodes_added, nodes_skipped} = apply_nodes(nodes, existing_node_ids, dry_run)
    {edges_added, edges_skipped} = apply_edges(edges, existing_edge_pairs, dry_run)

    action = if dry_run, do: "Would apply", else: "Applied"
    IO.puts("#{action} #{Path.basename(file)}: #{nodes_added} nodes, #{edges_added} edges")

    {:ok,
     %{
       nodes_added: nodes_added,
       nodes_skipped: nodes_skipped,
       edges_added: edges_added,
       edges_skipped: edges_skipped
     }}
  end

  defp get_existing_node_change_ids do
    SQL.query!(Repo, "SELECT change_id FROM decision_nodes")
    |> Map.get(:rows)
    |> Enum.map(&List.first/1)
    |> MapSet.new()
  end

  defp get_existing_edge_pairs do
    SQL.query!(Repo, "SELECT from_change_id, to_change_id FROM decision_edges")
    |> Map.get(:rows)
    |> Enum.map(fn [from, to] -> {from, to} end)
    |> MapSet.new()
  end

  defp apply_nodes(nodes, existing_ids, dry_run) do
    Enum.reduce(nodes, {0, 0}, fn node, acc ->
      apply_single_node(node, existing_ids, dry_run, acc)
    end)
  end

  defp apply_single_node(node, existing_ids, dry_run, {added, skipped}) do
    change_id = node["change_id"]

    if MapSet.member?(existing_ids, change_id) do
      {added, skipped + 1}
    else
      unless dry_run, do: insert_node(node)
      {added + 1, skipped}
    end
  end

  defp insert_node(node) do
    now = DateTime.utc_now() |> DateTime.to_iso8601()

    SQL.query!(
      Repo,
      """
      INSERT INTO decision_nodes (change_id, node_type, title, description, status, metadata_json, created_at, updated_at)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?)
      """,
      [
        node["change_id"],
        node["node_type"],
        node["title"],
        node["description"],
        node["status"] || "active",
        node["metadata_json"],
        node["created_at"] || now,
        now
      ]
    )
  end

  defp apply_edges(edges, existing_pairs, dry_run) do
    Enum.reduce(edges, {0, 0}, fn edge, acc ->
      apply_single_edge(edge, existing_pairs, dry_run, acc)
    end)
  end

  defp apply_single_edge(edge, existing_pairs, dry_run, {added, skipped}) do
    from_id = edge["from_change_id"]
    to_id = edge["to_change_id"]
    pair = {from_id, to_id}

    if MapSet.member?(existing_pairs, pair) do
      {added, skipped + 1}
    else
      unless dry_run, do: insert_edge(edge)
      {added + 1, skipped}
    end
  end

  defp insert_edge(edge) do
    # Look up node IDs from change_ids
    from_id = get_node_id_by_change_id(edge["from_change_id"])
    to_id = get_node_id_by_change_id(edge["to_change_id"])

    if from_id && to_id do
      now = DateTime.utc_now() |> DateTime.to_iso8601()

      SQL.query!(
        Repo,
        """
        INSERT INTO decision_edges (from_node_id, to_node_id, from_change_id, to_change_id, edge_type, rationale, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        """,
        [
          from_id,
          to_id,
          edge["from_change_id"],
          edge["to_change_id"],
          edge["edge_type"] || "leads_to",
          edge["rationale"],
          now
        ]
      )
    end
  end

  defp get_node_id_by_change_id(change_id) do
    result = SQL.query!(Repo, "SELECT id FROM decision_nodes WHERE change_id = ?", [change_id])

    case result.rows do
      [[id] | _] -> id
      _ -> nil
    end
  end

  # Status subcommand

  defp show_status(path) do
    unless File.exists?(path) do
      IO.puts("Info: No patches directory found at #{path}")
      IO.puts("Create one with: mkdir -p #{path}")
      return()
    end

    case File.ls(path) do
      {:ok, files} ->
        files
        |> Enum.filter(&String.ends_with?(&1, ".json"))
        |> show_patches_list(path)

      {:error, reason} ->
        IO.puts(:stderr, "Error reading directory: #{inspect(reason)}")
    end
  end

  defp show_patches_list([], path), do: IO.puts("No patch files found in #{path}")

  defp show_patches_list(patches, path) do
    IO.puts("Available patches in #{path}:")
    Enum.each(patches, &show_patch_info(Path.join(path, &1), &1))
  end

  defp show_patch_info(path, filename) do
    with {:ok, content} <- File.read(path),
         {:ok, patch} <- Jason.decode(content) do
      print_patch_summary(filename, patch)
    else
      {:error, %Jason.DecodeError{}} -> IO.puts("  #{filename}: Invalid JSON")
      {:error, _} -> IO.puts("  #{filename}: Cannot read")
    end
  end

  defp print_patch_summary(filename, patch) do
    nodes = patch["nodes"] || []
    edges = patch["edges"] || []
    author = patch["author"] || "unknown"
    branch = patch["branch"] || "unknown"
    IO.puts("  #{filename}: #{length(nodes)} nodes, #{length(edges)} edges (#{author}, #{branch})")
  end

  # Validate subcommand

  defp validate_patches([]) do
    IO.puts(:stderr, "Error: No patch files specified")
    System.halt(1)
  end

  defp validate_patches(files) do
    any_errors =
      Enum.reduce(files, false, fn file, has_errors ->
        case validate_patch_file(file) do
          :ok -> has_errors
          :error -> true
        end
      end)

    if any_errors do
      System.halt(1)
    end
  end

  defp validate_patch_file(file) do
    with {:ok, content} <- File.read(file),
         {:ok, patch} <- Jason.decode(content) do
      validate_patch_structure(patch, file)
    else
      {:error, %Jason.DecodeError{} = reason} ->
        IO.puts(:stderr, "#{file}: Invalid JSON - #{inspect(reason)}")
        :error

      {:error, reason} ->
        IO.puts(:stderr, "#{file}: Cannot read - #{inspect(reason)}")
        :error
    end
  end

  defp validate_patch_structure(patch, file) do
    nodes = patch["nodes"] || []
    edges = patch["edges"] || []

    # Collect all node change_ids
    node_ids = MapSet.new(nodes, & &1["change_id"])

    # Check all edges reference valid nodes
    errors =
      Enum.flat_map(edges, fn edge ->
        from_id = edge["from_change_id"]
        to_id = edge["to_change_id"]

        errors = []
        errors = if from_id && !MapSet.member?(node_ids, from_id), do: ["Missing node: #{from_id}" | errors], else: errors
        errors = if to_id && !MapSet.member?(node_ids, to_id), do: ["Missing node: #{to_id}" | errors], else: errors
        errors
      end)

    if Enum.empty?(errors) do
      IO.puts("#{file}: Valid (#{length(nodes)} nodes, #{length(edges)} edges)")
      :ok
    else
      IO.puts(:stderr, "#{file}: Invalid")
      Enum.each(errors, &IO.puts(:stderr, "  #{&1}"))
      :error
    end
  end

  # Helper functions

  defp get_current_branch do
    case System.cmd("git", ["rev-parse", "--abbrev-ref", "HEAD"], stderr_to_stdout: true) do
      {branch, 0} -> String.trim(branch)
      _ -> nil
    end
  end

  defp get_head_commit do
    case System.cmd("git", ["rev-parse", "HEAD"], stderr_to_stdout: true) do
      {hash, 0} -> String.trim(hash)
      _ -> nil
    end
  end

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

  # Argument parsing

  defp parse_export_args(args), do: parse_export_args(args, %{})

  defp parse_export_args([], opts), do: opts
  defp parse_export_args(["-o", path | rest], opts), do: parse_export_args(rest, Map.put(opts, :output, path))
  defp parse_export_args(["--output", path | rest], opts), do: parse_export_args(rest, Map.put(opts, :output, path))
  defp parse_export_args(["-n", nodes | rest], opts), do: parse_export_args(rest, Map.put(opts, :nodes, nodes))
  defp parse_export_args(["--nodes", nodes | rest], opts), do: parse_export_args(rest, Map.put(opts, :nodes, nodes))
  defp parse_export_args(["-b", branch | rest], opts), do: parse_export_args(rest, Map.put(opts, :branch, branch))
  defp parse_export_args(["--branch", branch | rest], opts), do: parse_export_args(rest, Map.put(opts, :branch, branch))
  defp parse_export_args(["--author", author | rest], opts), do: parse_export_args(rest, Map.put(opts, :author, author))
  defp parse_export_args(["--base-commit", commit | rest], opts), do: parse_export_args(rest, Map.put(opts, :base_commit, commit))
  defp parse_export_args([_ | rest], opts), do: parse_export_args(rest, opts)

  defp parse_apply_args(args), do: parse_apply_args(args, %{files: []})

  defp parse_apply_args([], opts), do: opts
  defp parse_apply_args(["--dry-run" | rest], opts), do: parse_apply_args(rest, Map.put(opts, :dry_run, true))

  defp parse_apply_args(["-" <> _ | rest], opts), do: parse_apply_args(rest, opts)

  defp parse_apply_args([file | rest], opts) do
    files = [file | opts[:files] || []]
    parse_apply_args(rest, Map.put(opts, :files, files))
  end

  # Suppress warning about unused return
  defp return, do: :ok
end
