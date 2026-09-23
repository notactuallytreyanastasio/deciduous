defmodule DeciduousMcp.Web.ChangeIdIdentityTest do
  @moduledoc """
  One change_id is one node, and every path says so when a write names a
  different one under it.

  Verification of chapter 30 (new, high): add_node takes a change_id the
  agent chooses, and POST /ops create_node never compared what it found
  under one. An agent's goal "agent title" under X, then the CLI's
  create_node X as an action "cli title": `exists`. The CLI took that as
  its action being on the server; the server kept the goal.

  T3: an add_node retry with a different description answered
  created: false and dropped the new body.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Test.McpHttp

  @ws "cid-identity"

  defp ops(ops) do
    {200, _, body} =
      McpHttp.request("POST", "/ops", Jason.encode!(%{workspace: @ws, ops: ops}), [
        {"content-type", "application/json"}
      ])

    Jason.decode!(body)["results"]
  end

  defp create_op(cid, type, title, extra \\ %{}) do
    Map.merge(
      %{
        op_id: Ecto.UUID.generate(),
        kind: "create_node",
        change_id: cid,
        node_type: type,
        title: title
      },
      extra
    )
  end

  defp nodes(cid) do
    %{rows: rows} =
      Repo.query!(
        """
        SELECT n.node_type, n.title, n.description FROM decision_nodes n
          JOIN workspaces w ON w.id = n.workspace_id
         WHERE w.name = $1 AND n.change_id = $2
        """,
        [@ws, cid]
      )

    Enum.map(rows, &List.to_tuple/1)
  end

  setup do
    %{sid: McpHttp.session()}
  end

  defp add(sid, args) do
    McpHttp.call(sid, "add_node", Map.merge(%{"workspace" => @ws}, args))
  end

  test "new (high): /ops create_node of another type under an agent's change_id is rejected, by both",
       %{sid: sid} do
    cid = Ecto.UUID.generate()

    {:ok, %{"created" => true}} =
      add(sid, %{"change_id" => cid, "node_type" => "goal", "title" => "agent title"})

    [result] = ops([create_op(cid, "action", "cli title", %{description: "cli body"})])

    assert result["result"] == "rejected"
    assert result["reason"] =~ ~s(change_id #{cid} is already a goal "agent title" on the server)
    assert result["reason"] =~ ~s(this op creates an action "cli title")
    assert nodes(cid) == [{"goal", "agent title", nil}]
  end

  test "new (high): the same type under another title is `exists`: a title changes, a type does not",
       %{sid: sid} do
    cid = Ecto.UUID.generate()

    {:ok, %{"id" => id}} =
      add(sid, %{"change_id" => cid, "node_type" => "goal", "title" => "first"})

    {:ok, _} =
      McpHttp.call(sid, "update_node", %{"node_id" => id, "title" => "renamed on the server"})

    # A CLI that pulled "first" and seeds it back, or replays its create
    # after a lost ack, sends the title it had. That is this node.
    assert [%{"result" => "exists"}] = ops([create_op(cid, "goal", "first")])
    assert nodes(cid) == [{"goal", "renamed on the server", nil}]
  end

  test "T3: an add_node retry with another description is refused, not dropped", %{sid: sid} do
    cid = Ecto.UUID.generate()
    args = %{"change_id" => cid, "node_type" => "goal", "title" => "t", "description" => "one"}
    {:ok, %{"created" => true}} = add(sid, args)
    assert {:ok, %{"created" => false}} = add(sid, args)
    assert {:ok, %{"created" => false}} = add(sid, Map.delete(args, "description"))

    assert {:tool_error, message} = add(sid, Map.put(args, "description", "two"))
    assert message =~ "change_id #{cid} is already node"
    assert message =~ "another description"
    assert message =~ "update_node"
    assert nodes(cid) == [{"goal", "t", "one"}]
  end

  test "T3: change_id is compared exactly, on every path alike", %{sid: sid} do
    {:ok, %{"created" => true}} =
      add(sid, %{"change_id" => "CID-N", "node_type" => "goal", "title" => "n"})

    {:ok, %{"created" => true}} =
      add(sid, %{"change_id" => "cid-N", "node_type" => "goal", "title" => "n"})

    # /ops, which every CLI write takes, and the CLI's own database key on
    # the same text: two change_ids that differ in case are two nodes there
    # too, so folding case here alone would make MCP disagree with them.
    assert [%{"result" => "exists"}] = ops([create_op("CID-N", "goal", "n")])
    assert [%{"result" => "applied"}] = ops([create_op("Cid-N", "goal", "n")])
    assert length(nodes("CID-N") ++ nodes("cid-N") ++ nodes("Cid-N")) == 3
  end
end
