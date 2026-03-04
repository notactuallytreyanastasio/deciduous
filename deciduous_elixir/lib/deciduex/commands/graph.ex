defmodule Deciduex.Commands.Graph do
  @moduledoc """
  Implements `deciduex graph` — outputs full graph as JSON.

  Mirrors the output format of `deciduous graph` from the Rust CLI.
  """

  def run do
    graph = Deciduex.Queries.get_graph()

    output = %{
      "nodes" => Enum.map(graph.nodes, &node_to_map/1),
      "edges" => Enum.map(graph.edges, &edge_to_map/1),
      "documents" => []
    }

    IO.puts(Jason.encode!(output, pretty: true))
  end

  defp node_to_map(node) do
    %{
      "id" => node.id,
      "change_id" => node.change_id,
      "node_type" => node.node_type,
      "title" => node.title,
      "description" => node.description,
      "status" => node.status,
      "created_at" => node.created_at,
      "updated_at" => node.updated_at,
      "metadata_json" => node.metadata_json
    }
  end

  defp edge_to_map(edge) do
    %{
      "id" => edge.id,
      "from_node_id" => edge.from_node_id,
      "to_node_id" => edge.to_node_id,
      "from_change_id" => edge.from_change_id,
      "to_change_id" => edge.to_change_id,
      "edge_type" => edge.edge_type,
      "weight" => edge.weight,
      "rationale" => edge.rationale,
      "created_at" => edge.created_at
    }
  end
end
