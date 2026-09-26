defmodule DeciduousMcp.Repo.Migrations.RelatedIdentifierIndexes do
  @moduledoc """
  Indexes for the relations `DeciduousMcp.Graph.Related` reads off
  `metadata.files` and `metadata.commit`.

  Related used to load every live node of the workspace that had files or a
  commit, on every show_node and every ask_graph round, and rank in Elixir.
  With 30,000 nodes naming one file, show_node took 1.1-1.4 s and ask_graph
  22-41 s. It now asks PostgreSQL only for the nodes naming the paths and
  commits it holds, which needs these indexes.

  Two IMMUTABLE SQL functions carry Related's identity rules into the
  database, so an index can be built on them:

    * `deciduous_normalize_path(text)` is `Related.normalize_path/1`: trim
      whitespace, collapse `//` and `/./` to `/`, drop leading `./`. The
      Elixir version replaces `//` and `/./` until nothing changes; the
      single regexp `/(\\.?/)+` -> `/` reaches the same fixed point, and
      RelatedTest checks the two agree on generated paths.
    * `deciduous_node_files(jsonb)` is the normalised, de-duplicated
      `files` list, or NULL when `files` is absent or is not a list of
      non-blank strings (Related's "malformed", which matches nothing).
    * `deciduous_commit_key(jsonb)` is `Related.commit_key/1`: the trimmed,
      lowercased commit when it is 7 to 40 hex digits, else NULL.

  Every reference to another function is schema-qualified: STRUCTURE.sql
  and pg_restore run with an empty search_path, and an index expression
  that resolved `deciduous_normalize_path` through search_path would fail to
  build there.

  The GIN index has `fastupdate = off`. With the default, new entries wait
  in an unsorted pending list until a vacuum merges them, and every scan
  reads the whole list: right after 30,000 inserts one lookup of the hub
  path took 15 ms instead of 0.5 ms, and ask_graph does one per path per
  round. Nodes are written one at a time by people and agents, so paying
  at insert time is the right side of that trade.

  CONCURRENTLY, as the other search indexes, so writes keep flowing while
  they build.
  """
  use Ecto.Migration

  @disable_ddl_transaction true
  @disable_migration_lock true

  def up do
    execute("""
    CREATE OR REPLACE FUNCTION public.deciduous_normalize_path(p text) RETURNS text
      LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE
      AS $$
        SELECT regexp_replace(
                 regexp_replace(
                   regexp_replace(p, '^[[:space:]]+|[[:space:]]+$', '', 'g'),
                   '/(\\.?/)+', '/', 'g'),
                 '^(\\./)+', '')
      $$
    """)

    execute("""
    CREATE OR REPLACE FUNCTION public.deciduous_node_files(metadata jsonb) RETURNS text[]
      LANGUAGE sql IMMUTABLE PARALLEL SAFE
      AS $$
        SELECT CASE
          WHEN jsonb_typeof(metadata) IS DISTINCT FROM 'object'
            OR jsonb_typeof(metadata -> 'files') IS DISTINCT FROM 'array' THEN NULL
          WHEN EXISTS (
            SELECT 1 FROM jsonb_array_elements(metadata -> 'files') AS e(v)
            WHERE jsonb_typeof(e.v) <> 'string'
               OR public.deciduous_normalize_path(e.v #>> '{}') = '') THEN NULL
          ELSE ARRAY(
            SELECT DISTINCT public.deciduous_normalize_path(e.v)
            FROM jsonb_array_elements_text(metadata -> 'files') AS e(v)
            ORDER BY 1)
        END
      $$
    """)

    execute("""
    CREATE OR REPLACE FUNCTION public.deciduous_commit_key(metadata jsonb) RETURNS text
      LANGUAGE sql IMMUTABLE PARALLEL SAFE
      AS $$
        SELECT CASE
          WHEN jsonb_typeof(metadata) = 'object'
           AND jsonb_typeof(metadata -> 'commit') = 'string'
           AND lower(btrim(metadata ->> 'commit')) ~ '^[0-9a-f]{7,40}$'
          THEN lower(btrim(metadata ->> 'commit'))
        END
      $$
    """)

    execute("""
    CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_nodes_files
      ON decision_nodes USING gin (public.deciduous_node_files(metadata))
      WITH (fastupdate = off)
      WHERE deleted_at IS NULL
    """)

    execute("""
    CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_nodes_ws_commit7
      ON decision_nodes (workspace_id, left(public.deciduous_commit_key(metadata), 7))
      WHERE deleted_at IS NULL
    """)
  end

  def down do
    execute("DROP INDEX CONCURRENTLY IF EXISTS idx_nodes_ws_commit7")
    execute("DROP INDEX CONCURRENTLY IF EXISTS idx_nodes_files")
    execute("DROP FUNCTION IF EXISTS public.deciduous_commit_key(jsonb)")
    execute("DROP FUNCTION IF EXISTS public.deciduous_node_files(jsonb)")
    execute("DROP FUNCTION IF EXISTS public.deciduous_normalize_path(text)")
  end
end
