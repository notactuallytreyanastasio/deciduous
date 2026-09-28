defmodule DeciduousMcp.Web.GraphSocket do
  @moduledoc """
  WebSocket half of the event stream: subscribes to the workspace's PubSub
  topic on connect, pushes each `{:graph_event, event}` as a JSON text frame,
  and pings the client on a timer. It holds no state a client can mutate, so
  there is nothing here for `handle_in/2` to act on.

  ## Why the server pings

  Bandit closes a WebSocket that has received nothing for 60 seconds, with
  close code 1002 and reason `:timeout`. A subscriber to this stream sends
  nothing by design — it only listens — so without the ping every subscriber
  was cut off at exactly sixty seconds, whether or not any event had been
  pushed, and had to reconnect and hope nothing landed in the gap. The first
  arena run reconnected nineteen times in twenty minutes because of this.

  A ping frame every 30 seconds fixes it from this side: every WebSocket
  client that matters (the browser constructor, Node's global `WebSocket`,
  Claude Code's `Monitor`) answers a ping with a pong automatically, and that
  pong is inbound traffic, which resets Bandit's idle timer. The default
  timeout is left alone on purpose. A client that has gone away without
  closing stops answering pings and is reaped after 60 seconds, which is
  what the timeout is for; `timeout: :infinity` would have kept every dead
  socket open forever.
  """
  @behaviour WebSock

  @ping_every_ms 30_000

  # At most this many missed events are replayed on a resume; past it the
  # client gets a gap frame and should re-read the graph.
  @max_backlog 5_000

  @impl true
  def init(%{topic: topic} = opts) do
    # Subscribe first, then read the backlog: an event committed in
    # between arrives both ways, and `sent` drops the second copy.
    Phoenix.PubSub.subscribe(DeciduousMcp.PubSub, topic)
    schedule_ping()

    case opts[:since] do
      nil ->
        {:ok, %{sent: MapSet.new()}}

      since ->
        {frames, sent} = backlog(topic, since)
        {:push, frames, %{sent: sent}}
    end
  end

  @impl true
  def handle_in(_data, state), do: {:ok, state}

  # Each event has a `seq` from graph_events (see the 20260924020000
  # migration). One the backlog already sent is not sent again. The test is
  # membership, not "at or below where the backlog ended": live events do
  # not arrive in seq order (a transaction that took its seq early can
  # commit late, and the listener's catch-up fills such holes afterwards),
  # so a live 10 after a backlog that ended at 11 is new, not a copy. Live
  # events are already deduplicated by Events.Listener.
  @impl true
  def handle_info({:graph_event, event}, state) do
    seq = event["seq"]

    if is_integer(seq) and MapSet.member?(state.sent, seq) do
      {:ok, state}
    else
      {:push, {:text, Jason.encode!(event)}, state}
    end
  end

  def handle_info(:ping, state) do
    schedule_ping()
    {:push, {:ping, ""}, state}
  end

  def handle_info(_other, state), do: {:ok, state}

  @impl true
  def terminate(_reason, state), do: {:ok, state}

  defp schedule_ping, do: Process.send_after(self(), :ping, @ping_every_ms)

  # The events this topic delivered that a client which saw `since` may
  # not have, oldest seq first: every `seq > since`, and every `seq < since`
  # whose transaction was running when `since` was written, or began after
  # (its `xact_id` is at or above that row's `horizon`; see the
  # 20260928120000 migration). A lower seq committed before `since` did went
  # out before it, in commit order; only one committed after can have been
  # missed. The second part is empty unless writers overlapped, and what it
  # returns that the client already has, the client drops by `seq`.
  #
  # `since` is looked up as the first row at or after it, so a seq that
  # rolled back still has a horizon. A NULL horizon (a row from before the
  # migration) means no look-back. A resume from before what the table
  # still holds (pruned after a week) or past @max_backlog starts with a
  # gap frame, so the client knows the stream is not complete and can
  # re-read. The two halves are separate selects so each has an index to
  # walk (seq for one, xact_id for the other); an OR of the two made the
  # planner read the workspace's whole week.
  defp backlog(topic, since) do
    {scope, params} =
      case topic do
        "graph:*" -> {"", [since, @max_backlog + 1]}
        "graph:" <> name -> {"AND workspace = $3", [since, @max_backlog + 1, name]}
      end

    rows =
      DeciduousMcp.Repo.query!(
        """
        WITH h AS (SELECT horizon FROM graph_events WHERE seq >= $1 ORDER BY seq LIMIT 1)
        (SELECT seq, payload FROM graph_events
          WHERE seq < $1 AND xact_id >= (SELECT horizon FROM h) #{scope}
          ORDER BY seq LIMIT $2)
        UNION ALL
        (SELECT seq, payload FROM graph_events
          WHERE seq > $1 #{scope}
          ORDER BY seq LIMIT $2)
        ORDER BY seq
        LIMIT $2
        """,
        params
      ).rows
      |> Enum.map(fn [seq, payload] -> {seq, payload} end)

    [[oldest]] = DeciduousMcp.Repo.query!("SELECT min(seq) FROM graph_events").rows
    truncated = length(rows) > @max_backlog
    rows = Enum.take(rows, @max_backlog)

    gap =
      if truncated or (is_integer(oldest) and oldest > since + 1 and since > 0) do
        [{:text, Jason.encode!(%{gap: true, since: since, reason: gap_reason(truncated)})}]
      else
        []
      end

    sent = MapSet.new(rows, &elem(&1, 0))
    {gap ++ Enum.map(rows, fn {_, payload} -> {:text, Jason.encode!(payload)} end), sent}
  end

  defp gap_reason(true), do: "more than #{@max_backlog} events were missed; re-read the graph"
  defp gap_reason(false), do: "events before this server's oldest kept event were missed"
end
