defmodule DeciduousMcp.Web.DocumentsPinTest do
  @moduledoc """
  S2 bypass: GET /documents/:id ran no WorkspacePlug and no Scope check, and
  took a content hash as well as a UUID. A client pinned to A read O's
  attachment bytes by id or by hash. Over the router, because the pin is a
  header.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.{Nodes, Workspaces}
  alias DeciduousMcp.Schema.Document
  alias DeciduousMcp.Storage
  alias DeciduousMcp.Test.McpClient

  @secret "SECRET DOCUMENT BYTES dp-o"

  setup do
    {:ok, _a} = Workspaces.find_or_create("dp-a")
    {:ok, o} = Workspaces.find_or_create("dp-o")
    {:ok, node} = Nodes.create_node(o.id, %{node_type: "goal", title: "O goal"})
    hash = Storage.hash(@secret)
    :ok = Storage.put(hash, @secret, mime_type: "text/plain")
    doc = attach(o.id, node.id, hash, "o-doc")
    %{o: o, node: node, hash: hash, doc: doc}
  end

  defp attach(workspace_id, node_id, hash, change_id) do
    Repo.insert!(%Document{
      workspace_id: workspace_id,
      node_id: node_id,
      change_id: change_id,
      content_hash: hash,
      original_filename: "secret.txt",
      storage_filename: hash <> ".txt",
      mime_type: "text/plain",
      file_size: byte_size(@secret),
      description_source: "none",
      storage: "postgres"
    })
  end

  defp fetch(client, id), do: McpClient.get(client, "/documents/" <> id)

  test "a client pinned elsewhere cannot read the document by id or by hash", %{
    doc: doc,
    hash: hash
  } do
    pinned = McpClient.connect(pin: "dp-a")

    for id <- [doc.id, hash, String.upcase(hash)] do
      conn = fetch(pinned, id)
      assert conn.status == 404, "#{id}: #{conn.status} #{conn.resp_body}"
      refute conn.resp_body =~ "SECRET"
    end
  end

  test "a client pinned to the document's workspace reads it", %{doc: doc, hash: hash} do
    pinned = McpClient.connect(pin: "dp-o")
    assert fetch(pinned, doc.id).resp_body == @secret
    assert fetch(pinned, hash).resp_body == @secret
  end

  test "the same bytes attached in the pinned workspace are served by hash", %{hash: hash} do
    {:ok, a} = Workspaces.find_or_create("dp-a")
    {:ok, mine} = Nodes.create_node(a.id, %{node_type: "goal", title: "A goal"})
    attach(a.id, mine.id, hash, "a-doc")

    assert fetch(McpClient.connect(pin: "dp-a"), hash).status == 200
  end

  test "an unpinned client reads any workspace's document", %{doc: doc} do
    assert fetch(McpClient.connect(), doc.id).status == 200
  end

  # A delete hides a node's content from every read; its attachments are
  # its content too.
  test "a document on a deleted node is not served", %{node: node, doc: doc, hash: hash} do
    {:ok, _} = Nodes.delete_node(node.id)

    for id <- [doc.id, hash] do
      conn = fetch(McpClient.connect(), id)
      assert conn.status == 404, "#{id}: #{conn.status}"
      refute conn.resp_body =~ "SECRET"
    end
  end
end
