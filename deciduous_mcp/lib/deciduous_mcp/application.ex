defmodule DeciduousMcp.Application do
  @moduledoc """
  OTP Application for the Deciduous MCP Server.

  Supervision tree:
  - Repo (Ecto/Postgres)
  - PubSub (real-time collaboration)
  - Hermes Server Registry (MCP tool/resource management)
  - MCP Server with STDIO transport (Hermes)
  """
  use Application

  @impl true
  def start(_type, _args) do
    children = [
      # Database
      DeciduousMcp.Repo,

      # PubSub for broadcasting graph changes to connected clients
      {Phoenix.PubSub, name: DeciduousMcp.PubSub},

      # Hermes MCP Server Registry
      Hermes.Server.Registry,

      # Deciduous MCP Server with STDIO transport
      {DeciduousMcp.MCP.Server, transport: :stdio}
    ]

    opts = [strategy: :one_for_one, name: DeciduousMcp.Supervisor]
    Supervisor.start_link(children, opts)
  end
end
