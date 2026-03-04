defmodule Deciduex.Commands.Add do
  @moduledoc """
  Implements the `add` command to create new decision nodes.

  Usage: deciduex add <type> <title> [options]

  Options:
    -c, --confidence <n>   Confidence level 0-100
    -p, --prompt <text>    User prompt text
    --prompt-stdin         Read prompt from stdin
    -f, --files <list>     Comma-separated file list
    -b, --branch <name>    Git branch name
    --no-branch            Don't auto-detect branch
    --commit <hash>        Git commit hash (use HEAD for current)
    --date <date>          Backdate node (YYYY-MM-DD or RFC3339)
    -d, --description <text>  Node description
  """

  alias Deciduex.Mutations

  @valid_types ~w(goal option decision action outcome observation revisit)

  def run(args) do
    case parse_args(args) do
      {:ok, node_type, title, opts} ->
        create_node(node_type, title, opts)

      {:error, reason} ->
        IO.puts(:stderr, "Error: #{reason}")
        print_usage()
        System.halt(1)
    end
  end

  defp create_node(node_type, title, opts) do
    # Handle prompt from stdin if requested
    prompt = get_effective_prompt(opts)

    # Warn if prompt looks like a summary
    warn_short_prompt(prompt)

    # Auto-detect branch if not disabled
    branch = get_effective_branch(opts)

    # Expand HEAD to actual commit hash
    commit = get_effective_commit(opts[:commit])

    # Parse date if provided
    created_at = parse_date(opts[:date])

    attrs = %{
      node_type: node_type,
      title: title,
      description: opts[:description],
      confidence: opts[:confidence],
      commit: commit,
      prompt: prompt,
      files: opts[:files],
      branch: branch,
      created_at: created_at
    }

    case Mutations.create_node(attrs) do
      {:ok, id} ->
        print_success(id, node_type, title, attrs)
        log_command(node_type, title, opts)

      {:error, reason} ->
        IO.puts(:stderr, "Error: #{inspect(reason)}")
        System.halt(1)
    end
  end

  defp parse_args([]), do: {:error, "Missing node type and title"}
  defp parse_args([_type]), do: {:error, "Missing title"}

  defp parse_args([type | rest]) do
    if type in @valid_types do
      parse_title_and_opts(type, rest, %{})
    else
      {:error, "Invalid node type: #{type}. Valid types: #{Enum.join(@valid_types, ", ")}"}
    end
  end

  defp parse_title_and_opts(_type, [], _opts), do: {:error, "Missing title"}

  defp parse_title_and_opts(type, ["-c", val | rest], opts) do
    parse_title_and_opts(type, rest, Map.put(opts, :confidence, parse_int(val)))
  end

  defp parse_title_and_opts(type, ["--confidence", val | rest], opts) do
    parse_title_and_opts(type, rest, Map.put(opts, :confidence, parse_int(val)))
  end

  defp parse_title_and_opts(type, ["-p", val | rest], opts) do
    parse_title_and_opts(type, rest, Map.put(opts, :prompt, val))
  end

  defp parse_title_and_opts(type, ["--prompt", val | rest], opts) do
    parse_title_and_opts(type, rest, Map.put(opts, :prompt, val))
  end

  defp parse_title_and_opts(type, ["--prompt-stdin" | rest], opts) do
    parse_title_and_opts(type, rest, Map.put(opts, :prompt_stdin, true))
  end

  defp parse_title_and_opts(type, ["-f", val | rest], opts) do
    parse_title_and_opts(type, rest, Map.put(opts, :files, val))
  end

  defp parse_title_and_opts(type, ["--files", val | rest], opts) do
    parse_title_and_opts(type, rest, Map.put(opts, :files, val))
  end

  defp parse_title_and_opts(type, ["-b", val | rest], opts) do
    parse_title_and_opts(type, rest, Map.put(opts, :branch, val))
  end

  defp parse_title_and_opts(type, ["--branch", val | rest], opts) do
    parse_title_and_opts(type, rest, Map.put(opts, :branch, val))
  end

  defp parse_title_and_opts(type, ["--no-branch" | rest], opts) do
    parse_title_and_opts(type, rest, Map.put(opts, :no_branch, true))
  end

  defp parse_title_and_opts(type, ["--commit", val | rest], opts) do
    parse_title_and_opts(type, rest, Map.put(opts, :commit, val))
  end

  defp parse_title_and_opts(type, ["--date", val | rest], opts) do
    parse_title_and_opts(type, rest, Map.put(opts, :date, val))
  end

  defp parse_title_and_opts(type, ["-d", val | rest], opts) do
    parse_title_and_opts(type, rest, Map.put(opts, :description, val))
  end

  defp parse_title_and_opts(type, ["--description", val | rest], opts) do
    parse_title_and_opts(type, rest, Map.put(opts, :description, val))
  end

  defp parse_title_and_opts(type, [title | rest], opts) do
    # First non-flag argument is the title, rest are additional flags
    if String.starts_with?(title, "-") do
      {:error, "Unknown option: #{title}"}
    else
      # Continue parsing remaining args as options
      parse_remaining_opts(type, title, rest, opts)
    end
  end

  defp parse_remaining_opts(type, title, [], opts), do: {:ok, type, title, opts}

  defp parse_remaining_opts(type, title, ["-c", val | rest], opts) do
    parse_remaining_opts(type, title, rest, Map.put(opts, :confidence, parse_int(val)))
  end

  defp parse_remaining_opts(type, title, ["--confidence", val | rest], opts) do
    parse_remaining_opts(type, title, rest, Map.put(opts, :confidence, parse_int(val)))
  end

  defp parse_remaining_opts(type, title, ["-p", val | rest], opts) do
    parse_remaining_opts(type, title, rest, Map.put(opts, :prompt, val))
  end

  defp parse_remaining_opts(type, title, ["--prompt", val | rest], opts) do
    parse_remaining_opts(type, title, rest, Map.put(opts, :prompt, val))
  end

  defp parse_remaining_opts(type, title, ["--prompt-stdin" | rest], opts) do
    parse_remaining_opts(type, title, rest, Map.put(opts, :prompt_stdin, true))
  end

  defp parse_remaining_opts(type, title, ["-f", val | rest], opts) do
    parse_remaining_opts(type, title, rest, Map.put(opts, :files, val))
  end

  defp parse_remaining_opts(type, title, ["--files", val | rest], opts) do
    parse_remaining_opts(type, title, rest, Map.put(opts, :files, val))
  end

  defp parse_remaining_opts(type, title, ["-b", val | rest], opts) do
    parse_remaining_opts(type, title, rest, Map.put(opts, :branch, val))
  end

  defp parse_remaining_opts(type, title, ["--branch", val | rest], opts) do
    parse_remaining_opts(type, title, rest, Map.put(opts, :branch, val))
  end

  defp parse_remaining_opts(type, title, ["--no-branch" | rest], opts) do
    parse_remaining_opts(type, title, rest, Map.put(opts, :no_branch, true))
  end

  defp parse_remaining_opts(type, title, ["--commit", val | rest], opts) do
    parse_remaining_opts(type, title, rest, Map.put(opts, :commit, val))
  end

  defp parse_remaining_opts(type, title, ["--date", val | rest], opts) do
    parse_remaining_opts(type, title, rest, Map.put(opts, :date, val))
  end

  defp parse_remaining_opts(type, title, ["-d", val | rest], opts) do
    parse_remaining_opts(type, title, rest, Map.put(opts, :description, val))
  end

  defp parse_remaining_opts(type, title, ["--description", val | rest], opts) do
    parse_remaining_opts(type, title, rest, Map.put(opts, :description, val))
  end

  defp parse_remaining_opts(_type, _title, [unknown | _], _opts) do
    {:error, "Unknown option: #{unknown}"}
  end

  defp parse_int(val) do
    case Integer.parse(val) do
      {n, ""} -> min(n, 100)
      _ -> nil
    end
  end

  defp get_effective_prompt(opts) do
    if opts[:prompt_stdin] do
      case IO.read(:stdio, :eof) do
        {:error, _} -> nil
        data -> String.trim(data) |> empty_to_nil()
      end
    else
      opts[:prompt]
    end
  end

  defp empty_to_nil(""), do: nil
  defp empty_to_nil(s), do: s

  defp warn_short_prompt(nil), do: :ok

  defp warn_short_prompt(prompt) when byte_size(prompt) < 200 do
    IO.puts(:stderr, "Warning: Prompt is only #{byte_size(prompt)} chars. This looks like a summary, not a full prompt.")
    IO.puts(:stderr, "         Capture the verbatim user message for better context recovery.")
  end

  defp warn_short_prompt(_), do: :ok

  defp get_effective_branch(opts) do
    cond do
      opts[:no_branch] -> nil
      opts[:branch] -> opts[:branch]
      true -> get_current_git_branch()
    end
  end

  defp get_current_git_branch do
    case System.cmd("git", ["rev-parse", "--abbrev-ref", "HEAD"], stderr_to_stdout: true) do
      {branch, 0} -> String.trim(branch)
      _ -> nil
    end
  end

  defp get_effective_commit(nil), do: nil

  defp get_effective_commit(commit) do
    if String.downcase(commit) == "head" do
      case System.cmd("git", ["rev-parse", "HEAD"], stderr_to_stdout: true) do
        {hash, 0} -> String.trim(hash)
        _ -> commit
      end
    else
      commit
    end
  end

  defp parse_date(nil), do: nil

  defp parse_date(date_str) do
    cond do
      # Try RFC3339 first
      match?({:ok, _, _}, DateTime.from_iso8601(date_str)) ->
        date_str

      # Try YYYY-MM-DD format
      match?({:ok, _}, Date.from_iso8601(date_str)) ->
        {:ok, date} = Date.from_iso8601(date_str)
        DateTime.new!(date, ~T[00:00:00], "Etc/UTC") |> DateTime.to_iso8601()

      true ->
        IO.puts(:stderr, "Warning: Could not parse date '#{date_str}'. Use RFC3339 or YYYY-MM-DD format.")
        date_str
    end
  end

  defp print_success(id, node_type, title, attrs) do
    parts = ["Created node #{id} (type: #{node_type}, title: #{title})"]

    parts = if attrs[:confidence], do: parts ++ ["[confidence: #{attrs[:confidence]}%]"], else: parts
    parts = if attrs[:commit], do: parts ++ ["[commit: #{String.slice(attrs[:commit], 0..6)}]"], else: parts
    parts = if attrs[:prompt], do: parts ++ ["[prompt: #{byte_size(attrs[:prompt])} chars]"], else: parts
    parts = if attrs[:files], do: parts ++ ["[files: #{attrs[:files]}]"], else: parts
    parts = if attrs[:branch], do: parts ++ ["[branch: #{attrs[:branch]}]"], else: parts
    parts = if attrs[:created_at], do: parts ++ ["[date: #{attrs[:created_at]}]"], else: parts

    IO.puts(Enum.join(parts, " "))
  end

  defp log_command(node_type, title, opts) do
    args = build_command_args(node_type, title, opts)
    Mutations.log_command("add", args, 0)
  end

  defp build_command_args(node_type, title, opts) do
    args = [node_type, title]
    args = if opts[:confidence], do: args ++ ["-c", to_string(opts[:confidence])], else: args
    args = if opts[:branch], do: args ++ ["-b", opts[:branch]], else: args
    args = if opts[:commit], do: args ++ ["--commit", opts[:commit]], else: args
    args
  end

  defp print_usage do
    IO.puts(:stderr, """
    Usage: deciduex add <type> <title> [options]

    Types: #{Enum.join(@valid_types, ", ")}

    Options:
      -c, --confidence <n>     Confidence level 0-100
      -p, --prompt <text>      User prompt text
      --prompt-stdin           Read prompt from stdin
      -f, --files <list>       Comma-separated file list
      -b, --branch <name>      Git branch name
      --no-branch              Don't auto-detect branch
      --commit <hash>          Git commit hash (use HEAD for current)
      --date <date>            Backdate node (YYYY-MM-DD or RFC3339)
      -d, --description <text> Node description
    """)
  end
end
