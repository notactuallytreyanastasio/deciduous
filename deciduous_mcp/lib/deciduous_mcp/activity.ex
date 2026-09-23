defmodule DeciduousMcp.Activity do
  @moduledoc """
  Who wrote to which branch of a workspace, and when. A record, never a
  refusal.

  ## What this replaced, and why

  Until 1.0.8 every MCP write claimed an advisory lock on (workspace,
  branch) with a ten-second lease, and a second session writing the same
  branch was refused with the holder's name. The team probe (T2) ran three
  agents on one workspace and found the lock stopping the one writer it
  should never stop and unable to stop the other:

    * A one-shot client (initialize, one tool call, exit; every agent that
      shells out to an MCP client works this way) left its lease behind,
      and its own next process, a new session with the same client name,
      was refused for ten seconds: "locked by battery (0), session
      GNgCXo-N". Nothing tells a server that a session's process is gone.
    * The CLI takes no lock and cannot: its writes are replayed from a log
      (POST /ops), minutes or days after they were made, and it wrote 27 ms
      after an MCP lock was taken.

  And there is nothing for a lease to protect. Every write is atomic on its
  own: a node insert; an update read FOR UPDATE and written in one
  transaction (T1); capture_conversation_turn, log_decision and close_thread
  each in one transaction; /ops field by field, compare-and-set. Two agents
  writing one branch interleave their nodes, which is what two people
  working on one branch do, and neither write is lost or torn.

  What the agents used the lock for was the other half: seeing who else is
  writing where, so they could say so or pick another branch. That is what
  this keeps. Every MCP write records (workspace, branch, session, client)
  with the time; every /ops batch that applies something records the CLI
  the same way; `check_activity` lists what was recorded in the last five
  minutes. The workspace setting `lock_scope` no longer has an effect.
  """

  alias DeciduousMcp.Repo

  @window_seconds 300

  @doc "How far back `recent/2` looks by default, in seconds."
  def window_seconds, do: @window_seconds

  @doc """
  Records a write by `session_id` to `branch` (nil and "" are the same:
  no branch given). Always `:ok`; a failure to record raises, inside the
  write's own transaction when there is one.
  """
  def record(workspace_id, branch, session_id, client_name, client_version) do
    now = DateTime.utc_now()

    Repo.query!(
      """
      INSERT INTO write_activity
        (workspace_id, branch, session_id, client_name, client_version, first_seen_at, last_seen_at)
      VALUES ($1, $2, $3, $4, $5, $6, $6)
      ON CONFLICT (workspace_id, branch, session_id) DO UPDATE
        SET client_name = EXCLUDED.client_name,
            client_version = EXCLUDED.client_version,
            last_seen_at = EXCLUDED.last_seen_at
      """,
      [Ecto.UUID.dump!(workspace_id), branch || "", session_id, client_name, client_version, now]
    )

    :ok
  end

  @doc """
  Every (branch, session) that wrote to the workspace in the last
  `within_seconds`, most recent first.
  """
  def recent(workspace_id, within_seconds \\ @window_seconds) do
    since = DateTime.add(DateTime.utc_now(), -within_seconds, :second)

    %{rows: rows, columns: cols} =
      Repo.query!(
        """
        SELECT branch, session_id, client_name, client_version, first_seen_at, last_seen_at
          FROM write_activity
         WHERE workspace_id = $1 AND last_seen_at >= $2
         ORDER BY last_seen_at DESC
        """,
        [Ecto.UUID.dump!(workspace_id), since]
      )

    Enum.map(rows, fn row ->
      cols
      |> Enum.map(&String.to_existing_atom/1)
      |> Enum.zip(row)
      |> Map.new()
      |> Map.update!(:first_seen_at, &as_utc/1)
      |> Map.update!(:last_seen_at, &as_utc/1)
    end)
  end

  # Raw Postgrex results skip Ecto's type layer: a `timestamp` column
  # decodes as NaiveDateTime, and DateTime.diff/2 refuses a mixed pair.
  defp as_utc(%NaiveDateTime{} = naive), do: DateTime.from_naive!(naive, "Etc/UTC")
  defp as_utc(%DateTime{} = dt), do: dt
end
