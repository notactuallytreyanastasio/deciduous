defmodule DeciduousMcp.MCP.Tools do
  @moduledoc """
  Convenience module listing all Deciduous MCP tool components.

  Each tool is implemented as a `Hermes.Server.Component` in its own module
  under `DeciduousMcp.MCP.Tools.*` and registered in `DeciduousMcp.MCP.Server`.

  ## Available Tools

  ### Graph CRUD
  - `AddNode` — Create a decision graph node
  - `UpdateNode` — Modify a node's title, description, or status
  - `DeleteNode` — Soft-delete a node
  - `ShowNode` — Get full node details with connections
  - `QueryNodes` — Search/filter nodes

  ### Graph Structure
  - `AddEdge` — Connect two nodes with a typed relationship
  - `DeleteEdge` — Remove a connection

  ### Graph Export & Analysis
  - `GetGraph` — Export the full graph
  - `FindOrphans` — Find disconnected non-goal nodes
  - `GetAncestors` — Walk backward from a node
  - `GetDescendants` — Walk forward from a node

  ### High-Level Capture
  - `CaptureConversationTurn` — Atomically capture a full conversation exchange
  - `LogDecision` — Record a decision with chosen/rejected options
  - `LogObservation` — Quick insight capture
  - `CloseThread` — Close out a reasoning thread with outcome and lessons

  ### Natural Language Query
  - `AskGraph` — Ask questions about the decision graph in plain English
  """

  @crud_tools [
    DeciduousMcp.MCP.Tools.AddNode,
    DeciduousMcp.MCP.Tools.UpdateNode,
    DeciduousMcp.MCP.Tools.DeleteNode,
    DeciduousMcp.MCP.Tools.ShowNode,
    DeciduousMcp.MCP.Tools.QueryNodes
  ]

  @structure_tools [
    DeciduousMcp.MCP.Tools.AddEdge,
    DeciduousMcp.MCP.Tools.DeleteEdge
  ]

  @analysis_tools [
    DeciduousMcp.MCP.Tools.GetGraph,
    DeciduousMcp.MCP.Tools.FindOrphans,
    DeciduousMcp.MCP.Tools.GetAncestors,
    DeciduousMcp.MCP.Tools.GetDescendants
  ]

  @capture_tools [
    DeciduousMcp.MCP.Tools.CaptureConversationTurn,
    DeciduousMcp.MCP.Tools.LogDecision,
    DeciduousMcp.MCP.Tools.LogObservation,
    DeciduousMcp.MCP.Tools.CloseThread
  ]

  @query_tools [
    DeciduousMcp.MCP.Tools.AskGraph
  ]

  @all_tools @crud_tools ++ @structure_tools ++ @analysis_tools ++ @capture_tools ++ @query_tools

  @doc "Returns the list of all tool component modules."
  def all_modules, do: @all_tools

  @doc "Returns all tool definitions (for testing/inspection)."
  def all_definitions do
    Enum.map(@all_tools, & &1.definition())
  end

  @doc "Returns tool modules grouped by category."
  def by_category do
    %{
      crud: @crud_tools,
      structure: @structure_tools,
      analysis: @analysis_tools,
      capture: @capture_tools,
      query: @query_tools
    }
  end
end
