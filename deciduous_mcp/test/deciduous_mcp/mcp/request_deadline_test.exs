defmodule DeciduousMcp.MCP.RequestDeadlineTest do
  @moduledoc """
  A handler that outlives `request_deadline` is killed, and its request is
  answered under its own id with a sentence, well before the client's own
  timeout. Driven over the real Streamable HTTP plug and transport, against a
  second server whose only tool sleeps for as long as it is asked.
  """
  use ExUnit.Case, async: false

  import ExUnit.CaptureLog
  import Plug.Test
  import Plug.Conn

  alias DeciduousMcp.Test.DeadlineServer
  alias Hermes.Server.Transport.StreamableHTTP.Plug, as: McpPlug

  @deadline 300

  setup do
    start_supervised!(
      {DeadlineServer,
       transport: {:streamable_http, start: true},
       request_deadline: @deadline,
       request_timeout: 5_000}
    )

    Process.register(self(), :deadline_test)
    %{opts: McpPlug.init(server: DeadlineServer)}
  end

  defp post(opts, body, session_id \\ nil) do
    conn =
      conn(:post, "/", Jason.encode!(body))
      |> put_req_header("content-type", "application/json")
      |> put_req_header("accept", "application/json, text/event-stream")

    conn = if session_id, do: put_req_header(conn, "mcp-session-id", session_id), else: conn
    McpPlug.call(conn, opts)
  end

  defp session(opts) do
    conn =
      post(opts, %{
        jsonrpc: "2.0",
        id: 1,
        method: "initialize",
        params: %{
          protocolVersion: "2025-03-26",
          capabilities: %{},
          clientInfo: %{name: "t", version: "0"}
        }
      })

    [sid] = get_resp_header(conn, "mcp-session-id")
    post(opts, %{jsonrpc: "2.0", method: "notifications/initialized"}, sid)
    # A request queues behind the initialized cast, so once this answers the
    # session is ready.
    assert %{"result" => _} =
             Jason.decode!(
               post(opts, %{jsonrpc: "2.0", id: 0, method: "tools/list"}, sid).resp_body
             )

    sid
  end

  defp sleep(opts, sid, id, ms) do
    post(
      opts,
      %{
        jsonrpc: "2.0",
        id: id,
        method: "tools/call",
        params: %{name: "sleep", arguments: %{ms: ms}}
      },
      sid
    )
  end

  test "a call over the deadline is stopped and answered under its own id", %{opts: opts} do
    sid = session(opts)

    log =
      capture_log(fn ->
        {elapsed_us, conn} = :timer.tc(fn -> sleep(opts, sid, 42, 5_000) end)
        send(self(), {:result, elapsed_us, conn})
        Process.sleep(50)
      end)

    assert_received {:result, elapsed_us, conn}
    assert log =~ "request_deadline_exceeded"
    assert log =~ "tool: \"sleep\""

    assert conn.status == 200
    assert %{"id" => 42, "error" => %{"message" => message}} = Jason.decode!(conn.resp_body)
    assert message =~ "sleep did not finish within 300ms and was stopped"

    assert elapsed_us < 2_000_000,
           "answered after #{div(elapsed_us, 1000)}ms, deadline is #{@deadline}ms"

    # The handler was killed, not abandoned: it never reaches its last line.
    refute_receive {:sleep_finished, 5_000}, 5_500
  end

  test "a call under the deadline is untouched", %{opts: opts} do
    sid = session(opts)
    conn = sleep(opts, sid, 7, 20)

    assert %{"id" => 7, "result" => _} = Jason.decode!(conn.resp_body)
    assert_receive {:sleep_finished, 20}
  end

  test "the session and other sessions keep working after a stopped call", %{opts: opts} do
    a = session(opts)
    b = session(opts)

    task = Task.async(fn -> sleep(opts, a, 1, 5_000) end)
    Process.sleep(50)
    # b is not queued behind a's stuck handler
    {us, conn} = :timer.tc(fn -> sleep(opts, b, 2, 10) end)
    assert %{"id" => 2, "result" => _} = Jason.decode!(conn.resp_body)
    assert us < 200_000

    assert %{"id" => 1, "error" => _} = Jason.decode!(Task.await(task).resp_body)
    assert %{"id" => 3, "result" => _} = Jason.decode!(sleep(opts, a, 3, 10).resp_body)
  end
end
