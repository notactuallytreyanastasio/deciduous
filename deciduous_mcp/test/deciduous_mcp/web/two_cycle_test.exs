defmodule DeciduousMcp.Web.TwoCycleTest do
  @moduledoc """
  No path writes a 2-cycle, and no path answers a duplicate edge with a
  wrong field.

  Verification of chapter 30 (T6, T3): add_edge refused an edge whose
  reverse existed, but the check lived in that tool only, as a read before
  an insert. So:

    * add_edge A -> B and B -> A from two sessions at once: 15/15 rounds
      wrote both;
    * MCP add_edge A -> B, then /ops create_edge B -> A: applied;
    * add_node with a change_id and the parent_id of the node's own child
      linked the existing node under it: "(linked under parent_id)",
      leaving N -> C and C -> N; a retry naming another parent quietly
      gave the node a second one.

  And an MCP add_edge that lost a race to an /ops create of the same edge
  answered "Failed to create edge: from_node_id: has already been taken".

  Real transactions and parallel connections, as on a server.
  """
  use DeciduousMcp.RealDbCase, async: false

  alias DeciduousMcp.Test.McpHttp

  defp node(sid, ws, title, extra \\ %{}) do
    {:ok, %{"id" => id} = reply} =
      McpHttp.call(
        sid,
        "add_node",
        Map.merge(%{"workspace" => ws, "node_type" => "goal", "title" => title}, extra)
      )

    {id, reply}
  end

  defp edges(ws) do
    %{rows: rows} =
      DeciduousMcp.Repo.query!(
        """
        SELECT f.title, t.title, e.edge_type FROM decision_edges e
          JOIN decision_nodes f ON f.id = e.from_node_id
          JOIN decision_nodes t ON t.id = e.to_node_id
          JOIN workspaces w ON w.id = e.workspace_id
         WHERE w.name = $1
        """,
        [ws]
      )

    Enum.map(rows, &List.to_tuple/1)
  end

  defp two_cycles(ws) do
    set = MapSet.new(edges(ws), fn {f, t, _} -> {f, t} end)
    Enum.filter(set, fn {f, t} -> MapSet.member?(set, {t, f}) end)
  end

  defp ops(ws, ops) do
    {200, _, body} =
      McpHttp.request("POST", "/ops", Jason.encode!(%{workspace: ws, ops: ops}), [
        {"content-type", "application/json"}
      ])

    Jason.decode!(body)["results"]
  end

  test "T6: add_edge A -> B and B -> A at once from two sessions writes one of them", ctx do
    ws = unique(ctx, "t6-race")
    a_sid = McpHttp.session()
    b_sid = McpHttp.session()

    for round <- 1..15 do
      {a, _} = node(a_sid, ws, "a#{round}")
      {b, _} = node(a_sid, ws, "b#{round}")

      [ra, rb] =
        [{a_sid, a, b}, {b_sid, b, a}]
        |> Enum.map(fn {sid, from, to} ->
          Task.async(fn ->
            McpHttp.call(sid, "add_edge", %{
              "workspace" => ws,
              "from_node_id" => from,
              "to_node_id" => to
            })
          end)
        end)
        |> Enum.map(&Task.await(&1, 30_000))

      oks = Enum.count([ra, rb], &match?({:ok, _}, &1))
      assert oks == 1, "round #{round}: #{inspect([ra, rb])}"

      {:tool_error, message} = Enum.find([ra, rb], &match?({:tool_error, _}, &1))
      assert message =~ "each other's parent"
    end

    assert two_cycles(ws) == []
  end

  test "T6: /ops create_edge B -> A after add_edge A -> B is rejected, by change_id", ctx do
    ws = unique(ctx, "t6-ops")
    sid = McpHttp.session()
    {a, %{"change_id" => a_cid}} = node(sid, ws, "a")
    {b, %{"change_id" => b_cid}} = node(sid, ws, "b")

    assert {:ok, _} =
             McpHttp.call(sid, "add_edge", %{
               "workspace" => ws,
               "from_node_id" => a,
               "to_node_id" => b
             })

    [result] =
      ops(ws, [
        %{
          op_id: Ecto.UUID.generate(),
          kind: "create_edge",
          from_change_id: b_cid,
          to_change_id: a_cid,
          edge_type: "leads_to"
        }
      ])

    assert result["result"] == "rejected"
    assert result["reason"] =~ "create_edge #{b_cid} -> #{a_cid}"
    assert result["reason"] =~ "#{a_cid} -> #{b_cid} (leads_to) already exists"
    assert two_cycles(ws) == []
  end

  test "T3/T6: an add_node retry naming its own child as parent_id is refused", ctx do
    ws = unique(ctx, "t3-cycle")
    sid = McpHttp.session()
    cid = Ecto.UUID.generate()
    {n, _} = node(sid, ws, "N", %{"change_id" => cid})
    {c, _} = node(sid, ws, "C", %{"node_type" => "action", "parent_id" => n})

    assert {:tool_error, message} =
             McpHttp.call(sid, "add_node", %{
               "workspace" => ws,
               "node_type" => "goal",
               "title" => "N",
               "change_id" => cid,
               "parent_id" => c
             })

    assert message =~ "change_id #{cid} is already node #{n}"
    assert message =~ "each other's parent"
    assert message =~ ~r/nothing was written/i
    assert two_cycles(ws) == []
    assert edges(ws) == [{"N", "C", "leads_to"}]
  end

  test "T3: an add_node retry naming another parent is refused, not given a second one", ctx do
    ws = unique(ctx, "t3-parent")
    sid = McpHttp.session()
    {p, _} = node(sid, ws, "P")
    {q, _} = node(sid, ws, "Q")
    cid = Ecto.UUID.generate()
    args = %{"node_type" => "action", "change_id" => cid, "parent_id" => p}
    {n, %{"created" => true}} = node(sid, ws, "N", args)

    # The same call again: the same node, the same edge.
    assert {n, %{"created" => false}} ==
             node(sid, ws, "N", args) |> then(&{elem(&1, 0), Map.take(elem(&1, 1), ["created"])})

    assert {:tool_error, message} =
             McpHttp.call(
               sid,
               "add_node",
               Map.merge(args, %{"workspace" => ws, "title" => "N", "parent_id" => q})
             )

    assert message =~ "under #{p}"
    assert message =~ "this call names parent_id #{q}"
    assert message =~ "add_edge"
    assert Enum.sort(edges(ws)) == [{"P", "N", "leads_to"}]
  end

  test "T3: a node the CLI made, with no parent yet, is linked by the add_node that names it",
       ctx do
    ws = unique(ctx, "t3-cli")
    sid = McpHttp.session()
    {p, _} = node(sid, ws, "P")

    [%{"result" => "applied"}] =
      ops(ws, [
        %{
          op_id: Ecto.UUID.generate(),
          kind: "create_node",
          change_id: "cli-made",
          node_type: "action",
          title: "N"
        }
      ])

    assert {_, %{"created" => false, "parent_id" => ^p}} =
             node(sid, ws, "N", %{
               "node_type" => "action",
               "change_id" => "cli-made",
               "parent_id" => p
             })

    assert edges(ws) == [{"P", "N", "leads_to"}]
  end

  test "new (low): add_edge of an edge that exists says so, also when it loses a race to /ops",
       ctx do
    ws = unique(ctx, "dup-edge")
    sid = McpHttp.session()

    for round <- 1..10 do
      {a, %{"change_id" => a_cid}} = node(sid, ws, "a#{round}")
      {b, %{"change_id" => b_cid}} = node(sid, ws, "b#{round}")

      mcp = fn ->
        McpHttp.call(sid, "add_edge", %{"workspace" => ws, "from_node_id" => a, "to_node_id" => b})
      end

      op = fn ->
        ops(ws, [
          %{
            op_id: Ecto.UUID.generate(),
            kind: "create_edge",
            from_change_id: a_cid,
            to_change_id: b_cid,
            edge_type: "leads_to"
          }
        ])
      end

      results =
        [mcp, op, mcp, op, mcp, op]
        |> Enum.map(&Task.async/1)
        |> Enum.map(&Task.await(&1, 30_000))

      for r <- results do
        case r do
          {:ok, %{"created" => created}} when is_boolean(created) -> :ok
          [%{"result" => res}] when res in ["applied", "exists"] -> :ok
          other -> flunk("round #{round}: #{inspect(other)}")
        end
      end

      created =
        Enum.count(results, &match?({:ok, %{"created" => true}}, &1)) +
          Enum.count(results, &match?([%{"result" => "applied"}], &1))

      assert created == 1, "round #{round}: #{inspect(results)}"
    end

    # And without a race: the second add_edge is answered, not failed.
    {a, _} = node(sid, ws, "x")
    {b, _} = node(sid, ws, "y")
    args = %{"workspace" => ws, "from_node_id" => a, "to_node_id" => b}
    assert {:ok, %{"created" => true}} = McpHttp.call(sid, "add_edge", args)

    assert {:ok, %{"created" => false, "message" => message}} =
             McpHttp.call(sid, "add_edge", args)

    assert message =~ "already exists"
  end
end
