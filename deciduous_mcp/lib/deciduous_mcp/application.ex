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
      # Two budgets, both under every timeout between here and the model:
      # Cloudflare cuts a proxied origin at 100s (524), and Claude Code moves
      # an MCP call to the background at 120s and aborts it at 300s. Anything
      # this server lets run past those produces exactly the "120 second call"
      # symptom, with no sentence attached.
      #
      # `request_deadline` is enforced in Hermes.Server.Base, where the
      # handler task lives (vendor/hermes_mcp, DECIDUOUS-PATCHES.md): at 60s
      # the task is killed, its open transaction rolls back, and the request
      # is answered under its own id with "<tool> did not finish within 60s".
      # The heaviest call measured on production is 7.2s (eight concurrent
      # 12 MB get_graph on the 7,805-node workspace), so that is 8x headroom.
      #
      # `request_timeout` is how long the transport waits on Base. It only
      # stops waiting, it does not stop the handler, so it is a backstop set
      # above the deadline and below Cloudflare. It used to be 4 minutes,
      # which was above both cut-offs it exists to stay under.
      {DeciduousMcp.MCP.Server,
       transport: :streamable_http,
       session_idle_timeout: to_timeout(hour: 24),
       request_deadline:
         Application.get_env(:deciduous_mcp, :request_deadline, to_timeout(second: 60)),
       request_timeout: to_timeout(second: 90)},

      # Bridges Postgres NOTIFY to PubSub, for the WebSocket event stream
      DeciduousMcp.Events.Listener,

      # Public surface: /health, /mcp, /import
      {Bandit, plug: DeciduousMcp.Web.Router, scheme: :http, port: port}
    ]

    opts = [strategy: :one_for_one, name: DeciduousMcp.Supervisor]

    case Supervisor.start_link(children, opts) do
      {:ok, pid} ->
        Logger.info("Deciduous MCP listening on port #{port}")
        {:ok, pid}

      error ->
        error
    end
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
