defmodule DeciduousMcp.Web.OpsEndpointTest do
  @moduledoc """
  `POST /ops` applies a CLI's queued operations one field at a time, and
  applies each one at most once.

  The endpoint exists because `POST /import` replaces title, status,
  description and metadata on every node it is sent. The CLI used it to carry
  single edits, so `deciduous status 2 completed` put back the title an agent
  had changed a minute earlier (bridge finding C1). An op names the fields it
  changed and nothing else, and carries an id so a replay after a lost ack
  changes nothing.
  """
  use DeciduousMcp.DataCase, async: false

  import Plug.Test
  import Plug.Conn

  alias DeciduousMcp.Graph.{Nodes, Workspaces}
  alias DeciduousMcp.Schema.{Edge, Node}
  alias DeciduousMcp.Web.Router

  @opts Router.init([])

  setup do
    %{token: Application.fetch_env!(:deciduous_mcp, :api_token)}
  end

  defp post_json(token, path, body) do
    conn(:post, path, Jason.encode!(body))
    |> put_req_header("authorization", "Bearer " <> token)
    |> put_req_header("content-type", "application/json")
    |> Router.call(@opts)
  end

  defp ops(token, workspace, ops, extra \\ %{}) do
    conn = post_json(token, "/ops", Map.merge(%{workspace: workspace, ops: ops}, extra))
    {conn.status, Jason.decode!(conn.resp_body)}
  end

  defp create(cid, title, extra \\ %{}) do
    Map.merge(
      %{
        op_id: Ecto.UUID.generate(),
        kind: "create_node",
        change_id: cid,
        node_type: "goal",
        title: title,
        status: "pending",
        metadata: %{"branch" => "main"},
        created_at: "2016-02-01T00:00:00-05:00",
        updated_at: "2016-02-01T00:00:00-05:00"
      },
      extra
    )
  end

  defp node(ws, cid), do: Repo.get_by!(Node, workspace_id: ws.id, change_id: cid)

  test "a status op changes the status and leaves an agent's title and description alone",
       %{token: token} do
    {200, _} = ops(token, "ops-c1", [create("a1", "a1")])
    ws = Repo.get_by!(DeciduousMcp.Schema.Workspace, name: "ops-c1")

    # An agent edits the node through MCP between the CLI's two writes.
    {:ok, _} =
      Nodes.update_node(node(ws, "a1").id, %{
        title: "a1 retitled by agent",
        description: "agent detail"
      })

    {200, %{"results" => [r]}} =
      ops(token, "ops-c1", [
        %{
          op_id: Ecto.UUID.generate(),
          kind: "update_node",
          change_id: "a1",
          set: %{status: "completed"},
          was: %{status: "pending"}
        }
      ])

    assert r["result"] == "applied"
    n = node(ws, "a1")
    assert n.status == "completed"
    assert n.title == "a1 retitled by agent"
    assert n.description == "agent detail"
  end

  test "a metadata op merges its keys into what the server holds", %{token: token} do
    {200, _} = ops(token, "ops-meta", [create("m1", "m1")])
    ws = Repo.get_by!(DeciduousMcp.Schema.Workspace, name: "ops-meta")

    {:ok, _} =
      Nodes.update_node(node(ws, "m1").id, %{metadata: %{"branch" => "main", "confidence" => 70}})

    {200, _} =
      ops(token, "ops-meta", [
        %{
          op_id: Ecto.UUID.generate(),
          kind: "update_node",
          change_id: "m1",
          metadata: %{prompt: "the words"},
          was_metadata: %{prompt: nil}
        }
      ])

    assert node(ws, "m1").metadata == %{
             "branch" => "main",
             "confidence" => 70,
             "prompt" => "the words"
           }
  end

  test "replaying the same op id applies it once", %{token: token} do
    {200, _} = ops(token, "ops-dup", [create("d1", "d1")])
    ws = Repo.get_by!(DeciduousMcp.Schema.Workspace, name: "ops-dup")

    op = %{
      op_id: Ecto.UUID.generate(),
      kind: "update_node",
      change_id: "d1",
      set: %{status: "completed"},
      was: %{status: "pending"}
    }

    {200, %{"results" => [first]}} = ops(token, "ops-dup", [op])
    assert first["result"] == "applied"

    # Someone changes the status after the op landed; the CLI never saw the
    # ack and sends the op again. The replay must not undo the later edit.
    {:ok, _} = Nodes.update_node(node(ws, "d1").id, %{status: "abandoned"})
    {200, %{"results" => [again]}} = ops(token, "ops-dup", [op])

    assert again["result"] == "duplicate"
    assert node(ws, "d1").status == "abandoned"
  end

  test "create keeps the CLI's created_at, and a second create of the same node changes nothing",
       %{token: token} do
    {200, %{"results" => [r]}} = ops(token, "ops-create", [create("c1", "first")])
    assert r["result"] == "applied"
    ws = Repo.get_by!(DeciduousMcp.Schema.Workspace, name: "ops-create")
    assert node(ws, "c1").inserted_at.year == 2016

    {200, %{"results" => [r2]}} = ops(token, "ops-create", [create("c1", "second")])
    assert r2["result"] == "exists"
    assert node(ws, "c1").title == "first"
  end

  test "edges are created and removed by change id, and removal reaches the server",
       %{token: token} do
    {200, _} = ops(token, "ops-edge", [create("e1", "e1"), create("e2", "e2")])
    ws = Repo.get_by!(DeciduousMcp.Schema.Workspace, name: "ops-edge")

    edge = %{
      kind: "create_edge",
      from_change_id: "e1",
      to_change_id: "e2",
      edge_type: "leads_to",
      rationale: "why"
    }

    {200, %{"results" => [r]}} =
      ops(token, "ops-edge", [Map.put(edge, :op_id, Ecto.UUID.generate())])

    assert r["result"] == "applied"
    assert Repo.aggregate(from(e in Edge, where: e.workspace_id == ^ws.id), :count) == 1

    del = %{
      op_id: Ecto.UUID.generate(),
      kind: "delete_edge",
      from_change_id: "e1",
      to_change_id: "e2",
      edge_type: "leads_to"
    }

    {200, %{"results" => [r]}} = ops(token, "ops-edge", [del])
    assert r["result"] == "applied"
    assert Repo.aggregate(from(e in Edge, where: e.workspace_id == ^ws.id), :count) == 0

    {200, %{"results" => [r]}} =
      ops(token, "ops-edge", [
        %{op_id: Ecto.UUID.generate(), kind: "delete_node", change_id: "e2"}
      ])

    assert r["result"] == "applied"
    refute is_nil(node(ws, "e2").deleted_at)
  end

  test "an edit to a node an agent deleted is rejected with the reason, not applied",
       %{token: token} do
    {200, _} = ops(token, "ops-dead", [create("z1", "z1")])
    ws = Repo.get_by!(DeciduousMcp.Schema.Workspace, name: "ops-dead")
    {:ok, _} = Nodes.delete_node(node(ws, "z1").id)

    {200, %{"results" => [r]}} =
      ops(token, "ops-dead", [
        %{
          op_id: Ecto.UUID.generate(),
          kind: "update_node",
          change_id: "z1",
          set: %{status: "completed"},
          was: %{status: "pending"}
        }
      ])

    assert r["result"] == "rejected"
    assert r["reason"] =~ "deleted on the server"
    assert node(ws, "z1").status == "pending"
  end

  test "an unknown op kind or field is rejected by name", %{token: token} do
    {200, _} = ops(token, "ops-unknown", [create("u1", "u1")])

    {200, %{"results" => [a, b]}} =
      ops(token, "ops-unknown", [
        %{op_id: Ecto.UUID.generate(), kind: "rename_everything", change_id: "u1"},
        %{
          op_id: Ecto.UUID.generate(),
          kind: "update_node",
          change_id: "u1",
          set: %{colour: "red"},
          was: %{colour: nil}
        }
      ])

    assert a["result"] == "rejected" and a["reason"] =~ "rename_everything"
    assert b["result"] == "rejected" and b["reason"] =~ "colour"
  end

  test "an edit replayed after someone changed the same field is refused, not applied",
       %{token: token} do
    {200, _} = ops(token, "ops-cas", [create("s1", "s1"), create("s2", "s2")])
    ws = Repo.get_by!(DeciduousMcp.Schema.Workspace, name: "ops-cas")

    # An agent changes s1's status, and only s2's title, after the CLI made
    # its (queued) edits and before they arrive.
    {:ok, _} = Nodes.update_node(node(ws, "s1").id, %{status: "rejected"})
    {:ok, _} = Nodes.update_node(node(ws, "s2").id, %{title: "s2 retitled"})

    status = fn cid ->
      %{
        op_id: Ecto.UUID.generate(),
        kind: "update_node",
        change_id: cid,
        set: %{status: "completed"},
        was: %{status: "pending"}
      }
    end

    {200, %{"results" => [a, b]}} = ops(token, "ops-cas", [status.("s1"), status.("s2")])

    assert a["result"] == "rejected"
    assert a["reason"] =~ "changed on the server"
    assert a["reason"] =~ ~s(the server has "rejected")
    assert node(ws, "s1").status == "rejected"

    assert b["result"] == "applied"
    assert node(ws, "s2").status == "completed"
    assert node(ws, "s2").title == "s2 retitled"
  end

  test "an update that does not say what it replaced is refused", %{token: token} do
    {200, _} = ops(token, "ops-nowas", [create("w1", "w1")])

    {200, %{"results" => [r]}} =
      ops(token, "ops-nowas", [
        %{
          op_id: Ecto.UUID.generate(),
          kind: "update_node",
          change_id: "w1",
          set: %{status: "completed"}
        }
      ])

    assert r["result"] == "rejected"
    assert r["reason"] =~ "does not say what it replaced"
  end

  test "a batch without op ids is refused whole", %{token: token} do
    {422, body} = ops(token, "ops-noid", [%{kind: "delete_node", change_id: "x"}])
    assert body["error"] =~ "op_id"
  end

  test "ops are refused without a token" do
    conn = conn(:post, "/ops", Jason.encode!(%{workspace: "x", ops: []})) |> Router.call(@opts)
    assert conn.status == 401
  end

  test "workspace lookup is the same normalisation /import uses", %{token: token} do
    {200, %{"workspace" => name}} = ops(token, "Ops-Case", [create("k1", "k1")])
    assert name == "ops-case"
    assert {:ok, _} = Workspaces.normalize_name("Ops-Case")
  end
end
