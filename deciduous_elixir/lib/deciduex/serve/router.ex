defmodule Deciduex.Serve.Router do
  @moduledoc """
  Plug router for the decision graph viewer API.

  API Endpoints:
    GET /api/graph      Full graph data (nodes, edges, documents)
    GET /api/commands   Recent command log
    GET /api/documents  List documents (optional ?node_id=N)
    GET /                SPA viewer (serves embedded HTML)
  """

  use Plug.Router

  alias Deciduex.Queries

  plug(:match)
  plug(:dispatch)

  # API: Get decision graph
  get "/api/graph" do
    graph = Queries.get_graph()
    send_json(conn, 200, %{ok: true, data: graph})
  end

  # API: Get recent commands
  get "/api/commands" do
    commands = Queries.list_recent_commands(100)
    send_json(conn, 200, %{ok: true, data: commands})
  end

  # API: Get documents
  get "/api/documents" do
    conn = fetch_query_params(conn)
    node_id = parse_int(conn.params["node_id"])
    documents = Queries.list_documents(node_id, false)
    send_json(conn, 200, %{ok: true, data: documents})
  end

  # SPA: Serve embedded viewer for all other GET requests
  get _ do
    html = get_viewer_html()

    conn
    |> put_resp_content_type("text/html")
    |> send_resp(200, html)
  end

  # 404 for non-GET requests to unknown paths
  match _ do
    send_resp(conn, 404, "Not found")
  end

  defp send_json(conn, status, data) do
    conn
    |> put_resp_content_type("application/json")
    |> send_resp(status, Jason.encode!(data))
  end

  defp parse_int(nil), do: nil

  defp parse_int(str) do
    case Integer.parse(str) do
      {n, ""} -> n
      _ -> nil
    end
  end

  defp get_viewer_html do
    # Try to load from file first (development), then embedded
    viewer_path = Path.join(:code.priv_dir(:deciduex), "viewer.html")

    if File.exists?(viewer_path) do
      File.read!(viewer_path)
    else
      default_viewer_html()
    end
  end

  defp default_viewer_html do
    """
    <!DOCTYPE html>
    <html>
    <head>
      <title>Deciduous Graph Viewer</title>
      <style>
        body { font-family: system-ui, sans-serif; padding: 2rem; background: #1a1a2e; color: #eee; }
        h1 { color: #4ade80; }
        a { color: #60a5fa; }
        pre { background: #0f0f1a; padding: 1rem; border-radius: 4px; overflow: auto; }
      </style>
    </head>
    <body>
      <h1>Deciduous Graph Viewer</h1>
      <p>API endpoints available:</p>
      <ul>
        <li><a href="/api/graph">/api/graph</a> - Full decision graph</li>
        <li><a href="/api/commands">/api/commands</a> - Recent command log</li>
        <li><a href="/api/documents">/api/documents</a> - Document attachments</li>
      </ul>
      <p>To use the full React viewer, copy <code>src/viewer.html</code> to <code>priv/viewer.html</code>.</p>
    </body>
    </html>
    """
  end
end
