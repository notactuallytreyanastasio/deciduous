defmodule DeciduousMcp.Sync.Event do
  @moduledoc """
  Event type definitions matching the Rust CLI's event format.

  The Deciduous CLI emits events to `.deciduous/sync/events/{user}.jsonl`.
  This module can parse those events and convert them to Postgres operations.

  ## Event Types

  Matches the 11 event variants from the Rust `Event` enum in events.rs:
  - AddNode, UpdateNode, DeleteNode
  - AddEdge, DeleteEdge
  - AddTheme, DeleteTheme
  - TagNode, UntagNode
  - AttachDocument, DetachDocument
  """

  @event_types ~w(
    AddNode UpdateNode DeleteNode
    AddEdge DeleteEdge
    AddTheme DeleteTheme
    TagNode UntagNode
    AttachDocument DetachDocument
  )

  defstruct [:type, :data, :timestamp, :author]

  @doc """
  Parses a JSONL line into an Event struct.
  """
  def parse(json_line) when is_binary(json_line) do
    case Jason.decode(json_line) do
      {:ok, data} -> parse_event(data)
      {:error, reason} -> {:error, {:json_parse_error, reason}}
    end
  end

  @doc """
  Parses all events from a JSONL file.
  Returns a list of {:ok, event} | {:error, reason} tuples.
  """
  def parse_file(file_path) do
    file_path
    |> File.stream!()
    |> Stream.map(&String.trim/1)
    |> Stream.reject(&(&1 == ""))
    |> Enum.map(&parse/1)
  end

  @doc """
  Converts an Event to a map suitable for database operations.
  """
  def to_db_params(%__MODULE__{type: "AddNode", data: data}) do
    {:create_node,
     %{
       change_id: data["change_id"],
       node_type: data["node_type"],
       title: data["title"],
       description: data["description"],
       status: data["status"] || "pending",
       metadata: data["metadata_json"] && Jason.decode!(data["metadata_json"])
     }}
  end

  def to_db_params(%__MODULE__{type: "UpdateNode", data: data}) do
    attrs =
      %{}
      |> maybe_put(:title, data["title"])
      |> maybe_put(:description, data["description"])
      |> maybe_put(:status, data["status"])
      |> maybe_put(:metadata, data["metadata_json"] && Jason.decode!(data["metadata_json"]))

    {:update_node, data["change_id"], attrs}
  end

  def to_db_params(%__MODULE__{type: "DeleteNode", data: data}) do
    {:delete_node, data["change_id"]}
  end

  def to_db_params(%__MODULE__{type: "AddEdge", data: data}) do
    {:create_edge,
     %{
       from_change_id: data["from_change_id"],
       to_change_id: data["to_change_id"],
       edge_type: data["edge_type"] || "leads_to",
       rationale: data["rationale"]
     }}
  end

  def to_db_params(%__MODULE__{type: "DeleteEdge", data: data}) do
    {:delete_edge, data["edge_id"]}
  end

  def to_db_params(%__MODULE__{type: "AddTheme", data: data}) do
    {:create_theme,
     %{
       change_id: data["change_id"],
       name: data["name"],
       color: data["color"] || "#6b7280",
       description: data["description"]
     }}
  end

  def to_db_params(%__MODULE__{type: "DeleteTheme", data: data}) do
    {:delete_theme, data["change_id"]}
  end

  def to_db_params(%__MODULE__{type: "TagNode", data: data}) do
    {:tag_node, data["node_change_id"], data["theme_change_id"], data["source"] || "manual"}
  end

  def to_db_params(%__MODULE__{type: "UntagNode", data: data}) do
    {:untag_node, data["node_change_id"], data["theme_change_id"]}
  end

  def to_db_params(%__MODULE__{type: "AttachDocument", data: data}) do
    {:attach_document,
     %{
       change_id: data["doc_change_id"],
       node_change_id: data["node_change_id"],
       content_hash: data["content_hash"],
       original_filename: data["original_filename"],
       storage_filename: data["storage_filename"],
       mime_type: data["mime_type"],
       file_size: data["file_size"],
       description: data["description"],
       description_source: data["description_source"] || "none"
     }}
  end

  def to_db_params(%__MODULE__{type: "DetachDocument", data: data}) do
    {:detach_document, data["doc_change_id"]}
  end

  # --- Private ---

  defp parse_event(%{"type" => type} = data) when type in @event_types do
    {:ok,
     %__MODULE__{
       type: type,
       data: data,
       timestamp: data["timestamp"],
       author: data["author"]
     }}
  end

  defp parse_event(%{"type" => type}) do
    {:error, {:unknown_event_type, type}}
  end

  defp parse_event(_) do
    {:error, :missing_type_field}
  end

  defp maybe_put(map, _key, nil), do: map
  defp maybe_put(map, key, value), do: Map.put(map, key, value)
end
