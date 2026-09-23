defmodule DeciduousMcp.Repo.Migrations.CreateGraphEventLog do
  use Ecto.Migration

  @moduledoc """
  Every graph event gets a sequence number and is kept for a week, and a
  delete is an event of its own.

  `deciduous remote watch` hid what it could not tell apart (round-2
  BRIDGE-N6). A frame carried no timestamp and no field diff, so the third
  status change of a node, or a prompt edit, produced a frame byte-identical
  to an earlier one, and the watcher's 60-second duplicate filter dropped
  it: four updates, two lines. A node's soft delete fired the UPDATE
  trigger and printed "(updated)". There was no DELETE trigger, so an
  unlink printed nothing. And a watcher that lost its connection
  reconnected with no way to ask what it had missed: NOTIFY is not queued
  for a listener that is not there.

  So the trigger function now:

    * writes each event to `graph_events` before it notifies, and puts the
      row's `seq` in the payload. A client dedupes by `seq`, not by bytes,
      and reconnects with `?since=<seq>`; the socket replays what it
      missed from this table before going live;
    * calls a node's update that sets `deleted_at` a DELETE, and so is an
      insert of a row that is already deleted (a tombstone for a node the
      server never had);
    * says which fields an update changed (`changed`);
    * fires on DELETE from `decision_edges` (an unlink).

  Rows older than seven days are pruned by the trigger itself, one write
  in a thousand, so the table stays bounded without a job to run. A
  watcher away longer than that gets a gap frame and should re-read.
  """

  def up do
    create table(:graph_events, primary_key: false) do
      add :seq, :bigserial, primary_key: true
      add :workspace, :text, null: false
      add :payload, :map, null: false

      add :inserted_at, :utc_datetime_usec,
        null: false,
        default: fragment("(now() AT TIME ZONE 'UTC')")
    end

    create index(:graph_events, [:workspace, :seq])

    execute("""
    CREATE OR REPLACE FUNCTION notify_graph_event() RETURNS trigger AS $$
    DECLARE
      ws_name text;
      from_branch text;
      body jsonb;
      op text := TG_OP;
      changed text[] := ARRAY[]::text[];
      n decision_nodes%ROWTYPE;
      e decision_edges%ROWTYPE;
      new_seq bigint;
    BEGIN
      IF TG_TABLE_NAME = 'decision_nodes' THEN
        n := NEW;
        IF TG_OP = 'UPDATE' AND OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL THEN
          op := 'DELETE';
        ELSIF TG_OP = 'INSERT' AND NEW.deleted_at IS NOT NULL THEN
          op := 'DELETE';
        ELSIF TG_OP = 'UPDATE' THEN
          IF OLD.title IS DISTINCT FROM NEW.title THEN changed := array_append(changed, 'title'); END IF;
          IF OLD.status IS DISTINCT FROM NEW.status THEN changed := array_append(changed, 'status'); END IF;
          IF OLD.description IS DISTINCT FROM NEW.description THEN changed := array_append(changed, 'description'); END IF;
          IF OLD.metadata IS DISTINCT FROM NEW.metadata THEN changed := array_append(changed, 'metadata'); END IF;
          IF OLD.node_type IS DISTINCT FROM NEW.node_type THEN changed := array_append(changed, 'node_type'); END IF;
          IF OLD.deleted_at IS NOT NULL AND NEW.deleted_at IS NULL THEN changed := array_append(changed, 'restored'); END IF;
        END IF;
        SELECT name INTO ws_name FROM workspaces WHERE id = n.workspace_id;

        body := jsonb_build_object(
          'table', 'decision_nodes',
          'op', op,
          'workspace', ws_name,
          'id', n.id,
          'change_id', n.change_id,
          'node_type', n.node_type,
          'title', left(n.title, 200),
          'status', n.status,
          'branch', left(n.metadata ->> 'branch', 200)
        );
        IF TG_OP = 'UPDATE' AND op = 'UPDATE' THEN
          body := body || jsonb_build_object('changed', to_jsonb(changed));
        END IF;
      ELSIF TG_TABLE_NAME = 'decision_edges' THEN
        IF TG_OP = 'DELETE' THEN e := OLD; ELSE e := NEW; END IF;
        SELECT name INTO ws_name FROM workspaces WHERE id = e.workspace_id;
        SELECT left(metadata ->> 'branch', 200) INTO from_branch
          FROM decision_nodes WHERE id = e.from_node_id;

        body := jsonb_build_object(
          'table', 'decision_edges',
          'op', op,
          'workspace', ws_name,
          'id', e.id,
          'edge_type', e.edge_type,
          'from_change_id', e.from_change_id,
          'to_change_id', e.to_change_id,
          'branch', from_branch
        );
      END IF;

      new_seq := nextval(pg_get_serial_sequence('graph_events', 'seq'));
      body := body || jsonb_build_object(
        'seq', new_seq,
        'at', to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.US"Z"')
      );
      INSERT INTO graph_events (seq, workspace, payload)
        VALUES (new_seq, coalesce(ws_name, ''), body);

      IF new_seq % 1000 = 0 THEN
        DELETE FROM graph_events WHERE inserted_at < (now() AT TIME ZONE 'UTC') - interval '7 days';
      END IF;

      PERFORM pg_notify('graph_events', body::text);
      RETURN NULL;
    END;
    $$ LANGUAGE plpgsql;
    """)

    execute("""
    CREATE TRIGGER decision_edges_notify_delete
      AFTER DELETE ON decision_edges
      FOR EACH ROW EXECUTE FUNCTION notify_graph_event();
    """)
  end

  def down do
    execute("DROP TRIGGER IF EXISTS decision_edges_notify_delete ON decision_edges;")

    # The 20260922130000 body.
    execute("""
    CREATE OR REPLACE FUNCTION notify_graph_event() RETURNS trigger AS $$
    DECLARE
      ws_name text;
      from_branch text;
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
          'title', left(NEW.title, 200),
          'status', NEW.status,
          'branch', left(NEW.metadata ->> 'branch', 200)
        );
      ELSIF TG_TABLE_NAME = 'decision_edges' THEN
        SELECT name INTO ws_name FROM workspaces WHERE id = NEW.workspace_id;
        SELECT left(metadata ->> 'branch', 200) INTO from_branch
          FROM decision_nodes WHERE id = NEW.from_node_id;

        payload := json_build_object(
          'table', 'decision_edges',
          'op', TG_OP,
          'workspace', ws_name,
          'id', NEW.id,
          'edge_type', NEW.edge_type,
          'from_change_id', NEW.from_change_id,
          'to_change_id', NEW.to_change_id,
          'branch', from_branch
        );
      END IF;

      PERFORM pg_notify('graph_events', payload::text);
      RETURN NEW;
    END;
    $$ LANGUAGE plpgsql;
    """)

    drop table(:graph_events)
  end
end
