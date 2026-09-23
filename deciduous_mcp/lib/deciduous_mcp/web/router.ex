defmodule DeciduousMcp.Web.Router do
  @moduledoc """
  HTTP surface of the shared decision graph.

  Three routes, and deliberately no more:

    * `GET  /health` — unauthenticated liveness, so Caddy and compose can check
      the container without holding a token.
    * `GET  /ready` — database connectivity and required migrations; 503 until ready.
    * `ALL  /mcp`    — the MCP endpoint, forwarded to Hermes' Streamable HTTP
      plug.
    * `POST /import` — bulk ingest of one project's graph.
    * `PUT  /blob/:hash` — raw document bytes, verified against the hash.
    * `GET  /documents/:id` — a document's bytes, by its id or content hash.
    * `GET  /export` — one workspace's whole graph, for refreshing a local cache.
    * `GET  /events` — a WebSocket stream of writes as they happen, one frame per
      trigger firing (see `DeciduousMcp.Events.Listener` for the payload and
      `DeciduousMcp.Web.GraphSocket` for why the server pings).

  `Plug.Parsers` is deliberately NOT in this pipeline. Hermes' plug reads the
  request body itself, but only when `body_params` is still unfetched
  (`maybe_read_request_body/2`); a parser upstream consumes the body first and
  the MCP endpoint then sees an empty message. `/import` reads its own body for
  the same reason, with its own size limit.
  """
  use Plug.Router

  alias DeciduousMcp.Graph.{Documents, Query, Workspaces}
  alias DeciduousMcp.MCP.Scope
  alias DeciduousMcp.Web.SessionGuard

  @session_guard SessionGuard.init(server: DeciduousMcp.MCP.Server)
  alias DeciduousMcp.Storage
  alias DeciduousMcp.Sync.Import
  alias DeciduousMcp.Web.{Auth, GraphSocket, WorkspacePlug}

  # 64MB: the largest graph on disk today is 24MB of SQLite, which is smaller
  # again as exported JSON. A project past this should be split, not streamed.
  @max_import_bytes 64 * 1024 * 1024

  plug(:match)
  plug(:dispatch)

  get "/health" do
    send_resp(conn, 200, "ok")
  end

  get "/ready" do
    case DeciduousMcp.Readiness.check() do
      :ok -> send_resp(conn, 200, "ready")
      :unavailable -> send_resp(conn, 503, "not ready")
    end
  end

  forward("/mcp",
    to: Hermes.Server.Transport.StreamableHTTP.Plug,
    init_opts: [server: DeciduousMcp.MCP.Server]
  )

  post "/import" do
    conn = Auth.call(conn, [])

    # The pin applies here as it does to /export, /events and every MCP
    # tool. Without it a client pinned to A could rewrite any workspace's
    # nodes by naming it in the body: the pinned-write guard close_thread
    # and the id-taking tools enforce, bypassed by the CLI's push path.
    conn = if conn.halted, do: conn, else: WorkspacePlug.call(conn, [])

    if conn.halted do
      conn
    else
      case read_whole_body(conn) do
        {:ok, body, conn} ->
          handle_import(conn, body)

        {:too_large, conn} ->
          json(conn, 413, %{error: "import exceeds #{@max_import_bytes} bytes"})
      end
    end
  end

  # Bytes arrive here rather than inside the graph payload: the largest
  # document on disk is 16MB and the largest graph is already megabytes of
  # JSON, and base64 inside that would be a 22MB string inside a 50MB body.
  put "/blob/:hash" do
    conn = Auth.call(conn, [])

    cond do
      conn.halted ->
        conn

      not valid_hash?(hash) ->
        json(conn, 400, %{error: "hash must be 64 hex characters (sha256)"})

      true ->
        case read_whole_body(conn) do
          {:ok, content, conn} ->
            store_blob(conn, hash, content)

          {:too_large, conn} ->
            json(conn, 413, %{error: "blob exceeds #{@max_import_bytes} bytes"})

          {:error, _} ->
            json(conn, 400, %{error: "could not read body"})
        end
    end
  end

  # The pull half of the sync. `POST /import` has existed since the first
  # deploy; without a matching export the remote could only ever be a
  # write-only mirror, and a local database could never be refreshed from it.
  get "/export" do
    conn = Auth.call(conn, [])

    if conn.halted do
      conn
    else
      # Run the pin plug here too, so a repo that pinned itself by header gets
      # the same workspace on a pull as it does on every MCP call.
      conn = conn |> WorkspacePlug.call([]) |> fetch_query_params()

      case Scope.read_scope(conn_frame(conn), conn.query_params) do
        {:ok, scope} ->
          # Tombstones: without them a node deleted on the server never
          # left a pulled graph, and the next push re-sent it.
          json(conn, 200, Query.get_full_graph(scope, tombstones: true))

        {:error, message} ->
          json(conn, 422, %{error: message})
      end
    end
  end

  # Pinned like /export: without the plug a client pinned to A read O's
  # attachment bytes by id, or by the content hash of any file it could
  # name.
  get "/documents/:id" do
    conn = Auth.call(conn, [])
    conn = if conn.halted, do: conn, else: WorkspacePlug.call(conn, [])
    if conn.halted, do: conn, else: serve_document(conn, id)
  end

  # The WebSocket handshake itself cannot carry a custom Authorization header
  # in most clients that matter here — not a workaround for one client, a
  # limit of the browser `WebSocket` constructor and of every simple client
  # built against it, this session's own `Monitor` included. `?token=` is the
  # accepted pattern for that reason. It costs a real thing: a bearer token in
  # a URL can end up in a proxy's access log where a header would not, so the
  # header is still tried first and this is strictly a fallback for a
  # connection that arrives with none.
  get "/events" do
    conn = maybe_token_from_query(conn)
    conn = Auth.call(conn, [])

    if conn.halted do
      conn
    else
      conn = conn |> WorkspacePlug.call([]) |> fetch_query_params()

      case Scope.read_scope(conn_frame(conn), conn.query_params) do
        {:ok, scope} -> upgrade_to_event_stream(conn, scope)
        {:error, message} -> json(conn, 422, %{error: message})
      end
    end
  end

  match _ do
    send_resp(conn, 404, "not found")
  end

  # --- Plug pipeline for /mcp -------------------------------------------------
  #
  # `forward` does not run the parent router's later plugs, so auth and
  # workspace pinning are applied to the MCP endpoint by wrapping the forward
  # target rather than by adding plugs above. See `call/2`.

  def call(conn, opts) do
    if mcp_path?(conn) do
      conn = Auth.call(conn, [])

      if conn.halted do
        conn
      else
        conn
        |> WorkspacePlug.call([])
        |> refuse_sse_stream()
        |> then(fn c -> if c.halted, do: c, else: SessionGuard.call(c, @session_guard) end)
        |> then(fn c -> if c.halted, do: c, else: super(c, opts) end)
      end
    else
      super(conn, opts)
    end
  end

  # Refuse the server-to-client SSE stream, and only that.
  #
  # This deployment sits behind Cloudflare, which buffers a streaming response
  # until it completes. An SSE stream never completes, so its headers never
  # reach the client:
  #
  #     GET /mcp  (accept: text/event-stream)
  #       at the origin:      HTTP/2 200, content-type: text/event-stream
  #       through Cloudflare: nothing, ever
  #
  # Claude Code opens that stream after initializing and waits on it, so every
  # tool call hung until the client gave up at 300s — against a server that
  # answers the same POST in 88ms.
  #
  # POST is untouched: those responses already come back as JSON through
  # Cloudflare and are fast. The MCP spec permits refusing the GET stream with
  # 405, and this server never initiates messages, so it has nothing to stream.
  #
  # The `accept` header is deliberately NOT rewritten to steer Hermes away from
  # SSE. `validate_accept_header/1` and `wants_sse?/1` both read
  # `get_req_header("accept") |> List.first("")`, and validation *requires*
  # text/event-stream to be present — so any header that passes validation also
  # selects SSE. Stripping it produced `Not Acceptable: Client must accept
  # both`.
  defp refuse_sse_stream(%Plug.Conn{method: "GET"} = conn) do
    conn
    |> put_resp_content_type("application/json")
    |> send_resp(
      405,
      Jason.encode!(%{
        error: "this server does not offer a server-to-client stream",
        detail: "POST responses are returned directly; see MCP Streamable HTTP"
      })
    )
    |> halt()
  end

  defp refuse_sse_stream(conn), do: conn

  defp mcp_path?(%Plug.Conn{path_info: ["mcp" | _]}), do: true
  defp mcp_path?(_), do: false

  # --- Import -----------------------------------------------------------------

  defp handle_import(conn, body) do
    with {:ok, payload} <- Jason.decode(body),
         {:ok, report} <-
           Import.run(payload, pinned_workspace_id: conn.assigns[:pinned_workspace_id]) do
      json(conn, 200, report)
    else
      {:error, %Jason.DecodeError{} = err} ->
        json(conn, 400, %{error: "invalid json", detail: Exception.message(err)})

      {:error, {:pinned, message}} ->
        json(conn, 403, %{error: message})

      {:error, reason} ->
        json(conn, 422, %{error: to_string_reason(reason)})
    end
  end

  # --- Documents --------------------------------------------------------------

  defp store_blob(conn, hash, content) do
    # The hash is the primary key and the cross-project dedup key, so it is
    # verified rather than trusted: a client sending the wrong one would
    # shadow another document's bytes for every project referencing it.
    case Storage.verify(hash, content) do
      :ok ->
        :ok = Storage.put(hash, content, mime_type: content_type(conn))
        Import.mark_content_found(hash)
        json(conn, 200, %{stored: hash, bytes: byte_size(content)})

      {:error, {:hash_mismatch, expected: expected, actual: actual}} ->
        json(conn, 422, %{
          error: "content does not match hash",
          expected: expected,
          actual: actual
        })
    end
  end

  defp serve_document(conn, id) do
    case Documents.fetch(id, workspace_id: conn.assigns[:pinned_workspace_id]) do
      {:ok, doc, content} ->
        conn
        # Set directly rather than via put_resp_content_type/2, which appends
        # "; charset=utf-8" to every type — including application/pdf, where it
        # is meaningless and some viewers treat it as a reason to mistrust the
        # body.
        |> put_resp_header("content-type", doc.mime_type)
        # These are private plans and PDFs reachable from the public internet,
        # behind a bearer token and a CDN. Cloudflare reports DYNAMIC for this
        # route today, but that is a default that a later page rule could
        # change; no-store says it explicitly and also keeps the bytes out of
        # the requesting browser's disk cache.
        |> put_resp_header("cache-control", "no-store, private, max-age=0")
        |> put_resp_header(
          "content-disposition",
          ~s(inline; filename="#{doc.original_filename}")
        )
        |> send_resp(200, content)

      {:error, :content_missing} ->
        # 410, not 404. The attachment is real and its metadata imported; the
        # bytes were already gone before this server ever saw them, and that is
        # a different thing from a bad id.
        json(conn, 410, %{
          error: "document content is gone",
          detail: "the row imported but no bytes were ever found for its hash"
        })

      {:error, :not_found} ->
        json(conn, 404, %{error: "no such document"})
    end
  end

  # Scope resolution reads `frame.assigns`, which on the MCP path Hermes
  # inherits from Plug.Conn. This route talks to it directly, so it presents
  # the same shape rather than duplicating the precedence rules.
  defp conn_frame(conn), do: %{assigns: conn.assigns}

  # --- Events -------------------------------------------------------------

  defp maybe_token_from_query(conn) do
    case get_req_header(conn, "authorization") do
      [] ->
        conn = fetch_query_params(conn)

        case conn.query_params["token"] do
          token when is_binary(token) and token != "" ->
            put_req_header(conn, "authorization", "Bearer " <> token)

          _ ->
            conn
        end

      _ ->
        conn
    end
  end

  # `Scope.read_scope/2` returns a workspace id (or :global) so every other
  # read tool can query decision_nodes by an indexed FK. The PubSub topics
  # this stream runs on are keyed by name instead, because that is what the
  # trigger's NOTIFY payload already carries — resolving the id back to a
  # name here, rather than teaching the trigger or the topic scheme to key on
  # ids, keeps exactly one place that knows the mapping.
  defp upgrade_to_event_stream(conn, :global) do
    conn |> Plug.Conn.upgrade_adapter(:websocket, {GraphSocket, %{topic: "graph:*"}, []})
  end

  defp upgrade_to_event_stream(conn, workspace_id) do
    case Workspaces.get_workspace(workspace_id) do
      {:ok, workspace} ->
        conn
        |> Plug.Conn.upgrade_adapter(
          :websocket,
          {GraphSocket, %{topic: "graph:" <> workspace.name}, []}
        )

      {:error, :not_found} ->
        json(conn, 404, %{error: "no such workspace"})
    end
  end

  defp content_type(conn) do
    case get_req_header(conn, "content-type") do
      [t | _] -> t |> String.split(";") |> hd() |> String.trim()
      [] -> nil
    end
  end

  defp valid_hash?(hash), do: is_binary(hash) and String.match?(hash, ~r/\A[0-9a-fA-F]{64}\z/)

  # Bandit's default body read timeout is 15s per read, which is fine for an
  # MCP call and far too short for an import. The largest graph here is a 23MB
  # payload pushed from a laptop over a home uplink; every one of the 11
  # biggest projects failed with `Bandit.HTTPError: Body read timeout`,
  # surfacing to the client as a bare 408 with no body to explain it.
  @body_read_timeout to_timeout(minute: 5)
  @body_read_chunk 8 * 1024 * 1024

  defp read_whole_body(conn, acc \\ [], size \\ 0) do
    case Plug.Conn.read_body(conn,
           length: @body_read_chunk,
           read_length: @body_read_chunk,
           read_timeout: @body_read_timeout
         ) do
      {:ok, chunk, conn} ->
        total = size + byte_size(chunk)

        if total > @max_import_bytes,
          do: {:too_large, conn},
          else: {:ok, IO.iodata_to_binary([acc, chunk]), conn}

      {:more, chunk, conn} ->
        total = size + byte_size(chunk)

        if total > @max_import_bytes,
          do: {:too_large, conn},
          else: read_whole_body(conn, [acc, chunk], total)

      {:error, _} = err ->
        err
    end
  end

  defp json(conn, status, payload) do
    conn
    |> put_resp_content_type("application/json")
    |> send_resp(status, Jason.encode!(payload))
  end

  defp to_string_reason(reason) when is_binary(reason), do: reason
  defp to_string_reason(reason), do: inspect(reason)
end
