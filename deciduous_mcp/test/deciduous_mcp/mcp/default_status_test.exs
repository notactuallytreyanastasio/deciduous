defmodule DeciduousMcp.MCP.DefaultStatusTest do
  @moduledoc """
  An outcome or an observation records something that already happened, so
  it is born `completed`. Born `pending` or `active` it stays that way for
  good, and "what is open" (pending or active) lists every finding ever
  written down.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.{Nodes, Workspaces}
  alias DeciduousMcp.Test.McpClient

  @ws %{"workspace" => "ds-ws", "branch" => "ds"}

  setup do
    {:ok, ws} = Workspaces.find_or_create("ds-ws")
    {:ok, goal} = Nodes.create_node(ws.id, %{node_type: "goal", title: "G"})
    %{client: McpClient.connect(), goal: goal}
  end

  defp status_of(id) do
    {:ok, node} = Nodes.get_node(id)
    node.status
  end

  test "add_node: records are born completed, work is born pending", %{client: c, goal: goal} do
    for {type, expected} <- [
          {"outcome", "completed"},
          {"observation", "completed"},
          {"action", "pending"},
          {"option", "pending"},
          {"decision", "pending"},
          {"revisit", "pending"}
        ] do
      made =
        McpClient.call!(
          c,
          "add_node",
          Map.merge(@ws, %{"node_type" => type, "title" => "a #{type}", "parent_id" => goal.id})
        )

      assert status_of(made["id"]) == expected, "#{type} was born #{status_of(made["id"])}"
    end
  end

  test "add_node: a status the caller names wins", %{client: c, goal: goal} do
    made =
      McpClient.call!(
        c,
        "add_node",
        Map.merge(@ws, %{
          "node_type" => "outcome",
          "title" => "still running",
          "status" => "active",
          "parent_id" => goal.id
        })
      )

    assert status_of(made["id"]) == "active"
  end

  test "log_observation writes a completed observation", %{client: c, goal: goal} do
    made =
      McpClient.call!(
        c,
        "log_observation",
        Map.merge(@ws, %{"title" => "the cache is cold on boot", "related_to" => goal.id})
      )

    assert status_of(made["id"]) == "completed"
  end
end
