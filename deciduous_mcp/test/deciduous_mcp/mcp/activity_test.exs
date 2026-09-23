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
end
