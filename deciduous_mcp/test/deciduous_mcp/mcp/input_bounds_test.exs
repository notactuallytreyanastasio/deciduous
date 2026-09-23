defmodule DeciduousMcp.MCP.InputBoundsTest do
  @moduledoc """
  The limits the schema-enforcement commit left open, each found over HTTP
  by a verifier:

    * a title made only of invisible format characters (U+200B, U+2060,
      U+FEFF) passed the blank check, which trimmed whitespace only;
    * update_node's free-form metadata took any type under the documented
      keys and any size: 28 keys of 262,144 characters stored 7,340,032
      characters on one node;
    * arrays had no count limit: add_node files with 200,000 items and
      capture_conversation_turn with 5,000 observations were written;
    * an id that is not an id was echoed in Erlang's binary syntax
      (`<<97, 0>>`).

  Over real HTTP to the running listener.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.{Nodes, Workspaces}
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.Node
  alias DeciduousMcp.Test.McpHttp

  setup do
    {:ok, ws} = Workspaces.find_or_create("bounds")
    {:ok, n} = Nodes.create_node(ws.id, %{node_type: "goal", title: "target"})
    %{sid: McpHttp.session(), node: n, ws: ws}
  end

  defp nodes_in(ws), do: Repo.aggregate(from(n in Node, where: n.workspace_id == ^ws.id), :count)

  test "a title of invisible format characters is blank", %{sid: sid, ws: ws} do
    before = nodes_in(ws)

    zwsp = <<0x200B::utf8>>
    word_joiner = <<0x2060::utf8>>
    bom = <<0xFEFF::utf8>>

    for title <- [zwsp, word_joiner, bom, " " <> zwsp <> word_joiner <> " "] do
      answer =
        McpHttp.call(sid, "add_node", %{
          "node_type" => "goal",
          "title" => title,
          "workspace" => "bounds"
        })

      assert {:tool_error, message} = answer, "#{inspect(title)} answered #{inspect(answer)}"
      assert message =~ "blank"
    end

    assert nodes_in(ws) == before
  end

  test "a visible title with a format character in it is kept", %{sid: sid} do
    assert {:ok, _} =
             McpHttp.call(sid, "add_node", %{
               "node_type" => "goal",
               "title" => "zero" <> <<0x200B::utf8>> <> "width",
               "workspace" => "bounds"
             })
  end

  test "an id that is not an id is quoted as text", %{sid: sid} do
    assert {:tool_error, message} = McpHttp.call(sid, "show_node", %{"node_id" => "a\u0000"})
    refute message =~ "<<"
    assert message =~ ~s("a\\0")
  end

  test "update_node metadata is held to the documented types", %{sid: sid, node: n} do
    for bad <- [
          %{"files" => 5},
          %{"files" => [1]},
          %{"commit" => %{"x" => [1, 2]}},
          %{"confidence" => "high"},
          %{"confidence" => 101},
          %{"branch" => 7},
          %{"prompt" => ["x"]}
        ] do
      answer = McpHttp.call(sid, "update_node", %{"node_id" => n.id, "metadata" => bad})
      assert {:tool_error, message} = answer, "#{inspect(bad)} answered #{inspect(answer)}"
      assert message =~ "metadata."
    end

    assert Repo.get!(Node, n.id).metadata == n.metadata

    assert {:ok, _} =
             McpHttp.call(sid, "update_node", %{
               "node_id" => n.id,
               "metadata" => %{
                 "confidence" => 50,
                 "branch" => "b",
                 "files" => ["a.rs"],
                 "other" => "kept"
               }
             })
  end

  test "update_node metadata has a size limit", %{sid: sid, node: n} do
    big = String.duplicate("m", 262_144)
    metadata = Map.new(1..28, fn i -> {"k#{i}", big} end)

    answer = McpHttp.call(sid, "update_node", %{"node_id" => n.id, "metadata" => metadata})

    assert {:tool_error, message} = answer,
           "answered #{inspect(answer, limit: 3, printable_limit: 200)}"

    assert message =~ "limit"
    assert Repo.get!(Node, n.id).metadata == n.metadata
  end

  test "arrays have a count limit", %{sid: sid, ws: ws, node: n} do
    before = nodes_in(ws)
    files = Enum.map(1..200_000, &"f#{&1}")

    assert {:tool_error, m1} =
             McpHttp.call(sid, "add_node", %{
               "node_type" => "goal",
               "title" => "many files",
               "files" => files,
               "workspace" => "bounds"
             })

    assert m1 =~ "files has 200000 items"

    assert {:tool_error, m2} =
             McpHttp.call(sid, "update_node", %{
               "node_id" => n.id,
               "metadata" => %{"files" => files}
             })

    assert m2 =~ "200000 items"

    observations = Enum.map(1..5_000, &"obs #{&1}")

    assert {:tool_error, m3} =
             McpHttp.call(sid, "capture_conversation_turn", %{
               "summary" => "s",
               "observations" => observations,
               "workspace" => "bounds"
             })

    assert m3 =~ "observations has 5000 items"
    assert nodes_in(ws) == before
  end
end
