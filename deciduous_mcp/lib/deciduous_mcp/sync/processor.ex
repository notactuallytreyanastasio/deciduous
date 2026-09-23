defmodule DeciduousMcp.Sync.Processor do
  @moduledoc """
  Processes events from the Deciduous CLI's JSONL event files and applies
  them to the shared PostgreSQL database.

  This is the bridge between the local CLI (SQLite + event files) and the
  shared Postgres backend. Events are idempotent — replaying the same event
  file multiple times produces the same result.
  """
  require Logger

  alias DeciduousMcp.Sync.Event
  alias DeciduousMcp.Graph.{Nodes, Edges}

  @doc """
  Processes all event files in a directory.
  Typically called with `.deciduous/sync/events/` path.

  Returns a summary of applied events.
  """
  def process_directory(events_dir, workspace_id) do
    case File.ls(events_dir) do
      {:ok, files} ->
        results =
          files
          |> Enum.filter(&String.ends_with?(&1, ".jsonl"))
          |> Enum.flat_map(fn file ->
            path = Path.join(events_dir, file)
            Logger.info("Processing event file: #{path}")
            process_file(path, workspace_id)
          end)

        succeeded = Enum.count(results, &match?({:ok, _}, &1))
        failed = Enum.count(results, &match?({:error, _}, &1))

        {:ok, %{total: length(results), succeeded: succeeded, failed: failed}}

      {:error, reason} ->
        {:error, {:directory_read_error, reason}}
    end
  end

  @doc """
  Processes a single JSONL event file.
  """
  def process_file(file_path, workspace_id) do
    file_path
    |> Event.parse_file()
    |> Enum.map(fn
      {:ok, event} -> apply_event(event, workspace_id)
      {:error, reason} -> {:error, reason}
    end)
  end

  @doc """
  Applies a single event to the database.
  Idempotent: if the entity already exists (by change_id), it's updated rather than duplicated.
  """
  def apply_event(%Event{} = event, workspace_id) do
    case Event.to_db_params(event) do
      {:create_node, attrs} ->
        case Nodes.get_node_by_change_id(workspace_id, attrs.change_id) do
          {:ok, _existing} ->
            Logger.debug("Node #{attrs.change_id} already exists, skipping")
            {:ok, :already_exists}

          {:error, :not_found} ->
            Nodes.create_node(workspace_id, attrs)
        end

      {:update_node, change_id, attrs} ->
        case Nodes.get_node_by_change_id(workspace_id, change_id) do
          {:ok, node} -> Nodes.update_node(node.id, attrs)
          {:error, :not_found} -> {:error, {:node_not_found, change_id}}
        end

      {:delete_node, change_id} ->
        case Nodes.get_node_by_change_id(workspace_id, change_id) do
          {:ok, node} ->
            case Nodes.delete_node(node.id) do
              {:error, :already_deleted} -> {:ok, :already_deleted}
              other -> other
            end

          {:error, :not_found} ->
            {:ok, :already_deleted}
        end

      {:create_edge, attrs} ->
        Edges.create_edge_by_change_id(
          workspace_id,
          attrs.from_change_id,
          attrs.to_change_id,
          %{edge_type: attrs.edge_type, rationale: attrs.rationale}
        )

      {:delete_edge, _edge_id} ->
        # Edge deletion by edge_id requires looking up the edge
        # For now, log and skip — full implementation in Phase 3
        Logger.warning("Edge deletion by edge_id not yet implemented for sync")
        {:ok, :skipped}

      other ->
        Logger.warning("Unhandled event type in sync processor: #{inspect(other)}")
        {:ok, :skipped}
    end
  rescue
    e ->
      Logger.error("Error applying event: #{inspect(e)}")
      {:error, {:exception, e}}
  end
end
