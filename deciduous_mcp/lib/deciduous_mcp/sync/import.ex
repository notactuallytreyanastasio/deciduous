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
    with {:ok, workspace} <-
           target_workspace(payload["workspace"], opts[:pinned_workspace_id], opts[:pinned_workspace_name]),
         {:ok, nodes} <- validate_nodes(graph["nodes"] || []) do
      Repo.transaction(
        fn ->
          deleted = deleted_change_ids(workspace.id)
          node_report = upsert_nodes(workspace.id, nodes, deleted)
          edge_report = upsert_edges(workspace.id, graph["edges"] || [], nodes, deleted)
          doc_report = upsert_documents(workspace.id, graph["documents"] || [], deleted)

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
  defp target_workspace(name, nil, nil) do
    with {:ok, name} <- workspace_name(name || "") do
      Workspaces.find_or_create(name)
    end
  end

  # Pinned to a name nothing has been written to yet. The plug no longer
  # creates the pinned workspace, so it hands over a name and no id; read as
  # "no id, so no pin", this let a client pinned to a new name import into
  # any workspace its body named.
  defp target_workspace(name, nil, pinned_name) do
    case name && workspace_name(name) do
      nil -> Workspaces.find_or_create(pinned_name)
      {:ok, same} when same == pinned_name -> Workspaces.find_or_create(pinned_name)
      {:ok, other} -> {:error, {:pinned, pinned_refusal(pinned_name, other)}}
      {:error, _} = err -> err
    end
  end

  # Pinned by X-Deciduous-Workspace: the pinned workspace, and a body naming
  # a different one is refused rather than redirected. Quietly importing
  # into the pin would report success for a push the sender meant for
  # somewhere else; the MCP tools can ignore their workspace argument
  # because it is a default, but this one names where every row goes.
  defp target_workspace(name, pinned_id, _pinned_name) do
    {:ok, pinned} = Workspaces.get_workspace(pinned_id)

    case name && workspace_name(name) do
      nil ->
        {:ok, pinned}

      {:ok, same} when same == pinned.name ->
        {:ok, pinned}

      {:ok, other} ->
        {:error, {:pinned, pinned_refusal(pinned.name, other)}}

      {:error, _} = err ->
        err
    end
  end

  defp pinned_refusal(pinned, other) do
    "this client is pinned to workspace \"#{pinned}\" by " <>
      "X-Deciduous-Workspace; the import names \"#{other}\". Nothing was written."
  end

  # Before, "*" normalized to a name like any other and the import created a
  # workspace called "*", which a read of "*" (the global view) never shows.
  defp workspace_name(raw) do
    case Workspaces.normalize_name(raw) do
      {:ok, "*"} -> {:error, Workspaces.describe_name_error(raw, :global)}
      {:ok, name} -> {:ok, name}
      {:error, reason} -> {:error, Workspaces.describe_name_error(raw, reason)}
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
      true -> validate_metadata(nodes)
    end
  end

  # insert_all skips Node.changeset, so the metadata rules update_node
  # enforces were never applied here: {"confidence": "999"} and
  # {"confidence": true} were stored, and metadata_json that did not decode
  # to an object was stored as %{} -- a node's prompt and branch dropped
  # without a word. Refused whole, like unknown vocabulary, and for the
  # same reason: a row dropped from a 200 is a row lost silently.
  # Returns the nodes with metadata_json decoded, so it is parsed once.
  defp validate_metadata(nodes) do
    {decoded, problems} =
      Enum.map_reduce(nodes, [], fn n, problems ->
        case decode_metadata(n["metadata_json"]) do
          {:ok, meta} ->
            case Node.confidence_error(meta["confidence"]) do
              nil -> {Map.put(n, "metadata_json", meta), problems}
              message -> {n, ["node #{n["change_id"]}: #{message}" | problems]}
            end

          {:error, message} ->
            {n, ["node #{n["change_id"]}: #{message}" | problems]}
        end
      end)

    case problems do
      [] ->
        {:ok, decoded}

      _ ->
        shown = problems |> Enum.reverse() |> Enum.take(20)

        {:error,
         "metadata rejected, nothing was written (#{length(problems)} node(s)): " <>
           Enum.join(shown, "; ")}
    end
  end

  # A node deleted on the server stays deleted through an import. The
  # upsert used to replace title, status and metadata on any change_id, so
  # a push rewrote the tombstone ("REWRITTEN VIA IMPORT", status
  # completed, deleted_at still set) while every MCP write tool refused
  # the same edit. Those rows are left alone and named in the report, so
  # the CLI can tell its user the edit did not land and why.
  defp upsert_nodes(workspace_id, nodes, deleted) do
    now = DateTime.utc_now()

    {refused, nodes} = Enum.split_with(nodes, &Map.has_key?(deleted, &1["change_id"]))

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
          # Decoded and checked by validate_metadata/1.
          metadata: n["metadata_json"],
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
            on_conflict: replace_unless_deleted(),
            conflict_target: [:workspace_id, :change_id]
          )

        acc + count
      end)

    %{
      received: length(rows) + length(refused),
      upserted: inserted,
      refused_deleted: length(refused),
      refused_deleted_examples:
        refused
        |> Enum.take(20)
        |> Enum.map(&%{change_id: &1["change_id"], deleted_at: Map.fetch!(deleted, &1["change_id"])})
    }
  end

  # The split above names the rows that were already deleted. This guard
  # covers a delete_node that commits between that read and the insert:
  # the conflicting row is then skipped rather than rewritten, and shows
  # up as upserted < received.
  defp replace_unless_deleted do
    from(n in Node,
      where: is_nil(n.deleted_at),
      update: [
        set: [
          node_type: fragment("EXCLUDED.node_type"),
          title: fragment("EXCLUDED.title"),
          description: fragment("EXCLUDED.description"),
          status: fragment("EXCLUDED.status"),
          metadata: fragment("EXCLUDED.metadata"),
          updated_at: fragment("EXCLUDED.updated_at")
        ]
      ]
    )
  end

  defp deleted_change_ids(workspace_id) do
    from(n in Node,
      where: n.workspace_id == ^workspace_id and not is_nil(n.deleted_at),
      select: {n.change_id, n.deleted_at}
    )
    |> Repo.all()
    |> Map.new(fn {cid, at} -> {cid, DateTime.to_iso8601(at)} end)
  end

  # --- Edges ------------------------------------------------------------------

  defp upsert_edges(workspace_id, edges, nodes, deleted) do
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

    {rows, unresolved, dead, stale} =
      Enum.reduce(edges, {[], [], [], 0}, fn e, {ok, bad, dead, stale} ->
        from_cid = Map.get(by_sqlite_id, e["from_node_id"]) || e["from_change_id"]
        to_cid = Map.get(by_sqlite_id, e["to_node_id"]) || e["to_change_id"]

        stale =
          stale +
            count_stale(e["from_change_id"], from_cid) +
            count_stale(e["to_change_id"], to_cid)

        from_id = Map.get(pg_ids, from_cid)
        to_id = Map.get(pg_ids, to_cid)

        cond do
          # Before the unresolved check: the endpoint exists, and saying
          # "missing" would send the reader looking for the wrong thing.
          # An edge touching a deleted node is dropped by every read, so
          # writing one only makes a row nothing can see or remove.
          Map.has_key?(deleted, from_cid) or Map.has_key?(deleted, to_cid) ->
            {ok, bad, [%{edge: e["id"], from: from_cid, to: to_cid} | dead], stale}

          is_nil(from_id) or is_nil(to_id) ->
            {ok, [%{edge: e["id"], from: from_cid, to: to_cid} | bad], dead, stale}

          from_id == to_id ->
            # The Ecto changeset forbids self-loops; insert_all bypasses it, so
            # the check is repeated here rather than quietly writing one. There
            # are 46 of these across the graphs on disk.
            {ok, [%{edge: e["id"], self_loop: from_cid} | bad], dead, stale}

          true ->
            {[edge_row(workspace_id, e, from_id, to_id, from_cid, to_cid) | ok], bad, dead, stale}
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
      refused_deleted: length(dead),
      refused_deleted_examples: Enum.take(dead, 10),
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

  defp upsert_documents(_workspace_id, [], _deleted),
    do: %{received: 0, upserted: 0, content_missing: 0}

  defp upsert_documents(workspace_id, documents, deleted) do
    pg_ids = Map.drop(node_ids_by_change_id(workspace_id), Map.keys(deleted))
    now = DateTime.utc_now()

    # A document on a deleted node is refused like an edge to one: it would
    # be attached to content the delete was meant to hide.
    {refused, documents} = Enum.split_with(documents, &Map.has_key?(deleted, &1["node_change_id"]))

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
              {:replace,
               [:description, :description_source, :detached_at, :content_missing, :updated_at]},
            conflict_target: [:workspace_id, :change_id]
          )

        acc + count
      end)

    %{
      received: length(documents) + length(refused),
      upserted: inserted,
      refused_deleted: length(refused),
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

  defp decode_metadata(nil), do: {:ok, %{}}
  defp decode_metadata(map) when is_map(map), do: {:ok, map}

  defp decode_metadata(json) when is_binary(json) do
    case Jason.decode(json) do
      {:ok, map} when is_map(map) -> {:ok, map}
      {:ok, other} -> {:error, "metadata_json must be a JSON object, got #{inspect(other)}"}
      {:error, _} -> {:error, "metadata_json is not valid JSON: #{inspect(String.slice(json, 0, 80))}"}
    end
  end

  defp decode_metadata(other),
    do: {:error, "metadata_json must be a JSON object or a string holding one, got #{inspect(other)}"}

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
