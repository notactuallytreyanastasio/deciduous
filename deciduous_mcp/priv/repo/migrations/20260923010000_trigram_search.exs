defmodule DeciduousMcp.Repo.Migrations.TrigramSearch do
  @moduledoc """
  Trigram GIN indexes for the two searches that no b-tree can serve.

  `query_nodes(search:)` is `title ILIKE '%term%' OR description ILIKE
  '%term%'`, and `ask_graph` adds `metadata::text ILIKE '%term%'`. A leading
  wildcard defeats every b-tree, so on production (2026-09-23, 30,249 nodes)
  both planned as a sequential scan of the whole table:

      Seq Scan on decision_nodes
        Filter: ((deleted_at IS NULL) AND ((title ~~* '%flight%') OR ...))
        Rows Removed by Filter: 29873
      Execution Time: 84.537 ms

  pg_trgm GIN indexes answer `ILIKE '%...%'` for patterns of three or more
  characters. The metadata index is on the expression `(metadata::text)`,
  which is exactly what ask_graph's `fragment("?::text", n.metadata)` emits;
  a different spelling of the cast would not match it.

  CONCURRENTLY, so writes keep flowing while the indexes build. pg_trgm is a
  trusted extension (PostgreSQL 13+), and the app role on production is a
  superuser anyway.
  """
  use Ecto.Migration

  @disable_ddl_transaction true
  @disable_migration_lock true

  def up do
    execute("CREATE EXTENSION IF NOT EXISTS pg_trgm")

    execute("""
    CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_nodes_title_trgm
      ON decision_nodes USING gin (title gin_trgm_ops)
    """)

    execute("""
    CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_nodes_description_trgm
      ON decision_nodes USING gin (description gin_trgm_ops)
    """)

    execute("""
    CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_nodes_metadata_text_trgm
      ON decision_nodes USING gin ((metadata::text) gin_trgm_ops)
    """)
  end

  def down do
    execute("DROP INDEX CONCURRENTLY IF EXISTS idx_nodes_metadata_text_trgm")
    execute("DROP INDEX CONCURRENTLY IF EXISTS idx_nodes_description_trgm")
    execute("DROP INDEX CONCURRENTLY IF EXISTS idx_nodes_title_trgm")
    # The extension stays: dropping it would drop anything else built on it.
  end
end
