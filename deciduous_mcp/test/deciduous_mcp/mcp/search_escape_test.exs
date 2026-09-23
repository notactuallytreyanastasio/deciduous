defmodule DeciduousMcp.MCP.SearchEscapeTest do
  @moduledoc """
  A search term is text, not a LIKE pattern. `query_nodes(search:)` and
  `ask_graph` wrapped the caller's term in `%...%` unescaped, so `%` and `_`
  matched everything and a backslash (LIKE's escape character) matched
  nothing, not even a title containing one.

  Over real HTTP to the running listener.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Test.McpHttp

  @titles ["meta 100% done", "meta abc", "meta a_c literal", "meta back\\slash"]

  setup do
    sid = McpHttp.session()

    for title <- @titles do
      {:ok, _} =
        McpHttp.call(sid, "add_node", %{
          "node_type" => "observation",
          "title" => title,
          "workspace" => "search-escape"
        })
    end

    %{sid: sid}
  end

  defp search(sid, term) do
    {:ok, %{"nodes" => nodes}} =
      McpHttp.call(sid, "query_nodes", %{"workspace" => "search-escape", "search" => term})

    nodes |> Enum.map(& &1["title"]) |> Enum.sort()
  end

  test "query_nodes matches LIKE metacharacters literally", %{sid: sid} do
    assert search(sid, "%") == ["meta 100% done"]
    assert search(sid, "_") == ["meta a_c literal"]
    assert search(sid, "a_c") == ["meta a_c literal"]
    assert search(sid, "\\") == ["meta back\\slash"]
    assert search(sid, "back\\slash") == ["meta back\\slash"]
    assert search(sid, "meta") == Enum.sort(@titles)
  end

  test "ask_graph matches LIKE metacharacters literally", %{sid: sid} do
    {:ok, %{"results" => results}} =
      McpHttp.call(sid, "ask_graph", %{"workspace" => "search-escape", "question" => "a_c"})

    assert Enum.map(results, & &1["title"]) == ["meta a_c literal"]
  end
end
