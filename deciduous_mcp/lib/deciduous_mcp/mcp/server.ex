defmodule DeciduousMcp.MCP.Server do
  @moduledoc """
  Deciduous MCP Server — powered by Hermes MCP.

  Exposes decision graph operations as MCP tools that any MCP client can
  discover and invoke, over Streamable HTTP.

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
    version: "1.0.3",
    capabilities: [:tools, :prompts]

  require Logger

  @doc "Sent as `instructions` on initialize (vendored Hermes patch 5)."
  def server_instructions, do: DeciduousMcp.MCP.Instructions.text()

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

  # --- Cross-project ---
  component DeciduousMcp.MCP.Tools.ListWorkspaces
  component DeciduousMcp.MCP.Tools.CheckActivity

  # --- Prompts ---
  component DeciduousMcp.MCP.Prompts.AlwaysCapture

  @impl true
  def init(client_info, frame) do
    Logger.info("Deciduous MCP session initialized: #{inspect(client_info)}")

    # No workspace is resolved here. One server backs every project, so the
    # workspace is decided per call by `DeciduousMcp.MCP.Scope` — from the
    # X-Deciduous-Workspace header if the client pinned one (the plug puts it
    # in assigns, which Frame inherits from Plug.Conn on HTTP transports), and
    # otherwise from the call's own `workspace` argument.
    #
    # The previous version resolved it once from an application env var and
    # pinned it for the whole connection. That made every tool call from every
    # project land in a single workspace named "default".
    {:ok, frame}
  end
end
