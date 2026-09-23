defmodule DeciduousMcp.Web.PinnedReadsTest do
  @moduledoc """
  S2: a client pinned by `X-Deciduous-Workspace` reads only its own
  workspace, including through the tools that take a node id rather than a
  workspace (show_node, get_descendants, get_ancestors) and through
  list_workspaces. Over the router, because the pin is a header.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.{Edges, Nodes, Workspaces}
  alias DeciduousMcp.Test.McpClient

  setup do
    {:ok, mine} = Workspaces.find_or_create("pr-mine")
    {:ok, theirs} = Workspaces.find_or_create("pr-theirs")

    {:ok, secret} =
      Nodes.create_node(theirs.id, %{
        node_type: "goal",
        title: "their secret goal",
        description: "private description",
        metadata: %{"prompt" => "private prompt", "branch" => "t"}
      })

    {:ok, child} = Nodes.create_node(theirs.id, %{node_type: "action", title: "their action"})
    {:ok, _} = Edges.create_edge(theirs.id, %{from_node_id: secret.id, to_node_id: child.id})

    {:ok, own} = Nodes.create_node(mine.id, %{node_type: "goal", title: "my goal"})
    {:ok, own_child} = Nodes.create_node(mine.id, %{node_type: "action", title: "my action"})
    {:ok, _} = Edges.create_edge(mine.id, %{from_node_id: own.id, to_node_id: own_child.id})

    %{
      pinned: McpClient.connect(pin: "pr-mine"),
      secret: secret,
      child: child,
      own: own,
      own_child: own_child
    }
  end

  test "show_node refuses another workspace's node and leaks none of it", %{
    pinned: pinned,
    secret: secret
  } do
    assert {:error, message} = McpClient.call(pinned, "show_node", %{"node_id" => secret.id})
    assert message =~ "another workspace"
    refute message =~ "private"
  end

  test "get_descendants and get_ancestors refuse to start in another workspace", %{
    pinned: pinned,
    secret: secret,
    child: child
  } do
    assert {:error, m1} = McpClient.call(pinned, "get_descendants", %{"node_id" => secret.id})
    assert m1 =~ "another workspace"
    assert {:error, m2} = McpClient.call(pinned, "get_ancestors", %{"node_id" => child.id})
    assert m2 =~ "another workspace"
  end

  test "list_workspaces shows a pinned client only its own workspace", %{pinned: pinned} do
    assert %{"count" => 1, "workspaces" => [%{"name" => "pr-mine"}]} =
             McpClient.call!(pinned, "list_workspaces", %{})
  end

  test "the pinned workspace itself is still readable", %{
    pinned: pinned,
    own: own,
    own_child: own_child
  } do
    assert %{"title" => "my goal"} = McpClient.call!(pinned, "show_node", %{"node_id" => own.id})

    assert %{"nodes" => nodes} =
             McpClient.call!(pinned, "get_descendants", %{"node_id" => own.id})

    assert Enum.map(nodes, & &1["id"]) == [own.id, own_child.id]

    assert %{"nodes" => up} =
             McpClient.call!(pinned, "get_ancestors", %{"node_id" => own_child.id})

    assert Enum.sort(Enum.map(up, & &1["id"])) == Enum.sort([own.id, own_child.id])
  end

  test "an unpinned client still reads any node by id and lists every workspace", %{
    secret: secret
  } do
    client = McpClient.connect()

    assert %{"title" => "their secret goal"} =
             McpClient.call!(client, "show_node", %{"node_id" => secret.id})

    names =
      McpClient.call!(client, "list_workspaces", %{})["workspaces"] |> Enum.map(& &1["name"])

    assert "pr-mine" in names and "pr-theirs" in names
  end
end
