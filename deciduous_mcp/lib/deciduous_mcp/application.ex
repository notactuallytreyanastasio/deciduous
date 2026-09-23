defmodule DeciduousMcp.Application do
  @moduledoc """
  OTP Application for the Deciduous MCP Server.

  Supervision tree:
  - Repo (Ecto/Postgres)
  - PubSub (real-time collaboration)
  - Hermes Server Registry (MCP tool/resource management)
  - MCP Server over Streamable HTTP (Hermes)
  - Bandit serving `DeciduousMcp.Web.Router`

  The transport used to be `:stdio`, one server process per project, with the
  workspace fixed at boot from an environment variable. This server instead
  backs every project on the machine at once, so it runs as a long-lived HTTP
  service and resolves the workspace per request.
  """
  use Application

  require Logger

  @impl true
  def start(_type, _args) do
    verify_token!()
    port = port()

    children = [
      # Database
      DeciduousMcp.Repo,

      # PubSub for broadcasting graph changes to connected clients
      {Phoenix.PubSub, name: DeciduousMcp.PubSub},

      # Hermes MCP Server Registry
      Hermes.Server.Registry,

      # Deciduous MCP Server, reachable over HTTP.
      #
      # Hermes expires a session after 30 idle minutes by default. Claude Code
      # keeps one session for the life of the process and does not notice the
      # expiry, so a session that was merely quiet for an afternoon came back
      # to find every call refused (see DeciduousMcp.Web.SessionGuard for what
      # that refusal has to look like). A day covers a working session; a
      # restart still drops everything, and the guard handles that case.
      #
      # `request_timeout` is how long the transport's per-request task waits on
      # `GenServer.call(Base, ...)`. Hermes' default is 30s. With handlers run
      # in tasks (vendor/hermes_mcp, DECIDUOUS-PATCHES.md) that is the budget
      # for one call's own work; before, it was queue time plus work, and a
      # caller queued behind a slow `get_graph` got `:server_unavailable` at
      # 30s while the server then ran its request anyway for nobody. Claude
      # Code itself gives up at 300s, so four minutes keeps the server's budget
      # under the client's.
      {DeciduousMcp.MCP.Server,
       transport: :streamable_http,
       session_idle_timeout: to_timeout(hour: 24),
       request_timeout: to_timeout(minute: 4)},

      # Bridges Postgres NOTIFY to PubSub, for the WebSocket event stream
      DeciduousMcp.Events.Listener,

      # Public surface: /health, /mcp, /import
      {Bandit, plug: DeciduousMcp.Web.Router, scheme: :http, port: port}
    ]

    Logger.info("Deciduous MCP listening on port #{port}")

    opts = [strategy: :one_for_one, name: DeciduousMcp.Supervisor]
    Supervisor.start_link(children, opts)
  end

  # Refusing to boot is the point. This process is reachable from the public
  # internet and holds every decision graph on the machine; starting it with
  # authentication disabled because a variable was missing is the one failure
  # mode worth making impossible.
  defp verify_token! do
    case Application.get_env(:deciduous_mcp, :api_token) do
      token when is_binary(token) and byte_size(token) >= 32 ->
        :ok

      token when is_binary(token) ->
        raise """
        DECIDUOUS_MCP_TOKEN is #{byte_size(token)} bytes. Use at least 32.
        Generate one with: openssl rand -hex 32
        """

      _ ->
        raise """
        DECIDUOUS_MCP_TOKEN is not set.

        This server exposes every decision graph over HTTP and has no other
        authentication. Generate a token with:

            openssl rand -hex 32
        """
    end
  end

  defp port do
    case Integer.parse(System.get_env("PORT") || "4000") do
      {p, ""} -> p
      _ -> raise "PORT must be an integer, got: #{inspect(System.get_env("PORT"))}"
    end
  end
end
