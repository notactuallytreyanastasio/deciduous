defmodule Deciduex.Queries do
  @moduledoc """
  Centralized Ecto queries for reading from the deciduous database.
  """

  import Ecto.Query

  alias Deciduex.Repo
  alias Deciduex.Schema.CommandLog
  alias Deciduex.Schema.DecisionEdge
  alias Deciduex.Schema.DecisionNode

  def list_nodes do
    DecisionNode
    |> order_by(asc: :created_at)
    |> Repo.all()
  end

  def list_edges do
    DecisionEdge
    |> order_by(asc: :created_at)
    |> Repo.all()
  end

  def get_node(id) do
    Repo.get(DecisionNode, id)
  end

  def get_node_edges(node_id) do
    incoming =
      DecisionEdge
      |> where([e], e.to_node_id == ^node_id)
      |> order_by(asc: :created_at)
      |> Repo.all()

    outgoing =
      DecisionEdge
      |> where([e], e.from_node_id == ^node_id)
      |> order_by(asc: :created_at)
      |> Repo.all()

    {incoming, outgoing}
  end

  def get_graph do
    %{
      nodes: list_nodes(),
      edges: list_edges(),
      documents: []
    }
  end

  def list_recent_commands(limit \\ 20) do
    CommandLog
    |> order_by(desc: :started_at)
    |> limit(^limit)
    |> Repo.all()
  end
end
