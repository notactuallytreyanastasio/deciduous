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

  A pointer with a label, not the row. For a node: `table`, `op` (INSERT or
  UPDATE), `workspace`, `id`, `change_id`, `node_type`, `title` (cut at 200
  characters), `status`, `branch`. For an edge: `table`, `op`, `workspace`,
  `id`, `edge_type`, `from_change_id`, `to_change_id`, and `branch` taken
  from the edge's source node, since an edge row has none of its own. The
  title is there so a watcher can quote what landed instead of counting
  what kind of thing it was; the first arena's watcher counted, and reported
  a convention that did not exist.

  ## What this does not guarantee

  `Postgrex.Notifications` documents its own limit plainly: notifications that
  arrive while the connection is down are not queued and cannot be recovered,
  and reconnects race new LISTENs against notifications issued at the same
  moment. This is advisory, the same word used for the write locks — a
  subscriber that needs certainty calls `check_activity` or `query_nodes` to
  catch up; it does not treat a gap in this stream as proof nothing happened.
  """
  use GenServer
  require Logger

  alias DeciduousMcp.Repo

  @channel "graph_events"

  def start_link(opts) do
    GenServer.start_link(__MODULE__, opts, name: __MODULE__)
  end

  @impl true
  def init(_opts) do
    config = Repo.config()

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

    {:ok, %{conn: pid, ref: ref}}
  end

  @impl true
  def handle_info({:notification, _pid, _ref, @channel, payload}, state) do
    case Jason.decode(payload) do
      {:ok, %{"workspace" => workspace} = event} when is_binary(workspace) ->
        Phoenix.PubSub.broadcast(
          DeciduousMcp.PubSub,
          "graph:" <> workspace,
          {:graph_event, event}
        )

        Phoenix.PubSub.broadcast(DeciduousMcp.PubSub, "graph:*", {:graph_event, event})

      {:ok, other} ->
        Logger.warning("Events.Listener: payload with no workspace: #{inspect(other)}")

      {:error, reason} ->
        Logger.warning("Events.Listener: undecodable payload: #{inspect(reason)}")
    end

    {:noreply, state}
  end

  # Postgrex.Notifications also delivers connect/disconnect lifecycle
  # messages on this same mailbox; anything that is not the notification shape
  # above is one of those, not an error.
  @impl true
  def handle_info(_other, state), do: {:noreply, state}
end
