defmodule DeciduousMcp.MCP.Tools.ShowNode do
  @moduledoc "MCP Tool: get detailed info about a single node with its connections."
  use DeciduousMcp.MCP.Component, type: :tool

  import Ecto.Query

  alias DeciduousMcp.Graph.Related
  alias DeciduousMcp.MCP.Scope
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.Node

  def definition do
    %{
      name: "show_node",
      description:
        "Get detailed information about a single node, including connected edges, documents, and themes. " <>
          "`related` lists up to 10 nodes of the same workspace that name one of this node's files " <>
          "or its commit, best overlap first; these are read off metadata, not edges.",
      input_schema: %{
        type: "object",
        properties: %{
          node_id: %{type: "string", description: "UUID of the node to show"}
        },
        required: ["node_id"]
      }
    }
    |> DeciduousMcp.MCP.Scope.with_node_workspace_arg()
  end

  def call(%{arguments: %{"node_id" => node_id}, server: frame}) do
    case Scope.read_node(frame, node_id, [:edges_from, :edges_to, :documents, :themes]) do
      {:ok, node} ->
        live = live_neighbours(node)

        result = %{
          id: node.id,
          change_id: node.change_id,
          node_type: node.node_type,
          title: node.title,
          description: node.description,
          status: node.status,
          metadata: node.metadata,
          edges_from:
            node.edges_from
            |> Enum.filter(&MapSet.member?(live, &1.to_node_id))
            |> Enum.map(fn e ->
              %{id: e.id, to: e.to_node_id, type: e.edge_type, rationale: e.rationale}
            end),
          edges_to:
            node.edges_to
            |> Enum.filter(&MapSet.member?(live, &1.from_node_id))
            |> Enum.map(fn e ->
              %{id: e.id, from: e.from_node_id, type: e.edge_type, rationale: e.rationale}
            end),
          documents:
            Enum.map(node.documents, fn d ->
              %{id: d.id, filename: d.original_filename, mime_type: d.mime_type}
            end),
          themes:
            Enum.map(node.themes, fn t ->
              %{id: t.id, name: t.name, color: t.color}
            end),
          created_at: DateTime.to_iso8601(node.inserted_at)
        }

        {:ok, Jason.encode!(Map.merge(result, related_section(node)))}

      {:error, message} ->
        {:error, %{code: -1, message: message}}
    end
  end

  # Derived at read time from metadata.files / metadata.commit; see
  # DeciduousMcp.Graph.Related. A node whose own metadata cannot be read as
  # paths says so under `related_error` instead of showing an empty list.
  defp related_section(node) do
    case Related.related(node, limit: 10) do
      {:ok, %{related: rels, commit_ignored: nil}} ->
        %{related: rels}

      {:ok, %{related: rels, commit_ignored: reason}} ->
        %{related: rels, related_commit_ignored: reason}

      {:error, message} ->
        %{related: [], related_error: message}
    end
  end

  # The ids at the far end of this node's edges that are not soft-deleted.
  # An edge to a deleted node is invisible everywhere else (get_graph,
  # /export, the walks); listing it here named a node no other read shows.
  defp live_neighbours(node) do
    ids = Enum.map(node.edges_from, & &1.to_node_id) ++ Enum.map(node.edges_to, & &1.from_node_id)

    Node
    |> where([n], n.id in ^ids and is_nil(n.deleted_at))
    |> select([n], n.id)
    |> Repo.all()
    |> MapSet.new()
  end
end
