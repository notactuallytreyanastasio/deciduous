defmodule DeciduousMcp.Repo.Migrations.GraphEventsVisibilityHorizon do
  use Ecto.Migration

  @moduledoc """
  Each graph_events row records which transaction wrote it (`xact_id`) and
  the oldest transaction still running when it did (`horizon`), so a
  resume with `?since=N` can find the lower seqs that committed after N.

  `seq` comes from nextval inside the writing transaction; a row becomes
  visible at COMMIT. Writer A takes 10, writer B takes 11 and commits
  first, and a client that saw 11 and resumed with `since=11` was replayed
  `seq > 11` and never got 10.

  Any event a client has not seen by the time it saw N committed after N
  did, so its transaction was running when N's took its snapshot, or began
  after. Either way its xid is at or above that snapshot's xmin, N's
  `horizon`. The replay is therefore `seq > N`, plus `seq < N` with
  `xact_id >= horizon(N)`. With no other writer open while N was written,
  horizon(N) is N's own xid and the second part is empty.

  Both columns are defaults rather than assignments in the two trigger
  functions (`notify_graph_event`, `notify_agent_message`): every insert
  into graph_events gets them, and neither 100-line function body is
  copied into one more migration. `pg_current_xact_id()` is the top-level
  transaction's id also from inside a savepoint.

  Rows written before this migration keep NULL in both. A NULL horizon
  means no look-back (the old behaviour), and a NULL xact_id is never
  replayed as a late commit; those rows are pruned within a week.

  The columns are added without a default and given one afterwards, so the
  table is not rewritten: a volatile default on ADD COLUMN would fill every
  existing row with the migration's own xid.
  """

  def up do
    execute("ALTER TABLE graph_events ADD COLUMN xact_id xid8, ADD COLUMN horizon xid8")

    execute("""
    ALTER TABLE graph_events
      ALTER COLUMN xact_id SET DEFAULT pg_current_xact_id(),
      ALTER COLUMN horizon SET DEFAULT pg_snapshot_xmin(pg_current_snapshot())
    """)

    create index(:graph_events, [:xact_id])
  end

  def down do
    drop index(:graph_events, [:xact_id])
    execute("ALTER TABLE graph_events DROP COLUMN horizon, DROP COLUMN xact_id")
  end
end
