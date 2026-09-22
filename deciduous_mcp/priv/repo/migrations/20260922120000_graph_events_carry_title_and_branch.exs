defmodule DeciduousMcp.Repo.Migrations.GraphEventsCarryTitleAndBranch do
  use Ecto.Migration

  @moduledoc """
  The event payload gains what a watcher needs to say something true without
  a second call.

  The first arena run had a session narrating the event stream, and it
  reported a "second goal" convention spreading between agents that did not
  exist: the graph had ten goals, one per agent. The payload carried
  `node_type` and `op` but no title, so the watcher summarised from a tally
  it kept, counted an UPDATE as a new node, and invented a pattern. A watcher
  that can quote the title of the node that just landed does not have to
  count anything.

  Nodes now carry `title` (cut at 200 characters) and `status`. NOTIFY's
  payload is capped near 8000 bytes; a title is a phrase and a status is one
  word, so this stays a pointer with a label, not a copy of the row.

  Edges now carry `branch`, looked up from the edge's source node. An edge
  row has no branch of its own — the branch lives in node metadata — so the
  original payload had no field to put it in, and every edge event arrived
  branchless. A watcher could not tell an edge written by an agent that
  passed its branch from one that forgot, and the arena rules say forgetting
  takes the shared lock. One indexed lookup on the from-node answers it.

  `CREATE OR REPLACE FUNCTION` keeps the four existing triggers bound; they
  call the function by name.
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

  def down do
    execute("""
    CREATE OR REPLACE FUNCTION notify_graph_event() RETURNS trigger AS $$
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
  end
end
