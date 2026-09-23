defmodule DeciduousMcp.Sync.Import do
  @moduledoc """
  Bulk ingest of one project's graph, as emitted by the Rust CLI's
  `deciduous graph`.

  This replaces `DeciduousMcp.Sync.Bridge.import_events/2`, which read
  `.deciduous/sync/events/*.jsonl`. That format is gone twice over: v0.17.0
  replaced the JSONL log with a per-record store, and v0.18.0 replaced that with
  a single graph file. `deciduous graph` is the one export that has survived
  both, and it carries the `change_id` values this schema keys on.

  Idempotent on `[workspace_id, change_id]`, so re-importing a project updates
  in place instead of duplicating. That matters because the import will be run
  repeatedly across 91 repositories.

  ## Payload

      %{
        "workspace" => "deciduous",
        "source" => "/Users/bg/code/deciduous",   # optional, recorded on the workspace
        "graph" => %{"nodes" => [...], "edges" => [...], "documents" => [...]}
      }

  ## What is not imported

  Themes. `deciduous graph` emits `nodes`, `edges` and `documents` only, while
  the SQLite `themes` / `node_themes` tables and the matching Postgres columns
  both exist. Theme assignments will not survive this path until the CLI
  exports them.

  Document *bytes* do not travel in this payload either — they are pushed
  separately to `PUT /blob/:content_hash` so a 16MB PDF does not ride along
  inside a graph that is already megabytes of JSON. This import records the
  metadata and marks a row `content_missing` when no blob has arrived for its
  hash.
  """
  import Ecto.Query

  alias DeciduousMcp.Graph.Workspaces
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.{Document, Edge, Node}

  @chunk 1_000

  def run(payload, opts \\ [])

  def run(%{"graph" => graph} = payload, opts) when is_map(graph) do
    with {:ok, workspace} <- target_workspace(payload["workspace"], opts[:pinned_workspace_id]),
         {:ok, nodes} <- validate_nodes(graph["nodes"] || []) do
      Repo.transaction(
        fn ->
          node_report = upsert_nodes(workspace.id, nodes)
          edge_report = upsert_edges(workspace.id, graph["edges"] || [], nodes)
          doc_report = upsert_documents(workspace.id, graph["documents"] || [])

          %{
            workspace: workspace.name,
            workspace_id: workspace.id,
            nodes: node_report,
            edges: edge_report,
            documents: doc_report,
            themes_skipped: "deciduous graph does not export themes"
          }
        end,
        timeout: :infinity
      )
    end
  end

  def run(_, _), do: {:error, "payload must contain a \"graph\" object"}

  # Unpinned: the body names the workspace, as it always has.
  defp target_workspace(name, nil) do
    with {:ok, name} <- Workspaces.normalize_name(name || "") do
      Workspaces.find_or_create(name)
    end
  end

  # Pinned by X-Deciduous-Workspace: the pinned workspace, and a body naming
  # a different one is refused rather than redirected. Quietly importing
  # into the pin would report success for a push the sender meant for
  # somewhere else; the MCP tools can ignore their workspace argument
  # because it is a default, but this one names where every row goes.
  defp target_workspace(name, pinned_id) do
    {:ok, pinned} = Workspaces.get_workspace(pinned_id)

    case name && Workspaces.normalize_name(name) do
      nil ->
        {:ok, pinned}

      {:ok, same} when same == pinned.name ->
        {:ok, pinned}

      {:ok, other} ->
        {:error,
         {:pinned,
          "this client is pinned to workspace \"#{pinned.name}\" by " <>
            "X-Deciduous-Workspace; the import names \"#{other}\". Nothing was written."}}

      {:error, _} = err ->
        err
    end
  end

  @doc """
  Clears `content_missing` on every row waiting for this hash.

  Blobs can arrive after the metadata that references them — the import script
  uploads bytes first, but a document attached later, or a file recovered from
  another project, arrives the other way round. Without this, a row imported
  while its bytes were absent would answer 410 forever even once they showed up.
  """
  def mark_content_found(hash) do
    {count, _} =
      from(d in Document, where: d.content_hash == ^String.downcase(hash) and d.content_missing)
      |> Repo.update_all(set: [content_missing: false, updated_at: DateTime.utc_now()])

    count
  end

  # --- Nodes ------------------------------------------------------------------

  # Validation happens up front and rejects the whole import, rather than
  # per-row inside the insert. A payload carrying an unknown node_type is a
  # signal that the CLI's vocabulary moved; dropping those rows and reporting
  # success would lose nodes silently and leave dangling edges behind them.
  defp validate_nodes(nodes) do
    types = MapSet.new(Node.node_types())
    statuses = MapSet.new(Node.statuses())

    problems =
      nodes
      |> Enum.flat_map(fn n ->
        bad_type =
          if MapSet.member?(types, n["node_type"]),
            do: [],
            else: [%{change_id: n["change_id"], unknown_node_type: n["node_type"]}]

        status = n["status"] || "pending"

        bad_status =
          if MapSet.member?(statuses, status),
            do: [],
            else: [%{change_id: n["change_id"], unknown_status: status}]

        bad_type ++ bad_status
      end)
      |> Enum.take(20)

    cond do
      problems != [] -> {:error, %{rejected: "unknown vocabulary", examples: problems}}
      Enum.any?(nodes, &is_nil(&1["change_id"])) -> {:error, "every node needs a change_id"}
      true -> {:ok, nodes}
    end
  end

  defp upsert_nodes(workspace_id, nodes) do
    now = DateTime.utc_now()

    rows =
      Enum.map(nodes, fn n ->
        %{
          id: Ecto.UUID.generate(),
          workspace_id: workspace_id,
          change_id: n["change_id"],
          node_type: n["node_type"],
          title: n["title"] || "(untitled)",
          description: n["description"],
          status: n["status"] || "pending",
          metadata: decode_metadata(n["metadata_json"]),
          inserted_at: parse_time(n["created_at"], now),
          updated_at: parse_time(n["updated_at"], now)
        }
      end)

    inserted =
      rows
      |> Enum.chunk_every(@chunk)
      |> Enum.reduce(0, fn chunk, acc ->
        {count, _} =
          Repo.insert_all(Node, chunk,
            on_conflict: {:replace, [:node_type, :title, :description, :status, :metadata, :updated_at]},
            conflict_target: [:workspace_id, :change_id]
          )

        acc + count
      end)

    %{received: length(rows), upserted: inserted}
  end

  # --- Edges ------------------------------------------------------------------

  defp upsert_edges(workspace_id, edges, nodes) do
    # An edge names its endpoints twice: `from_node_id` (SQLite's integer
    # primary key, a real foreign key) and `from_change_id` (a denormalized
    # copy added later). The copies go stale. In one graph on disk, 9,185 of
    # 51,158 edges carry a from_change_id that belongs to no node at all, while
    # all 51,158 integer endpoints resolve — edge 81346 stores node 82 and
    # change_id de82201d…, but node 82's change_id is 08e5c211….
    #
    # So the integer id is tried first, against the payload's own node list,
    # and the stored change_id is the fallback for rows written before those
    # columns existed. Preferring change_id — which reads as the more portable
    # identifier — silently drops every edge whose copy drifted.
    by_sqlite_id = Map.new(nodes, fn n -> {n["id"], n["change_id"]} end)

    pg_ids = node_ids_by_change_id(workspace_id)

    {rows, unresolved, stale} =
      Enum.reduce(edges, {[], [], 0}, fn e, {ok, bad, stale} ->
        from_cid = Map.get(by_sqlite_id, e["from_node_id"]) || e["from_change_id"]
        to_cid = Map.get(by_sqlite_id, e["to_node_id"]) || e["to_change_id"]

        stale =
          stale +
            count_stale(e["from_change_id"], from_cid) +
            count_stale(e["to_change_id"], to_cid)

        from_id = Map.get(pg_ids, from_cid)
        to_id = Map.get(pg_ids, to_cid)

        cond do
          is_nil(from_id) or is_nil(to_id) ->
            {ok, [%{edge: e["id"], from: from_cid, to: to_cid} | bad], stale}

          from_id == to_id ->
            # The Ecto changeset forbids self-loops; insert_all bypasses it, so
            # the check is repeated here rather than quietly writing one. There
            # are 46 of these across the graphs on disk.
            {ok, [%{edge: e["id"], self_loop: from_cid} | bad], stale}

          true ->
            {[edge_row(workspace_id, e, from_id, to_id, from_cid, to_cid) | ok], bad, stale}
        end
      end)

    inserted =
      rows
      |> Enum.chunk_every(@chunk)
      |> Enum.reduce(0, fn chunk, acc ->
        {count, _} =
          Repo.insert_all(Edge, chunk,
            on_conflict: {:replace, [:rationale, :weight, :updated_at]},
            conflict_target: [:from_node_id, :to_node_id, :edge_type]
          )

        acc + count
      end)

    %{
      received: length(edges),
      upserted: inserted,
      unresolved: length(unresolved),
      unresolved_examples: Enum.take(unresolved, 10),
      stale_change_ids: stale
    }
  end

  # Counts endpoints whose stored change_id disagrees with the node its integer
  # id actually points at. Reported rather than silently corrected, because a
  # rising number here means the CLI is writing the denormalized column wrong.
  defp count_stale(nil, _resolved), do: 0
  defp count_stale(stored, resolved) when stored == resolved, do: 0
  defp count_stale(_stored, _resolved), do: 1

  defp edge_row(workspace_id, e, from_id, to_id, from_cid, to_cid) do
    now = DateTime.utc_now()

    %{
      id: Ecto.UUID.generate(),
      workspace_id: workspace_id,
      from_node_id: from_id,
      to_node_id: to_id,
      from_change_id: from_cid,
      to_change_id: to_cid,
      edge_type: e["edge_type"] || "leads_to",
      weight: as_float(e["weight"]),
      rationale: e["rationale"],
      inserted_at: parse_time(e["created_at"], now),
      updated_at: now
    }
  end

  # --- Documents ---------------------------------------------------------------

  defp upsert_documents(_workspace_id, []), do: %{received: 0, upserted: 0, content_missing: 0}

  defp upsert_documents(workspace_id, documents) do
    pg_ids = node_ids_by_change_id(workspace_id)
    now = DateTime.utc_now()

    {rows, orphaned} =
      Enum.reduce(documents, {[], []}, fn d, {ok, bad} ->
        case Map.get(pg_ids, d["node_change_id"]) do
          nil ->
            {ok, [%{document: d["original_filename"], node: d["node_change_id"]} | bad]}

          node_id ->
            hash = d["content_hash"]

            row = %{
              id: Ecto.UUID.generate(),
              workspace_id: workspace_id,
              node_id: node_id,
              change_id: d["change_id"],
              content_hash: hash,
              original_filename: d["original_filename"],
              storage_filename: d["storage_filename"],
              mime_type: d["mime_type"] || "application/octet-stream",
              file_size: d["file_size"] || 0,
              description: d["description"],
              description_source: d["description_source"] || "none",
              attached_by: d["attached_by"],
              detached_at: parse_optional_time(d["detached_at"]),
              # Whether the bytes arrived is checked here rather than assumed.
              # Five documents on this machine are referenced by live rows whose
              # files are gone; they import as history with this flag set, so a
              # fetch can say "gone" instead of "never existed".
              content_missing: not DeciduousMcp.Storage.exists?(hash),
              storage: "postgres",
              inserted_at: parse_time(d["attached_at"], now),
              updated_at: now
            }

            {[row | ok], bad}
        end
      end)

    inserted =
      rows
      |> Enum.chunk_every(@chunk)
      |> Enum.reduce(0, fn chunk, acc ->
        {count, _} =
          Repo.insert_all(Document, chunk,
            on_conflict:
              {:replace, [:description, :description_source, :detached_at, :content_missing, :updated_at]},
            conflict_target: [:workspace_id, :change_id]
          )

        acc + count
      end)

    %{
      received: length(documents),
      upserted: inserted,
      content_missing: Enum.count(rows, & &1.content_missing),
      orphaned: length(orphaned),
      orphaned_examples: Enum.take(orphaned, 5)
    }
  end

  defp parse_optional_time(nil), do: nil
  defp parse_optional_time(value), do: parse_time(value, nil)

  defp node_ids_by_change_id(workspace_id) do
    from(n in Node, where: n.workspace_id == ^workspace_id, select: {n.change_id, n.id})
    |> Repo.all()
    |> Map.new()
  end

  # --- Coercion ---------------------------------------------------------------

  defp decode_metadata(nil), do: %{}

  defp decode_metadata(json) when is_binary(json) do
    case Jason.decode(json) do
      {:ok, map} when is_map(map) -> map
      _ -> %{}
    end
  end

  defp decode_metadata(map) when is_map(map), do: map
  defp decode_metadata(_), do: %{}

  # The CLI writes timestamps with an offset ("2016-02-01T00:00:00-05:00") and
  # backdated archaeology nodes reach back years, so these are parsed rather
  # than stamped with the import time.
  defp parse_time(nil, fallback), do: fallback

  defp parse_time(value, fallback) when is_binary(value) do
    case DateTime.from_iso8601(value) do
      # The columns are :utc_datetime_usec, which rejects anything that is not
      # 6-digit precision. CLI timestamps carry none ("2016-02-01T00:00:00-05:00"),
      # and `DateTime.truncate/2` only ever removes precision, so it leaves a
      # 0-digit value 0-digit and Ecto raises on dump. The precision has to be
      # widened explicitly.
      {:ok, dt, _offset} -> %{dt | microsecond: {elem(dt.microsecond, 0), 6}}
      _ -> fallback
    end
  end

  defp parse_time(_, fallback), do: fallback

  defp as_float(nil), do: 1.0
  defp as_float(n) when is_float(n), do: n
  defp as_float(n) when is_integer(n), do: n * 1.0
  defp as_float(_), do: 1.0
end
