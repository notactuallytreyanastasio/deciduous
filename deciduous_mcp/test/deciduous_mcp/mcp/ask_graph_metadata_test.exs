defmodule DeciduousMcp.MCP.AskGraphMetadataTest do
  @moduledoc """
  ask_graph searches metadata values, not metadata key names.

  The text search matched `metadata::text ILIKE '%term%'`, the whole JSON
  document, keys included. Every node add_node writes has a "branch" key, so
  ask_graph "branch" returned every node in the workspace though no title,
  description or value said branch; "prompt", "confidence" and "files"
  did the same.

  Over real HTTP to the running listener.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.{Nodes, Workspaces}
  alias DeciduousMcp.Test.McpHttp

  setup do
    {:ok, ws} = Workspaces.find_or_create("ask-meta")

    for title <- ["alpha", "beta", "gamma"] do
      {:ok, _} =
        Nodes.create_node(ws.id, %{
          node_type: "goal",
          title: title,
          metadata: %{
            "branch" => "feat-x",
            "prompt" => "use a write ahead log",
            "confidence" => 80,
            "files" => ["src/main.rs"]
          }
        })
    end

    %{sid: McpHttp.session()}
  end

  defp titles(sid, question) do
    {:ok, %{"results" => results}} =
      McpHttp.call(sid, "ask_graph", %{"question" => question, "workspace" => "ask-meta"})

    results |> Enum.map(& &1["title"]) |> Enum.sort()
  end

  test "a metadata key name matches nothing", %{sid: sid} do
    for key <- ["branch", "prompt", "confidence", "files"] do
      assert titles(sid, key) == [], "ask_graph #{inspect(key)} matched on the key"
    end
  end

  test "metadata values are still searched", %{sid: sid} do
    assert titles(sid, "feat-x") == ["alpha", "beta", "gamma"]
    assert titles(sid, "ahead") == ["alpha", "beta", "gamma"]
    assert titles(sid, "main") == ["alpha", "beta", "gamma"]
  end
end
