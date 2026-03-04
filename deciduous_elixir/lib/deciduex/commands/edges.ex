defmodule Deciduex.Commands.Edges do
  @moduledoc """
  Implements `deciduex edges` — lists all decision graph edges.

  Mirrors the output format of `deciduous edges` from the Rust CLI.
  """

  alias Deciduex.Queries

  @id_width 5
  @from_width 6
  @to_width 6
  @type_width 12

  def run do
    edges = Queries.list_edges()

    if edges == [] do
      IO.puts("No edges found. Link nodes with: deciduous link 1 2 -r \"reason\"")
    else
      header =
        String.pad_trailing("ID", @id_width) <>
          " " <>
          String.pad_trailing("FROM", @from_width) <>
          " " <>
          String.pad_trailing("TO", @to_width) <>
          " " <>
          String.pad_trailing("TYPE", @type_width) <>
          " " <>
          "RATIONALE"

      IO.puts(header)
      IO.puts(String.duplicate("-", 70))

      Enum.each(edges, fn edge ->
        IO.puts(
          String.pad_trailing(to_string(edge.id), @id_width) <>
            " " <>
            String.pad_trailing(to_string(edge.from_node_id), @from_width) <>
            " " <>
            String.pad_trailing(to_string(edge.to_node_id), @to_width) <>
            " " <>
            String.pad_trailing(edge.edge_type, @type_width) <>
            " " <>
            (edge.rationale || "")
        )
      end)
    end
  end
end
