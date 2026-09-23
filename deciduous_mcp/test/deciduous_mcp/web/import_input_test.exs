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

  test "an imported node with an empty title can still be updated and deleted" do
    cid = Ecto.UUID.generate()
    assert {200, _} = import!("import-blank-title", [gnode(%{"change_id" => cid, "title" => ""})])

    {:ok, ws} = Workspaces.get_by_name("import-blank-title")

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
end
