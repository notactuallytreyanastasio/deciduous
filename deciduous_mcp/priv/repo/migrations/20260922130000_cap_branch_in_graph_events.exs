defmodule DeciduousMcp.Repo.Migrations.CapBranchInGraphEvents do
  use Ecto.Migration

  @moduledoc """
  Caps the `branch` in the NOTIFY payload at 200 characters, the way the
  title already is.

  The previous function capped `title` and reasoned about the ~8000-byte
  `pg_notify` limit for it, then copied `metadata ->> 'branch'` through
  unbounded. Nothing bounds a branch on the way in: `add_node` declares it
  as a plain string, `validate_metadata` only checks confidence, and import
  passes any metadata map through. A branch near 8 KB makes `pg_notify`
  raise `payload string too long` inside the AFTER trigger, which rolls back
  the statement. For a node that is one lost write. For an edge it is
  worse, because the edge payload looks the branch up on the *source* node:
  one node with an oversized branch then aborts every edge written from it,
  and a bulk import (one transaction per chunk) loses the whole chunk.

  Byte budget after this change, worst case: title 200 chars, every one a
  control character escaped to `\\uXXXX`, is 1200 bytes; branch the same,
  1200; workspace name is validated to 128 chars, 512 bytes; change_id is a
  255-char varchar, 1020 bytes; the fixed keys and a UUID are under 300.
  About 4.2 KB, comfortably under the cap.

  A new migration rather than an edit of 20260922120000, because that one
  has already run in production and `schema_migrations` would not run it
  again.
  """

  def up do
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
  end

  # The 20260922120000 body, verbatim: title capped, branch not.
  def down do
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
          'branch', NEW.metadata ->> 'branch'
        );
      ELSIF TG_TABLE_NAME = 'decision_edges' THEN
        SELECT name INTO ws_name FROM workspaces WHERE id = NEW.workspace_id;
        SELECT metadata ->> 'branch' INTO from_branch
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
  end
end
