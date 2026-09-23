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

  describe "S3: all of close_thread lands, or none of it" do
    setup do
      %{client: McpClient.connect()}
    end

    defp count_all(ws_id), do: length(Nodes.list_nodes(ws_id, limit: 1000))

    test "a parent_node_id that names no node is refused and nothing is written", %{
      client: client,
      a: a
    } do
      before = count_all(a.id)
      missing = Ecto.UUID.generate()

      assert {:error, message} =
               McpClient.call(client, "close_thread", %{
                 "title" => "ct bogus parent",
                 "parent_node_id" => missing,
                 "workspace" => "ct-a",
                 "branch" => "b3"
               })

      assert message =~ missing
      assert count_all(a.id) == before
    end

    test "a parent_node_id in another workspace is refused and nothing is written", %{
      client: client,
      a: a,
      victim: victim
    } do
      before = count_all(a.id)

      assert {:error, _} =
               McpClient.call(client, "close_thread", %{
                 "title" => "ct foreign parent",
                 "parent_node_id" => victim.id,
                 "workspace" => "ct-a",
                 "branch" => "b3"
               })

      assert count_all(a.id) == before
    end

    for bad <- ["PLACEHOLDER_SKIP", "abc", ""] do
      test "goal_node_id #{inspect(bad)} is a clean refusal, not a crash after the outcome", %{
        client: client,
        a: a
      } do
        before = count_all(a.id)

        assert {:error, message} =
                 McpClient.call(client, "close_thread", %{
                   "title" => "ct bad goal",
                   "goal_node_id" => unquote(bad),
                   "workspace" => "ct-a",
                   "branch" => "b3"
                 })

        assert message =~ "goal_node_id"
        assert count_all(a.id) == before
      end
    end

    test "a lesson that is not a string is refused before anything is written", %{
      client: client,
      a: a,
      own_goal: goal
    } do
      before = count_all(a.id)

      assert {:error, message} =
               McpClient.call(client, "close_thread", %{
                 "title" => "ct bad lesson",
                 "parent_node_id" => goal.id,
                 "goal_node_id" => goal.id,
                 "lessons_learned" => ["fine", %{"title" => "an object"}],
                 "workspace" => "ct-a",
                 "branch" => "b3"
               })

      assert message =~ "lessons_learned"
      assert count_all(a.id) == before
      assert {:ok, %{status: "active"}} = Nodes.get_node(goal.id)
    end

    test "a next step that is not an object with a title is refused before anything is written",
         %{client: client, a: a, own_goal: goal} do
      before = count_all(a.id)

      for bad <- ["a bare string", %{"description" => "no title"}, %{"title" => 7}] do
        assert {:error, message} =
                 McpClient.call(client, "close_thread", %{
                   "title" => "ct bad step",
                   "parent_node_id" => goal.id,
                   "next_steps" => [%{"title" => "ok step"}, bad],
                   "workspace" => "ct-a",
                   "branch" => "b3"
                 })

        assert message =~ "next_steps"
      end

      assert count_all(a.id) == before
    end

    test "a good call writes outcome, edge, lessons and follow-ups together", %{
      client: client,
      a: a,
      own_goal: goal
    } do
      before = count_all(a.id)

      assert %{"outcome_id" => outcome_id, "lessons_logged" => 2, "next_goals_created" => 1} =
               McpClient.call!(client, "close_thread", %{
                 "title" => "ct good",
                 "parent_node_id" => goal.id,
                 "goal_node_id" => goal.id,
                 "lessons_learned" => ["one", "two"],
                 "next_steps" => [%{"title" => "follow"}],
                 "workspace" => "ct-a",
                 "branch" => "b3"
               })

      assert count_all(a.id) == before + 4
      assert [%{from_node_id: from}] = DeciduousMcp.Graph.Edges.edges_to(outcome_id)
      assert from == goal.id
    end
  end
end
