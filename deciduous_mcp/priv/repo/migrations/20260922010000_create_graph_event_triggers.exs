defmodule DeciduousMcp.Repo.Migrations.CreateGraphEventTriggers do
  use Ecto.Migration

  @moduledoc """
  Live fanout: every insert into `decision_nodes` or `decision_edges` now also
  calls `pg_notify('graph_events', ...)`, so a connected listener finds out
  the moment it happens instead of polling.

  A trigger, not an application-level broadcast call added at every insertion
  point. `decision_nodes` has three insert paths today — `Nodes.create_node/2`
  (single-row, Ecto changeset), `Import.run/1` (bulk `insert_all`, which skips
  Ecto callbacks entirely), and whatever the next one turns out to be. A
  trigger fires on every one of them, including future ones, because it runs
  at the table level; an application hook only fires where someone remembered
  to add it.

  The payload is a pointer, not the row: `workspace`, `id`, `change_id` (or the
  edge's endpoints), `node_type`/`edge_type`, `branch`. NOTIFY's payload caps
  at ~8000 bytes and the whole design here is "go look," not "here it is" —
  content is one `check_activity` or `query_nodes` call away for whoever
  receives the pointer.

  `workspace` is looked up inside the trigger (one indexed point lookup per
  write) rather than left as `workspace_id`, so a subscriber can filter by the
  same name every other endpoint already uses instead of maintaining its own
  id-to-name cache.

  Covers INSERT and UPDATE, not DELETE, and covering UPDATE at all was a
  correction, not the original plan. `ON CONFLICT (...) DO UPDATE` — exactly
  what `Import.run/1`'s idempotent re-import uses — does not fire an AFTER
  INSERT trigger for the row it conflicts with; Postgres treats that as an
  UPDATE, full stop. Proved it against a throwaway table before trusting the
  docstring above: a second `INSERT ... ON CONFLICT DO UPDATE` on the same id
  produced no event under an INSERT-only trigger, and did under an UPDATE one.
  Skipping UPDATE would have meant "fires on every write path" was true only
  for the first time any given node was ever written — false for the whole
  steady state of `deciduous remote push` re-imports.

  The UPDATE trigger carries `WHEN (OLD IS DISTINCT FROM NEW)`, also proved
  against a throwaway table: an idempotent re-import with byte-identical
  content still runs `UPDATE ... SET`, and without this guard it would fire an
  event for a write that changed nothing, on every single re-push.

  DELETE is left out — soft-deletes go through `deleted_at`, which is already
  an UPDATE this trigger catches; there is no hard-delete path today.
  """

  def up do
    execute("""
    CREATE OR REPLACE FUNCTION notify_graph_event() RETURNS trigger AS $$
    -- TG_OP is INSERT or UPDATE here (this function is only ever attached to
    -- those two), so `op` in the payload is always one of those two values.
    DECLARE
      ws_name text;
      payload json;
    BEGIN
      IF TG_TABLE_NAME = 'decision_nodes' THEN
        SELECT name INTO ws_name FROM workspaces WHERE id = NEW.workspace_id;

        payload := json_build_object(
          'table', 'decision_nodes',
          'op', TG_OP,
          'workspace', ws_name,
          'id', NEW.id,
          'change_id', NEW.change_id,
          'node_type', NEW.node_type,
          'branch', NEW.metadata ->> 'branch'
        );
      ELSIF TG_TABLE_NAME = 'decision_edges' THEN
        SELECT name INTO ws_name FROM workspaces WHERE id = NEW.workspace_id;

        payload := json_build_object(
          'table', 'decision_edges',
          'op', TG_OP,
          'workspace', ws_name,
          'id', NEW.id,
          'edge_type', NEW.edge_type,
          'from_change_id', NEW.from_change_id,
          'to_change_id', NEW.to_change_id
        );
      END IF;

      PERFORM pg_notify('graph_events', payload::text);
      RETURN NEW;
    END;
    $$ LANGUAGE plpgsql;
    """)

    execute("""
    CREATE TRIGGER decision_nodes_notify_insert
      AFTER INSERT ON decision_nodes
      FOR EACH ROW EXECUTE FUNCTION notify_graph_event();
    """)

    execute("""
    CREATE TRIGGER decision_nodes_notify_update
      AFTER UPDATE ON decision_nodes
      FOR EACH ROW WHEN (OLD IS DISTINCT FROM NEW)
      EXECUTE FUNCTION notify_graph_event();
    """)

    execute("""
    CREATE TRIGGER decision_edges_notify_insert
      AFTER INSERT ON decision_edges
      FOR EACH ROW EXECUTE FUNCTION notify_graph_event();
    """)

    execute("""
    CREATE TRIGGER decision_edges_notify_update
      AFTER UPDATE ON decision_edges
      FOR EACH ROW WHEN (OLD IS DISTINCT FROM NEW)
      EXECUTE FUNCTION notify_graph_event();
    """)
  end

  def down do
    execute("DROP TRIGGER IF EXISTS decision_nodes_notify_insert ON decision_nodes;")
    execute("DROP TRIGGER IF EXISTS decision_nodes_notify_update ON decision_nodes;")
    execute("DROP TRIGGER IF EXISTS decision_edges_notify_insert ON decision_edges;")
    execute("DROP TRIGGER IF EXISTS decision_edges_notify_update ON decision_edges;")
    execute("DROP FUNCTION IF EXISTS notify_graph_event();")
  end
end
