defmodule DeciduousMcp.MCP.WorkspaceUnicodeTest do
  @moduledoc """
  One project name is one workspace, however its accents are encoded.

  Workspace names were lowercased and trimmed but not Unicode-normalized, so
  "café" written precomposed (NFC, U+00E9) and decomposed (NFD, "e" and
  U+0301) became two workspaces that list under the same name: the display
  spoofing the control-character check was added to stop, reached another
  way. macOS file APIs commonly hand back decomposed names, so a repo's
  basename can arrive either way.

  Over real HTTP to the running listener.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.Workspaces
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.Workspace
  alias DeciduousMcp.Test.McpHttp

  @nfc "caf" <> <<0x00E9::utf8>>
  @nfd "cafe" <> <<0x0301::utf8>>

  defp add(sid, workspace, title) do
    McpHttp.call(sid, "add_node", %{
      "node_type" => "goal",
      "title" => title,
      "workspace" => workspace
    })
  end

  defp workspaces_named_cafe do
    Repo.all(Workspace)
    |> Enum.filter(&(String.normalize(&1.name, :nfc) == @nfc))
  end

  test "NFC and NFD spellings of a name are one workspace" do
    refute @nfc == @nfd
    sid = McpHttp.session()

    assert {:ok, _} = add(sid, @nfc, "precomposed")
    assert {:ok, _} = add(sid, @nfd, "decomposed")

    assert [ws] = workspaces_named_cafe()
    assert ws.name == @nfc

    {:ok, %{"nodes" => results}} =
      McpHttp.call(sid, "query_nodes", %{"workspace" => @nfd})

    assert results |> Enum.map(& &1["title"]) |> Enum.sort() == ["decomposed", "precomposed"]
  end

  test "a workspace created under a decomposed name before this fix is still the one found" do
    legacy = Repo.insert!(%Workspace{name: @nfd})
    sid = McpHttp.session()

    assert {:ok, _} = add(sid, @nfc, "after the fix")

    assert [ws] = workspaces_named_cafe()
    assert ws.id == legacy.id
    assert {:ok, %{id: id}} = Workspaces.get_by_name(@nfc)
    assert id == legacy.id
  end

  # Mint will not send a byte over 0x7F in a header value, and HTTP/1.1
  # allows it (obs-text), so this one goes over a raw socket.
  test "a header that is not UTF-8 is refused" do
    body = Jason.encode!(McpHttp.initialize_body())

    request =
      "POST /mcp HTTP/1.1\r\nhost: 127.0.0.1\r\n" <>
        "authorization: Bearer #{McpHttp.token()}\r\n" <>
        "accept: application/json, text/event-stream\r\n" <>
        "content-type: application/json\r\n" <>
        "x-deciduous-workspace: " <>
        <<0xFF, 0x61>> <>
        "\r\n" <>
        "content-length: #{byte_size(body)}\r\nconnection: close\r\n\r\n" <> body

    {:ok, socket} = :gen_tcp.connect(~c"127.0.0.1", McpHttp.port(), [:binary, active: false])
    :ok = :gen_tcp.send(socket, request)
    response = recv_all(socket, "")
    :gen_tcp.close(socket)

    assert response =~ ~r{\AHTTP/1.1 400}, response
    assert response =~ "UTF-8"
  end

  defp recv_all(socket, acc) do
    case :gen_tcp.recv(socket, 0, 10_000) do
      {:ok, data} -> recv_all(socket, acc <> data)
      {:error, :closed} -> acc
    end
  end
end
