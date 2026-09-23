defmodule DeciduousMcp.Repo.Migrations.IndexHotReadPaths do
  use Ecto.Migration

  @moduledoc """
  Indexes for the four read shapes the MCP tools actually run, and two dead
  indexes removed. Every plan quoted here is `EXPLAIN (ANALYZE, BUFFERS)`
  against production (PostgreSQL 17.8, 30,201 nodes / 81,087 edges,
  work_mem 4MB, random_page_cost 4) on 2026-09-22, and the "after" against a
  plain-SQL pg_dump of that database restored to PostgreSQL 16.14 with the
  same settings.

  First the thing an index does not fix. `get_graph` on the epstein
  workspace (7,805 nodes, 51,156 edges) is 4.5s at the client. Its two
  queries take 36ms and 54ms wall time in psql *including* transferring
  9,988 kB of node rows and 12 MB of edge rows. The other ~4.4s is Ecto
  decode, serialisation, JSON encoding and the trip through Cloudflare. That
  is a query-shape problem (projection, pagination, or not asking for the
  whole workspace), not an index problem, and none of the indexes below
  change it.

  What they do change:

  1. `list_nodes` with a type filter inside a workspace. Today it walks the
     `(workspace_id, inserted_at)` index backwards and filters:

         Q5f node_type='action' in epstein (18 of 7,805 rows):
         Index Scan Backward using decision_nodes_workspace_id_inserted_at_index
           Filter: ((deleted_at IS NULL) AND ((node_type)::text = 'action'::text))
           Rows Removed by Filter: 7787
           Buffers: shared hit=1344 read=10
         Execution Time: 3.069 ms

     With `idx_nodes_ws_type_inserted`:

         Index Scan using idx_nodes_ws_type_inserted
           Index Cond: ((workspace_id = ...) AND ((node_type)::text = 'action'::text))
           Buffers: shared hit=10
         Execution Time: 0.011 ms

     The existing `(workspace_id, node_type)` index is kept: this one is
     partial on `deleted_at IS NULL` and `fetch_nodes(include_deleted: true)`
     and the bitmap-on-workspace_id plans still want a non-partial one.

  2. `list_nodes` and `get_graph` with a branch filter inside a workspace.
     Today, `BitmapAnd` of `idx_nodes_branch` and the workspace index, then
     a sort:

         Q4b branch='main' in epstein (52 rows):
         BitmapAnd
           -> Bitmap Index Scan on idx_nodes_branch  (rows=14423)
           -> Bitmap Index Scan on decision_nodes_workspace_id_node_type_index (rows=7805)
         Sort Key: inserted_at DESC
         Execution Time: 0.761 ms

     With `idx_nodes_ws_branch_inserted`, a single ordered index scan,
     `Buffers: shared hit=11 read=1`, `Execution Time: 0.032 ms`. Small in
     absolute terms, but the 14,423-row bitmap on `idx_nodes_branch` grows
     with every workspace that also has a `main` branch.

  3. `check_activity` -> `Nodes.latest_per_branch`, the DISTINCT ON over
     `coalesce(metadata->>'branch','')`. Every call sorts the whole
     workspace's full-width rows to disk:

         Q6 epstein:
         Unique
           -> Sort  Sort Key: (COALESCE((metadata ->> 'branch'), '')), inserted_at DESC, id DESC
                Sort Method: external merge  Disk: 9424kB
                -> Bitmap Heap Scan on decision_nodes n  (rows=7805)
         Execution Time: 36.850 ms

     `idx_nodes_ws_branchkey_latest` matches that ORDER BY column for
     column. The planner only picks it once random_page_cost reflects a
     cached database (see the companion settings migration); with both:

         Unique
           -> Index Scan using idx_nodes_ws_branchkey_latest
                Buffers: shared hit=7835
         Execution Time: 4.772 ms

     And it makes the loose-index-scan rewrite in
     `latest_per_branch.patch` possible, which reads one index entry per
     branch instead of one per node: `Execution Time: 0.065 ms` for
     epstein, identical result set to the DISTINCT ON (checked with EXCEPT
     both ways, 11 = 11, 0 differences, on tetris-arena).

  4. Edges by workspace in insertion order (`Edges.list_edges`,
     `Query.fetch_edges`). Today a seq scan of the whole edge table and an
     11.7 MB disk sort:

         Q3 list_edges epstein:
         Sort  Sort Method: external merge  Disk: 11696kB
           -> Seq Scan on decision_edges e  Rows Removed by Filter: 29931
         Execution Time: 38.752 ms

     With `idx_edges_ws_inserted`:

         Index Scan using idx_edges_ws_inserted on decision_edges e
           Buffers: shared hit=1700 read=47
         Execution Time: 11.088 ms

     `fetch_edges` (Q2, with the two `= ANY(7,805 ids)` filters) goes from
     49.5 ms with a spilled sort to 14.6 ms with no sort once the settings
     migration is in as well. Its 14-16 ms of *planning* time is the two
     array literals and is not an index problem.

  Removed:

  * `idx_nodes_metadata` (GIN jsonb_path_ops, 3,768 kB): `idx_scan = 0`
    since the postmaster started on 2026-07-08, and nothing in `lib/` uses
    `@>`. Maintained on every node write for no reader.
  * `decision_nodes_deleted_at_index` (344 kB): `idx_scan = 3`, and
    `pg_stats` says `deleted_at` has `n_distinct = 0` (every row is NULL),
    so a lookup on it matches the whole table.

  Cost on the write path, measured warm on the local copy with EXPLAIN
  ANALYZE INSERT in a rolled-back transaction: 0.30 ms with today's 8
  indexes, 0.21-0.27 ms with the 11 that result from this migration (the
  two drops pay for the three adds).

  `CONCURRENTLY` because production is live; that requires running outside
  a transaction, hence both module attributes.
  """

  @disable_ddl_transaction true
  @disable_migration_lock true

  def up do
    # 1. list_nodes type filter -> Q5f 3.069 ms / 1354 buffers -> 0.011 ms / 10 buffers
    execute("""
    CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_nodes_ws_type_inserted
      ON decision_nodes (workspace_id, node_type, inserted_at DESC)
      WHERE deleted_at IS NULL
    """)

    # 2. list_nodes / get_graph branch filter -> Q4b 0.761 ms BitmapAnd+Sort -> 0.032 ms
    execute("""
    CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_nodes_ws_branch_inserted
      ON decision_nodes (workspace_id, (metadata ->> 'branch'), inserted_at DESC)
      WHERE deleted_at IS NULL
    """)

    # 3. latest_per_branch DISTINCT ON -> Q6 36.850 ms, 9424kB disk sort -> 4.772 ms
    #    (0.065 ms with latest_per_branch.patch)
    execute("""
    CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_nodes_ws_branchkey_latest
      ON decision_nodes (workspace_id, (coalesce(metadata ->> 'branch', '')), inserted_at DESC, id DESC)
      WHERE deleted_at IS NULL
    """)

    # 4. list_edges / fetch_edges ORDER BY inserted_at -> Q3 38.752 ms, 11696kB disk sort -> 11.088 ms
    execute("""
    CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_edges_ws_inserted
      ON decision_edges (workspace_id, inserted_at)
    """)

    # Dead: 0 scans since 2026-07-08, no `@>` query anywhere in lib/.
    execute("DROP INDEX CONCURRENTLY IF EXISTS idx_nodes_metadata")

    # Dead: 3 scans, and every row has deleted_at IS NULL (n_distinct = 0).
    execute("DROP INDEX CONCURRENTLY IF EXISTS decision_nodes_deleted_at_index")
  end

  def down do
    execute("DROP INDEX CONCURRENTLY IF EXISTS idx_nodes_ws_type_inserted")
    execute("DROP INDEX CONCURRENTLY IF EXISTS idx_nodes_ws_branch_inserted")
    execute("DROP INDEX CONCURRENTLY IF EXISTS idx_nodes_ws_branchkey_latest")
    execute("DROP INDEX CONCURRENTLY IF EXISTS idx_edges_ws_inserted")

    execute("""
    CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_nodes_metadata
      ON decision_nodes USING GIN (metadata jsonb_path_ops)
    """)

    execute("""
    CREATE INDEX CONCURRENTLY IF NOT EXISTS decision_nodes_deleted_at_index
      ON decision_nodes (deleted_at)
    """)
  end
end
