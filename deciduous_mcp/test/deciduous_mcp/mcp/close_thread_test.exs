defmodule DeciduousMcp.MCP.CloseThreadTest do
  @moduledoc """
  close_thread over the wire: it writes an outcome, links it, completes a
  goal, and fans out lessons and follow-ups. Every id it is handed must be a
  live node in the workspace it writes to, and either all of it lands or
  none of it does.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.{Nodes, Workspaces}
  alias DeciduousMcp.Test.McpClient

  setup do
    {:ok, a} = Workspaces.find_or_create("ct-a")
    {:ok, other} = Workspaces.find_or_create("ct-other")

    {:ok, victim} =
      Nodes.create_node(other.id, %{node_type: "goal", title: "other's goal", status: "active"})

    {:ok, own_goal} =
      Nodes.create_node(a.id, %{node_type: "goal", title: "own goal", status: "active"})

    %{a: a, other: other, victim: victim, own_goal: own_goal}
  end

  defp outcomes(ws_id, title) do
    Nodes.list_nodes(ws_id, search: title) |> Enum.filter(&(&1.title == title))
  end

  describe "S1: goal_node_id is held to the workspace the call writes to" do
    test "a pinned client cannot complete another workspace's goal", %{a: a, victim: victim} do
      pinned = McpClient.connect(pin: "ct-a")

      assert {:error, message} =
               McpClient.call(pinned, "close_thread", %{
                 "title" => "ct pinned",
                 "goal_node_id" => victim.id,
                 "branch" => "b1"
               })

      assert message =~ "goal_node_id"
      assert message =~ victim.id
      assert {:ok, %{status: "active"}} = Nodes.get_node(victim.id)
      assert outcomes(a.id, "ct pinned") == []
    end

    test "an unpinned client naming workspace a cannot complete workspace other's goal", %{
      a: a,
      victim: victim
    } do
      client = McpClient.connect()

      assert {:error, _} =
               McpClient.call(client, "close_thread", %{
                 "title" => "ct unpinned",
                 "goal_node_id" => victim.id,
                 "workspace" => "ct-a",
                 "branch" => "b1"
               })

      assert {:ok, %{status: "active"}} = Nodes.get_node(victim.id)
      assert outcomes(a.id, "ct unpinned") == []
    end

    test "the workspace's own goal is still completed", %{own_goal: goal} do
      pinned = McpClient.connect(pin: "ct-a")

      assert %{"status" => "completed"} =
               McpClient.call!(pinned, "close_thread", %{
                 "title" => "ct own",
                 "goal_node_id" => goal.id,
                 "branch" => "b1"
               })

      assert {:ok, %{status: "completed"}} = Nodes.get_node(goal.id)
    end
  end
end
