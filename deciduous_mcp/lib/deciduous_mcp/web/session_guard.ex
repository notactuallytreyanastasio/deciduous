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

  ## The other five seconds: a probe Hermes cannot name

  Claude Code opens every connection with a version-negotiation probe, a
  JSON-RPC *request* (it has an id) whose method is `server/discover`:

      {"jsonrpc":"2.0","id":"server-discover-probe-1","method":"server/discover",
       "params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28",...}}}

  Hermes validates a request's method against the twelve it knows. This one
  fails validation, the decoder strips the method, and what is left has an id
  but no method, so the transport treats it as a notification and answers
  `202 {}`. The client is waiting for a reply carrying that id. It waits
  five seconds, gives up, and falls back to the legacy handshake, which then
  succeeds in under 100 ms. Every reconnect paid those five seconds; captured
  verbatim through a logging relay on 2026-09-22.

  JSON-RPC says what a server does with a method it does not implement:
  answer `-32601 Method not found` under the request's id. The guard does
  that for any request whose method Hermes does not know, and the client
  moves on at once. A notification with an unknown method is accepted with
  202 and dropped, as JSON-RPC requires; this comment used to say Hermes
  ignored them, and Hermes in fact answered 400 "Parse error".

  ## Everything else a POST can be

  The guard is the first thing to decode the body, so it answers the
  malformed shapes too (`classify/1`): an empty body, a batch, an id that is
  null or an object, a tools/call without usable params, and a request with
  no session header. Each gets the JSON-RPC error the protocol prescribes,
  under the request's id when it has a usable one.

  Reading the body here would normally starve Hermes, which reads it itself.
  `Hermes.Server.Transport.StreamableHTTP.Plug` accepts an already-fetched
  binary in `body_params`, so the raw body is stored there for it when the
  request goes on.
  """
  @behaviour Plug

  import Plug.Conn

  @session_header "mcp-session-id"
  @not_found_code -32001
  @method_not_found_code -32601

  # Hermes.MCP.Message's @request_methods, 0.14.1. A request whose method is
  # not here never reaches a handler: the decoder drops the method and the
  # transport answers 202 as if it were a notification.
  @known_request_methods ~w(initialize ping resources/list resources/read prompts/get
    prompts/list tools/call tools/list logging/setLevel completion/complete roots/list
    sampling/createMessage)

  @impl true
  def init(opts), do: Keyword.fetch!(opts, :server)

  # Hermes.MCP.Message's notification_schema, 0.14.1.
  @known_notifications ~w(notifications/initialized notifications/cancelled
    notifications/progress notifications/message notifications/roots/list_changed)

  @parse_error_code -32700
  @invalid_request_code -32600
  @invalid_params_code -32602

  @impl true
  def call(%Plug.Conn{method: "POST"} = conn, server) do
    case read_body(conn) do
      {:ok, body, conn} ->
        case classify(body) do
          {:pass, message} ->
            # Hermes' plug accepts already-fetched body_params and skips its
            # own read (maybe_read_request_body/2). The decoded map, not the
            # raw binary: given a binary, Hermes splits it on newlines and
            # parses each line, so a pretty-printed request was a -32700.
            conn = %{conn | body_params: message}
            route(conn, server, {:ok, message})

          {:ignore, method} ->
            ignore_notification(conn, method)

          {:refuse, status, code, message, id} ->
            jsonrpc_error(conn, status, code, message, id)
        end

      # Bandit hands back at most 8 MB per read; a body past that cannot be
      # a request this server would ever answer.
      {:more, _partial, conn} ->
        too_large(conn)

      {:error, _reason} ->
        conn
    end
  end

  def call(conn, _server), do: conn

  # What the protocol prescribes for each shape of message, before Hermes
  # sees it. Hermes answered every one of these that it did not crash on
  # with 400 "Parse error" under an id it made up: an empty body (a 500 with
  # no body: its `{:ok, [message]}` match fails on zero messages), a batch,
  # an id of null, a request without an id, an unknown notification. A
  # tools/call without params reached the handler and came back as
  # "request handler crashed" with the session's Frame inspected into `data`.
  defp classify(body) do
    if String.trim(body) == "" do
      {:refuse, 400, @parse_error_code, "Parse error: the request body is empty", nil}
    else
      case Jason.decode(body) do
        {:ok, %{} = message} -> classify_message(message)
        {:ok, list} when is_list(list) -> {:refuse, 400, @invalid_request_code, batch_text(), nil}
        {:ok, _} -> {:refuse, 400, @invalid_request_code, "Invalid Request: not an object", nil}
        {:error, _} -> {:refuse, 400, @parse_error_code, "Parse error: the body is not JSON", nil}
      end
    end
  end

  defp batch_text,
    do:
      "Invalid Request: JSON-RPC batches are not supported by this server; " <>
        "send one message per POST"

  defp classify_message(%{"jsonrpc" => version} = message) when version != "2.0" do
    {:refuse, 400, @invalid_request_code, ~s(Invalid Request: "jsonrpc" must be "2.0"),
     usable_id(message)}
  end

  defp classify_message(%{"method" => method} = message) when not is_binary(method) do
    {:refuse, 400, @invalid_request_code, "Invalid Request: method must be a string",
     usable_id(message)}
  end

  defp classify_message(%{"method" => method, "id" => id} = message) do
    cond do
      not (is_binary(id) or is_integer(id)) ->
        {:refuse, 400, @invalid_request_code,
         "Invalid Request: id must be a string or an integer, got #{describe(id)}", nil}

      method == "tools/call" ->
        case tool_call_params_problem(Map.get(message, "params")) do
          nil -> {:pass, message}
          problem -> {:refuse, 200, @invalid_params_code, "Invalid params: " <> problem, id}
        end

      true ->
        {:pass, message}
    end
  end

  # No id: a notification. JSON-RPC forbids answering one, including with an
  # error, and a request method sent without an id is a notification too.
  defp classify_message(%{"method" => method} = message) do
    if method in @known_notifications, do: {:pass, message}, else: {:ignore, method}
  end

  # A response to a request the server sent. This server sends none, but
  # Hermes owns that answer.
  defp classify_message(%{"id" => _} = message)
       when is_map_key(message, "result") or is_map_key(message, "error"),
       do: {:pass, message}

  defp classify_message(message) do
    {:refuse, 400, @invalid_request_code, "Invalid Request: no method", usable_id(message)}
  end

  defp tool_call_params_problem(%{"name" => name} = params) when is_binary(name) do
    case Map.get(params, "arguments") do
      nil -> nil
      %{} -> nil
      other -> "tools/call arguments must be an object, got #{describe(other)}"
    end
  end

  defp tool_call_params_problem(%{"name" => name}),
    do: "tools/call params.name must be a string, got #{describe(name)}"

  defp tool_call_params_problem(nil), do: "tools/call needs params with the tool's name"
  defp tool_call_params_problem(%{}), do: "tools/call params.name is required"

  defp tool_call_params_problem(other),
    do: "tools/call params must be an object, got #{describe(other)}"

  defp usable_id(%{"id" => id}) when is_binary(id) or is_integer(id), do: id
  defp usable_id(_), do: nil

  defp describe(nil), do: "null"
  defp describe(v) when is_list(v), do: "an array"
  defp describe(v) when is_map(v), do: "an object"
  defp describe(v), do: inspect(v, limit: 5, printable_limit: 40)

  defp route(conn, _server, {:ok, %{"method" => method, "id" => id}})
       when is_binary(method) and method not in @known_request_methods do
    method_not_found(conn, method, id)
  end

  defp route(conn, server, decoded) do
    case get_req_header(conn, @session_header) do
      [session_id | _] when session_id != "" ->
        case session_state(server, session_id) do
          :initialized -> conn
          :uninitialized -> refuse_unless_initializing(conn, server, session_id, decoded)
          :gone -> refuse_unless_initialize(conn, session_id, decoded)
        end

      _ ->
        refuse_sessionless_request(conn, decoded)
    end
  end

  # The spec: a server that requires sessions answers a request without one
  # (other than initialize) with 400. Hermes answered 200 "Server not
  # initialized" under an id it generated, which the client cannot match to
  # anything it sent.
  defp refuse_sessionless_request(conn, {:ok, %{"method" => method, "id" => id}})
       when method != "initialize" do
    jsonrpc_error(
      conn,
      400,
      @invalid_request_code,
      "Bad Request: no Mcp-Session-Id header. Send initialize first and use the " <>
        "session id it returns.",
      id
    )
  end

  defp refuse_sessionless_request(conn, _decoded), do: conn

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

  defp refuse_unless_initializing(conn, server, session_id, decoded) do
    case decoded do
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

  defp refuse_unless_initialize(conn, session_id, decoded) do
    case decoded do
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
  end

  defp ignore_notification(conn, method) do
    require Logger
    Logger.info("ignored notification #{inspect(method)}: not one this server handles")

    conn
    |> put_resp_content_type("application/json")
    |> send_resp(202, "")
    |> halt()
  end

  defp jsonrpc_error(conn, status, code, message, id) do
    body = %{jsonrpc: "2.0", id: id, error: %{code: code, message: message}}

    conn
    |> put_resp_content_type("application/json")
    |> send_resp(status, Jason.encode!(body))
    |> halt()
  end

  defp method_not_found(conn, method, id) do
    body = %{
      jsonrpc: "2.0",
      id: id,
      error: %{
        code: @method_not_found_code,
        message: "Method not found",
        data: %{method: method}
      }
    }

    conn
    |> put_resp_content_type("application/json")
    |> send_resp(200, Jason.encode!(body))
    |> halt()
  end

  defp too_large(conn) do
    body = %{
      jsonrpc: "2.0",
      id: nil,
      error: %{code: -32600, message: "Request body too large"}
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
