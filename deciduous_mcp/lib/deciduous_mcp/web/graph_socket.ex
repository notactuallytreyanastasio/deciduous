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
    # between arrives both ways, and `last` drops the second copy.
    Phoenix.PubSub.subscribe(DeciduousMcp.PubSub, topic)
    schedule_ping()

    case opts[:since] do
      nil ->
        {:ok, %{last: 0}}

      since ->
        {frames, last} = backlog(topic, since)
        {:push, frames, %{last: last}}
    end
  end

  @impl true
  def handle_in(_data, state), do: {:ok, state}

  # Each event has a `seq` from graph_events (see the 20260924020000
  # migration). One already sent, from the backlog, is not sent again.
  @impl true
  def handle_info({:graph_event, event}, state) do
    case event["seq"] do
      seq when is_integer(seq) and seq <= state.last ->
        {:ok, state}

      seq when is_integer(seq) ->
        {:push, {:text, Jason.encode!(event)}, %{state | last: seq}}

      _ ->
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

  # The events after `since` that this topic would have delivered, oldest
  # first. A resume from before what the table still holds (pruned after a
  # week) or past @max_backlog starts with a gap frame, so the client knows
  # the stream is not complete and can re-read.
  defp backlog(topic, since) do
    import Ecto.Query

    q =
      from(e in "graph_events",
        where: e.seq > ^since,
        order_by: [asc: e.seq],
        limit: ^(@max_backlog + 1),
        select: {e.seq, e.payload}
      )

    q =
      case topic do
        "graph:*" -> q
        "graph:" <> name -> where(q, [e], e.workspace == ^name)
      end

    rows = DeciduousMcp.Repo.all(q)
    oldest = DeciduousMcp.Repo.one(from(e in "graph_events", select: min(e.seq)))
    truncated = length(rows) > @max_backlog
    rows = Enum.take(rows, @max_backlog)

    gap =
      if truncated or (is_integer(oldest) and oldest > since + 1 and since > 0) do
        [{:text, Jason.encode!(%{gap: true, since: since, reason: gap_reason(truncated)})}]
      else
        []
      end

    last = rows |> List.last() |> then(fn r -> if r, do: elem(r, 0), else: since end)
    {gap ++ Enum.map(rows, fn {_, payload} -> {:text, Jason.encode!(payload)} end), last}
  end

  defp gap_reason(true), do: "more than #{@max_backlog} events were missed; re-read the graph"
  defp gap_reason(false), do: "events before this server's oldest kept event were missed"
end
