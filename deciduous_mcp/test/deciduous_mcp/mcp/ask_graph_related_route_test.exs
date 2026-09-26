defmodule DeciduousMcp.MCP.AskGraphRelatedRouteTest do
  @moduledoc """
  ask_graph expands along shared files and commits, not only edges.

  Retrieval (01-closed-loop-ask-graph) takes extra routes; Related
  (03-file-and-commit-links) supplies one that reads metadata.files and
  metadata.commit. The two nodes here have no edge between them, so the
  second can only be reached through the files they share.

  Two shared paths here, though one is now enough: Related scores one
  ordinary shared path as usefulness 0.7, and with no word of the question
  in the neighbour Retrieval's score is (0.7 + 0.5 * edge weight 1.0) / 4.5
  = 0.267, over its 0.25 threshold. (It used to be 0.5 and 0.222, which is
  why this fixture shares two.)

  Over real HTTP to the running listener.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.{Nodes, Workspaces}
  alias DeciduousMcp.Test.McpHttp

  setup do
    {:ok, ws} = Workspaces.find_or_create("ask-related")

    {:ok, anchor} =
      Nodes.create_node(ws.id, %{
        node_type: "action",
        title: "Put a redis cache in front of the lookup",
        metadata: %{"files" => ["src/lookup.rs", "src/evict.rs"]}
      })

    {:ok, sibling} =
      Nodes.create_node(ws.id, %{
        node_type: "action",
        title: "Tune eviction thresholds",
        metadata: %{"files" => ["./src/lookup.rs", "src/evict.rs"]}
      })

    %{sid: McpHttp.session(), anchor: anchor, sibling: sibling}
  end

  test "a node sharing a file with an anchor is reached by shared_identifier", ctx do
    {:ok, r} =
      McpHttp.call(ctx.sid, "ask_graph", %{
        "question" => "which files did the redis cache touch",
        "workspace" => "ask-related"
      })

    assert "shared_identifier" in Enum.map(r["routes"], & &1["name"])

    by_id = Map.new(r["results"], &{&1["id"], &1})
    assert by_id[ctx.anchor.id]["reached_by"] == "anchor"
    assert by_id[ctx.sibling.id]["reached_by"] == "shared_identifier"
  end
end
