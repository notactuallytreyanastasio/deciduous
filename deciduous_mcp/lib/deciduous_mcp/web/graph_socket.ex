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

  @impl true
  def init(%{topic: topic}) do
    Phoenix.PubSub.subscribe(DeciduousMcp.PubSub, topic)
    schedule_ping()
    {:ok, %{}}
  end

  @impl true
  def handle_in(_data, state), do: {:ok, state}

  @impl true
  def handle_info({:graph_event, event}, state) do
    {:push, {:text, Jason.encode!(event)}, state}
  end

  def handle_info(:ping, state) do
    schedule_ping()
    {:push, {:ping, ""}, state}
  end

  def handle_info(_other, state), do: {:ok, state}

  @impl true
  def terminate(_reason, state), do: {:ok, state}

  defp schedule_ping, do: Process.send_after(self(), :ping, @ping_every_ms)
end
