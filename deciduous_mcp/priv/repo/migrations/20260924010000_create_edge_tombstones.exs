defmodule DeciduousMcp.Repo.Migrations.CreateEdgeTombstones do
  use Ecto.Migration

  @moduledoc """
  An unlink leaves a tombstone, so it can reach a clone and outlive a stale
  link.

  Edges were hard-deleted. Two things followed. `/export` had nothing to say
  about an edge an agent removed, so a clone that had it kept it after every
  pull (the model test's seeds 1790191170825091000 and 1790191560410294000).
  And an unlink of an edge the server never had (it reached the unlinker
  through git) left nothing behind, so the link replayed later from the
  laptop that made it put the edge back for good (round-2 NEW-3).

  A tombstone is keyed like the edge, by the endpoints' change_ids and the
  type, because an unlink can name an edge whose nodes this server has not
  got. It holds when the edge was removed. It is written by a trigger, so
  every path that deletes an edge row leaves one: the MCP tool, `/ops`, and
  whatever comes next. `/ops` sets `deciduous.unlinked_at` in its
  transaction to date the tombstone by the unlink rather than by its
  arrival, the same clock git's merge driver compares a tombstone with. An
  edge written again removes its tombstone.
  """

  def up do
    create table(:edge_tombstones, primary_key: false) do
      add :workspace_id, references(:workspaces, type: :binary_id, on_delete: :delete_all),
        null: false,
        primary_key: true

      add :from_change_id, :string, null: false, primary_key: true
      add :to_change_id, :string, null: false, primary_key: true
      add :edge_type, :string, null: false, primary_key: true
      add :deleted_at, :utc_datetime_usec, null: false
    end

    execute("""
    CREATE OR REPLACE FUNCTION edge_tombstone_on_delete() RETURNS trigger AS $$
    DECLARE
      stamp timestamp;
    BEGIN
      IF OLD.from_change_id IS NULL OR OLD.to_change_id IS NULL THEN
        RETURN OLD;
      END IF;
      -- The column is a UTC timestamp without a zone, like every other here.
      stamp := coalesce(
        nullif(current_setting('deciduous.unlinked_at', true), '')::timestamptz,
        now()
      ) AT TIME ZONE 'UTC';
      INSERT INTO edge_tombstones (workspace_id, from_change_id, to_change_id, edge_type, deleted_at)
        VALUES (OLD.workspace_id, OLD.from_change_id, OLD.to_change_id, OLD.edge_type, stamp)
        ON CONFLICT (workspace_id, from_change_id, to_change_id, edge_type)
        DO UPDATE SET deleted_at = greatest(edge_tombstones.deleted_at, EXCLUDED.deleted_at);
      RETURN OLD;
    END;
    $$ LANGUAGE plpgsql;
    """)

    execute("""
    CREATE OR REPLACE FUNCTION edge_tombstone_on_insert() RETURNS trigger AS $$
    BEGIN
      DELETE FROM edge_tombstones
        WHERE workspace_id = NEW.workspace_id
          AND from_change_id = NEW.from_change_id
          AND to_change_id = NEW.to_change_id
          AND edge_type = NEW.edge_type;
      RETURN NEW;
    END;
    $$ LANGUAGE plpgsql;
    """)

    execute("""
    CREATE TRIGGER decision_edges_tombstone_delete
      AFTER DELETE ON decision_edges
      FOR EACH ROW EXECUTE FUNCTION edge_tombstone_on_delete();
    """)

    execute("""
    CREATE TRIGGER decision_edges_tombstone_insert
      AFTER INSERT ON decision_edges
      FOR EACH ROW EXECUTE FUNCTION edge_tombstone_on_insert();
    """)
  end

  def down do
    execute("DROP TRIGGER IF EXISTS decision_edges_tombstone_delete ON decision_edges;")
    execute("DROP TRIGGER IF EXISTS decision_edges_tombstone_insert ON decision_edges;")
    execute("DROP FUNCTION IF EXISTS edge_tombstone_on_delete();")
    execute("DROP FUNCTION IF EXISTS edge_tombstone_on_insert();")
    drop table(:edge_tombstones)
  end
end
