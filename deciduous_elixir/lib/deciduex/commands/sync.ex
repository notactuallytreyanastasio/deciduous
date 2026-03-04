defmodule Deciduex.Commands.Sync do
  @moduledoc """
  Implements the `sync` command to export the decision graph.

  Usage: deciduex sync [output_path]

  Exports the decision graph to JSON for static hosting (GitHub Pages).
  Default output: docs/graph-data.json
  """

  alias Deciduex.Mutations
  alias Deciduex.Queries

  def run(args) do
    output_path = get_output_path(args)
    export_graph(output_path)
  end

  defp get_output_path([]), do: "docs/graph-data.json"
  defp get_output_path([path | _]), do: path

  defp export_graph(output_path) do
    # Create parent directories if needed
    output_path |> Path.dirname() |> File.mkdir_p!()

    graph = Queries.get_graph()
    json = Jason.encode!(graph, pretty: true)

    case File.write(output_path, json) do
      :ok ->
        IO.puts("Exported graph to #{output_path}")
        IO.puts("  #{length(graph.nodes)} nodes, #{length(graph.edges)} edges")
        export_to_demo(json)
        export_git_history(graph.nodes, Path.dirname(output_path))
        Mutations.log_command("sync", [output_path], 0)

      {:error, reason} ->
        IO.puts(:stderr, "Error writing file: #{inspect(reason)}")
        Deciduex.CLI.exit_with_error()
    end
  end

  defp export_to_demo(json) do
    demo_path = "docs/demo/graph-data.json"

    if File.exists?(Path.dirname(demo_path)) do
      case File.write(demo_path, json) do
        :ok -> :ok
        {:error, reason} -> IO.puts(:stderr, "Warning: Also writing to demo/: #{inspect(reason)}")
      end
    end
  end

  defp export_git_history(nodes, output_dir) do
    # Extract commit hashes from node metadata
    commits =
      nodes
      |> Enum.filter(&has_commit?/1)
      |> Enum.map(&extract_commit/1)
      |> Enum.uniq()
      |> Enum.filter(&(&1 != nil))
      |> Enum.map(&get_git_commit_info/1)
      |> Enum.filter(&(&1 != nil))
      |> Enum.sort_by(& &1.date, :desc)

    write_git_history(commits, output_dir)
  end

  defp write_git_history([], _output_dir), do: :ok

  defp write_git_history(commits, output_dir) do
    history_path = Path.join(output_dir, "git-history.json")
    json = Jason.encode!(commits, pretty: true)

    case File.write(history_path, json) do
      :ok -> IO.puts("Exported git-history.json (#{length(commits)} commits)")
      {:error, _} -> :ok
    end
  end

  defp has_commit?(%{metadata_json: nil}), do: false

  defp has_commit?(%{metadata_json: json}) do
    case Jason.decode(json) do
      {:ok, %{"commit" => commit}} when is_binary(commit) and commit != "" -> true
      _ -> false
    end
  end

  defp extract_commit(%{metadata_json: json}) do
    case Jason.decode(json) do
      {:ok, %{"commit" => commit}} -> commit
      _ -> nil
    end
  end

  defp get_git_commit_info(hash) do
    case System.cmd("git", ["log", "-1", "--format=%H%x00%an%x00%aI%x00%B", hash],
           stderr_to_stdout: true
         ) do
      {output, 0} ->
        output |> String.trim() |> String.split("\x00") |> parse_git_parts(hash)

      _ ->
        nil
    end
  end

  defp parse_git_parts([full_hash, author, date, message | _], hash) do
    %{
      hash: full_hash,
      short_hash: String.slice(full_hash, 0, 7),
      author: author,
      date: date,
      message: String.trim(message),
      files_changed: get_files_changed(hash)
    }
  end

  defp parse_git_parts(_, _hash), do: nil

  defp get_files_changed(hash) do
    case System.cmd("git", ["diff-tree", "--no-commit-id", "--name-only", "-r", hash],
           stderr_to_stdout: true
         ) do
      {output, 0} ->
        output |> String.trim() |> String.split("\n") |> length()

      _ ->
        nil
    end
  end
end
