defmodule DeciduousMcp.MCP.ServerVersionTest do
  @moduledoc """
  SERVER-N9: the server reported serverInfo.version "1.0.7" on the stack
  being released as 1.0.8. The version lived in three places (mix.exs,
  the Hermes `use` in server.ex, the Protocol module), and a server built
  from any commit but the release one says the old number. Now it lives
  in mix.exs only, and the other two read it.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Test.McpHttp

  @vsn to_string(Application.spec(:deciduous_mcp, :vsn))

  test "SERVER-N9: initialize reports the application's own version" do
    {200, _, body} = McpHttp.post(McpHttp.initialize_body(3))
    assert %{"result" => %{"serverInfo" => %{"version" => @vsn}}} = McpHttp.decode(body)
    assert DeciduousMcp.MCP.Protocol.server_version() == @vsn
  end

  test "SERVER-N9: no module but mix.exs spells the version out" do
    for file <- ["lib/deciduous_mcp/mcp/server.ex", "lib/deciduous_mcp/mcp/protocol.ex"] do
      refute File.read!(file) =~ ~r/version[:\s=]+"\d+\.\d+\.\d+"/,
             "#{file} carries its own version string"
    end
  end
end
