defmodule DeciduousMcp.Web.SessionGuard do
  @moduledoc """
  Answers a request for a session this server no longer has with `404` and a
  JSON-RPC error that carries the request's own id, so the client fails fast
  and starts a new session instead of waiting on a reply it can never match.

  ## Why this exists

  Hermes expires a session after `session_idle_timeout` (30 minutes by
  default, see `DeciduousMcp.Application`), and a server restart drops every
  session at once. Claude Code keeps one HTTP session for as long as the
  process lives: hours, sometimes days. When it sends the next tool call with
  the old `mcp-session-id`, Hermes sees a session that is not initialized and
  replies `200` with

      {"jsonrpc":"2.0","id":"err_GNei6oQrILpGMINvCtk=","error":{"code":-32600,
       "message":"Invalid Request","data":{"message":"Server not initialized"}}}

  The `id` is freshly generated. It is not the id of the request, so the
  client cannot match the reply to anything it sent. Claude Code logs
  `Received a response for an unknown message ID`, keeps waiting, and gives
  up 300 seconds later with "sent no response or progress". Every call after
  that does the same. The server, meanwhile, answered in 70 milliseconds.

  The MCP spec says what to do instead: a server that has terminated a
  session responds `404 Not Found` to requests carrying that session id, and
  the client starts a new session with a fresh `initialize`. The reference
  TypeScript SDK pairs the 404 with JSON-RPC error code `-32001` and the
  message `Session not found`. That is the shape sent here.

  ## What it checks

  Only `POST` requests that carry a session header. If the header names a
  session with no live process in `Hermes.Server.Registry` and the request
  is not `initialize`, the request is refused. `initialize` is passed
  through with the stale header removed: a client re-initializing after a
  404 may still send it, and Hermes only reports the id of the session it
  creates when the request arrived without one.

  Reading the body here would normally starve Hermes, which reads it itself.
  `Hermes.Server.Transport.StreamableHTTP.Plug` accepts an already-fetched
  binary in `body_params`, so the raw body is stored there for it when the
  request goes on.
  """
  @behaviour Plug

  import Plug.Conn

  @session_header "mcp-session-id"
  @not_found_code -32001

  @impl true
  def init(opts), do: Keyword.fetch!(opts, :server)

  @impl true
  def call(%Plug.Conn{method: "POST"} = conn, server) do
    case get_req_header(conn, @session_header) do
      [session_id | _] when session_id != "" ->
        case session_state(server, session_id) do
          :initialized -> conn
          :uninitialized -> refuse_unless_initializing(conn, server, session_id)
          :gone -> refuse_unless_initialize(conn, session_id)
        end

      _ ->
        conn
    end
  end

  def call(conn, _server), do: conn

  # The registry drops a dead process's key when it gets the :DOWN message,
  # not at the instant the process dies, so a lookup can still return a pid
  # that is gone. Ask the pid too; the answer is the one Hermes would need.
  #
  # Alive is not enough either. If the idle timer fires between this check
  # and Hermes handling the request, Hermes recreates an *uninitialized*
  # session under the old id, answers the original wrong-id error, and that
  # phantom would then pass a liveness-only check on every later call. An
  # uninitialized session is one the client cannot use for a tool call, so
  # it is treated as its own state.
  defp session_state(server, session_id) do
    case Hermes.Server.Registry.whereis_server_session(server, session_id) do
      nil ->
        :gone

      pid ->
        cond do
          not Process.alive?(pid) -> :gone
          initialized?(server, session_id) -> :initialized
          true -> :uninitialized
        end
    end
  end

  defp initialized?(server, session_id) do
    name = Hermes.Server.Registry.server_session(server, session_id)

    try do
      match?(%{initialized: true}, Hermes.Server.Session.get(name))
    catch
      :exit, _ -> false
    end
  end

  # A session that exists but is not initialized is either mid-handshake
  # (the client's `notifications/initialized` is on its way, or was just
  # handled as a cast that has not landed yet) or the phantom described
  # above. The two requests that belong to a handshake pass through; any
  # other request gets a short grace period for the cast to land, then the
  # same 404 a dead session gets. A real session flips within milliseconds;
  # the phantom never does.
  @initialized_grace_tries 25
  @initialized_grace_ms 10

  defp refuse_unless_initializing(conn, server, session_id) do
    case read_body(conn) do
      {:ok, body, conn} ->
        conn = %{conn | body_params: body}

        case decode(body) do
          {:ok, %{"method" => "initialize"}} ->
            delete_req_header(conn, @session_header)

          {:ok, %{"method" => "notifications/initialized"}} ->
            conn

          {:ok, message} ->
            if wait_initialized(server, session_id, @initialized_grace_tries) do
              conn
            else
              not_found(conn, session_id, Map.get(message, "id"))
            end

          :error ->
            not_found(conn, session_id, nil)
        end

      {:more, _partial, conn} ->
        too_large(conn, session_id)

      {:error, _reason} ->
        not_found(conn, session_id, nil)
    end
  end

  defp wait_initialized(server, session_id, tries) do
    cond do
      initialized?(server, session_id) ->
        true

      tries == 0 ->
        false

      true ->
        Process.sleep(@initialized_grace_ms)
        wait_initialized(server, session_id, tries - 1)
    end
  end

  defp refuse_unless_initialize(conn, session_id) do
    case read_body(conn) do
      {:ok, body, conn} ->
        conn = %{conn | body_params: body}

        case decode(body) do
          {:ok, %{"method" => "initialize"}} ->
            # Hermes only echoes a session id when the request arrived
            # without one. Left in place, a stale header would make it mint
            # a session the client is never told about.
            delete_req_header(conn, @session_header)

          {:ok, %{"id" => id}} ->
            not_found(conn, session_id, id)

          _ ->
            not_found(conn, session_id, nil)
        end

      # Bandit hands back at most 8 MB per read; a body past that cannot be
      # a request this server would ever answer, and the session is dead
      # anyway.
      {:more, _partial, conn} ->
        too_large(conn, session_id)

      {:error, _reason} ->
        not_found(conn, session_id, nil)
    end
  end

  defp decode(body) do
    case Jason.decode(body) do
      {:ok, %{} = message} -> {:ok, message}
      {:ok, [%{} = message | _]} -> {:ok, message}
      _ -> :error
    end
  end

  defp too_large(conn, session_id) do
    body = %{
      jsonrpc: "2.0",
      id: nil,
      error: %{
        code: -32600,
        message: "Request body too large",
        data: %{session_id: session_id}
      }
    }

    conn
    |> put_resp_content_type("application/json")
    |> send_resp(413, Jason.encode!(body))
    |> halt()
  end

  defp not_found(conn, session_id, id) do
    body = %{
      jsonrpc: "2.0",
      id: id,
      error: %{
        code: @not_found_code,
        message: "Session not found",
        data: %{
          session_id: session_id,
          detail:
            "This session expired or the server restarted. Send a new initialize " <>
              "request without a session id to start another."
        }
      }
    }

    conn
    |> put_resp_content_type("application/json")
    |> send_resp(404, Jason.encode!(body))
    |> halt()
  end
end
