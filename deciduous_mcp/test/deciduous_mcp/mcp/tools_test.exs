defmodule DeciduousMcp.MCP.ToolsTest do
  @moduledoc """
  Tests for the Hermes-based MCP tool components.
  Tests tool definitions and direct component calls.
  """
  use DeciduousMcp.DataCase

  alias DeciduousMcp.MCP.Tools
  alias DeciduousMcp.Graph.Nodes

  setup do
    workspace = create_test_workspace()
    %{workspace_id: workspace.id}
  end

  describe "all_definitions/0" do
    test "returns all available tool definitions" do
      definitions = Tools.all_definitions()
      tool_names = Enum.map(definitions, & &1.name)

      assert "add_node" in tool_names
      assert "add_edge" in tool_names
      assert "get_graph" in tool_names
      assert "query_nodes" in tool_names
      assert "show_node" in tool_names
      assert "find_orphans" in tool_names
      assert "get_ancestors" in tool_names
      assert "get_descendants" in tool_names
    end

    test "each tool definition has required MCP fields" do
      for defn <- Tools.all_definitions() do
        assert Map.has_key?(defn, :name), "Tool missing name"
        assert Map.has_key?(defn, :description), "Tool #{defn.name} missing description"
        assert Map.has_key?(defn, :input_schema), "Tool #{defn.name} missing input_schema"
      end
    end
  end

  describe "AddNode component" do
    test "definition has correct name" do
      assert %{name: "add_node"} = DeciduousMcp.MCP.Tools.AddNode.definition()
    end
  end

  describe "Graph operations via context modules directly" do
    test "create and query nodes", %{workspace_id: wid} do
      {:ok, goal} =
        Nodes.create_node(wid, %{
          node_type: "goal",
          title: "Add rate limiting"
        })

      assert goal.node_type == "goal"
      assert goal.title == "Add rate limiting"
      assert goal.change_id != nil

      # Query it back
      nodes = Nodes.list_nodes(wid, type: "goal")
      assert length(nodes) == 1
      assert hd(nodes).title == "Add rate limiting"
    end

    test "create node with metadata", %{workspace_id: wid} do
      {:ok, action} =
        Nodes.create_node(wid, %{
          node_type: "action",
          title: "Implementing Redis rate limiter",
          metadata: %{"confidence" => 85, "branch" => "feature-rate-limit"}
        })

      assert action.metadata["confidence"] == 85
      assert action.metadata["branch"] == "feature-rate-limit"
    end
  end
end
