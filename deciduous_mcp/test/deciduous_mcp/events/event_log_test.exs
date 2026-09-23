defmodule DeciduousMcp.Events.EventLogTest do
  @moduledoc """
  `deciduous remote watch` showed four real updates as two lines (repeated
  frames were byte-identical and deduplicated), a node delete as
  "(updated)", and nothing for an unlink; and a watcher that reconnected
  could not ask for what it missed (round-2 BRIDGE-N6). Every event now
  has a `seq`, a delete is a DELETE, an unlink fires, and the socket
  replays from `since`.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.{Edges, Nodes, Workspaces}
  alias DeciduousMcp.Web.GraphSocket

  defp events(ws) do
    from(e in "graph_events", where: e.workspace == ^ws, order_by: e.seq, select: e.payload)
    |> Repo.all()
  end

  defp node!(ws, title) do
    {:ok, n} =
      Nodes.create_node(ws.id, %{node_type: "goal", title: title, metadata: %{"branch" => "b"}})

    n
  end

  test "bridge_n6: repeated updates, a delete and an unlink are each their own event" do
    name = "events-n6-" <> Integer.to_string(System.unique_integer([:positive]))
    {:ok, ws} = Workspaces.find_or_create(name)
    a = node!(ws, "a")
    b = node!(ws, "b")
    {:ok, _} = Edges.create_edge(ws.id, %{from_node_id: a.id, to_node_id: b.id})
    for s <- ~w(completed pending completed), do: {:ok, _} = Nodes.update_node(a.id, %{status: s})
    {:ok, _} = Edges.delete_edge(a.id, b.id)
    {:ok, _} = Nodes.delete_node(b.id)

    got = events(name)
    ops = Enum.map(got, &{&1["table"], &1["op"]})

    assert ops == [
             {"decision_nodes", "INSERT"},
             {"decision_nodes", "INSERT"},
             {"decision_edges", "INSERT"},
             {"decision_nodes", "UPDATE"},
             {"decision_nodes", "UPDATE"},
             {"decision_nodes", "UPDATE"},
             {"decision_edges", "DELETE"},
             {"decision_nodes", "DELETE"}
           ]

    seqs = Enum.map(got, & &1["seq"])
    assert seqs == Enum.sort(Enum.uniq(seqs)), "every event has its own, rising seq"
    assert Enum.at(got, 3)["changed"] == ["status"]
    assert Enum.all?(got, &is_binary(&1["at"]))
  end

  test "bridge_n6: a socket opened with since replays what it missed, and only once" do
    name = "events-resume-" <> Integer.to_string(System.unique_integer([:positive]))
    {:ok, ws} = Workspaces.find_or_create(name)
    a = node!(ws, "a")
    [first] = events(name)
    {:ok, _} = Nodes.update_node(a.id, %{status: "completed"})
    {:ok, _} = Nodes.update_node(a.id, %{title: "a, retitled"})

    {:push, frames, state} = GraphSocket.init(%{topic: "graph:" <> name, since: first["seq"]})
    replayed = Enum.map(frames, fn {:text, t} -> Jason.decode!(t) end)
    assert Enum.map(replayed, & &1["changed"]) == [["status"], ["title"]]

    # The live copy of an event the backlog already sent is dropped.
    assert {:ok, ^state} = GraphSocket.handle_info({:graph_event, List.last(replayed)}, state)

    {:ok, fresh} = GraphSocket.init(%{topic: "graph:" <> name, since: nil})
    assert fresh.last == 0
  end
end
