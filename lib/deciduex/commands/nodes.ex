defmodule Deciduex.Commands.Nodes do
  @moduledoc """
  Implements `deciduex nodes` — lists all decision graph nodes.

  Mirrors the output format of `deciduous nodes` from the Rust CLI.
  """

  alias Deciduex.Queries

  @type_width 12
  @status_width 10
  @id_width 5

  def run(args) do
    opts = parse_args(args)

    Queries.list_nodes()
    |> apply_filters(opts)
    |> render()
  end

  defp parse_args(args, acc \\ %{})
  defp parse_args([], acc), do: acc

  defp parse_args([flag, value | rest], acc) when flag in ["-b", "--branch"] do
    parse_args(rest, Map.put(acc, :branch, value))
  end

  defp parse_args([flag, value | rest], acc) when flag in ["-t", "--type"] do
    parse_args(rest, Map.put(acc, :type, value))
  end

  defp parse_args([_ | rest], acc) do
    parse_args(rest, acc)
  end

  defp apply_filters(nodes, opts) do
    nodes
    |> maybe_filter_type(opts[:type])
    |> maybe_filter_branch(opts[:branch])
  end

  defp maybe_filter_type(nodes, nil), do: nodes

  defp maybe_filter_type(nodes, type) do
    Enum.filter(nodes, &(&1.node_type == type))
  end

  defp maybe_filter_branch(nodes, nil), do: nodes

  defp maybe_filter_branch(nodes, branch) do
    Enum.filter(nodes, fn node ->
      match?({:ok, %{"branch" => ^branch}}, Jason.decode(node.metadata_json || ""))
    end)
  end

  defp render(nodes) do
    count = length(nodes)
    IO.puts("#{count} nodes:")

    header =
      String.pad_trailing("ID", @id_width) <>
        "  " <>
        String.pad_trailing("TYPE", @type_width) <>
        "  " <>
        String.pad_trailing("STATUS", @status_width) <>
        "  " <>
        "TITLE"

    IO.puts(header)

    separator =
      String.duplicate("-", @id_width) <>
        "  " <>
        String.duplicate("-", @type_width) <>
        "  " <>
        String.duplicate("-", @status_width) <>
        "  " <>
        String.duplicate("-", 30)

    IO.puts(separator)

    Enum.each(nodes, fn node ->
      line =
        node.id
        |> to_string()
        |> String.pad_trailing(@id_width)

      IO.puts(
        line <>
          "  " <>
          String.pad_trailing(node.node_type, @type_width) <>
          "  " <>
          String.pad_trailing(node.status, @status_width) <>
          "  " <>
          node.title
      )
    end)
  end
end
