defmodule DeciduousMcp.MCP.Protocol do
  @moduledoc """
  Protocol constants for reference.

  The actual MCP protocol handling (JSON-RPC 2.0, capability negotiation,
  STDIO transport) is provided by Hermes MCP. This module just holds
  Deciduous-specific protocol metadata.
  """

  @mcp_version "2024-11-05"
  @server_name "deciduous-mcp"
  @server_version "0.1.0"

  def mcp_version, do: @mcp_version
  def server_name, do: @server_name
  def server_version, do: @server_version
end
