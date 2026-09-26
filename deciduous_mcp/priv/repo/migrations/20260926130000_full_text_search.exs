defmodule DeciduousMcp.Repo.Migrations.FullTextSearch do
  @moduledoc """
  A full-text GIN index for ask_graph's second anchor ranking.

  ask_graph fuses two rankings (reciprocal-rank fusion, see
  `DeciduousMcp.Graph.Retrieval`): trigram similarity over the ILIKE
  matches, which idx_nodes_title_trgm and idx_nodes_description_trgm
  already serve, and `ts_rank` over

      to_tsvector('english', title || ' ' || coalesce(description, ''))

  The index is on exactly that expression. Retrieval's fragment spells it
  the same way; a different spelling (another config, no coalesce, a
  different separator) would not match the index and would plan as a
  sequential scan with a to_tsvector per row.

  CONCURRENTLY, as the trigram migration, so writes keep flowing while it
  builds.
  """
  use Ecto.Migration

  @disable_ddl_transaction true
  @disable_migration_lock true

  def up do
    execute("""
    CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_nodes_fts
      ON decision_nodes
      USING gin (to_tsvector('english'::regconfig, title || ' ' || coalesce(description, '')))
    """)
  end

  def down do
    execute("DROP INDEX CONCURRENTLY IF EXISTS idx_nodes_fts")
  end
end
