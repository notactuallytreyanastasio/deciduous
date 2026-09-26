defmodule DeciduousMcp.Events.ListenerOutageTest do
  @moduledoc """
  Release acceptance failed on unrelated PRs with a WebSocket that never
  saw the add_node it had just made, right after the database-outage
  step. The listener's Postgrex.Notifications connection reconnects on its
  own, but a NOTIFY sent before it has re-issued LISTEN is gone, and
  nothing tells the listener it was away.

  This kills a listener's backend and writes before it can reconnect.
  Postgrex reconnects at once when the server is still up, so the
  Notifications process is suspended to hold that window open. The
  listener is this test's own, broadcasting to the test process, so the
  app's listener (whose connection is untouched) cannot make it pass.
  """
  use DeciduousMcp.RealDbCase, async: false

  alias DeciduousMcp.Events.Listener
  alias DeciduousMcp.Graph.{Nodes, Workspaces}
  alias DeciduousMcp.Repo

  test "an event written while the listener's connection is down still reaches a subscriber",
       ctx do
    name = unique(ctx, "outage")
    {:ok, ws} = Workspaces.find_or_create(name)
    test = self()
    app_name = "listener-outage-#{System.unique_integer([:positive])}"

    start_supervised!(
      Supervisor.child_spec(
        {Listener,
         name: :outage_listener,
         catch_up_every_ms: 100,
         connect: [parameters: [application_name: app_name]],
         broadcast: fn event -> send(test, {:graph_event, event}) end},
        restart: :temporary
      )
    )

    %{conn: conn} = :sys.get_state(:outage_listener)
    :ok = :sys.suspend(conn)

    id =
      try do
        # With a timeout, pg_terminate_backend waits for the backend to exit.
        %{rows: [[true]]} =
          Repo.query!(
            "SELECT pg_terminate_backend(pid, 5000) FROM pg_stat_activity WHERE application_name = $1",
            [app_name]
          )

        {:ok, node} = Nodes.create_node(ws.id, %{node_type: "goal", title: "while away"})
        node.id
      after
        :sys.resume(conn)
      end

    assert_receive {:graph_event, %{"id" => ^id, "op" => "INSERT"}}, 5_000

    # The next write arrives by NOTIFY once LISTEN is back, and by the
    # timer too; it is broadcast once.
    {:ok, next} = Nodes.create_node(ws.id, %{node_type: "goal", title: "after"})
    next_id = next.id
    assert_receive {:graph_event, %{"id" => ^next_id}}, 5_000
    refute_receive {:graph_event, %{"id" => ^next_id}}, 300

    # Stopped between messages, not by the supervisor's exit signal, which
    # would cut a pass off mid-query.
    GenServer.stop(:outage_listener)
  end
end
