defmodule DeciduousMcp.Web.ImportPinTest do
  @moduledoc """
  S1/S2 bypass: POST /import took its workspace from the body and never ran
  WorkspacePlug, so a client pinned to A could rewrite O's goal by naming O
  in the payload, while /export and /events on the same router honoured the
  pin. Over the router, because the pin is a header.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.{Nodes, Workspaces}
  alias DeciduousMcp.Test.McpClient

  setup do
    {:ok, a} = Workspaces.find_or_create("ip-a")
    {:ok, o} = Workspaces.find_or_create("ip-o")
    {:ok, goal} = Nodes.create_node(o.id, %{node_type: "goal", title: "their goal"})
    %{a: a, o: o, goal: goal, pinned: McpClient.connect(pin: "ip-a")}
  end

  defp hijack(goal),
    do: %{
      "nodes" => [
        %{
          "change_id" => goal.change_id,
          "node_type" => "goal",
          "title" => "HIJACKED",
          "status" => "completed"
        }
      ]
    }

  test "a pinned client cannot import into another workspace", %{pinned: pinned, goal: goal} do
    {status, body} =
      McpClient.post_json(pinned, "/import", %{"workspace" => "ip-o", "graph" => hijack(goal)})

    assert status == 403, inspect(body)
    assert body["error"] =~ "pinned"
    assert {:ok, %{title: "their goal", status: "pending"}} = Nodes.get_node(goal.id)
  end

  test "a pinned client's import without a workspace lands in the pinned one", %{
    pinned: pinned,
    a: a
  } do
    graph = %{"nodes" => [%{"change_id" => "ip-new", "node_type" => "goal", "title" => "mine"}]}
    {status, body} = McpClient.post_json(pinned, "/import", %{"graph" => graph})

    assert status == 200, inspect(body)
    assert body["workspace"] == "ip-a"
    assert {:ok, %{title: "mine"}} = Nodes.get_node_by_change_id(a.id, "ip-new")
  end

  test "a pinned client naming its own workspace, in any case, imports", %{pinned: pinned} do
    graph = %{"nodes" => [%{"change_id" => "ip-2", "node_type" => "goal", "title" => "t"}]}

    {status, body} =
      McpClient.post_json(pinned, "/import", %{"workspace" => "IP-A", "graph" => graph})

    assert status == 200, inspect(body)
  end

  test "an unpinned client still names the workspace in the body", %{goal: goal} do
    {status, _} =
      McpClient.post_json(McpClient.connect(), "/import", %{
        "workspace" => "ip-o",
        "graph" => hijack(goal)
      })

    assert status == 200
    assert {:ok, %{title: "HIJACKED"}} = Nodes.get_node(goal.id)
  end
end
