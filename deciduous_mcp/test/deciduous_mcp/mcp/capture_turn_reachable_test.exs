defmodule DeciduousMcp.MCP.CaptureTurnReachableTest do
  @moduledoc """
  T6: capture_conversation_turn drew decision -> option (chosen/rejected)
  edges and, when there were options, nothing into the decision. So the
  decision was an orphan in find_orphans, and the turn's action and outcome,
  which hang under it, were unreachable from the goal the turn was captured
  under. An agent that then linked option -> decision to fix it made an
  option <-> decision 2-cycle, which add_edge allowed without a word.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Test.McpHttp

  @ws "t6-capture"

  setup do
    sid = McpHttp.session()

    {:ok, %{"id" => goal}} =
      McpHttp.call(sid, "add_node", %{"workspace" => @ws, "node_type" => "goal", "title" => "g"})

    %{sid: sid, goal: goal}
  end

  defp turn(sid, extra) do
    {:ok, result} =
      McpHttp.call(
        sid,
        "capture_conversation_turn",
        Map.merge(
          %{
            "workspace" => @ws,
            "summary" => "s",
            "options_considered" => [
              %{"title" => "use A", "chosen" => true},
              %{"title" => "use B", "chosen" => false}
            ],
            "decision" => %{"title" => "A over B", "rationale" => "faster"},
            "action" => "did A",
            "outcome" => "A works"
          },
          extra
        )
      )

    Map.new(result["nodes"], &{&1["type"], &1["id"]})
  end

  test "T6: a turn under a goal is reachable from it, decision included", %{sid: sid, goal: goal} do
    ids = turn(sid, %{"parent_node_id" => goal})

    {:ok, %{"nodes" => below}} = McpHttp.call(sid, "get_descendants", %{"node_id" => goal})
    below = MapSet.new(below, & &1["id"])

    for type <- ["decision", "action", "outcome"] do
      assert MapSet.member?(below, ids[type]), "#{type} is not reachable from the goal"
    end

    {:ok, orphans} = McpHttp.call(sid, "find_orphans", %{"workspace" => @ws})
    refute inspect(orphans) =~ ids["decision"]
  end

  test "T6: a turn that brings its own goal hangs the decision under it", %{sid: sid} do
    ids = turn(sid, %{"goal" => "own goal"})
    {:ok, %{"nodes" => below}} = McpHttp.call(sid, "get_descendants", %{"node_id" => ids["goal"]})
    assert Enum.any?(below, &(&1["id"] == ids["decision"]))
  end

  test "T6: add_edge refuses the reverse of an edge that exists, naming it", %{
    sid: sid,
    goal: goal
  } do
    ids = turn(sid, %{"parent_node_id" => goal})

    # The turn drew decision -> option (chosen). Linking option -> decision
    # is the 2-cycle the probe made.
    [chosen | _] =
      for {type, id} <- ids, type == "option", do: id

    assert {:tool_error, message} =
             McpHttp.call(sid, "add_edge", %{
               "workspace" => @ws,
               "from_node_id" => chosen,
               "to_node_id" => ids["decision"]
             })

    assert message =~ "already"
    assert message =~ ids["decision"]
    assert message =~ "nothing was written"
  end
end
