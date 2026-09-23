defmodule DeciduousMcp.Web.NewNamePinTest do
  @moduledoc """
  Stacking the two server chapters: the pin guards on /import, /documents
  and list_workspaces read `pinned_workspace_id` and took nil as "not
  pinned", while WorkspacePlug stopped creating the pinned workspace and so
  leaves the id nil for a name nothing has been written to. A client pinned
  to a new name was unpinned for all three. Over the router, because the pin
  is a header.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.{Nodes, Workspaces}
  alias DeciduousMcp.Schema.Document
  alias DeciduousMcp.Storage
  alias DeciduousMcp.Test.McpClient

  @secret "SECRET BYTES np-o"

  setup do
    {:ok, o} = Workspaces.find_or_create("np-o")
    {:ok, goal} = Nodes.create_node(o.id, %{node_type: "goal", title: "their goal"})
    hash = Storage.hash(@secret)
    :ok = Storage.put(hash, @secret, mime_type: "text/plain")

    doc =
      Repo.insert!(%Document{
        workspace_id: o.id,
        node_id: goal.id,
        change_id: "np-doc",
        content_hash: hash,
        original_filename: "secret.txt",
        storage_filename: hash <> ".txt",
        mime_type: "text/plain",
        file_size: byte_size(@secret),
        description_source: "none",
        storage: "postgres"
      })

    %{goal: goal, doc: doc, hash: hash, pinned: McpClient.connect(pin: "np-new")}
  end

  test "an import naming another workspace is refused", %{pinned: pinned, goal: goal} do
    graph = %{
      "nodes" => [%{"change_id" => goal.change_id, "node_type" => "goal", "title" => "HIJACKED"}]
    }

    {status, body} = McpClient.post_json(pinned, "/import", %{"workspace" => "np-o", "graph" => graph})

    assert status == 403, inspect(body)
    assert {:ok, %{title: "their goal"}} = Nodes.get_node(goal.id)
  end

  test "an import naming no workspace creates the pinned one", %{pinned: pinned} do
    graph = %{"nodes" => [%{"change_id" => "np-1", "node_type" => "goal", "title" => "mine"}]}
    {status, body} = McpClient.post_json(pinned, "/import", %{"graph" => graph})

    assert status == 200, inspect(body)
    assert body["workspace"] == "np-new"
  end

  test "another workspace's document is not served", %{pinned: pinned, doc: doc, hash: hash} do
    for id <- [doc.id, hash] do
      conn = McpClient.get(pinned, "/documents/" <> id)
      assert conn.status == 404, "#{id}: #{conn.status}"
      refute conn.resp_body =~ "SECRET"
    end
  end

  test "list_workspaces shows none of the others", %{pinned: pinned} do
    result = McpClient.call!(pinned, "list_workspaces", %{})
    refute inspect(result) =~ "np-o"
  end
end
