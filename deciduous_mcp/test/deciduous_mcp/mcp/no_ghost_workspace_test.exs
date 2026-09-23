defmodule DeciduousMcp.MCP.NoGhostWorkspaceTest do
  @moduledoc """
  A write that fails, or writes nothing, does not leave a workspace behind.

  Scope created the workspace (find_or_create) before the tool validated
  anything else, so after five refused or no-op writes to five new names,
  list_workspaces showed all five, each with node_count 0: the permanent
  empty project the "reads no longer create workspaces" fix was meant to
  stop.

  Real transactions (RealDbCase), over real HTTP.
  """
  use DeciduousMcp.RealDbCase, async: false

  alias DeciduousMcp.Graph.Workspaces
  alias DeciduousMcp.Test.McpHttp

  setup ctx do
    %{sid: McpHttp.session(), ctx: ctx}
  end

  defp absent?(name), do: match?({:error, :not_found}, Workspaces.get_by_name(name))

  test "refused and no-op writes to a new workspace create nothing", %{sid: sid, ctx: ctx} do
    ghost = Ecto.UUID.generate()

    cases = [
      {"ghost-edge", "add_edge",
       %{"from_node_id" => ghost, "to_node_id" => Ecto.UUID.generate()}},
      {"ghost-parent", "add_node",
       %{"node_type" => "goal", "title" => "t", "parent_id" => ghost}},
      {"ghost-obs", "log_observation", %{"title" => "o", "related_to" => ghost}},
      {"ghost-dec", "log_decision",
       %{"title" => "d", "chosen_option" => %{"title" => "c"}, "parent_node_id" => ghost}},
      {"ghost-cap", "capture_conversation_turn", %{"summary" => "s", "parent_node_id" => ghost}}
    ]

    for {suffix, tool, args} <- cases do
      name = unique(ctx, suffix)
      answer = McpHttp.call(sid, tool, Map.merge(args, %{"workspace" => name, "branch" => "gb"}))
      assert absent?(name), "#{tool} answered #{inspect(answer)} and left workspace #{name}"
    end
  end

  test "a refused write under a header pin to a new name creates nothing", %{ctx: ctx} do
    name = unique(ctx, "ghost-pin")
    headers = [{"x-deciduous-workspace", name}]
    sid = McpHttp.session(headers)

    answer =
      McpHttp.call(
        sid,
        "add_node",
        %{"node_type" => "goal", "title" => "t", "parent_id" => Ecto.UUID.generate()},
        headers
      )

    assert {:tool_error, _} = answer
    assert absent?(name)
  end

  test "a write that succeeds still creates its workspace", %{sid: sid, ctx: ctx} do
    name = unique(ctx, "real")

    assert {:ok, _} =
             McpHttp.call(sid, "add_node", %{
               "node_type" => "goal",
               "title" => "first",
               "workspace" => name
             })

    assert {:ok, ws} = Workspaces.get_by_name(name)

    assert [%{node_count: 1}] =
             Workspaces.list_with_counts() |> Enum.filter(&(&1.id == ws.id))
  end
end
