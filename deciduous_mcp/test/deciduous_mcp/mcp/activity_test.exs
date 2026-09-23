defmodule DeciduousMcp.MCP.ActivityTest do
  @moduledoc """
  What the branch "locks" are for, decided by the team probe.

  T2: a one-shot client (initialize, one call, exit) left a 10-second lease
  behind, and its own next process, a new session with the same client
  name, was refused: "locked by battery (0), session GNgCXo-N". The CLI
  took no lock at all and wrote 27 ms after an MCP lock was taken. So the
  lock stopped the one writer it should never stop and could not stop the
  other. Every write here is already atomic on its own (a node insert, a
  row-locked update, a capture_conversation_turn in one transaction), so
  there is nothing a lease protects; what agents used it for is seeing who
  else is writing where. It is now a record of that, and never a refusal.

  T10: check_activity reported 0 active sessions throughout a run of
  short-lived clients, since a lease 10 s long had always lapsed by the
  time anyone looked, and CLI writes never appeared; list_workspaces
  showed updated_at as the moment the workspace was created.
  """
  use DeciduousMcp.DataCase, async: false

  import Plug.Test
  import Plug.Conn

  alias DeciduousMcp.Test.McpHttp

  @ws "activity-probe"

  defp goal(title, branch \\ "main"),
    do: %{"workspace" => @ws, "branch" => branch, "node_type" => "goal", "title" => title}

  defp one_shot(fun) do
    sid = McpHttp.session()
    result = fun.(sid)
    McpHttp.request("DELETE", "/mcp", nil, [{"mcp-session-id", sid}])
    result
  end

  test "T2: a one-shot client's next call is not locked out by its previous session" do
    assert {:ok, %{"id" => _}} = one_shot(&McpHttp.call(&1, "add_node", goal("first")))
    assert {:ok, %{"id" => _}} = one_shot(&McpHttp.call(&1, "add_node", goal("second")))
  end

  test "T2: two sessions writing one branch at once both write" do
    a = McpHttp.session()
    b = McpHttp.session()
    assert {:ok, _} = McpHttp.call(a, "add_node", goal("a"))
    assert {:ok, _} = McpHttp.call(b, "add_node", goal("b"))
    assert {:ok, _} = McpHttp.call(a, "add_node", goal("a again"))
  end

  test "T10: check_activity shows recent writers, MCP and CLI, after their sessions ended" do
    one_shot(&McpHttp.call(&1, "add_node", goal("mcp write", "feat-x")))

    token = Application.fetch_env!(:deciduous_mcp, :api_token)

    conn =
      conn(
        :post,
        "/ops",
        Jason.encode!(%{
          workspace: @ws,
          ops: [
            %{
              op_id: Ecto.UUID.generate(),
              kind: "create_node",
              change_id: "act-cli",
              node_type: "goal",
              title: "cli write",
              metadata: %{"branch" => "feat-y"}
            }
          ]
        })
      )
      |> put_req_header("authorization", "Bearer " <> token)
      |> put_req_header("content-type", "application/json")
      |> DeciduousMcp.Web.Router.call(DeciduousMcp.Web.Router.init([]))

    assert conn.status == 200

    reader = McpHttp.session()
    assert {:ok, activity} = McpHttp.call(reader, "check_activity", %{"workspace" => @ws})
    assert activity["active_sessions"] == 2, inspect(activity)

    by_branch = Map.new(activity["sessions"], &{&1["branch"], &1})
    assert by_branch["feat-x"]["client"] == "mcp-http-test"
    assert by_branch["feat-y"]["client"] =~ "CLI"
  end

  test "T10: list_workspaces updated_at is the last write, not the creation" do
    sid = McpHttp.session()
    {:ok, %{"id" => id}} = McpHttp.call(sid, "add_node", goal("old"))

    Repo.query!(
      "UPDATE workspaces SET inserted_at = inserted_at - interval '1 day', updated_at = updated_at - interval '1 day' WHERE name = $1",
      [@ws]
    )

    {:ok, _} = McpHttp.call(sid, "update_node", %{"node_id" => id, "status" => "active"})

    {:ok, %{"workspaces" => list}} = McpHttp.call(sid, "list_workspaces", %{})
    %{"updated_at" => at} = Enum.find(list, &(&1["name"] == @ws))
    {:ok, at, _} = DateTime.from_iso8601(at)
    assert DateTime.diff(DateTime.utc_now(), at) < 60, "updated_at #{at}"
  end

  # Verification of T10: Scope.write_workspace_id recorded the write
  # before the tool ran, and nothing took the record back when the write
  # was refused. A session whose add_node (bad parent_id) and add_edge (a
  # self loop) both failed was listed writing branches "feat" and "feat2",
  # which held no nodes; /ops records only what it applied, so the two
  # surfaces disagreed.
  test "T10: a write that is refused is not recorded as activity" do
    writer = McpHttp.session()
    {:ok, %{"id" => id}} = McpHttp.call(writer, "add_node", goal("real", "main"))

    failing = McpHttp.session()

    assert {:tool_error, _} =
             McpHttp.call(
               failing,
               "add_node",
               Map.put(goal("x", "feat"), "parent_id", Ecto.UUID.generate())
             )

    assert {:tool_error, _} =
             McpHttp.call(failing, "add_edge", %{
               "workspace" => @ws,
               "branch" => "feat2",
               "from_node_id" => id,
               "to_node_id" => id
             })

    assert {:tool_error, _} =
             McpHttp.call(failing, "update_node", %{
               "node_id" => id,
               "branch" => "feat3",
               "status" => "bogus"
             })

    assert {:ok, activity} =
             McpHttp.call(McpHttp.session(), "check_activity", %{"workspace" => @ws})

    branches = activity["sessions"] |> Enum.map(& &1["branch"]) |> Enum.sort()
    assert branches == ["main"], inspect(activity["sessions"])
    assert activity["active_sessions"] == 1
  end

  test "new (low): a by-id write naming workspace \"*\" is refused, as every other write is" do
    sid = McpHttp.session()
    {:ok, %{"id" => id}} = McpHttp.call(sid, "add_node", goal("n"))

    for {tool, args} <- [
          {"update_node", %{"node_id" => id, "title" => "star"}},
          {"delete_node", %{"node_id" => id}}
        ] do
      assert {:tool_error, message} = McpHttp.call(sid, tool, Map.put(args, "workspace", "*"))
      assert message =~ ~s(workspace "*" is read-only), "#{tool}: #{message}"
    end

    assert %{title: "n", deleted_at: nil} = Repo.get!(DeciduousMcp.Schema.Node, id)

    # A read by id may name it: "*" is the view across every workspace.
    assert {:ok, _} = McpHttp.call(sid, "show_node", %{"node_id" => id, "workspace" => "*"})
  end
end
