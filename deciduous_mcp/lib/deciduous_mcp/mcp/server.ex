defmodule DeciduousMcp.MCP.Server do
  @moduledoc """
  Deciduous MCP Server — powered by Hermes MCP.

  Exposes decision graph operations as MCP tools that Cowork (or any MCP client)
  can discover and invoke. Uses STDIO transport for local integration.

  ## Tools

  ### Graph CRUD
  add_node, update_node, delete_node, show_node, query_nodes

  ### Graph Structure
  add_edge, delete_edge, get_graph

  ### Graph Analysis
  find_orphans, get_ancestors, get_descendants

  ### High-Level Capture
  capture_conversation_turn, log_decision, log_observation, close_thread

  ### Natural Language Query
  ask_graph

  ## Prompts

  deciduous_always_on — injects always-on capture instructions into any client
  """
  use Hermes.Server,
    name: "deciduous-mcp",
    version: "0.1.0",
    capabilities: [:tools, :prompts]

  require Logger

  # --- Graph CRUD tools ---
  component DeciduousMcp.MCP.Tools.AddNode
  component DeciduousMcp.MCP.Tools.UpdateNode
  component DeciduousMcp.MCP.Tools.DeleteNode
  component DeciduousMcp.MCP.Tools.ShowNode
  component DeciduousMcp.MCP.Tools.QueryNodes

  # --- Graph structure tools ---
  component DeciduousMcp.MCP.Tools.AddEdge
  component DeciduousMcp.MCP.Tools.DeleteEdge

  # --- Graph export & analysis tools ---
  component DeciduousMcp.MCP.Tools.GetGraph
  component DeciduousMcp.MCP.Tools.FindOrphans
  component DeciduousMcp.MCP.Tools.GetAncestors
  component DeciduousMcp.MCP.Tools.GetDescendants

  # --- High-level capture tools ---
  component DeciduousMcp.MCP.Tools.CaptureConversationTurn
  component DeciduousMcp.MCP.Tools.LogDecision
  component DeciduousMcp.MCP.Tools.LogObservation
  component DeciduousMcp.MCP.Tools.CloseThread

  # --- Natural language query ---
  component DeciduousMcp.MCP.Tools.AskGraph

  # --- Prompts ---
  component DeciduousMcp.MCP.Prompts.AlwaysCapture

  @impl true
  def init(_client_info, frame) do
    Logger.info("Deciduous MCP Server initialized")

    # Resolve workspace on init — store in assigns for all tool calls
    workspace_name =
      Application.get_env(:deciduous_mcp, :default_workspace_name, "default")

    case DeciduousMcp.Graph.Workspaces.find_or_create(workspace_name) do
      {:ok, workspace} ->
        {:ok, assign(frame, workspace_id: workspace.id)}

      {:error, reason} ->
        Logger.error("Failed to resolve workspace: #{inspect(reason)}")
        {:ok, assign(frame, workspace_id: nil)}
    end
  end
end
