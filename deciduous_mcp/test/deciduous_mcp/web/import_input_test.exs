defmodule DeciduousMcp.Web.ImportInputTest do
  @moduledoc """
  POST /import refuses a payload it cannot store, by field, before it
  writes anything; and what it does store stays editable.

  Import.validate_nodes checked node_type, status and whether change_id was
  present. Everything else went to `insert_all`, which skips the changeset:
  a NUL in a title, a NUL inside metadata_json, an integer change_id or an
  object title each came back as HTTP 500 with an empty body. A title of ""
  was stored, and once update_changeset required a title, that node could
  no longer be updated, and delete_node crashed on it (CaseClauseError).

  Over real HTTP to the running listener.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.Workspaces
  alias DeciduousMcp.Test.McpHttp

  defp import!(workspace, nodes, extra \\ %{}) do
    body = Jason.encode!(%{workspace: workspace, graph: Map.merge(%{nodes: nodes}, extra)})

    {status, _, resp} =
      McpHttp.request("POST", "/import", body, [{"content-type", "application/json"}])

    {status, resp}
  end

  defp gnode(fields) do
    Map.merge(
      %{"change_id" => Ecto.UUID.generate(), "node_type" => "goal", "title" => "t"},
      fields
    )
  end

  test "values that cannot be stored are refused by field, and nothing is created" do
    cases = [
      {"nul-title", [gnode(%{"title" => "a\u0000b"})], %{}, "title"},
      {"nul-meta", [gnode(%{"metadata_json" => ~s({"x":"\\u0000"})})], %{}, "metadata_json"},
      {"int-cid", [gnode(%{"change_id" => 5})], %{}, "change_id"},
      {"obj-title", [gnode(%{"title" => %{"a" => 1}})], %{}, "title"},
      {"not-a-node", ["x"], %{}, "nodes[0]"},
      {"edge-rationale", [gnode(%{"id" => 1})],
       %{edges: [%{"from_node_id" => 1, "to_node_id" => 1, "rationale" => %{"a" => 1}}]},
       "rationale"},
      {"doc-size", [gnode(%{"change_id" => "c1"})],
       %{
         documents: [
           %{
             "node_change_id" => "c1",
             "change_id" => "d1",
             "content_hash" => String.duplicate("a", 64),
             "original_filename" => "f",
             "storage_filename" => "f",
             "file_size" => "big"
           }
         ]
       }, "file_size"}
    ]

    for {name, nodes, extra, field} <- cases do
      ws = "import-input-" <> name
      {status, resp} = import!(ws, nodes, extra)

      assert status == 422, "#{name}: #{status} #{inspect(resp)}"
      assert resp =~ field, "#{name}: #{resp}"
      assert {:error, :not_found} = Workspaces.get_by_name(ws), "#{name} created #{ws}"
    end
  end

  # /import now refuses a blank title (the SERVER-N3 case below), as /ops
  # and MCP do. Rows it stored before that still exist, and must stay
  # editable; this one is written the way insert_all wrote it.
  test "a node stored with an empty title, as /import used to, can still be updated and deleted" do
    cid = Ecto.UUID.generate()
    {:ok, ws} = Workspaces.find_or_create("import-blank-title")
    now = DateTime.utc_now()

    DeciduousMcp.Repo.insert_all(DeciduousMcp.Schema.Node, [
      %{
        id: Ecto.UUID.generate(),
        workspace_id: ws.id,
        change_id: cid,
        node_type: "goal",
        title: "",
        status: "pending",
        metadata: %{},
        inserted_at: now,
        updated_at: now
      }
    ])

    id =
      DeciduousMcp.Repo.get_by!(DeciduousMcp.Schema.Node, workspace_id: ws.id, change_id: cid).id

    sid = McpHttp.session()

    assert {:ok, %{"status" => "completed"}} =
             McpHttp.call(sid, "update_node", %{"node_id" => id, "status" => "completed"})

    assert {:ok, %{"message" => "Node soft-deleted"}} =
             McpHttp.call(sid, "delete_node", %{"node_id" => id})
  end

  test "update_node still refuses to set a blank title" do
    {:ok, ws} = Workspaces.find_or_create("import-blank-refused")
    {:ok, n} = DeciduousMcp.Graph.Nodes.create_node(ws.id, %{node_type: "goal", title: "keep"})
    sid = McpHttp.session()

    assert {:tool_error, message} =
             McpHttp.call(sid, "update_node", %{"node_id" => n.id, "title" => " "})

    assert message =~ "title"
  end

  test "SERVER-N3 (verification): /import is held to the sizes /ops and MCP are, and nothing is created" do
    cases = [
      {"n3i-title", [gnode(%{"title" => String.duplicate("t", 1_000_000)})], %{},
       "title is 1000000 characters; the limit is 10000"},
      {"n3i-desc", [gnode(%{"description" => String.duplicate("d", 5_000_000)})], %{},
       "description is 5000000 characters; the limit is 262144"},
      {"n3i-blank", [gnode(%{"title" => "  "})], %{}, "title must not be blank"},
      {"n3i-branch",
       [gnode(%{"metadata_json" => Jason.encode!(%{"branch" => String.duplicate("b", 600)})})],
       %{}, "metadata.branch is 600 characters; the limit is 512"},
      {"n3i-rationale", [gnode(%{"id" => 1}), gnode(%{"id" => 2})],
       %{
         edges: [
           %{
             "from_node_id" => 1,
             "to_node_id" => 2,
             "rationale" => String.duplicate("r", 300_000)
           }
         ]
       }, "rationale is 300000 characters; the limit is 262144"}
    ]

    for {ws, nodes, extra, says} <- cases do
      {status, body} = import!(ws, nodes, extra)
      assert status == 422, "#{ws}: #{status} #{String.slice(body, 0, 300)}"
      assert body =~ says, "#{ws}: #{String.slice(body, 0, 300)}"
      assert {:error, :not_found} = Workspaces.get_by_name(ws), ws
    end
  end
end
