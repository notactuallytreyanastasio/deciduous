defmodule DeciduousMcp.Web.ImportMetadataTest do
  @moduledoc """
  S4 bypass: update_node refused a confidence that was not a number from 0
  to 100, but POST /import wrote metadata with insert_all, which skips
  Node.changeset, and stored {"confidence": "999"} and {"confidence": true}.
  A node holding such a value then refused every later metadata patch,
  even one that never touched confidence. Over the router.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.{Nodes, Workspaces}
  alias DeciduousMcp.Schema.Node
  alias DeciduousMcp.Test.McpClient

  setup do
    {:ok, ws} = Workspaces.find_or_create("im-ws")
    %{ws: ws, client: McpClient.connect()}
  end

  defp push(client, nodes),
    do: McpClient.post_json(client, "/import", %{"workspace" => "im-ws", "graph" => %{"nodes" => nodes}})

  defp node(cid, meta), do: %{"change_id" => cid, "node_type" => "goal", "title" => cid, "metadata_json" => meta}

  test "an import carrying an invalid confidence is refused whole, naming the node", %{
    client: client,
    ws: ws
  } do
    for {bad, shown} <- [
          {~s({"confidence":"999"}), ~s("999")},
          {~s({"confidence":true}), "true"},
          {~s({"confidence":150}), "150"},
          {~s({"confidence":-1}), "-1"}
        ] do
      {status, body} = push(client, [node("im-ok", ~s({"confidence":50})), node("im-bad", bad)])
      assert status == 422, "#{bad}: #{inspect(body)}"
      assert body["error"] =~ "im-bad", inspect(body)
      assert body["error"] =~ shown, inspect(body)
      assert {:error, :not_found} = Nodes.get_node_by_change_id(ws.id, "im-ok")
    end
  end

  test "metadata_json that is not a JSON object is refused, not stored as %{}", %{client: client} do
    for bad <- ["not json", "[1]", ~s("x")] do
      {status, body} = push(client, [node("im-x", bad)])
      assert status == 422, "#{bad}: #{inspect(body)}"
      assert body["error"] =~ "im-x"
    end
  end

  test "a valid confidence and a missing one import", %{client: client, ws: ws} do
    {200, _} = push(client, [node("im-a", ~s({"confidence":0})), node("im-b", nil), node("im-c", ~s({"confidence":100.0}))])
    assert {:ok, %{metadata: %{"confidence" => 0}}} = Nodes.get_node_by_change_id(ws.id, "im-a")
  end

  # Values stored before this validation existed: a patch that does not
  # touch confidence has to go through, or the node can never be edited
  # again without also rewriting a key the caller never mentioned.
  test "a stored invalid confidence does not block a patch to another key", %{
    client: client,
    ws: ws
  } do
    legacy =
      Repo.insert!(%Node{
        workspace_id: ws.id,
        change_id: "im-legacy",
        node_type: "goal",
        title: "legacy",
        metadata: %{"confidence" => "high", "prompt" => "keep me"}
      })

    assert %{} =
             McpClient.call!(client, "update_node", %{
               "node_id" => legacy.id,
               "metadata" => %{"files" => "a"},
               "branch" => "im"
             })

    assert %{metadata: %{"files" => "a", "confidence" => "high", "prompt" => "keep me"}} =
             Repo.get!(Node, legacy.id)

    # Setting it to another bad value is still refused.
    assert {:error, message} =
             McpClient.call(client, "update_node", %{
               "node_id" => legacy.id,
               "metadata" => %{"confidence" => "higher"},
               "branch" => "im"
             })

    assert message =~ "confidence"
  end
end
