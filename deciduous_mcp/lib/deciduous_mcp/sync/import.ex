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
  """
  import Ecto.Query

  alias DeciduousMcp.Graph.Workspaces
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.{Edge, Node}

  @chunk 1_000

  def run(%{"graph" => graph} = payload) when is_map(graph) do
    with {:ok, name} <- Workspaces.normalize_name(payload["workspace"] || ""),
         {:ok, workspace} <- Workspaces.find_or_create(name),
         {:ok, nodes} <- validate_nodes(graph["nodes"] || []) do
      Repo.transaction(
        fn ->
          node_report = upsert_nodes(workspace.id, nodes)
          edge_report = upsert_edges(workspace.id, graph["edges"] || [], nodes)

          %{
            workspace: workspace.name,
            workspace_id: workspace.id,
            nodes: node_report,
            edges: edge_report,
            themes_skipped: "deciduous graph does not export themes"
          }
        end,
        timeout: :infinity
      )
    end
  end

  def run(_), do: {:error, "payload must contain a \"graph\" object"}

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
    # Older rows predate the change_id columns and carry only SQLite integer
    # ids, so the payload's own node list is the fallback lookup. Both are
    # needed: neither alone covers every graph on disk.
    by_sqlite_id =
      Map.new(nodes, fn n -> {n["id"], n["change_id"]} end)

    pg_ids = node_ids_by_change_id(workspace_id)

    {rows, unresolved} =
      Enum.reduce(edges, {[], []}, fn e, {ok, bad} ->
        from_cid = e["from_change_id"] || Map.get(by_sqlite_id, e["from_node_id"])
        to_cid = e["to_change_id"] || Map.get(by_sqlite_id, e["to_node_id"])

        from_id = Map.get(pg_ids, from_cid)
        to_id = Map.get(pg_ids, to_cid)

        cond do
          is_nil(from_id) or is_nil(to_id) ->
            {ok, [%{edge: e["id"], from: from_cid, to: to_cid} | bad]}

          from_id == to_id ->
            # The Ecto changeset forbids self-loops; insert_all bypasses it, so
            # the check is repeated here rather than quietly writing one.
            {ok, [%{edge: e["id"], self_loop: from_cid} | bad]}

          true ->
            {[edge_row(workspace_id, e, from_id, to_id, from_cid, to_cid) | ok], bad}
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
      unresolved_examples: Enum.take(unresolved, 10)
    }
  end

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
