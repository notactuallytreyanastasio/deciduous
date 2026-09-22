defmodule DeciduousMcp.Web.Router do
  @moduledoc """
  HTTP surface of the shared decision graph.

  Three routes, and deliberately no more:

    * `GET  /health` — unauthenticated liveness, so Caddy and compose can check
      the container without holding a token.
    * `ALL  /mcp`    — the MCP endpoint, forwarded to Hermes' Streamable HTTP
      plug.
    * `POST /import` — bulk ingest of one project's graph.

  `Plug.Parsers` is deliberately NOT in this pipeline. Hermes' plug reads the
  request body itself, but only when `body_params` is still unfetched
  (`maybe_read_request_body/2`); a parser upstream consumes the body first and
  the MCP endpoint then sees an empty message. `/import` reads its own body for
  the same reason, with its own size limit.
  """
  use Plug.Router

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

  defp read_whole_body(conn, acc \\ [], size \\ 0) do
    case Plug.Conn.read_body(conn, length: 1_000_000) do
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
