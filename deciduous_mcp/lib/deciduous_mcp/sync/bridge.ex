defmodule DeciduousMcp.Sync.Bridge do
  @moduledoc """
  Bidirectional sync bridge between the Deciduous CLI (SQLite + event files)
  and the shared PostgreSQL database.

  ## Push: CLI → Postgres
  Reads `.deciduous/sync/events/*.jsonl` and applies to Postgres.

  ## Pull: Postgres → CLI
  Exports Postgres state as a checkpoint file the CLI can consume.
  """
  require Logger

  alias DeciduousMcp.Sync.Processor
  alias DeciduousMcp.Graph.Query

  @doc """
  Imports events from the CLI's event directory into Postgres.
  Call this after `git pull` brings in new event files.
  """
  def import_events(project_dir, workspace_id) do
    events_dir = Path.join(project_dir, ".deciduous/sync/events")

    if File.dir?(events_dir) do
      Logger.info("Importing events from #{events_dir}")
      Processor.process_directory(events_dir, workspace_id)
    else
      {:error, :events_dir_not_found}
    end
  end

  @doc """
  Exports the current Postgres state as a checkpoint file
  that the Deciduous CLI can consume via `deciduous events rebuild`.
  """
  def export_checkpoint(workspace_id, output_path) do
    graph = Query.get_full_graph(workspace_id, include_deleted: false)

    checkpoint = %{
      version: "1.1.0",
      created_at: DateTime.utc_now() |> DateTime.to_iso8601(),
      nodes:
        Enum.map(graph.nodes, fn n ->
          %{
            change_id: n.change_id,
            node_type: n.node_type,
            title: n.title,
            description: n.description,
            status: n.status,
            metadata_json: Jason.encode!(n.metadata || %{}),
            created_at: n.created_at,
            updated_at: n[:updated_at] || n.created_at
          }
        end),
      edges:
        Enum.map(graph.edges, fn e ->
          %{
            edge_id: e.id,
            from_change_id: e.from_change_id,
            to_change_id: e.to_change_id,
            edge_type: e.edge_type,
            rationale: e.rationale,
            created_at: e.created_at
          }
        end),
      themes:
        Enum.map(graph.themes, fn t ->
          %{
            change_id: t.change_id,
            name: t.name,
            color: t.color,
            description: t.description
          }
        end),
      node_themes:
        Enum.map(graph.node_themes, fn nt ->
          %{
            node_change_id: find_change_id(graph.nodes, nt.node_id),
            theme_change_id: find_change_id(graph.themes, nt.theme_id),
            source: nt.source
          }
        end),
      documents:
        Enum.map(graph.documents, fn d ->
          %{
            change_id: d.change_id,
            node_change_id: find_change_id(graph.nodes, d.node_id),
            content_hash: d[:content_hash],
            original_filename: d.original_filename,
            mime_type: d.mime_type,
            file_size: d.file_size,
            description: d.description,
            description_source: d.description_source
          }
        end)
    }

    case Jason.encode(checkpoint, pretty: true) do
      {:ok, json} ->
        File.mkdir_p!(Path.dirname(output_path))
        File.write!(output_path, json)
        {:ok, output_path}

      {:error, reason} ->
        {:error, {:json_encode_error, reason}}
    end
  end

  # --- Private ---

  defp find_change_id(entities, target_id) do
    case Enum.find(entities, fn e -> e[:id] == target_id end) do
      nil -> nil
      entity -> entity[:change_id]
    end
  end
end
