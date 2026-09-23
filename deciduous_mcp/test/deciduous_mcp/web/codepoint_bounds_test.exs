defmodule DeciduousMcp.Web.CodepointBoundsTest do
  @moduledoc """
  Verification of SERVER-N2, N3 and N6 (chapter 30). The length bounds
  counted graphemes (String.length), while varchar(255) counts codepoints
  and a btree index row is capped at 8191 bytes. "a" followed by 300
  combining acute accents is one grapheme and 301 codepoints: it passed
  every check and the insert failed, an empty HTTP 500 on /ops and
  /import and "add_node failed (Postgrex.Error)" over MCP. A branch of 500
  graphemes each carrying 20 combining marks (10,500 codepoints) failed
  the index on write_locks.

  And /import that wrote nothing created its workspace and recorded its
  repo_roots as the claim, which /ops no longer does.
  """
  use DeciduousMcp.DataCase, async: false

  import Plug.Test
  import Plug.Conn

  alias DeciduousMcp.Graph.Workspaces
  alias DeciduousMcp.Test.McpHttp
  alias DeciduousMcp.Web.Router

  @opts Router.init([])
  @a "1111111111111111111111111111111111111111"
  @b "3333333333333333333333333333333333333333"

  # 1 grapheme, 301 codepoints.
  @stacked "a" <> String.duplicate(<<0x0301::utf8>>, 300)

  defp token, do: Application.fetch_env!(:deciduous_mcp, :api_token)

  defp post(path, body) do
    conn =
      conn(:post, path, Jason.encode!(body))
      |> put_req_header("authorization", "Bearer " <> token())
      |> put_req_header("content-type", "application/json")
      |> Router.call(@opts)

    {conn.status, conn.resp_body}
  end

  defp create_op(cid, extra \\ %{}) do
    Map.merge(
      %{
        op_id: Ecto.UUID.generate(),
        kind: "create_node",
        change_id: cid,
        node_type: "goal",
        title: "t",
        status: "pending"
      },
      extra
    )
  end

  defp exists?(name), do: match?({:ok, _}, Workspaces.get_by_name(name))

  test "SERVER-N3: an op_id of 1 grapheme and 301 codepoints is refused by name" do
    op = %{create_op("n3-op") | op_id: @stacked}
    # Refused per op, as chapter 28 refuses every op the database cannot
    # store, so the ops queued after it still apply.
    assert {200, body} = post("/ops", %{workspace: "n3-opid", ops: [op]})
    assert %{"results" => [%{"result" => "rejected", "reason" => reason}]} = Jason.decode!(body)
    assert reason =~ "op_id is 301 characters; the limit is 255"
    refute exists?("n3-opid")
  end

  test "SERVER-N3/N6: a change_id of 301 codepoints is rejected on /ops and leaves no workspace" do
    assert {200, body} = post("/ops", %{workspace: "n6-ghost", ops: [create_op(@stacked)]})
    assert %{"results" => [%{"result" => "rejected", "reason" => reason}]} = Jason.decode!(body)
    assert reason =~ "change_id is 301 characters; the limit is 255"
    refute exists?("n6-ghost")
  end

  test "SERVER-N3: add_node refuses a change_id of 301 codepoints by name" do
    sid = McpHttp.session()

    assert {:tool_error, message} =
             McpHttp.call(sid, "add_node", %{
               "workspace" => "n3-mcp",
               "node_type" => "goal",
               "title" => "t",
               "change_id" => @stacked
             })

    assert message =~ "change_id is 301 characters; the limit is 255"
    refute message =~ "Postgrex"
  end

  test "SERVER-N3: /import refuses a change_id of 301 codepoints and writes nothing" do
    assert {status, body} =
             post("/import", %{
               workspace: "n3-import",
               graph: %{nodes: [%{change_id: @stacked, node_type: "goal", title: "t"}], edges: []}
             })

    assert status in [400, 422], body
    assert body =~ "change_id is 301 characters; the limit is 255"
    refute exists?("n3-import")
  end

  test "SERVER-N3: a workspace name of 1 grapheme and 301 codepoints is refused by name" do
    assert {422, body} = post("/ops", %{workspace: @stacked, ops: [create_op("x")]})
    assert body =~ "invalid workspace name"
  end

  test "SERVER-N2: a branch of 500 graphemes and 10,500 codepoints is refused, not a Postgrex error" do
    branch = String.duplicate("a" <> String.duplicate(<<0x0301::utf8>>, 20), 500)
    sid = McpHttp.session()

    assert {:tool_error, message} =
             McpHttp.call(sid, "add_node", %{
               "workspace" => "n2-cp",
               "node_type" => "goal",
               "title" => "t",
               "branch" => branch
             })

    assert message =~ "branch is 10500 characters; the limit is 512"

    assert {200, body} =
             post("/ops", %{
               workspace: "n2-cp",
               ops: [create_op("n2", %{metadata: %{branch: branch}})]
             })

    assert %{"results" => [%{"result" => "rejected", "reason" => reason}]} = Jason.decode!(body)
    assert reason =~ "branch is 10500 characters"

    # The session still writes.
    assert {:ok, %{"id" => _}} =
             McpHttp.call(sid, "add_node", %{
               "workspace" => "n2-cp",
               "node_type" => "goal",
               "title" => "t",
               "branch" => "main"
             })
  end

  test "SERVER-N6 import: an /import that writes nothing creates no workspace and claims none" do
    empty = %{nodes: [], edges: []}

    assert {200, _} = post("/import", %{workspace: "n6-imp-new", graph: empty, repo_roots: [@a]})
    refute exists?("n6-imp-new")

    {:ok, ws} = Workspaces.find_or_create("n6-imp")
    {:ok, _} = DeciduousMcp.Graph.Nodes.create_node(ws.id, %{node_type: "goal", title: "by mcp"})

    assert {200, _} = post("/import", %{workspace: "n6-imp", graph: empty, repo_roots: [@a]})
    {:ok, ws} = Workspaces.get_by_name("n6-imp")
    assert (ws.settings || %{})["repo_roots"] in [nil, []], inspect(ws.settings)

    # The real repository writes, and claims.
    assert {200, _} =
             post("/ops", %{workspace: "n6-imp", ops: [create_op("real")], repo_roots: [@b]})

    {:ok, ws} = Workspaces.get_by_name("n6-imp")
    assert ws.settings["repo_roots"] == [@b]

    # An import that writes is still refused from another repository.
    assert {409, _} =
             post("/import", %{
               workspace: "n6-imp",
               graph: %{nodes: [%{change_id: "z", node_type: "goal", title: "z"}], edges: []},
               repo_roots: [@a]
             })
  end
end
