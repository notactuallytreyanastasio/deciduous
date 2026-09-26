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
  alias DeciduousMcp.Web.GraphSocket

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

  # seq is taken inside the writing transaction; NOTIFY and visibility
  # follow commit order. A writer that takes its seq first and commits
  # second must still reach a live subscriber, through the listener and a
  # socket that has already pushed the higher seq.
  test "a lower seq that commits after a higher one reaches a live socket", ctx do
    name = unique(ctx, "order")
    {:ok, ws} = Workspaces.find_or_create(name)
    test = self()

    start_supervised!(
      Supervisor.child_spec(
        {Listener,
         name: :order_listener,
         catch_up_every_ms: nil,
         broadcast: fn event -> send(test, {:graph_event, event}) end},
        restart: :temporary
      )
    )

    {:ok, socket} = GraphSocket.init(%{topic: "graph:" <> name, since: nil})

    early =
      Task.async(fn ->
        Repo.transaction(fn ->
          {:ok, n} = Nodes.create_node(ws.id, %{node_type: "goal", title: "took seq first"})
          send(test, :early_written)
          receive do: (:commit -> n)
        end)
      end)

    assert_receive :early_written, 5_000
    {:ok, late} = Nodes.create_node(ws.id, %{node_type: "goal", title: "committed first"})
    send(early.pid, :commit)
    {:ok, early_node} = Task.await(early)

    [early_seq, late_seq] =
      for id <- [early_node.id, late.id] do
        Repo.query!("SELECT seq FROM graph_events WHERE payload->>'id' = $1", [id]).rows
        |> then(fn [[s]] -> s end)
      end

    assert early_seq < late_seq

    late_id = late.id
    early_id = early_node.id
    assert_receive {:graph_event, %{"id" => ^late_id} = late_event}, 5_000
    assert_receive {:graph_event, %{"id" => ^early_id} = early_event}, 5_000

    {:push, _, socket} = GraphSocket.handle_info({:graph_event, late_event}, socket)

    assert {:push, {:text, text}, _} =
             GraphSocket.handle_info({:graph_event, early_event}, socket)

    assert Jason.decode!(text)["seq"] == early_seq

    GenServer.stop(:order_listener)
  end
end
