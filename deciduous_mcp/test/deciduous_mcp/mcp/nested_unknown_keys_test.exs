defmodule DeciduousMcp.MCP.NestedUnknownKeysTest do
  @moduledoc """
  A key a tool does not declare is refused at every depth, not only at the
  top.

  Verification of chapter 30 (T7/T11): the unknown-argument check read only
  the call's top-level keys, and everything inside an object argument went
  on vanishing. capture_conversation_turn with
  options_considered [{title: "a", choosen: true}] answered "captured
  successfully" and stored option a as *rejected*, with a decision -> a
  `rejected` edge; outcome {sucess: false} stored a completed outcome.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Test.McpHttp

  @ws "nested-keys"
  @scope %{"workspace" => @ws, "branch" => "b"}

  setup do
    sid = McpHttp.session()

    {:ok, %{"id" => goal}} =
      McpHttp.call(sid, "add_node", Map.merge(@scope, %{"node_type" => "goal", "title" => "g"}))

    %{sid: sid, goal: goal}
  end

  defp nodes do
    %{rows: [[n]]} =
      Repo.query!(
        "SELECT count(*) FROM decision_nodes n JOIN workspaces w ON w.id = n.workspace_id WHERE w.name = $1",
        [@ws]
      )

    n
  end

  defp refused(sid, tool, args) do
    before = nodes()
    assert {:tool_error, message} = McpHttp.call(sid, tool, Map.merge(@scope, args))
    assert message =~ "nothing was written"
    assert nodes() == before
    message
  end

  test "T7/T11 nested: a misspelt chosen inside options_considered is refused, not recorded as rejected",
       %{sid: sid} do
    message =
      refused(sid, "capture_conversation_turn", %{
        "summary" => "s",
        "decision" => %{"title" => "D"},
        "options_considered" => [%{"title" => "a", "choosen" => true}, %{"title" => "b"}]
      })

    assert message =~ ~s(options_considered[0] has no key "choosen")
    assert message =~ "did you mean chosen?"
  end

  test "T7/T11 nested: outcome.sucess, decision.reason and action.file are refused by name",
       %{sid: sid} do
    assert refused(sid, "capture_conversation_turn", %{
             "summary" => "s",
             "action" => %{"title" => "A"},
             "outcome" => %{"title" => "O", "sucess" => false}
           }) =~ ~s(outcome has no key "sucess" \(did you mean success?\))

    assert refused(sid, "capture_conversation_turn", %{
             "summary" => "s",
             "decision" => %{"title" => "D", "reason" => "why"}
           }) =~ ~s(decision has no key "reason" \(did you mean rationale?\))

    assert refused(sid, "capture_conversation_turn", %{
             "summary" => "s",
             "action" => %{"title" => "A", "file" => ["x.rs"], "commits" => "abc"}
           }) =~
             ~s(action has no keys "commits" \(did you mean commit?\); "file" \(did you mean files?\))

    assert refused(sid, "capture_conversation_turn", %{
             "summary" => "s",
             "goal" => %{"title" => "G", "parent_id" => "x"}
           }) =~ ~s(goal has no key "parent_id")
  end

  test "T7/T11 nested: log_decision says where rationale goes when it is sent inside chosen_option",
       %{sid: sid} do
    message =
      refused(sid, "log_decision", %{
        "title" => "t",
        "chosen_option" => %{"title" => "x", "rationale" => "the why", "descripton" => "typo"}
      })

    assert message =~ ~s("descripton" \(did you mean description?\))
    assert message =~ ~s("rationale" \(an argument of log_decision itself, not of chosen_option\))

    assert refused(sid, "log_decision", %{
             "title" => "t",
             "chosen_option" => "x",
             "rejected_options" => [%{"title" => "y", "why" => "lost reason"}]
           }) =~ ~s(rejected_options[0] has no key "why")
  end

  test "T7/T11 nested: close_thread next_steps[0].descripton is refused", %{sid: sid, goal: goal} do
    assert refused(sid, "close_thread", %{
             "title" => "done",
             "parent_node_id" => goal,
             "next_steps" => [%{"title" => "n", "descripton" => "lost"}]
           }) =~ ~s(next_steps[0] has no key "descripton" \(did you mean description?\))
  end

  test "T7/T11 nested: update_node metadata keeps keys of the caller's own, and refuses a near miss",
       %{sid: sid, goal: goal} do
    # Not a silent drop: metadata is a map of the caller's keys, merged in.
    assert {:ok, %{"message" => "Node updated"}} =
             McpHttp.call(sid, "update_node", %{"node_id" => goal, "metadata" => %{"foo" => 1}})

    %{rows: [[meta]]} =
      Repo.query!("SELECT metadata FROM decision_nodes WHERE id = $1", [Ecto.UUID.dump!(goal)])

    assert meta["foo"] == 1

    # One letter from a key the server reads is a typo, not a key of one's own.
    message =
      refused(sid, "update_node", %{"node_id" => goal, "metadata" => %{"confidance" => 5}})

    assert message =~ ~s(metadata has no key "confidance" \(did you mean confidence?\))
  end

  test "T7/T11 nested: the advertised schema closes every object it describes", %{sid: sid} do
    {200, _, body} =
      McpHttp.post(%{jsonrpc: "2.0", id: 9, method: "tools/list"}, [{"mcp-session-id", sid}])

    tools = McpHttp.decode(body)["result"]["tools"]
    turn = Enum.find(tools, &(&1["name"] == "capture_conversation_turn"))["inputSchema"]
    assert turn["additionalProperties"] == false
    assert turn["properties"]["outcome"]["additionalProperties"] == false
    assert turn["properties"]["options_considered"]["items"]["additionalProperties"] == false

    update = Enum.find(tools, &(&1["name"] == "update_node"))["inputSchema"]
    assert update["properties"]["metadata"]["additionalProperties"] == true
  end

  test "new (low): a tool with no arguments says so, not 'Its arguments are: ;'", %{sid: sid} do
    assert {:tool_error, message} = McpHttp.call(sid, "list_workspaces", %{"x" => 1})
    assert message =~ ~s(list_workspaces takes no arguments; it was sent "x")
    refute message =~ "are: ;"
  end

  test "new (low): a read tool's refusal does not say 'nothing was written'", %{
    sid: sid,
    goal: goal
  } do
    assert {:tool_error, message} =
             McpHttp.call(sid, "get_descendants", %{"node_id" => goal, "max_depth" => 0})

    assert message =~ "max_depth must be at least 1, got 0"
    refute message =~ "nothing was written"

    assert {:tool_error, message} =
             McpHttp.call(sid, "query_nodes", %{"workspace" => @ws, "q" => 1})

    refute message =~ "nothing was written"

    # A write still says it.
    assert {:tool_error, message} =
             McpHttp.call(
               sid,
               "add_node",
               Map.merge(@scope, %{"node_type" => "goal", "title" => ""})
             )

    assert message =~ "nothing was written"
  end
end
