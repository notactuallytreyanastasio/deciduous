defmodule DeciduousMcp.MCP.IdempotentAddNodeTest do
  @moduledoc """
  T3: the same write sent through two paths at once (the CLI's /ops and
  MCP add_node) made two nodes, and so does an agent retrying add_node
  after a timeout. A change_id is the node's identity on every surface
  (the CLI makes one for every node it writes), so add_node takes one:
  a node that already has it is the answer, not a second node.
  """
  use DeciduousMcp.RealDbCase, async: false

  alias DeciduousMcp.Test.McpHttp

  defp goal(ws, cid, title \\ "Ship it"),
    do: %{"workspace" => ws, "node_type" => "goal", "title" => title, "change_id" => cid}

  defp nodes(ws, cid) do
    %{rows: rows} =
      DeciduousMcp.Repo.query!(
        "SELECT n.id FROM decision_nodes n JOIN workspaces w ON w.id = n.workspace_id WHERE w.name = $1 AND n.change_id = $2",
        [ws, cid]
      )

    length(rows)
  end

  test "T3: add_node twice with one change_id is one node, and the second says so", ctx do
    ws = unique(ctx, "t3-retry")
    cid = Ecto.UUID.generate()
    sid = McpHttp.session()

    assert {:ok, %{"id" => id, "created" => true}} = McpHttp.call(sid, "add_node", goal(ws, cid))

    assert {:ok, %{"id" => ^id, "change_id" => ^cid, "created" => false, "message" => message}} =
             McpHttp.call(sid, "add_node", goal(ws, cid))

    assert message =~ "already"
    assert nodes(ws, cid) == 1
  end

  test "T3: the CLI's /ops create and MCP add_node racing on one change_id make one node", ctx do
    ws = unique(ctx, "t3-race")

    for round <- 1..10 do
      cid = "t3-#{round}-" <> Ecto.UUID.generate()
      sid = McpHttp.session()

      op = %{
        op_id: Ecto.UUID.generate(),
        kind: "create_node",
        change_id: cid,
        node_type: "goal",
        title: "Ship it"
      }

      tasks = [
        Task.async(fn ->
          McpHttp.request("POST", "/ops", Jason.encode!(%{workspace: ws, ops: [op]}), [
            {"content-type", "application/json"}
          ])
        end),
        Task.async(fn -> McpHttp.call(sid, "add_node", goal(ws, cid)) end)
      ]

      [{200, _, _}, mcp] = Enum.map(tasks, &Task.await(&1, 30_000))
      assert {:ok, %{"change_id" => ^cid}} = mcp
      assert nodes(ws, cid) == 1, "round #{round}"
    end
  end

  test "T3: a change_id already used for a different node is refused, not merged", ctx do
    ws = unique(ctx, "t3-clash")
    cid = Ecto.UUID.generate()
    sid = McpHttp.session()

    {:ok, %{"id" => id}} = McpHttp.call(sid, "add_node", goal(ws, cid, "one thing"))

    assert {:tool_error, message} = McpHttp.call(sid, "add_node", goal(ws, cid, "another"))
    assert message =~ cid and message =~ id and message =~ "one thing"
    assert message =~ "Nothing was written"
    assert nodes(ws, cid) == 1
  end
end
