defmodule DeciduousMcp.Repo.Migrations.TrigramSearchEdgesAndDocuments do
  @moduledoc """
  Trigram GIN indexes for ask_graph's two new searches: edge `rationale`
  and attached-document `description`, both `ILIKE '%term%'`.

  Measured on the dev copy of production (2026-09-22: 29,853 nodes, 80,682
  edges, 85 documents), EXPLAIN ANALYZE, warm cache.

  Edges. ask_graph asks `EXISTS (edge into n whose rationale ILIKE ...)`. The
  planner either walks the workspace's nodes and probes each one's incoming
  edges by `to_node_id`, or finds the matching edges first. Without an index
  "first" means a sequential scan of every edge on the server:

      global scope, '%zzqxnomatch%'        Seq Scan on decision_edges
                                           Rows Removed by Filter: 80682
                                           39.5 ms  ->  0.20 ms
      global scope, '%flight%' (640 hits)  44.8 ms  ->  4.4 ms
      deciduous, 4 terms                   9.3 ms  ->  4.5 ms
      danielle_who, '%theorem%'            48.2 ms  ->  0.53 ms

  With the index the same queries become a Bitmap Index Scan on
  idx_edges_rationale_trgm. 6.2 MB next to a 49 MB table; built in 0.56 s.

  What the index does not fix: in the epstein workspace (7,805 nodes, 51,156
  edges, ~86 incoming edges on its newest nodes) the planner still prefers
  walking nodes newest-first under the LIMIT and filtering their edges.
  '%flight%' OR '%manifest%' reads 315 nodes and ~27k edge rows, 25-30 ms
  before and after. The index plan is available (forced with an OFFSET 0
  fence it runs in 4.4 ms) but the planner does not pick it.

  Documents. 85 rows, 13 pages: the planner seq-scans them in 0.1-1 ms and
  does not use this index even with enable_seqscan off. It is here because
  it is cheap (96 kB) and because on a synthetic copy of the table the
  planner switches to it by about 1,000 rows (0.03 ms at 1,020 rows, 2.2 ms
  at 102,000).

  CONCURRENTLY, so writes keep flowing while the indexes build. pg_trgm
  already exists (20260923010000_trigram_search).
  """
  use Ecto.Migration

  @disable_ddl_transaction true
  @disable_migration_lock true

  def up do
    execute("""
    CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_edges_rationale_trgm
      ON decision_edges USING gin (rationale gin_trgm_ops)
    """)

    execute("""
    CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_documents_description_trgm
      ON node_documents USING gin (description gin_trgm_ops)
    """)
  end

  def down do
    execute("DROP INDEX CONCURRENTLY IF EXISTS idx_documents_description_trgm")
    execute("DROP INDEX CONCURRENTLY IF EXISTS idx_edges_rationale_trgm")
  end
end
