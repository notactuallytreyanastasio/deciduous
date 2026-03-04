defmodule Deciduex.Queries do
  @moduledoc """
  Centralized Ecto queries for reading from the deciduous database.
  """

  import Ecto.Query

  alias Deciduex.Repo
  alias Deciduex.Schema.CommandLog
  alias Deciduex.Schema.DecisionEdge
  alias Deciduex.Schema.DecisionNode
  alias Deciduex.Schema.NodeDocument

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
      documents: list_documents()
    }
  end

  def list_documents(node_id \\ nil, include_detached \\ false) do
    query = NodeDocument |> order_by(desc: :attached_at)

    query =
      if node_id do
        query |> where([d], d.node_id == ^node_id)
      else
        query
      end

    query =
      if include_detached do
        query
      else
        query |> where([d], is_nil(d.detached_at))
      end

    Repo.all(query)
  end

  def get_document(id) do
    Repo.get(NodeDocument, id)
  end

  def get_active_content_hashes do
    NodeDocument
    |> where([d], is_nil(d.detached_at))
    |> select([d], d.content_hash)
    |> distinct(true)
    |> Repo.all()
  end

  def list_recent_commands(limit \\ 20) do
    CommandLog
    |> order_by(desc: :started_at)
    |> limit(^limit)
    |> Repo.all()
  end
end
