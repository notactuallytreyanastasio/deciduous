defmodule DeciduousMcp.MCP.TeamProbeRegressionsTest do
  @moduledoc """
  Two team-probe findings from the 1.0.7 server that this branch already
  answers, held here so they stay answered. Real transactions, real HTTP.

  T1: two concurrent update_node calls on one node, each with its own
  branch, left a row neither caller wrote (one's title, the other's
  status). 1.0.7 read the row without a lock and each changeset carried
  only the fields that differed from its stale copy.

  T13: get_descendants on an id that names no node answered count: 0, the
  same as a leaf.
  """
  use DeciduousMcp.RealDbCase, async: false

  alias DeciduousMcp.Test.McpHttp

  test "T1: concurrent update_node calls leave one caller's whole write, never a mix", ctx do
    ws = unique(ctx, "torn")
    a = McpHttp.session()
    b = McpHttp.session()

    for round <- 1..25 do
      {:ok, %{"id" => id}} =
        McpHttp.call(a, "add_node", %{
          "workspace" => ws,
          "branch" => "seed",
          "node_type" => "goal",
          "title" => "seed #{round}"
        })

      writes = [
        {a, %{"title" => "A#{round}", "status" => "completed", "description" => "by A"}, "ba"},
        {b, %{"title" => "B#{round}", "status" => "rejected", "description" => "by B"}, "bb"}
      ]

      results =
        writes
        |> Enum.map(fn {sid, fields, branch} ->
          Task.async(fn ->
            McpHttp.call(
              sid,
              "update_node",
              Map.merge(fields, %{"node_id" => id, "branch" => branch})
            )
          end)
        end)
        |> Enum.map(&Task.await(&1, 30_000))

      assert Enum.all?(results, &match?({:ok, _}, &1)), inspect(results)

      %{rows: [[title, status, description]]} =
        DeciduousMcp.Repo.query!(
          "SELECT title, status, description FROM decision_nodes WHERE id = $1",
          [Ecto.UUID.dump!(id)]
        )

      assert {title, status, description} in [
               {"A#{round}", "completed", "by A"},
               {"B#{round}", "rejected", "by B"}
             ],
             "round #{round}: torn row #{inspect({title, status, description})}"
    end
  end

  test "T13: get_descendants and get_ancestors on an id that names no node are errors" do
    sid = McpHttp.session()
    missing = Ecto.UUID.generate()

    for tool <- ["get_descendants", "get_ancestors"] do
      assert {:tool_error, message} = McpHttp.call(sid, tool, %{"node_id" => missing})
      assert message =~ missing
    end
  end
end
