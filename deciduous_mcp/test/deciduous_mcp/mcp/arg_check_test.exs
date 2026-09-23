defmodule DeciduousMcp.MCP.ArgCheckTest do
  @moduledoc """
  Every tool's advertised schema is enforced before the tool runs, and no
  string that Postgres cannot store, or should not, reaches it.

  Over real HTTP to the running listener: the point is what a client gets
  back, and that nothing was written.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Test.McpHttp

  @ws %{"workspace" => "arg-check", "branch" => "b"}

  setup do
    %{sid: McpHttp.session()}
  end

  defp refused(sid, tool, args) do
    result = McpHttp.call(sid, tool, args)

    assert {:tool_error, message} = result,
           "#{tool} #{inspect(Map.keys(args))}: #{inspect(result, limit: 20, printable_limit: 300)}"

    refute message =~ ~r/Postgrex|Ecto|Changeset|stacktrace/
    message
  end

  defp node_count do
    %{rows: [[n]]} =
      Repo.query!(
        "SELECT count(*) FROM decision_nodes n JOIN workspaces w ON w.id = n.workspace_id WHERE w.name = 'arg-check'"
      )

    n
  end

  test "enums, bounds and array item types are enforced", %{sid: sid} do
    goal = Map.merge(%{"node_type" => "goal", "title" => "t"}, @ws)

    assert refused(sid, "add_node", %{goal | "node_type" => "feedback"}) =~
             "node_type must be one of goal, decision"

    assert refused(sid, "add_node", Map.put(goal, "status", "done")) =~ "status must be one of"

    assert refused(sid, "add_node", Map.put(goal, "files", "notalist")) =~
             "files must be an array"

    assert refused(sid, "add_node", Map.put(goal, "files", ["a", 1])) =~
             "files[1] must be a string"

    assert refused(sid, "add_node", Map.put(goal, "confidence", 101)) =~
             "confidence must be at most 100"

    for {tool, args, says} <- [
          {"get_graph", %{"max_nodes" => 30_000}, "max_nodes must be at most 20000"},
          {"get_graph", %{"max_nodes" => 0}, "max_nodes must be at least 1"},
          {"get_graph", %{"max_nodes" => -1}, "max_nodes must be at least 1"},
          {"query_nodes", %{"limit" => 9_223_372_036_854_775_808}, "limit must be at most 10000"},
          {"get_descendants", %{"node_id" => Ecto.UUID.generate(), "max_depth" => 0},
           "max_depth must be at least 1"}
        ] do
      assert refused(sid, tool, Map.put(args, "workspace", "arg-check")) =~ says
    end

    assert node_count() == 0
  end

  test "a NUL in any string is refused by name, from every writing tool", %{sid: sid} do
    goal = Map.merge(%{"node_type" => "goal", "title" => "t"}, @ws)

    assert refused(sid, "add_node", %{goal | "title" => "x\u0000"}) =~ "title contains a NUL"

    assert refused(sid, "add_node", Map.put(goal, "description", "d\u0000")) =~
             "description contains a NUL"

    assert refused(sid, "add_node", Map.put(goal, "prompt", "p\u0000")) =~ "prompt contains a NUL"

    assert refused(
             sid,
             "log_decision",
             Map.merge(%{"title" => "t\u0000", "chosen_option" => "a"}, @ws)
           ) =~
             "title contains a NUL"

    assert refused(
             sid,
             "log_decision",
             Map.merge(%{"title" => "t", "chosen_option" => %{"title" => "a\u0000"}}, @ws)
           ) =~
             "chosen_option.title contains a NUL"

    assert refused(sid, "log_observation", Map.merge(%{"title" => "t\u0000"}, @ws)) =~
             "title contains a NUL"

    assert node_count() == 0
  end

  test "strings have a size limit, and the refusal does not echo the string", %{sid: sid} do
    goal = Map.merge(%{"node_type" => "goal"}, @ws)

    title = String.duplicate("A", 1_000_000)
    message = refused(sid, "add_node", Map.put(goal, "title", title))
    assert message =~ "title is 1000000 characters; the limit is 10000"
    assert byte_size(message) < 200

    message =
      refused(
        sid,
        "add_node",
        goal |> Map.put("title", "t") |> Map.put("description", String.duplicate("B", 300_000))
      )

    assert message =~ "description is 300000 characters; the limit is 262144"

    assert refused(sid, "log_observation", Map.merge(%{"title" => ""}, @ws)) =~
             "title must not be blank"

    assert node_count() == 0
  end

  test "the limits are advertised in tools/list, so a client can see them", %{sid: sid} do
    {200, _, body} =
      McpHttp.post(%{jsonrpc: "2.0", id: 5, method: "tools/list"}, [{"mcp-session-id", sid}])

    tools = McpHttp.decode(body)["result"]["tools"]
    add_node = Enum.find(tools, &(&1["name"] == "add_node"))
    assert add_node["inputSchema"]["properties"]["title"]["maxLength"] == 10_000
    assert add_node["inputSchema"]["properties"]["title"]["minLength"] == 1
    assert add_node["inputSchema"]["properties"]["description"]["maxLength"] == 262_144
  end

  test "valid calls are untouched, including log_decision's string options", %{sid: sid} do
    assert {:ok, %{"decision_id" => _}} =
             McpHttp.call(
               sid,
               "log_decision",
               Map.merge(
                 %{
                   "title" => "pick",
                   "chosen_option" => "a",
                   "rejected_options" => ["b", %{"title" => "c"}]
                 },
                 @ws
               )
             )

    assert {:ok, %{"id" => _}} =
             McpHttp.call(
               sid,
               "add_node",
               Map.merge(
                 %{
                   "node_type" => "goal",
                   "title" => "t",
                   "files" => ["a.rs"],
                   "confidence" => 90
                 },
                 @ws
               )
             )
  end
end
