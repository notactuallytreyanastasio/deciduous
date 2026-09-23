defmodule DeciduousMcp.Web.WorkspaceRaceTest do
  @moduledoc """
  The first writes to a new workspace arrive together: a swarm of agents
  starting in a fresh repo, or one client reconnecting several sessions at
  once. Each of them must get the workspace, not a unique-constraint error
  from losing the race to create it.

  Over real HTTP to the running listener, outside the sandbox, so the
  requests really are concurrent transactions on separate connections.
  """
  use DeciduousMcp.RealDbCase, async: false

  alias DeciduousMcp.Test.McpHttp

  test "120 parallel add_node calls naming a new workspace all succeed", ctx do
    workspace = unique(ctx, "swarm")
    sid = McpHttp.session()

    results =
      1..120
      |> Task.async_stream(
        fn i ->
          McpHttp.call(sid, "add_node", %{
            "node_type" => "goal",
            "title" => "race #{i}",
            "workspace" => workspace,
            "branch" => "b"
          })
        end,
        max_concurrency: 40,
        timeout: 60_000
      )
      |> Enum.map(fn {:ok, r} -> r end)

    failures = Enum.reject(results, &match?({:ok, _}, &1))

    assert failures == [],
           "#{length(failures)} of 120 failed, first: #{inspect(List.first(failures))}"

    %{rows: [[count]]} =
      DeciduousMcp.Repo.query!(
        "SELECT count(*) FROM decision_nodes n JOIN workspaces w ON w.id = n.workspace_id WHERE w.name = $1",
        [workspace]
      )

    assert count == 120
  end

  test "30 parallel initializes pinning a new workspace by header all succeed", ctx do
    workspace = unique(ctx, "pinned")

    statuses =
      1..30
      |> Task.async_stream(
        fn i ->
          {status, _, body} =
            McpHttp.post(McpHttp.initialize_body(i), [{"x-deciduous-workspace", workspace}])

          {status, body}
        end,
        max_concurrency: 30,
        timeout: 60_000
      )
      |> Enum.map(fn {:ok, r} -> r end)

    bad = Enum.reject(statuses, fn {status, _} -> status == 200 end)
    assert bad == [], "#{length(bad)} of 30 not 200, first: #{inspect(List.first(bad))}"
  end
end
