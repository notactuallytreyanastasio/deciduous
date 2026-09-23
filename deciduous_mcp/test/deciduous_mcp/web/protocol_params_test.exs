defmodule DeciduousMcp.Web.ProtocolParamsTest do
  @moduledoc """
  The shapes the first round of protocol fixes did not cover, each found by a
  verifier over real HTTP after that round:

    * a message with no `jsonrpc` member got Hermes's 400 -32700 "Parse
      error" under an `err_` id it made up;
    * params that parse as JSON but not as the method's params (`"x"`,
      `[1]`, a number where a string goes) got an empty HTTP 500, or 400
      under an `err_` id, or 202 as though the request were a notification;
    * tools/call without `arguments`, which MCP makes optional, and
      prompts/get or resources/read without params crashed Hermes's handler
      and returned the session's `#Frame<...>` and a stack trace in `data`;
    * a notification with bad params got an error reply, which JSON-RPC
      forbids.

  Over real HTTP to the running listener.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Test.McpHttp

  setup do
    sid = McpHttp.session()
    %{h: [{"mcp-session-id", sid}]}
  end

  defp rpc(body, headers) do
    {status, _, resp} = McpHttp.post(body, headers)
    {status, if(resp in ["", "{}"], do: resp, else: McpHttp.decode(resp))}
  end

  defp refute_internals(term) do
    text = inspect(term)
    refute text =~ ~r/Frame|function_clause|session_id|stacktrace|\.ex"?, line/
    refute text =~ "err_"
  end

  test "a message without a jsonrpc member is an invalid request under its own id", %{h: h} do
    assert {400, %{"id" => 5, "error" => %{"code" => -32600} = error} = reply} =
             rpc(%{id: 5, method: "ping"}, h)

    assert error["message"] =~ "jsonrpc"
    refute_internals(reply)
  end

  test "params of the wrong shape are invalid params under the request's id", %{h: h} do
    cases = [
      %{method: "tools/list", params: "x"},
      %{method: "prompts/get", params: "x"},
      %{method: "ping", params: [1]},
      %{method: "prompts/get", params: %{name: "always_capture", arguments: "x"}},
      %{method: "prompts/get", params: %{name: 5}},
      %{method: "logging/setLevel", params: %{level: 5}}
    ]

    for {c, id} <- Enum.with_index(cases, 40) do
      body = Map.merge(%{jsonrpc: "2.0", id: id}, c)
      result = rpc(body, h)

      assert {200, %{"id" => ^id, "error" => %{"code" => -32602, "message" => message}}} =
               result,
             "#{inspect(c)} answered #{inspect(result)}"

      assert message =~ "Invalid params"
      refute_internals(result)
    end
  end

  test "an initialize without a session and with bad params is answered, not dropped" do
    cases = [
      %{params: "x"},
      %{params: %{protocolVersion: "2025-06-18", capabilities: %{}, clientInfo: "x"}},
      %{}
    ]

    for {c, id} <- Enum.with_index(cases, 60) do
      body = Map.merge(%{jsonrpc: "2.0", id: id, method: "initialize"}, c)
      result = rpc(body, [])

      assert {200, %{"id" => ^id, "error" => %{"code" => -32602}}} = result,
             "#{inspect(c)} answered #{inspect(result)}"

      refute_internals(result)
    end
  end

  test "a notification with bad params is accepted and not answered", %{h: h} do
    assert {202, ""} = rpc(%{jsonrpc: "2.0", method: "notifications/cancelled", params: "x"}, h)

    assert {200, %{"id" => 9, "result" => %{}}} = rpc(%{jsonrpc: "2.0", id: 9, method: "ping"}, h)
  end

  test "tools/call without arguments runs the tool with none", %{h: h} do
    assert {200, %{"id" => 70, "result" => %{"content" => [_ | _]}} = reply} =
             rpc(
               %{
                 jsonrpc: "2.0",
                 id: 70,
                 method: "tools/call",
                 params: %{name: "list_workspaces"}
               },
               h
             )

    refute_internals(reply)

    # a tool with required arguments says which one is missing
    assert {200, %{"id" => 71, "error" => %{"code" => -32602} = error} = reply} =
             rpc(%{jsonrpc: "2.0", id: 71, method: "tools/call", params: %{name: "add_node"}}, h)

    assert inspect(error) =~ "node_type: is required"
    refute_internals(reply)
  end

  test "prompts/get and resources/read without params are invalid params", %{h: h} do
    for {method, id} <- [{"prompts/get", 80}, {"resources/read", 81}] do
      result = rpc(%{jsonrpc: "2.0", id: id, method: method}, h)

      assert {200, %{"id" => ^id, "error" => %{"code" => -32602}}} = result,
             "#{method} answered #{inspect(result)}"

      refute_internals(result)
    end
  end

  test "prompts/get without arguments gets the prompt", %{h: h} do
    assert {200, %{"id" => 90, "result" => %{"messages" => [_ | _]}}} =
             rpc(
               %{
                 jsonrpc: "2.0",
                 id: 90,
                 method: "prompts/get",
                 params: %{name: "always_capture"}
               },
               h
             )
  end
end
