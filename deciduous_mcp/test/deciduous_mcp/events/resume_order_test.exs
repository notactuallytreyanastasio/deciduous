defmodule DeciduousMcp.Events.ResumeOrderTest do
  @moduledoc """
  `seq` is taken by nextval inside the writing transaction, but a row
  becomes visible when that transaction commits. Writer A takes 10, writer
  B takes 11 and commits first. A client that saw 11 and reconnects with
  `?since=11` was replayed `seq > 11` and never got 10.

  These tests run two real transactions (no sandbox), so the order of
  commits is the one Postgres sees.
  """
  use DeciduousMcp.RealDbCase, async: false

  alias DeciduousMcp.Graph.{Nodes, Workspaces}
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Web.GraphSocket

  # Writer A: creates a node inside a transaction and holds it open until
  # told to commit.
  defp open_writer(ws, title) do
    test = self()

    task =
      Task.async(fn ->
        Repo.transaction(fn ->
          {:ok, n} = Nodes.create_node(ws.id, %{node_type: "goal", title: title})
          send(test, :written)
          receive do: (:commit -> n)
        end)
      end)

    assert_receive :written, 5_000
    task
  end

  defp commit(task) do
    send(task.pid, :commit)
    {:ok, node} = Task.await(task)
    node
  end

  defp seq_of(node) do
    Repo.query!("SELECT seq FROM graph_events WHERE payload->>'id' = $1", [node.id]).rows
    |> then(fn [[s]] -> s end)
  end

  defp replay(name, since) do
    case GraphSocket.init(%{topic: "graph:" <> name, since: since}) do
      {:push, frames, state} -> {Enum.map(frames, fn {:text, t} -> Jason.decode!(t) end), state}
      {:ok, state} -> {[], state}
    end
  end

  test "a resume after 11 replays 10 when 10 committed after 11", ctx do
    name = unique(ctx, "resume")
    {:ok, ws} = Workspaces.find_or_create(name)

    a = open_writer(ws, "took seq first, commits second")
    {:ok, b} = Nodes.create_node(ws.id, %{node_type: "goal", title: "committed first"})
    b_seq = seq_of(b)

    # The client sees 11 and nothing else, then disconnects.
    {seen, _} = replay(name, 0)
    assert Enum.map(seen, & &1["seq"]) == [b_seq]

    a_node = commit(a)
    a_seq = seq_of(a_node)
    assert a_seq < b_seq

    {frames, _} = replay(name, b_seq)
    assert a_seq in Enum.map(frames, & &1["seq"]), "since=#{b_seq} must replay #{a_seq}"
    refute b_seq in Enum.map(frames, & &1["seq"])
  end

  test "a resume with no writer open in between replays nothing below since", ctx do
    name = unique(ctx, "quiet")
    {:ok, ws} = Workspaces.find_or_create(name)

    nodes =
      for t <- ~w(a b c) do
        {:ok, n} = Nodes.create_node(ws.id, %{node_type: "goal", title: t})
        n
      end

    [s1, s2, s3] = Enum.map(nodes, &seq_of/1)
    {frames, _} = replay(name, s2)
    assert Enum.map(frames, & &1["seq"]) == [s3]
    assert s1 < s2
  end

  # The socket's own overlap filter had the same mistake: the backlog ended
  # at 11, so a live 10 arriving afterwards was taken for a copy.
  test "a socket whose backlog ended at 11 still pushes a live 10", ctx do
    name = unique(ctx, "overlap")
    {:ok, ws} = Workspaces.find_or_create(name)

    a = open_writer(ws, "late")
    {:ok, b} = Nodes.create_node(ws.id, %{node_type: "goal", title: "early"})
    {[b_event], socket} = replay(name, 0)
    assert b_event["seq"] == seq_of(b)

    a_node = commit(a)

    [[a_event]] =
      Repo.query!("SELECT payload FROM graph_events WHERE seq = $1", [seq_of(a_node)]).rows

    assert {:push, {:text, text}, socket} =
             GraphSocket.handle_info({:graph_event, a_event}, socket)

    assert Jason.decode!(text)["seq"] == a_event["seq"]

    # The live copy of what the backlog sent is still dropped.
    assert {:ok, _} = GraphSocket.handle_info({:graph_event, b_event}, socket)
  end
end
