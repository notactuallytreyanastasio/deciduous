defmodule DeciduousMcp.MCP.InstructionsTest do
  @moduledoc """
  The initialize result carries the logging instructions, through the whole
  router, as a client sees it on the wire.
  """
  use ExUnit.Case, async: false

  import Plug.Test
  import Plug.Conn

  alias DeciduousMcp.Web.Router

  @opts Router.init([])

  test "initialize answers with the instructions" do
    token = Application.fetch_env!(:deciduous_mcp, :api_token)

    body =
      Jason.encode!(%{
        jsonrpc: "2.0",
        id: 1,
        method: "initialize",
        params: %{
          protocolVersion: "2025-03-26",
          capabilities: %{},
          clientInfo: %{name: "instructions-test", version: "0"}
        }
      })

    conn =
      conn(:post, "/mcp", body)
      |> put_req_header("authorization", "Bearer " <> token)
      |> put_req_header("content-type", "application/json")
      |> put_req_header("accept", "application/json, text/event-stream")
      |> Router.call(@opts)

    assert conn.status == 200

    # JSON, or one SSE event whose data line is the JSON.
    json =
      case Regex.run(~r/^data:(.*)$/m, conn.resp_body) do
        [_, data] -> data
        nil -> conn.resp_body
      end

    %{"result" => result} = Jason.decode!(json)
    assert result["instructions"] == DeciduousMcp.MCP.Instructions.text()
    assert result["instructions"] =~ "capture_conversation_turn"
    assert result["instructions"] =~ "parent_id"
    assert result["serverInfo"]["name"] == "deciduous-mcp"
  end

  # After a rename, or a `remote init --workspace`, the CLI writes to the
  # name recorded in .deciduous/config.toml. An agent told to use the
  # directory name wrote to a different graph than the CLI beside it.
  test "agents are told to take the workspace from the project's config first" do
    text = DeciduousMcp.MCP.Instructions.text()
    assert text =~ ".deciduous/config.toml"
    refute text =~ "(the repository root's directory name, also from a worktree)"

    desc = DeciduousMcp.MCP.Scope.schema_property()[:workspace][:description]
    assert desc =~ ".deciduous/config.toml"
  end
end
