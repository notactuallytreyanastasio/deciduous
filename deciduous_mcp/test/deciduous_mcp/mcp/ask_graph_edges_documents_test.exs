defmodule DeciduousMcp.MCP.AskGraphEdgesDocumentsTest do
  @moduledoc """
  ask_graph matches a question against edge rationales and the descriptions
  of attached documents, and returns nodes: for an edge, the node it points
  to; for a document, the node it is attached to. Both stay inside the asked
  workspace and skip deleted nodes, the same as the node text search.

  The search words ("quokka", "narwhal") appear only in rationales and
  document descriptions, never in a node's own text, so every hit here is
  one the node search alone could not have produced.
  """
  use DeciduousMcp.DataCase

  alias DeciduousMcp.Graph.{Edges, Nodes}
  alias DeciduousMcp.MCP.Tools.AskGraph
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.Document

  setup do
    mine = create_test_workspace("ask-ed-mine")
    theirs = create_test_workspace("ask-ed-theirs")

    frame = %Hermes.Server.Frame{private: %{session_id: "session_ask_ed"}, assigns: %{}}
    %{mine: mine, theirs: theirs, frame: frame}
  end

  defp node!(ws, title) do
    {:ok, node} = Nodes.create_node(ws.id, %{node_type: "action", title: title})
    node
  end

  defp edge!(ws, from, to, rationale) do
    {:ok, edge} =
      Edges.create_edge(ws.id, %{from_node_id: from.id, to_node_id: to.id, rationale: rationale})

    edge
  end

  defp document!(ws, node, description, attrs \\ %{}) do
    hash = :crypto.hash(:sha256, description) |> Base.encode16(case: :lower)

    %Document{}
    |> Document.changeset(
      Map.merge(
        %{
          change_id: Ecto.UUID.generate(),
          content_hash: hash,
          original_filename: "plan.md",
          storage_filename: "#{hash}.md",
          mime_type: "text/markdown",
          file_size: 10,
          description: description,
          description_source: "user",
          node_id: node.id,
          workspace_id: ws.id
        },
        attrs
      )
    )
    |> Repo.insert!()
  end

  defp ask(frame, ws, question) do
    {:ok, json} =
      AskGraph.call(%{
        arguments: %{"workspace" => ws.name, "question" => question},
        server: frame
      })

    json |> Jason.decode!() |> Map.fetch!("results")
  end

  defp titles(results), do: results |> Enum.map(& &1["title"]) |> Enum.sort()

  test "an edge rationale match returns the node the edge points to", ctx do
    parent = node!(ctx.mine, "parent here")
    child = node!(ctx.mine, "child here")
    edge!(ctx.mine, parent, child, "quokka habitat survey")

    results = ask(ctx.frame, ctx.mine, "quokka")

    assert titles(results) == ["child here"]
    [hit] = results
    assert hit["matched_on"] == ["edge_rationale"]

    # The parent is not lost: it is in the child's context, with the
    # rationale that matched.
    assert [%{"title" => "parent here", "rationale" => "quokka habitat survey"}] =
             hit["connected_from"]
  end

  test "edge matches stay in the asked workspace and skip deleted nodes", ctx do
    a = node!(ctx.mine, "mine from")
    b = node!(ctx.mine, "mine to")
    edge!(ctx.mine, a, b, "quokka mine")

    x = node!(ctx.theirs, "theirs from")
    y = node!(ctx.theirs, "theirs to")
    edge!(ctx.theirs, x, y, "quokka theirs")

    c = node!(ctx.mine, "mine to deleted")
    edge!(ctx.mine, a, c, "quokka deleted")
    {:ok, _} = Nodes.delete_node(c.id)

    assert titles(ask(ctx.frame, ctx.mine, "quokka")) == ["mine to"]
    assert titles(ask(ctx.frame, ctx.theirs, "quokka")) == ["theirs to"]
  end

  test "a document description match returns the node it is attached to", ctx do
    node = node!(ctx.mine, "has a plan")
    document!(ctx.mine, node, "narwhal migration plan")

    [hit] = ask(ctx.frame, ctx.mine, "narwhal")
    assert hit["title"] == "has a plan"
    assert hit["matched_on"] == ["document"]
  end

  test "document matches stay in the asked workspace, skip deleted nodes and detached documents",
       ctx do
    kept = node!(ctx.mine, "mine attached")
    document!(ctx.mine, kept, "narwhal mine")

    theirs = node!(ctx.theirs, "theirs attached")
    document!(ctx.theirs, theirs, "narwhal theirs")

    gone = node!(ctx.mine, "mine deleted")
    document!(ctx.mine, gone, "narwhal on a deleted node")
    {:ok, _} = Nodes.delete_node(gone.id)

    detached = node!(ctx.mine, "mine detached")

    detached
    |> then(&document!(ctx.mine, &1, "narwhal detached"))
    |> Ecto.Changeset.change(detached_at: DateTime.utc_now())
    |> Repo.update!()

    assert titles(ask(ctx.frame, ctx.mine, "narwhal")) == ["mine attached"]
    assert titles(ask(ctx.frame, ctx.theirs, "narwhal")) == ["theirs attached"]
  end

  test "a node found more than one way says so", ctx do
    parent = node!(ctx.mine, "parent")
    child = node!(ctx.mine, "quokka in the title")
    edge!(ctx.mine, parent, child, "quokka in the rationale")
    document!(ctx.mine, child, "quokka in the document")

    [hit] = ask(ctx.frame, ctx.mine, "quokka")
    assert hit["title"] == "quokka in the title"
    assert hit["matched_on"] == ["text", "document", "edge_rationale"]
  end
end
