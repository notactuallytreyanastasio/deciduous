defmodule DeciduousMcp.Web.WorkspacePinTest do
  @moduledoc """
  A header pin belongs to the request that carries it, not to the server.

  Hermes 0.14.1 keeps one Frame per server and `populate_frame/4` does
  `Map.merge(frame.assigns, conn.assigns)` on every request, so an assign
  set by one session's request stays in the frame for every later request
  from every session unless something overwrites it. Seen on production
  2026-09-22 ~00:15 UTC: session P pinned `epstein` by header, then session
  Q with no header asked `query_nodes` for `tetris-arena` and got epstein's
  nodes.
  """
  use DeciduousMcp.DataCase, async: false

  import Plug.Test
  import Plug.Conn

  alias DeciduousMcp.Graph.{Nodes, Workspaces}
  alias DeciduousMcp.Web.Router

  @opts Router.init([])

  setup do
    token = Application.fetch_env!(:deciduous_mcp, :api_token)
    {:ok, a} = Workspaces.find_or_create("pin-test-a")
    {:ok, b} = Workspaces.find_or_create("pin-test-b")
    {:ok, _} = Nodes.create_node(a.id, %{node_type: "goal", title: "A's goal"})
    {:ok, _} = Nodes.create_node(b.id, %{node_type: "goal", title: "B's goal"})
    %{token: token}
  end

  defp post(token, body, headers) do
    conn =
      conn(:post, "/mcp", Jason.encode!(body))
      |> put_req_header("authorization", "Bearer " <> token)
      |> put_req_header("content-type", "application/json")
      |> put_req_header("accept", "application/json, text/event-stream")

    headers
    |> Enum.reduce(conn, fn {k, v}, c -> put_req_header(c, k, v) end)
    |> Router.call(@opts)
  end

  defp session(token, headers) do
    conn =
      post(
        token,
        %{
          jsonrpc: "2.0",
          id: 1,
          method: "initialize",
          params: %{
            protocolVersion: "2025-03-26",
            capabilities: %{},
            clientInfo: %{name: "pin", version: "0"}
          }
        },
        headers
      )

    [sid] = get_resp_header(conn, "mcp-session-id")

    post(token, %{jsonrpc: "2.0", method: "notifications/initialized"}, [
      {"mcp-session-id", sid} | headers
    ])

    # a call on the new session, so the initialized cast has landed
    post(token, %{jsonrpc: "2.0", id: 0, method: "tools/list"}, [
      {"mcp-session-id", sid} | headers
    ])

    sid
  end

  defp titles(conn) do
    %{"result" => %{"content" => [%{"text" => text}]}} = Jason.decode!(conn.resp_body)
    text |> Jason.decode!() |> Map.fetch!("nodes") |> Enum.map(& &1["title"])
  end

  test "a session with no header is not scoped by another session's pin", %{token: token} do
    pinned = session(token, [{"x-deciduous-workspace", "pin-test-a"}])

    conn =
      post(
        token,
        %{
          jsonrpc: "2.0",
          id: 2,
          method: "tools/call",
          params: %{name: "query_nodes", arguments: %{}}
        },
        [
          {"mcp-session-id", pinned},
          {"x-deciduous-workspace", "pin-test-a"}
        ]
      )

    assert titles(conn) == ["A's goal"]

    unpinned = session(token, [])

    conn =
      post(
        token,
        %{
          jsonrpc: "2.0",
          id: 3,
          method: "tools/call",
          params: %{name: "query_nodes", arguments: %{workspace: "pin-test-b"}}
        },
        [{"mcp-session-id", unpinned}]
      )

    assert titles(conn) == ["B's goal"]
  end
end
