defmodule DeciduousMcp.Repo.Migrations.PlannerCostsForCachedDb do
  use Ecto.Migration

  @moduledoc """
  Two planner settings, set on the database so every pool connection gets
  them. Not indexes, but the plans in 20260922200000 are only chosen with
  the first one, so it ships alongside.

  `random_page_cost = 1.1`. The default of 4 models a spinning disk where a
  random page costs four times a sequential one. This database is 45 MB of
  heap under 128 MB of shared_buffers; every plan captured today reports
  `Buffers: shared hit=...` with at most a handful of reads. At 4 the
  planner prefers a bitmap heap scan plus an external sort over walking an
  index that already has the order it needs. Proven on production, same
  session, read-only, with the index that already exists:

      Q1 get_graph fetch_nodes epstein, random_page_cost=4 (default):
      Gather Merge -> Sort  Sort Method: external merge  Disk: 4840kB
        -> Parallel Bitmap Heap Scan on decision_nodes n
      Execution Time: 29.294 ms

      same query, SET LOCAL random_page_cost = 1.1:
      Index Scan using decision_nodes_workspace_id_inserted_at_index
        Buffers: shared hit=1294
      Execution Time: 6.167 ms

  And on the local copy it is what lets `idx_nodes_ws_branchkey_latest`
  replace the 9,424 kB disk sort in `latest_per_branch` (36.9 ms -> 4.8 ms),
  and `idx_edges_ws_inserted` replace the 11,696 kB one in `fetch_edges`
  (49.5 ms -> 14.6 ms). At 4, both indexes exist and are ignored.

  `work_mem = '16MB'`. Four of the hot queries spill sorts of 4.8-11.7 MB to
  disk at the 4 MB default. Most of those sorts disappear with the indexes,
  but `fetch_edges` on a branch-filtered graph, `find_orphans` and any
  future ORDER BY over a whole workspace still sort. Pool is 10 (POOL_SIZE
  in compose); worst case is 160 MB on a box with 12 GB available.

  `ALTER DATABASE` takes a literal name, and dev/test/prod differ, so it is
  built with `format('%I', current_database())` inside a DO block. Verified
  locally: a fresh session after the DO block shows `1.1` and `16MB`.
  Takes effect for new connections, so the pool sees it on the next deploy
  or restart, not at migration time.
  """

  def up do
    execute("""
    DO $$
    BEGIN
      EXECUTE format('ALTER DATABASE %I SET random_page_cost = 1.1', current_database());
      EXECUTE format('ALTER DATABASE %I SET work_mem = ''16MB''', current_database());
    END
    $$
    """)
  end

  def down do
    execute("""
    DO $$
    BEGIN
      EXECUTE format('ALTER DATABASE %I RESET random_page_cost', current_database());
      EXECUTE format('ALTER DATABASE %I RESET work_mem', current_database());
    END
    $$
    """)
  end
end
