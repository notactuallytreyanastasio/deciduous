defmodule DeciduousMcp.Events.Listener do
  @moduledoc """
  Bridges Postgres `NOTIFY` to `Phoenix.PubSub`, so a WebSocket connection can
  subscribe to one workspace's topic instead of every connection running its
  own LISTEN.

  One `Postgrex.Notifications` connection for the whole app, not one per
  WebSocket. Postgres has a real connection-count ceiling, and every open
  WebSocket would otherwise cost a database connection just to sit idle
  waiting for the next write.

  Every event is rebroadcast to two topics: `"graph:" <> workspace` for a
  subscriber that only cares about one project, and `"graph:*"` for the
  cross-project view — the same `workspace: "*"` convention every other read
  tool already uses, kept consistent here rather than inventing a second
  meaning for the same character.

  ## What a frame carries

  A pointer with a label, not the row. For a node: `table`, `op` (INSERT,
  UPDATE, or DELETE for a soft delete), `workspace`, `id`, `change_id`,
  `node_type`, `title` (cut at 200 characters), `status`, `branch`, and for
  an update `changed`, the fields it changed. Every event also carries
  `seq`, its number in graph_events, and `at`. For an edge: `table`, `op`, `workspace`,
  `id`, `edge_type`, `from_change_id`, `to_change_id`, and `branch` taken
  from the edge's source node, since an edge row has none of its own. The
  title is there so a watcher can quote what landed instead of counting
  what kind of thing it was; the first arena's watcher counted, and reported
  a convention that did not exist.

  ## NOTIFY is a hint; graph_events is the record

  `Postgrex.Notifications` drops every notification sent while its
  connection is down, and with `auto_reconnect` it tells its subscriber
  nothing: in Postgrex 0.22.4 `handle_connect/1` re-issues LISTEN and
  `handle_disconnect/1` flips a private field, and neither sends a message.
  A Postgres restart therefore left a window, ending whenever the next
  reconnect attempt (every 500 ms) got through, in which `/ready` said 200
  and a write's event went nowhere. Release acceptance wrote a node in that
  window and waited for it until the socket timed out.

  Every event is in graph_events, numbered, before its NOTIFY is sent, so
  the listener reads what it missed from there:

    * it remembers the highest `seq` it has broadcast, starting from
      `max(seq)` at boot, so a restart does not replay the week;
    * every second, and at once when a notification skips past the next
      `seq`, it reads `seq > highest` (the primary key, at most
      500 rows; a larger backlog is logged and read in further
      batches straight away);
    * a `seq` it skipped over is a hole: a transaction that had not
      committed yet, or one that rolled back. Holes are asked for again on
      each pass for five minutes;
    * each `seq` is broadcast once, whichever way it arrives first.

  ## What this does not guarantee

  Delivery is at most about a second late: the pass reads through the
  Repo pool, so it catches up as soon as the pool reaches the database,
  whether or not LISTEN is back yet. That is also why `/ready` does not
  wait for the LISTEN connection: Postgrex exposes no way to ask whether
  it is connected short of reading its private state, and nothing is lost
  in the meantime.

  An event is lost only if its NOTIFY was missed and it also committed more
  than five minutes after a higher `seq` was broadcast, or behind more than
  1,000 other uncommitted seqs; a notification that does arrive
  is always broadcast. Order is by arrival, not by `seq`: an earlier `seq`
  that commits late is broadcast late.

  A WebSocket client that reconnects asks for what it missed with
  `?since=<seq>`, which reads graph_events (kept a week) directly: every
  higher `seq`, and every lower one whose transaction was still open when
  `since` was written (GraphSocket's `backlog/2`). That look-back assumes
  a lower seq that committed first went out first. It did if its NOTIFY
  was heard; if it was missed in an outage and the next pass delivers it
  after a later event, a client that disconnects within that second and
  resumes after the later event does not get it.
  """
  use GenServer
  require Logger

  alias DeciduousMcp.Repo

  @channel "graph_events"

  # How often to read graph_events for events that NOTIFY did not bring.
  # One indexed query that finds nothing, once a second.
  @catch_up_every_ms 1_000

  # The most events one pass reads; a larger backlog is read in batches.
  @batch 500

  # A hole is asked for again for this long, and at most this many are kept.
  @hole_ttl_ms 300_000
  @max_holes 1_000

  # How many broadcast seqs are remembered for dedupe. A duplicate can only
  # come from a pass and a NOTIFY for the same event, which are close together.
  @max_sent 10_000

  def start_link(opts) do
    GenServer.start_link(__MODULE__, opts, name: Keyword.get(opts, :name, __MODULE__))
  end

  @impl true
  def init(opts) do
    # Read where the log stands before listening: anything committed
    # between the two is read back by the first pass.
    state = initial_state(opts)
    config = Keyword.merge(Repo.config(), Keyword.get(opts, :connect, []))

    {:ok, pid} =
      Postgrex.Notifications.start_link(Keyword.merge(config, auto_reconnect: true))

    # Auto-reconnect accepts the subscription before Postgres is reachable.
    # Keep the listener alive; Postgrex reissues LISTEN after reconnecting.
    ref =
      case Postgrex.Notifications.listen(pid, @channel) do
        {:ok, ref} ->
          Logger.info("Events.Listener: listening on #{@channel}")
          ref

        {:eventually, ref} ->
          Logger.warning("Events.Listener: waiting for Postgres to subscribe to #{@channel}")
          ref
      end

    schedule(state)
    {:ok, %{state | conn: pid, ref: ref}}
  end

  @doc false
  # The listener's state without a connection, so a test can drive
  # `handle_info/2` directly. Options: `:broadcast` (a function of the
  # event), `:batch`, `:catch_up_every_ms` (nil turns the timer off).
  # `start_link/1` also takes `:name` and `:connect` (Postgrex options for
  # the LISTEN connection, merged over the Repo's).
  def initial_state(opts \\ []) do
    %{
      conn: nil,
      ref: nil,
      high: safe_max_seq(),
      # When the database was down at boot, `high` is unknown until a pass
      # can read it. That pass must not take max(seq) as it stands then: a
      # write made after boot but before that pass (the server says ready
      # as soon as the Repo answers) would already be at or below it and
      # never be broadcast. It starts from the events logged before boot.
      booted_at: NaiveDateTime.utc_now(),
      sent: :gb_sets.new(),
      holes: %{},
      broadcast: Keyword.get(opts, :broadcast, &broadcast/1),
      batch: Keyword.get(opts, :batch, @batch),
      every:
        Keyword.get_lazy(opts, :catch_up_every_ms, fn ->
          Application.get_env(:deciduous_mcp, :events_catch_up_every_ms, @catch_up_every_ms)
        end),
      failing: false
    }
  end

  @impl true
  def handle_info({:notification, _pid, _ref, @channel, payload}, state) do
    case Jason.decode(payload) do
      {:ok, %{"seq" => seq} = event} when is_integer(seq) ->
        # A notification past the next seq means some came and went
        # unheard, or have not committed yet. Read them first, so what did
        # commit goes out in order and this one is not sent twice.
        state =
          if is_integer(state.high) and seq > state.high + 1, do: catch_up(state), else: state

        {:noreply, deliver(state, seq, event)}

      {:ok, event} when is_map(event) ->
        state.broadcast.(event)
        {:noreply, state}

      {:ok, other} ->
        Logger.warning("Events.Listener: payload that is not an object: #{inspect(other)}")
        {:noreply, state}

      {:error, reason} ->
        Logger.warning("Events.Listener: undecodable payload: #{inspect(reason)}")
        {:noreply, state}
    end
  end

  def handle_info(:tick, state) do
    schedule(state)
    {:noreply, catch_up(state)}
  end

  # The rest of a backlog larger than one batch.
  def handle_info(:catch_up, state), do: {:noreply, catch_up(state)}

  # Nothing else is expected here: Postgrex.Notifications sends a
  # subscriber notifications and nothing more (see the moduledoc).
  def handle_info(_other, state), do: {:noreply, state}

  # A normal stop does not travel down the link, so the LISTEN connection
  # is closed here rather than left behind.
  @impl true
  def terminate(_reason, %{conn: conn}) when is_pid(conn) do
    if Process.alive?(conn), do: GenServer.stop(conn)
    :ok
  end

  def terminate(_reason, _state), do: :ok

  defp schedule(%{every: ms}) when is_integer(ms), do: Process.send_after(self(), :tick, ms)
  defp schedule(_), do: :ok

  # Broadcast `event` unless its seq already went out, and account for any
  # seq it skipped over.
  defp deliver(state, seq, event) do
    if :gb_sets.is_member(seq, state.sent) do
      state
    else
      state.broadcast.(event)
      holes = add_holes(Map.delete(state.holes, seq), state.high, seq)
      high = if is_integer(state.high), do: max(state.high, seq), else: seq
      %{state | high: high, holes: holes, sent: remember(state.sent, seq)}
    end
  end

  defp add_holes(holes, high, seq) when is_integer(high) and seq > high + 1 do
    now = System.monotonic_time(:millisecond)
    first = max(high + 1, seq - @max_holes)
    holes = Enum.reduce(first..(seq - 1)//1, holes, &Map.put_new(&2, &1, now))

    if map_size(holes) > @max_holes do
      holes |> Enum.sort() |> Enum.take(-@max_holes) |> Map.new()
    else
      holes
    end
  end

  defp add_holes(holes, _high, _seq), do: holes

  defp remember(sent, seq) do
    sent = :gb_sets.add(seq, sent)

    if :gb_sets.size(sent) > @max_sent do
      {_, sent} = :gb_sets.take_smallest(sent)
      sent
    else
      sent
    end
  end

  # One pass over graph_events: the holes still worth asking about, then
  # up to one batch past the highest seq broadcast. A database that is down
  # leaves the state as it was; the next pass tries again.
  defp catch_up(state) do
    state = expire_holes(state)

    try do
      state =
        if is_integer(state.high),
          do: state,
          else: %{state | high: max_seq_before(state.booted_at)}

      filled = if map_size(state.holes) == 0, do: [], else: fetch_holes(Map.keys(state.holes))
      state = Enum.reduce(filled, state, fn {seq, payload}, s -> deliver(s, seq, payload) end)

      rows = fetch_after(state.high, state.batch)
      state = Enum.reduce(rows, state, fn {seq, payload}, s -> deliver(s, seq, payload) end)

      if length(rows) >= state.batch do
        Logger.warning(
          "Events.Listener: read #{state.batch} missed events up to seq #{state.high}; reading on"
        )

        send(self(), :catch_up)
      end

      if state.failing, do: Logger.info("Events.Listener: graph_events readable again")
      %{state | failing: false}
    rescue
      e in [DBConnection.ConnectionError, DBConnection.OwnershipError, Postgrex.Error] ->
        unless state.failing or match?(%DBConnection.OwnershipError{}, e) do
          Logger.warning("Events.Listener: cannot read graph_events: #{Exception.message(e)}")
        end

        %{state | failing: true}
    end
  end

  defp expire_holes(%{holes: holes} = state) when map_size(holes) == 0, do: state

  defp expire_holes(state) do
    cutoff = System.monotonic_time(:millisecond) - @hole_ttl_ms
    %{state | holes: Map.reject(state.holes, fn {_, at} -> at < cutoff end)}
  end

  defp fetch_after(high, limit) do
    Repo.query!(
      "SELECT seq, payload FROM graph_events WHERE seq > $1 ORDER BY seq LIMIT $2",
      [high, limit]
    ).rows
    |> Enum.map(fn [seq, payload] -> {seq, payload} end)
  end

  defp fetch_holes(seqs) do
    Repo.query!(
      "SELECT seq, payload FROM graph_events WHERE seq = ANY($1) ORDER BY seq",
      [seqs]
    ).rows
    |> Enum.map(fn [seq, payload] -> {seq, payload} end)
  end

  defp max_seq do
    Repo.query!("SELECT coalesce(max(seq), 0) FROM graph_events", []).rows
    |> then(fn [[n]] -> n end)
  end

  # The highest seq logged before `booted_at`, less a margin for the
  # distance between this node's clock and the database's (inserted_at is
  # the database's now()). The margin can only re-send an event logged
  # just before boot, which no subscriber of this server has seen: sockets
  # connect after boot. A transaction that began more than the margin
  # before boot and committed after it is still missed.
  @boot_margin_seconds 5

  defp max_seq_before(booted_at) do
    cutoff = NaiveDateTime.add(booted_at, -@boot_margin_seconds, :second)

    Repo.query!(
      "SELECT coalesce(max(seq), 0) FROM graph_events WHERE inserted_at < $1",
      [cutoff]
    ).rows
    |> then(fn [[n]] -> n end)
  end

  # At boot the database may be down; the first pass that can reach it
  # starts from the events logged before boot (max_seq_before/1).
  defp safe_max_seq do
    max_seq()
  rescue
    _ in [DBConnection.ConnectionError, DBConnection.OwnershipError, Postgrex.Error] -> nil
  end

  defp broadcast(%{"workspace" => workspace} = event) when is_binary(workspace) do
    Phoenix.PubSub.broadcast(DeciduousMcp.PubSub, "graph:" <> workspace, {:graph_event, event})
    Phoenix.PubSub.broadcast(DeciduousMcp.PubSub, "graph:*", {:graph_event, event})
  end

  defp broadcast(other) do
    Logger.warning("Events.Listener: payload with no workspace: #{inspect(other)}")
  end
end
