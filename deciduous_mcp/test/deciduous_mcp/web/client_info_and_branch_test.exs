defmodule DeciduousMcp.Web.ClientInfoAndBranchTest do
  @moduledoc """
  SERVER-N2: a clientInfo name or version over 255 characters, or holding a
  NUL, was accepted at initialize, and then every write from that session
  failed with "add_node failed (MatchError)". So did a write with a branch
  over 255 characters from an ordinary client. Locks.acquire wrote all
  three into varchar(255) columns and matched `{:ok, _}` on the result.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Test.McpHttp

  defp initialize(client_info) do
    body = put_in(McpHttp.initialize_body(7), [:params, :clientInfo], client_info)
    {status, h, resp} = McpHttp.post(body)
    {status, h, McpHttp.decode(resp)}
  end

  defp goal(branch) do
    %{"workspace" => "n2-branch", "node_type" => "goal", "title" => "t", "branch" => branch}
  end

  test "SERVER-N2: a branch of 300 characters is written, and one past the limit is refused by name" do
    sid = McpHttp.session()
    long = String.duplicate("b", 300)

    assert {:ok, %{"id" => _}} = McpHttp.call(sid, "add_node", goal(long))

    assert {:ok, %{}} =
             McpHttp.call(sid, "log_decision", %{
               "workspace" => "n2-branch",
               "title" => "d",
               "chosen_option" => %{"title" => "o"},
               "branch" => long
             })

    assert {:tool_error, message} =
             McpHttp.call(sid, "add_node", goal(String.duplicate("b", 5000)))

    assert message =~ "branch is 5000 characters; the limit is 512"
    refute message =~ "MatchError"
  end

  test "SERVER-N2: a clientInfo name or version that cannot be stored is refused at initialize" do
    for {info, says} <- [
          {%{name: String.duplicate("n", 256), version: "1"},
           "clientInfo.name is 256 characters; the limit is 255"},
          {%{name: "ok", version: String.duplicate("v", 1_000_000)},
           "clientInfo.version is 1000000 characters"},
          {%{name: "has\u0000nul", version: "1"}, "clientInfo.name contains a NUL"}
        ] do
      assert {200, h, %{"id" => 7, "error" => %{"code" => -32602, "message" => message}}} =
               initialize(info)

      assert message =~ says
      refute Map.has_key?(h, "mcp-session-id")
    end
  end

  test "SERVER-N2: a 255-character client name can write" do
    {200, h, %{"result" => _}} = initialize(%{name: String.duplicate("n", 255), version: "1"})
    sid = h["mcp-session-id"]

    McpHttp.post(%{jsonrpc: "2.0", method: "notifications/initialized"}, [{"mcp-session-id", sid}])

    assert {:ok, %{"id" => _}} = McpHttp.call(sid, "add_node", goal("main"))
  end
end
