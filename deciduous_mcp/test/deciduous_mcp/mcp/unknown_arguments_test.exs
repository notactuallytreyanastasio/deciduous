defmodule DeciduousMcp.MCP.UnknownArgumentsTest do
  @moduledoc """
  An argument a tool does not declare is refused, naming the one it meant.

  Team probe T7 and T11: Peri hands a tool only the keys its schema
  declares, so a misspelt argument vanished before the tool saw it and the
  call succeeded doing something else. log_observation with `parent_id`
  (the argument is `related_to`) wrote an orphan; update_node with `commit`
  answered "Node updated" and changed nothing; close_thread with `node_id`
  wrote an unlinked outcome; query_nodes with `node_type` (it is `type`)
  returned every type.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Test.McpHttp

  @ws %{"workspace" => "unknown-args", "branch" => "b"}

  setup do
    sid = McpHttp.session()

    {:ok, %{"id" => goal}} =
      McpHttp.call(sid, "add_node", Map.merge(@ws, %{"node_type" => "goal", "title" => "g"}))

    %{sid: sid, goal: goal}
  end

  defp nodes do
    %{rows: [[n]]} =
      Repo.query!(
        "SELECT count(*) FROM decision_nodes n JOIN workspaces w ON w.id = n.workspace_id WHERE w.name = 'unknown-args'"
      )

    n
  end

  test "T7: log_observation with parent_id is refused and points at related_to", %{
    sid: sid,
    goal: goal
  } do
    before = nodes()

    assert {:tool_error, message} =
             McpHttp.call(
               sid,
               "log_observation",
               Map.merge(@ws, %{"title" => "o", "parent_id" => goal})
             )

    assert message =~ "parent_id"
    assert message =~ "related_to"
    assert message =~ "nothing was written"
    assert nodes() == before
  end

  test "T7: update_node with commit is refused and points at metadata.commit", %{
    sid: sid,
    goal: goal
  } do
    assert {:tool_error, message} =
             McpHttp.call(sid, "update_node", %{"node_id" => goal, "commit" => "abc123"})

    assert message =~ "commit"
    assert message =~ "metadata.commit"

    %{rows: [[meta]]} =
      Repo.query!("SELECT metadata FROM decision_nodes WHERE id = $1", [Ecto.UUID.dump!(goal)])

    refute Map.has_key?(meta, "commit")
  end

  test "T7: close_thread with node_id is refused and names parent_node_id", %{
    sid: sid,
    goal: goal
  } do
    before = nodes()

    assert {:tool_error, message} =
             McpHttp.call(
               sid,
               "close_thread",
               Map.merge(@ws, %{"title" => "done", "node_id" => goal})
             )

    assert message =~ "node_id"
    assert message =~ "parent_node_id"
    assert nodes() == before
  end

  test "T11: query_nodes with node_type is refused and names type", %{sid: sid} do
    assert {:tool_error, message} =
             McpHttp.call(sid, "query_nodes", Map.merge(@ws, %{"node_type" => "goal"}))

    assert message =~ "node_type"
    assert message =~ "did you mean type?"
  end

  test "T7: several unknown arguments are all named, and a name with no near match lists the real ones",
       %{sid: sid} do
    assert {:tool_error, message} =
             McpHttp.call(
               sid,
               "add_node",
               Map.merge(@ws, %{"node_type" => "goal", "title" => "t", "zzz" => 1, "qqq" => 2})
             )

    assert message =~ "qqq"
    assert message =~ "zzz"
    assert message =~ "title"
  end

  test "T7: workspace and branch on a tool that takes a node id are accepted when they agree",
       %{sid: sid, goal: goal} do
    # The instructions say to pass workspace and branch on every write; a
    # by-id tool must not refuse the caller for doing what it was told.
    assert {:ok, _} =
             McpHttp.call(
               sid,
               "update_node",
               Map.merge(@ws, %{"node_id" => goal, "status" => "active"})
             )

    assert {:ok, _} =
             McpHttp.call(sid, "show_node", %{"node_id" => goal, "workspace" => "unknown-args"})
  end

  test "T7: a workspace that disagrees with the node's own is refused, not ignored",
       %{sid: sid, goal: goal} do
    assert {:tool_error, message} =
             McpHttp.call(sid, "update_node", %{
               "node_id" => goal,
               "workspace" => "some-other-project",
               "title" => "x"
             })

    assert message =~ "unknown-args"
    assert message =~ "some-other-project"

    %{rows: [[title]]} =
      Repo.query!("SELECT title FROM decision_nodes WHERE id = $1", [Ecto.UUID.dump!(goal)])

    assert title == "g"
  end
end
