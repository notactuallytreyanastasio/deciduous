defmodule DeciduousMcp.Events.ListenerTest do
  @moduledoc """
  The listener's catch-up from graph_events, driven one message at a time.
  Inside the sandbox nothing commits, so no NOTIFY is ever sent: every
  event here is one the listener did not hear, which is the case it has
  to handle.
  """
  use DeciduousMcp.DataCase, async: false

  import ExUnit.CaptureLog

  alias DeciduousMcp.Events.Listener
  alias DeciduousMcp.Graph.{Nodes, Workspaces}
  alias DeciduousMcp.Web.GraphSocket

  setup do
    test = self()
    name = "listener-" <> Integer.to_string(System.unique_integer([:positive]))
    {:ok, ws} = Workspaces.find_or_create(name)

    state =
      Listener.initial_state(
        catch_up_every_ms: nil,
        broadcast: fn event -> send(test, {:got, event["seq"], event}) end
      )

    %{ws: ws, state: state}
  end

  defp node!(ws, title) do
    {:ok, n} = Nodes.create_node(ws.id, %{node_type: "goal", title: title})
    n
  end

  defp payload(seq) do
    Repo.one!(from(e in "graph_events", where: e.seq == ^seq, select: e.payload))
  end

  defp seqs(ws) do
    Repo.all(
      from(e in "graph_events", where: e.workspace == ^ws.name, order_by: e.seq, select: e.seq)
    )
  end

  defp notify(state, seq) do
    {:noreply, state} =
      Listener.handle_info(
        {:notification, self(), make_ref(), "graph_events", Jason.encode!(payload(seq))},
        state
      )

    state
  end

  defp tick(state) do
    {:noreply, state} = Listener.handle_info(:catch_up, state)
    state
  end

  defp received do
    receive do
      {:got, seq, _} -> [seq | received()]
    after
      0 -> []
    end
  end

  test "booted while the database was down: a write made before the first readable pass is broadcast",
       %{ws: ws, state: state} do
    # Release acceptance boots the server with PostgreSQL stopped, starts
    # it, waits for /ready and writes. The listener read max(seq) on its
    # first readable pass, after that write, so the event was taken as
    # already sent and the websocket read timed out.
    old = node!(ws, "logged before boot")
    [old_seq] = seqs(ws)

    Repo.query!(
      "UPDATE graph_events SET inserted_at = inserted_at - interval '1 hour' WHERE seq = $1",
      [old_seq]
    )

    %{rows: [[db_now]]} = Repo.query!("SELECT now() AT TIME ZONE 'UTC'", [])
    booted = state |> Map.put(:high, nil) |> Map.put(:booted_at, db_now)

    node!(ws, "written after boot, before the first readable pass")
    [_, new_seq] = seqs(ws)

    tick(booted)
    assert received() == [new_seq]
  end

  test "an event that never came by NOTIFY is broadcast by the next pass, once", %{
    ws: ws,
    state: state
  } do
    n = node!(ws, "unheard")
    [seq] = seqs(ws)

    state = tick(state)
    assert_received {:got, ^seq, %{"id" => id, "title" => "unheard"}}
    assert id == n.id

    # Its NOTIFY turning up late, or another pass, sends nothing more.
    state = notify(state, seq)
    _ = tick(state)
    assert received() == []
  end

  test "a fresh listener starts at max(seq) and does not replay the log", %{ws: ws} do
    node!(ws, "before boot")
    test = self()

    state =
      Listener.initial_state(
        catch_up_every_ms: nil,
        broadcast: &send(test, {:got, &1["seq"], &1})
      )

    _ = tick(state)
    assert received() == []
  end

  test "a notification past the next seq reads the ones it skipped first, in order", %{
    ws: ws,
    state: state
  } do
    for t <- ~w(a b c d), do: node!(ws, t)
    [s1, s2, s3, s4] = seqs(ws)

    # Seqs taken by earlier, rolled-back tests are holes; start just below.
    state = notify(%{state | high: s1 - 1}, s1)
    assert received() == [s1]

    # s2 and s3 never came by NOTIFY; s4 does.
    state = notify(state, s4)
    assert received() == [s2, s3, s4]

    # Late or repeated copies are dropped.
    state = notify(state, s2)
    state = notify(state, s4)
    _ = tick(state)
    assert received() == []
  end

  test "a seq that commits after a higher one is still broadcast, and the socket pushes it",
       %{ws: ws, state: state} do
    [[early]] = Repo.query!("SELECT nextval(pg_get_serial_sequence('graph_events', 'seq'))").rows
    node!(ws, "later")
    [late] = seqs(ws)
    assert late > early

    state = notify(state, late)
    assert received() == [late]

    # `early`'s transaction commits now; its NOTIFY is lost, and the pass
    # asks for the hole again.
    body = %{
      "workspace" => ws.name,
      "table" => "decision_nodes",
      "op" => "INSERT",
      "seq" => early
    }

    Repo.query!("INSERT INTO graph_events (seq, workspace, payload) VALUES ($1, $2, $3)", [
      early,
      ws.name,
      body
    ])

    state = tick(state)
    assert received() == [early]
    _ = tick(state)
    assert received() == []

    # A socket that already pushed `late` live still pushes `early`.
    {:ok, socket} = GraphSocket.init(%{topic: "graph:" <> ws.name, since: nil})
    {:push, _, socket} = GraphSocket.handle_info({:graph_event, payload(late)}, socket)

    assert {:push, {:text, text}, _} =
             GraphSocket.handle_info({:graph_event, payload(early)}, socket)

    assert Jason.decode!(text)["seq"] == early
  end

  test "a backlog larger than a batch is read in batches, and says so", %{ws: ws} do
    test = self()

    state =
      Listener.initial_state(
        catch_up_every_ms: nil,
        batch: 2,
        broadcast: &send(test, {:got, &1["seq"], &1})
      )

    for t <- ~w(a b c d e), do: node!(ws, t)
    all = seqs(ws)

    log = capture_log(fn -> send(test, {:state, tick(state)}) end)
    assert log =~ "read 2 missed events"
    assert_received {:state, state}
    assert_received :catch_up
    assert received() == Enum.take(all, 2)

    capture_log(fn -> send(test, {:state, tick(state)}) end)
    assert_received {:state, state}
    assert_received :catch_up
    state = tick(state)
    refute_received :catch_up
    assert received() == Enum.drop(all, 2)
    _ = state
  end
end
