defmodule DeciduousMcp.Repo.Migrations.CreateAgentMessages do
  use Ecto.Migration

  @moduledoc """
  The message board: agents working in parallel post interface changes,
  questions and answers here, instead of in a scratch markdown file.

  Messages are coordination, not graph. They are not exported, imported or
  synced, and nothing in decision_nodes refers to them.

  A reply stays in its workspace by construction: `(workspace_id, reply_to)`
  references `(workspace_id, id)`, so a reply_to naming a message of another
  workspace fails the foreign key even if the tool's own check were skipped.

  Every post is also a graph event (`message_posted`), numbered in
  graph_events like node and edge writes so `GET /events` streams it and a
  reconnecting watcher can replay it. Its own trigger function, rather than
  one more branch in notify_graph_event(), so the node and edge body is not
  copied into a fifth migration.
  """

  def up do
    create table(:agent_messages, primary_key: false) do
      add :id, :bigserial, primary_key: true

      add :workspace_id, references(:workspaces, type: :binary_id, on_delete: :delete_all),
        null: false

      add :branch, :text
      add :author, :text, null: false
      add :subject, :text, null: false
      add :body, :text, null: false
      add :mentions, {:array, :text}, null: false, default: []
      add :reply_to, :bigint

      add :created_at, :utc_datetime_usec,
        null: false,
        default: fragment("(now() AT TIME ZONE 'UTC')")
    end

    create unique_index(:agent_messages, [:workspace_id, :id])
    create index(:agent_messages, [:workspace_id, :reply_to])

    execute("""
    ALTER TABLE agent_messages
      ADD CONSTRAINT agent_messages_reply_to_fkey
      FOREIGN KEY (workspace_id, reply_to)
      REFERENCES agent_messages (workspace_id, id)
    """)

    # The tools hold these bounds first and say so in a sentence; these are
    # what a write that went round them would hit.
    #
    # One constraint per column, each a single BETWEEN. A single constraint
    # ANDing three BETWEENs does not survive pg_dump and reload: the migrated
    # database prints the nested ANDs with one more pair of parentheses than
    # a database loaded from STRUCTURE.sql, so the integration and release
    # acceptance structure checks can never both pass.
    execute("""
    ALTER TABLE agent_messages
      ADD CONSTRAINT agent_messages_author_bounds CHECK (char_length(author) BETWEEN 1 AND 100),
      ADD CONSTRAINT agent_messages_subject_bounds CHECK (char_length(subject) BETWEEN 1 AND 300),
      ADD CONSTRAINT agent_messages_body_bounds CHECK (octet_length(body) BETWEEN 1 AND 65536)
    """)

    execute("CREATE INDEX agent_messages_mentions_idx ON agent_messages USING GIN (mentions)")

    execute("""
    CREATE INDEX agent_messages_search_idx ON agent_messages
      USING GIN (to_tsvector('english', subject || ' ' || body))
    """)

    execute("""
    CREATE OR REPLACE FUNCTION notify_agent_message() RETURNS trigger AS $$
    DECLARE
      ws_name text;
      body jsonb;
      new_seq bigint;
    BEGIN
      SELECT name INTO ws_name FROM workspaces WHERE id = NEW.workspace_id;

      new_seq := nextval(pg_get_serial_sequence('graph_events', 'seq'));
      body := jsonb_build_object(
        'table', 'agent_messages',
        'op', 'INSERT',
        'event', 'message_posted',
        'workspace', ws_name,
        'id', NEW.id,
        'author', left(NEW.author, 200),
        'subject', left(NEW.subject, 200),
        'mentions', to_jsonb(NEW.mentions),
        'reply_to', NEW.reply_to,
        'branch', left(NEW.branch, 200),
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
    CREATE TRIGGER agent_messages_notify_insert
      AFTER INSERT ON agent_messages
      FOR EACH ROW EXECUTE FUNCTION notify_agent_message();
    """)
  end

  def down do
    execute("DROP TRIGGER IF EXISTS agent_messages_notify_insert ON agent_messages")
    execute("DROP FUNCTION IF EXISTS notify_agent_message()")
    drop table(:agent_messages)
  end
end
