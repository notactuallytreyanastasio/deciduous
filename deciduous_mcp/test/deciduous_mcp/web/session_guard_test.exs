defmodule DeciduousMcp.Web.SessionGuardTest do
  @moduledoc """
  A request for a session the server no longer has gets 404 and an error
  the client can match, not a 200 with an error under a made-up id.

  Exercises the whole router, not the plug in isolation: the point is what
  a real client sees on the wire after its session has gone.
  """
  use ExUnit.Case, async: false

  import Plug.Test
  import Plug.Conn

  alias DeciduousMcp.Web.Router

  @opts Router.init([])
  @server DeciduousMcp.MCP.Server

  setup do
    token = Application.fetch_env!(:deciduous_mcp, :api_token)
    %{token: token}
  end

  defp post(token, body, headers \\ []) do
    conn =
      conn(:post, "/mcp", Jason.encode!(body))
      |> put_req_header("authorization", "Bearer " <> token)
      |> put_req_header("content-type", "application/json")
      |> put_req_header("accept", "application/json, text/event-stream")

    headers
    |> Enum.reduce(conn, fn {k, v}, c -> put_req_header(c, k, v) end)
    |> Router.call(@opts)
  end

  defp initialize(token) do
    conn =
      post(token, %{
        jsonrpc: "2.0",
        id: 1,
        method: "initialize",
        params: %{
          protocolVersion: "2025-03-26",
          capabilities: %{},
          clientInfo: %{name: "guard-test", version: "0"}
        }
      })

    assert conn.status == 200
    [session_id] = get_resp_header(conn, "mcp-session-id")

    post(token, %{jsonrpc: "2.0", method: "notifications/initialized"}, [
      {"mcp-session-id", session_id}
    ])

    # Hermes handles that notification as a cast. A request on the same
    # session is queued behind it, so a successful call here means the
    # session is initialized before the test does anything else to it. Closing
    # the session with the cast still in flight makes Hermes' server process
    # crash on a dead session and take the transport down with it.
    conn =
      post(token, %{jsonrpc: "2.0", id: 0, method: "tools/list"}, [
        {"mcp-session-id", session_id}
      ])

    assert conn.status == 200
    assert %{"id" => 0, "result" => _} = Jason.decode!(conn.resp_body)

    session_id
  end

  test "a live session is left alone", %{token: token} do
    session_id = initialize(token)

    conn =
      post(token, %{jsonrpc: "2.0", id: 7, method: "tools/list"}, [
        {"mcp-session-id", session_id}
      ])

    assert conn.status == 200
    assert %{"id" => 7, "result" => %{"tools" => tools}} = Jason.decode!(conn.resp_body)
    assert Enum.any?(tools, &(&1["name"] == "check_activity"))
  end

  test "a request for an expired session is refused with 404 and the request's own id",
       %{token: token} do
    session_id = initialize(token)
    assert Hermes.Server.Registry.whereis_server_session(@server, session_id)

    # What Hermes does at session_idle_timeout, and what a restart does to
    # every session at once.
    :ok =
      Hermes.Server.Session.Supervisor.close_session(Hermes.Server.Registry, @server, session_id)

    # terminate_child returns when the process is dead; the registry forgets
    # it a moment later, on :DOWN.
    assert wait_until(fn ->
             Hermes.Server.Registry.whereis_server_session(@server, session_id) == nil
           end)

    conn =
      post(token, %{jsonrpc: "2.0", id: "call-42", method: "tools/list"}, [
        {"mcp-session-id", session_id}
      ])

    assert conn.status == 404
    body = Jason.decode!(conn.resp_body)
    assert body["id"] == "call-42"
    assert body["error"]["code"] == -32001
    assert body["error"]["message"] == "Session not found"
    assert body["error"]["data"]["session_id"] == session_id
  end

  test "a session id the server has never seen gets the same answer", %{token: token} do
    conn =
      post(token, %{jsonrpc: "2.0", id: 3, method: "check_activity"}, [
        {"mcp-session-id", "session_never_existed"}
      ])

    assert conn.status == 404
    assert %{"id" => 3, "error" => %{"code" => -32001}} = Jason.decode!(conn.resp_body)
  end

  test "initialize passes through even with a stale session header", %{token: token} do
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
            clientInfo: %{name: "guard-test", version: "0"}
          }
        },
        [{"mcp-session-id", "session_from_before_the_restart"}]
      )

    assert conn.status == 200
    [new_id] = get_resp_header(conn, "mcp-session-id")
    assert new_id != "session_from_before_the_restart"

    assert %{"id" => 1, "result" => %{"serverInfo" => %{"version" => "1.0.0"}}} =
             Jason.decode!(conn.resp_body)
  end

  test "a session that exists but was never initialized is refused too", %{token: token} do
    # What Hermes leaves behind when the idle timer fires between the guard's
    # check and its own handling: a fresh, uninitialized session under the
    # old id. It is alive, and it is useless to the client.
    {:ok, _pid} =
      Hermes.Server.Session.Supervisor.create_session(
        Hermes.Server.Registry,
        @server,
        "session_phantom"
      )

    on_exit(fn ->
      Hermes.Server.Session.Supervisor.close_session(
        Hermes.Server.Registry,
        @server,
        "session_phantom"
      )
    end)

    conn =
      post(token, %{jsonrpc: "2.0", id: 11, method: "tools/list"}, [
        {"mcp-session-id", "session_phantom"}
      ])

    assert conn.status == 404
    assert %{"id" => 11, "error" => %{"code" => -32001}} = Jason.decode!(conn.resp_body)
  end

  test "a body past the read limit on a dead session is 413, not a crash", %{token: token} do
    conn =
      conn(:post, "/mcp", String.duplicate("x", 8_000_001))
      |> put_req_header("authorization", "Bearer " <> token)
      |> put_req_header("content-type", "application/json")
      |> put_req_header("accept", "application/json, text/event-stream")
      |> put_req_header("mcp-session-id", "session_long_gone")
      |> Router.call(@opts)

    assert conn.status == 413

    assert %{"id" => nil, "error" => %{"message" => "Request body too large"}} =
             Jason.decode!(conn.resp_body)
  end

  defp wait_until(fun, tries \\ 100) do
    cond do
      fun.() ->
        true

      tries == 0 ->
        false

      true ->
        Process.sleep(10)
        wait_until(fun, tries - 1)
    end
  end
end
