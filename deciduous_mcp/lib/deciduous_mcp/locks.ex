defmodule DeciduousMcp.Locks do
  @moduledoc """
  Advisory write locks, so two agents on different sessions don't interleave
  nodes into the same branch of the same workspace without knowing about each
  other.

  Not enforced by a database constraint on every write — `add_node` and
  friends stay ordinary inserts. It's enforced by `acquire/5` returning
  `{:error, holder}` when someone else holds the key, and every write tool
  calling it through `DeciduousMcp.MCP.Scope.write_workspace_id/2` before it
  touches `decision_nodes` at all. A caller that ignores the return value and
  writes anyway still can; this is advisory, the way its name says.

  Raw SQL, not `Repo.insert_all`'s `:on_conflict` DSL: the DSL can express
  "replace on conflict" but not "replace on conflict *only if* the existing
  row is expired or mine" — that conditional is the entire mechanism, so it's
  one hand-written upsert rather than a rewrite of what Ecto already refuses
  to express.
  """

  alias DeciduousMcp.Repo
  alias Ecto.Adapters.SQL

  @default_lease_seconds 10

  @doc """
  Claims `lock_key` in `workspace_id` for `session_id`, or reports who has it.

  Renewal is not a separate call: the same `session_id` re-acquiring always
  succeeds and pushes `expires_at` out again, which is what turns a single
  10-second lease into cover for an arbitrary run of writes from one agent —
  as long as consecutive writes are less than the lease apart, the lock never
  visibly changes hands.
  """
  def acquire(
        workspace_id,
        lock_key,
        session_id,
        client_name,
        client_version,
        lease_seconds \\ @default_lease_seconds
      ) do
    now = DateTime.utc_now()
    expires_at = DateTime.add(now, lease_seconds, :second)

    # RETURNING is empty exactly when the WHERE clause rejected the conflict
    # branch — held by someone else, not yet expired. That single fact is
    # cheaper to act on than a second read: one round trip either claims the
    # lock or tells us nothing happened, and a following read only fires in
    # the conflict case, not on the hot path.
    {:ok, %{rows: rows, columns: cols}} =
      SQL.query(
        Repo,
        """
        INSERT INTO write_locks (workspace_id, lock_key, session_id, client_name, client_version, acquired_at, expires_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (workspace_id, lock_key) DO UPDATE
          SET session_id = EXCLUDED.session_id,
              client_name = EXCLUDED.client_name,
              client_version = EXCLUDED.client_version,
              acquired_at = CASE
                WHEN write_locks.session_id = EXCLUDED.session_id THEN write_locks.acquired_at
                ELSE EXCLUDED.acquired_at
              END,
              expires_at = EXCLUDED.expires_at
          WHERE write_locks.expires_at < $6 OR write_locks.session_id = $3
        RETURNING workspace_id, lock_key, session_id, client_name, client_version, acquired_at, expires_at
        """,
        [
          Ecto.UUID.dump!(workspace_id),
          lock_key,
          session_id,
          client_name,
          client_version,
          now,
          expires_at
        ]
      )

    case rows do
      [row] ->
        {:ok, to_map(cols, row)}

      [] ->
        {:error, holder(workspace_id, lock_key)}
    end
  end

  @doc "The current, unexpired holder of a lock, or nil."
  def holder(workspace_id, lock_key) do
    now = DateTime.utc_now()

    {:ok, %{rows: rows, columns: cols}} =
      SQL.query(
        Repo,
        """
        SELECT workspace_id, lock_key, session_id, client_name, client_version, acquired_at, expires_at
        FROM write_locks
        WHERE workspace_id = $1 AND lock_key = $2 AND expires_at >= $3
        """,
        [Ecto.UUID.dump!(workspace_id), lock_key, now]
      )

    case rows do
      [row] -> to_map(cols, row)
      [] -> nil
    end
  end

  @doc """
  Every unexpired lock in a workspace — the read side of the original ask:
  "is another agent active here right now, and on what branch."
  """
  def active(workspace_id) do
    now = DateTime.utc_now()

    {:ok, %{rows: rows, columns: cols}} =
      SQL.query(
        Repo,
        """
        SELECT workspace_id, lock_key, session_id, client_name, client_version, acquired_at, expires_at
        FROM write_locks
        WHERE workspace_id = $1 AND expires_at >= $2
        ORDER BY acquired_at DESC
        """,
        [Ecto.UUID.dump!(workspace_id), now]
      )

    Enum.map(rows, &to_map(cols, &1))
  end

  @doc """
  The lock key for a write: the branch, unless the workspace is configured
  for `lock_scope: "workspace"`, in which case every branch shares one key
  and two agents on different branches do contend. Default is per-branch —
  "multi-branch sessions anywhere" — so this only narrows when asked to.
  """
  def lock_key_for(%{settings: settings}, branch) do
    case settings do
      %{"lock_scope" => "workspace"} -> "*"
      _ -> normalize_branch(branch)
    end
  end

  defp normalize_branch(nil), do: ""
  defp normalize_branch(""), do: ""
  defp normalize_branch(branch) when is_binary(branch), do: branch

  defp to_map(cols, row) do
    cols = Enum.map(cols, &String.to_atom/1)

    cols
    |> Enum.zip(row)
    |> Map.new()
    |> Map.update!(:workspace_id, &Ecto.UUID.cast!/1)
    |> Map.update!(:acquired_at, &as_utc/1)
    |> Map.update!(:expires_at, &as_utc/1)
  end

  # Raw Postgrex results skip Ecto's type layer, so a `timestamp` (no tz)
  # column decodes as NaiveDateTime, not DateTime — and DateTime.diff/2
  # rejects a mixed pair outright. Caught by the first real lock conflict in
  # testing: it crashed the whole server process, not just the one request,
  # because Hermes' request handling doesn't wrap tool execution in a rescue.
  # UTC is safe to assume: every write in this schema already is.
  defp as_utc(%NaiveDateTime{} = naive), do: DateTime.from_naive!(naive, "Etc/UTC")
  defp as_utc(%DateTime{} = dt), do: dt
end
