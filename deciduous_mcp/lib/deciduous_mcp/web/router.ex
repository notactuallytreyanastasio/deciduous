defmodule DeciduousMcp.Web.Router do
  @moduledoc """
  HTTP surface of the shared decision graph.

  Three routes, and deliberately no more:

    * `GET  /health` — unauthenticated liveness, so Caddy and compose can check
      the container without holding a token.
    * `ALL  /mcp`    — the MCP endpoint, forwarded to Hermes' Streamable HTTP
      plug.
    * `POST /import` — bulk ingest of one project's graph.
    * `PUT  /blob/:hash` — raw document bytes, verified against the hash.
    * `GET  /documents/:id` — a document's bytes, by its id or content hash.

  `Plug.Parsers` is deliberately NOT in this pipeline. Hermes' plug reads the
  request body itself, but only when `body_params` is still unfetched
  (`maybe_read_request_body/2`); a parser upstream consumes the body first and
  the MCP endpoint then sees an empty message. `/import` reads its own body for
  the same reason, with its own size limit.
  """
  use Plug.Router

  alias DeciduousMcp.Graph.Documents
  alias DeciduousMcp.Storage
  alias DeciduousMcp.Sync.Import
  alias DeciduousMcp.Web.{Auth, WorkspacePlug}

  # 64MB: the largest graph on disk today is 24MB of SQLite, which is smaller
  # again as exported JSON. A project past this should be split, not streamed.
  @max_import_bytes 64 * 1024 * 1024

  plug :match
  plug :dispatch

  get "/health" do
    send_resp(conn, 200, "ok")
  end

  forward "/mcp",
    to: Hermes.Server.Transport.StreamableHTTP.Plug,
    init_opts: [server: DeciduousMcp.MCP.Server]

  post "/import" do
    conn = Auth.call(conn, [])

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
          {:ok, content, conn} -> store_blob(conn, hash, content)
          {:too_large, conn} -> json(conn, 413, %{error: "blob exceeds #{@max_import_bytes} bytes"})
          {:error, _} -> json(conn, 400, %{error: "could not read body"})
        end
    end
  end

  get "/documents/:id" do
    conn = Auth.call(conn, [])
    if conn.halted, do: conn, else: serve_document(conn, id)
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
        |> then(fn c -> if c.halted, do: c, else: super(c, opts) end)
      end
    else
      super(conn, opts)
    end
  end

  defoverridable call: 2

  defp mcp_path?(%Plug.Conn{path_info: ["mcp" | _]}), do: true
  defp mcp_path?(_), do: false

  # --- Import -----------------------------------------------------------------

  defp handle_import(conn, body) do
    with {:ok, payload} <- Jason.decode(body),
         {:ok, report} <- Import.run(payload) do
      json(conn, 200, report)
    else
      {:error, %Jason.DecodeError{} = err} ->
        json(conn, 400, %{error: "invalid json", detail: Exception.message(err)})

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
        json(conn, 422, %{error: "content does not match hash", expected: expected, actual: actual})
    end
  end

  defp serve_document(conn, id) do
    case Documents.fetch(id) do
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
