defmodule DeciduousMcp.Web.ProtocolEdgesTest do
  @moduledoc """
  Malformed or unusual JSON-RPC gets the answer the protocol prescribes,
  under the request's own id when it has one: never an empty 500, never a
  "Parse error" for JSON that parsed, never an id the server made up, never
  a dump of the server's Frame.

  Over real HTTP to the running listener.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Test.McpHttp

  setup do
    sid = McpHttp.session()
    %{sid: sid, h: [{"mcp-session-id", sid}]}
  end

  defp rpc(body, headers) do
    {status, _, resp} = McpHttp.post(body, headers)
    {status, if(resp in ["", "{}"], do: resp, else: McpHttp.decode(resp))}
  end

  test "an empty or blank body is a parse error, not an empty 500", %{h: h} do
    for body <- ["", "  \n"] do
      assert {400, %{"id" => nil, "error" => %{"code" => -32700}}} = rpc(body, h)
    end
  end

  test "a batch is refused as an invalid request, not as unparseable", %{h: h} do
    for body <- [[%{jsonrpc: "2.0", id: 1, method: "tools/list"}], []] do
      assert {400, %{"id" => nil, "error" => %{"code" => -32600, "message" => message}}} =
               rpc(body, h)

      assert message =~ "batch"
    end
  end

  test "an id that is null or not a string or integer is an invalid request", %{h: h} do
    for id <- [nil, %{"a" => 1}, [1]] do
      assert {400, %{"id" => nil, "error" => %{"code" => -32600, "message" => message}}} =
               rpc(%{jsonrpc: "2.0", id: id, method: "tools/list"}, h)

      assert message =~ "id"
    end
  end

  test "a message without an id is a notification: accepted and not answered", %{h: h} do
    for method <- ["notifications/foo", "tools/list"] do
      assert {202, _} = rpc(%{jsonrpc: "2.0", method: method}, h)
    end

    # and the session still works afterwards
    assert {200, %{"id" => 9, "result" => %{"tools" => [_ | _]}}} =
             rpc(%{jsonrpc: "2.0", id: 9, method: "tools/list"}, h)
  end

  test "tools/call with missing or malformed params is invalid params under the request's id",
       %{h: h} do
    for params <- [:absent, %{}, %{"name" => 5}, %{"name" => "query_nodes", "arguments" => [1]}] do
      body = %{jsonrpc: "2.0", id: 13, method: "tools/call"}
      body = if params == :absent, do: body, else: Map.put(body, :params, params)

      assert {200, %{"id" => 13, "error" => %{"code" => -32602} = error}} = rpc(body, h)
      refute inspect(error) =~ ~r/Frame|function_clause|session_id/
    end
  end

  test "a request without a session is refused under its own id" do
    assert {400, %{"id" => 20, "error" => %{"code" => -32600, "message" => message}}} =
             rpc(
               %{
                 jsonrpc: "2.0",
                 id: 20,
                 method: "tools/call",
                 params: %{name: "list_workspaces", arguments: %{}}
               },
               []
             )

    assert message =~ "session"
  end

  test "a request spread over several lines is one request", %{h: h} do
    body = Jason.encode!(%{jsonrpc: "2.0", id: 31, method: "tools/list"}, pretty: true)
    assert body =~ "\n"
    assert {200, %{"id" => 31, "result" => %{"tools" => [_ | _]}}} = rpc(body, h)
  end
end
