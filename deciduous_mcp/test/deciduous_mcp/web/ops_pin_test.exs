defmodule DeciduousMcp.Web.OpsPinTest do
  @moduledoc """
  Stacking this chapter on chapter 21: /ops, /claim and /locate are new
  routes that name a workspace in the body, and none ran WorkspacePlug.
  Chapter 21 closed exactly that hole on /import (S1). A client pinned to A
  could retitle O's goal through /ops, adopt O through /claim, and list which
  workspaces hold a change_id through /locate. Over the router, because the
  pin is a header.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.{Nodes, Workspaces}
  alias DeciduousMcp.Test.McpClient

  setup do
    {:ok, _a} = Workspaces.find_or_create("op-a")
    {:ok, o} = Workspaces.find_or_create("op-o")
    {:ok, goal} = Nodes.create_node(o.id, %{node_type: "goal", title: "their goal"})
    %{goal: goal, pinned: McpClient.connect(pin: "op-a")}
  end

  defp retitle(goal),
    do: %{
      "op_id" => Ecto.UUID.generate(),
      "kind" => "update_node",
      "change_id" => goal.change_id,
      "set" => %{"title" => "HIJACKED"},
      "was" => %{"title" => "their goal"}
    }

  test "an op naming another workspace is refused", %{pinned: pinned, goal: goal} do
    {status, body} =
      McpClient.post_json(pinned, "/ops", %{"workspace" => "op-o", "ops" => [retitle(goal)]})

    assert status == 403, inspect(body)
    assert {:ok, %{title: "their goal"}} = Nodes.get_node(goal.id)
  end

  test "an op naming no workspace goes to the pinned one", %{pinned: pinned, goal: goal} do
    {status, body} = McpClient.post_json(pinned, "/ops", %{"ops" => [retitle(goal)]})

    assert status == 200, inspect(body)
    assert body["workspace"] == "op-a"
    assert {:ok, %{title: "their goal"}} = Nodes.get_node(goal.id)
  end

  test "a claim naming another workspace is refused", %{pinned: pinned} do
    roots = [String.duplicate("a", 40)]

    {status, body} =
      McpClient.post_json(pinned, "/claim", %{
        "workspace" => "op-o",
        "repo_roots" => roots,
        "adopt" => true
      })

    assert status == 403, inspect(body)
    {:ok, o} = Workspaces.get_by_name("op-o")
    assert (o.settings || %{})["repo_roots"] in [nil, []]
  end

  test "locate names no workspace but the pinned one", %{pinned: pinned, goal: goal} do
    {status, body} = McpClient.post_json(pinned, "/locate", %{"change_ids" => [goal.change_id]})

    assert status == 200, inspect(body)
    assert body["workspaces"] == []

    {200, unpinned} =
      McpClient.post_json(McpClient.connect(), "/locate", %{"change_ids" => [goal.change_id]})

    assert [%{"name" => "op-o"}] = unpinned["workspaces"]
  end
end
