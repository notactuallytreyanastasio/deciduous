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

  # A delete says what the node held; `create/3`'s defaults unless changed.
  defp delete(cid, was, meta \\ %{"branch" => "main"}) do
    %{
      op_id: Ecto.UUID.generate(),
      kind: "delete_node",
      change_id: cid,
      was: Map.merge(%{"title" => cid, "description" => nil, "status" => "pending"}, was),
      was_metadata: meta
    }
  end

  defp link(from, to, made) do
    %{
      op_id: Ecto.UUID.generate(),
      kind: "create_edge",
      from_change_id: from,
      to_change_id: to,
      edge_type: "leads_to",
      created_at: made
    }
  end

  defp unlink(from, to, at) do
    %{
      op_id: Ecto.UUID.generate(),
      kind: "delete_edge",
      from_change_id: from,
      to_change_id: to,
      edge_type: "leads_to",
      at: at
    }
  end

  defp export(token, ws) do
    conn =
      conn(:get, "/export?workspace=" <> ws)
      |> put_req_header("authorization", "Bearer " <> token)
      |> Router.call(@opts)

    Jason.decode!(conn.resp_body)
  end

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
      rationale: "why",
      created_at: "2016-02-01T00:00:01-05:00"
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
      edge_type: "leads_to",
      at: "2016-02-01T00:00:02-05:00"
    }

    {200, %{"results" => [r]}} = ops(token, "ops-edge", [del])
    assert r["result"] == "applied"
    assert Repo.aggregate(from(e in Edge, where: e.workspace_id == ^ws.id), :count) == 0

    {200, %{"results" => [r]}} =
      ops(token, "ops-edge", [
        delete("e2", %{"title" => "e2"})
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

  # SERVER-N1: an op carrying a NUL, an over-long id or a kind that is not a
  # string raised inside Repo.transaction (Postgrex 22021, a varchar(255)
  # overflow, Protocol.UndefinedError), and the whole request answered an
  # empty 500. The CLI resent the batch on every write, got the same 500, and
  # nothing after the bad op ever reached the server. Each is an answer about
  # one op: rejected, with the reason, and the ops around it applied.
  test "server_n1: a poisoned op is rejected alone and the ops around it are applied",
       %{token: token} do
    nul = "has" <> <<0>> <> "nul"
    long = String.duplicate("x", 256)

    poisons = [
      {"nul in metadata", create("p1", "p1", %{metadata: %{"prompt" => nul}})},
      {"nul in a metadata key", create("p2", "p2", %{metadata: %{nul => "v"}})},
      {"nul in the title", create("p3", nul)},
      {"op_id over 255", create("p4", "p4", %{op_id: long})},
      {"op_id with nul", create("p5", "p5", %{op_id: "id" <> nul})},
      {"kind an object", %{op_id: Ecto.UUID.generate(), kind: %{"a" => 1}, change_id: "p6"}},
      {"kind over 255", %{op_id: Ecto.UUID.generate(), kind: long, change_id: "p7"}},
      {"change_id over 255", create(long, "p8")},
      {"change_id with nul", create("p9" <> nul, "p9")},
      {"nul in set.title",
       %{
         op_id: Ecto.UUID.generate(),
         kind: "update_node",
         change_id: "a",
         set: %{title: nul},
         was: %{title: "a"}
       }},
      {"nul in a rationale",
       %{
         op_id: Ecto.UUID.generate(),
         kind: "create_edge",
         from_change_id: "a",
         to_change_id: "c",
         edge_type: "leads_to",
         rationale: nul
       }}
    ]

    for {label, poison} <- poisons do
      ws = "ops-n1-" <> Integer.to_string(System.unique_integer([:positive]))
      {200, _} = ops(token, ws, [create("a", "a"), create("c", "c")])
      after_op = create("after", "after")
      {status, body} = ops(token, ws, [poison, after_op])
      assert status == 200, "#{label}: #{status} #{inspect(body)}"
      [r, after_result] = body["results"]
      assert r["result"] == "rejected", "#{label}: #{inspect(r)}"
      assert is_binary(r["reason"]) and r["reason"] != "", "#{label}: #{inspect(r)}"
      assert after_result["result"] == "applied", "#{label}: #{inspect(after_result)}"
      # Sent again, the same answer: no 500 the second time either.
      {200, %{"results" => [again, dup]}} = ops(token, ws, [poison, after_op])
      assert again["result"] == "rejected", "#{label} again: #{inspect(again)}"
      assert dup["result"] == "duplicate"
    end
  end

  # NEW (low), adversarial round 2: an integer weight too large for a
  # float raised ArgumentError while the edge was cast, and the whole
  # request answered an empty 500. 1e300 and 1.7976931348623157e308 were
  # applied; 10**400 was not. The Rust client sends an f64 and cannot send
  # it; any other /ops client can.
  test "new: a weight too large for a float is rejected alone", %{token: token} do
    ws = "ops-weight-" <> Integer.to_string(System.unique_integer([:positive]))
    {200, _} = ops(token, ws, [create("a", "a"), create("c", "c")])

    huge = %{
      op_id: Ecto.UUID.generate(),
      kind: "create_edge",
      from_change_id: "a",
      to_change_id: "c",
      edge_type: "leads_to",
      weight: Integer.pow(10, 400),
      created_at: "2016-02-01T00:00:01-05:00"
    }

    {status, body} = ops(token, ws, [huge, create("after", "after")])
    assert status == 200, "#{status} #{inspect(body)}"
    [r, after_result] = body["results"]
    assert r["result"] == "rejected", inspect(r)
    assert r["reason"] =~ "weight", inspect(r)
    assert after_result["result"] == "applied"

    fine = %{huge | op_id: Ecto.UUID.generate(), weight: 1.7976931348623157e308}
    {200, %{"results" => [ok]}} = ops(token, ws, [fine])
    assert ok["result"] == "applied", inspect(ok)
  end

  # NEW (low): the reason for a NUL in a metadata key printed the key as an
  # Elixir binary, "at metadata.<<97, 0, 98>>".
  test "new: a NUL in a metadata key is named as text", %{token: token} do
    ws = "ops-nulkey-" <> Integer.to_string(System.unique_integer([:positive]))
    key = "a" <> <<0>> <> "b"
    {200, %{"results" => [r]}} = ops(token, ws, [create("k", "k", %{metadata: %{key => "v"}})])
    assert r["result"] == "rejected"
    refute r["reason"] =~ "<<", r["reason"]
    assert r["reason"] =~ ~S(metadata."a\0b"), r["reason"]
  end

  # --- round 2: every op carries what it was made against -------------------

  test "new_2: a delete made before a newer edit is refused, and the node stays",
       %{token: token} do
    ws = "ops-new2-" <> Integer.to_string(System.unique_integer([:positive]))
    {200, _} = ops(token, ws, [create("c", "C")])
    w = Repo.get_by!(DeciduousMcp.Schema.Workspace, name: ws)
    # Bob's edit reached the server first; alice's delete was made before it.
    {:ok, _} = Nodes.update_node(node(w, "c").id, %{status: "completed"})

    {200, %{"results" => [r]}} = ops(token, ws, [delete("c", %{"title" => "C"})])
    assert r["result"] == "rejected", inspect(r)

    assert r["reason"] =~
             ~s(status: the server has "completed", this copy deleted it at "pending")

    assert is_nil(node(w, "c").deleted_at)

    # A delete made over what the server holds goes through.
    {200, %{"results" => [r]}} =
      ops(token, ws, [delete("c", %{"title" => "C", "status" => "completed"})])

    assert r["result"] == "applied", inspect(r)
    refute is_nil(node(w, "c").deleted_at)
  end

  # Verification of chapter 29 (delete first, edit later): alice deleted C
  # online, bob, offline and unaware, set its status 1.1 s later. git keeps
  # bob's edit; the server refused it as an edit to a deleted node, and the
  # next pulls deleted his edit on every clone.
  defp edit_at(cid, at, set, was) do
    %{
      op_id: Ecto.UUID.generate(),
      kind: "update_node",
      change_id: cid,
      at: at,
      set: set,
      was: was
    }
  end

  defp iso(dt), do: DateTime.to_iso8601(dt)

  test "log_conflicts_delete_first: an edit made after an agent's delete brings the node back",
       %{token: token} do
    ws = "ops-dfirst-" <> Integer.to_string(System.unique_integer([:positive]))
    {200, _} = ops(token, ws, [create("c", "C")])
    w = Repo.get_by!(DeciduousMcp.Schema.Workspace, name: ws)
    {:ok, _} = Nodes.delete_node(node(w, "c").id)
    later = DateTime.add(DateTime.utc_now(), 1, :second)

    {200, %{"results" => [r]}} =
      ops(token, ws, [edit_at("c", iso(later), %{status: "completed"}, %{status: "pending"})])

    assert r["result"] == "applied", inspect(r)
    assert is_nil(node(w, "c").deleted_at)
    assert node(w, "c").status == "completed"
    assert node(w, "c").title == "C"
  end

  test "log_conflicts_delete_first: an edit made before the delete stays refused, with both times",
       %{token: token} do
    ws = "ops-dfirst2-" <> Integer.to_string(System.unique_integer([:positive]))
    {200, _} = ops(token, ws, [create("c", "C")])
    w = Repo.get_by!(DeciduousMcp.Schema.Workspace, name: ws)
    earlier = DateTime.add(DateTime.utc_now(), -5, :second)
    {:ok, _} = Nodes.delete_node(node(w, "c").id)

    {200, %{"results" => [r]}} =
      ops(token, ws, [edit_at("c", iso(earlier), %{status: "completed"}, %{status: "pending"})])

    assert r["result"] == "rejected", inspect(r)
    assert r["reason"] =~ "was deleted on the server at"
    assert r["reason"] =~ iso(DateTime.truncate(earlier, :microsecond))
    refute is_nil(node(w, "c").deleted_at)
  end

  test "log_conflicts_delete_first: an offline delete is dated when it was made, not when it arrived",
       %{token: token} do
    ws = "ops-dfirst3-" <> Integer.to_string(System.unique_integer([:positive]))
    {200, _} = ops(token, ws, [create("c", "C")])
    w = Repo.get_by!(DeciduousMcp.Schema.Workspace, name: ws)
    made = DateTime.add(DateTime.utc_now(), -60, :second)
    edited = DateTime.add(made, 30, :second)

    {200, %{"results" => [r]}} =
      ops(token, ws, [Map.put(delete("c", %{"title" => "C"}), :at, iso(made))])

    assert r["result"] == "applied", inspect(r)
    assert DateTime.compare(node(w, "c").deleted_at, DateTime.truncate(made, :microsecond)) == :eq

    # Bob's edit was made after the delete and reached the server after it.
    {200, %{"results" => [r]}} =
      ops(token, ws, [edit_at("c", iso(edited), %{status: "completed"}, %{status: "pending"})])

    assert r["result"] == "applied", inspect(r)
    assert node(w, "c").status == "completed"
    assert is_nil(node(w, "c").deleted_at)
  end

  test "log_conflicts_delete_first: an edit does not bring back a node the server never had",
       %{token: token} do
    ws = "ops-dfirst4-" <> Integer.to_string(System.unique_integer([:positive]))
    {200, %{"results" => [r]}} = ops(token, ws, [delete("t", %{"title" => "T"})])
    assert r["result"] == "absent"
    later = DateTime.add(DateTime.utc_now(), 1, :second)

    {200, %{"results" => [r]}} =
      ops(token, ws, [edit_at("t", iso(later), %{status: "completed"}, %{status: "pending"})])

    assert r["result"] == "rejected", inspect(r)
    assert r["reason"] =~ "never held"
    w = Repo.get_by!(DeciduousMcp.Schema.Workspace, name: ws)
    refute is_nil(node(w, "t").deleted_at)
  end

  test "new_2: a delete that does not say what it deleted is refused", %{token: token} do
    ws = "ops-new2b-" <> Integer.to_string(System.unique_integer([:positive]))
    {200, _} = ops(token, ws, [create("c", "C")])

    {200, %{"results" => [r]}} =
      ops(token, ws, [%{op_id: Ecto.UUID.generate(), kind: "delete_node", change_id: "c"}])

    assert r["result"] == "rejected"
    assert r["reason"] =~ "does not say what the node held"
  end

  test "new_2: a metadata key changed on the server stops a delete", %{token: token} do
    ws = "ops-new2c-" <> Integer.to_string(System.unique_integer([:positive]))
    {200, _} = ops(token, ws, [create("c", "C")])
    w = Repo.get_by!(DeciduousMcp.Schema.Workspace, name: ws)

    {:ok, _} =
      Nodes.update_node(node(w, "c").id, %{metadata: %{"branch" => "main", "prompt" => "p"}})

    {200, %{"results" => [r]}} = ops(token, ws, [delete("c", %{"title" => "C"})])
    assert r["result"] == "rejected"
    assert r["reason"] =~ ~s(metadata.prompt: the server has "p", this copy deleted it at nil)
  end

  test "new_3: a delete of a node the server never had leaves a tombstone the create meets",
       %{token: token} do
    ws = "ops-new3n-" <> Integer.to_string(System.unique_integer([:positive]))
    {200, %{"results" => [r]}} = ops(token, ws, [delete("t", %{"title" => "T"})])
    assert r["result"] == "absent"

    {200, %{"results" => [r]}} = ops(token, ws, [create("t", "T")])
    assert r["result"] == "rejected"
    assert r["reason"] =~ "was deleted on the server"
    w = Repo.get_by!(DeciduousMcp.Schema.Workspace, name: ws)
    refute is_nil(node(w, "t").deleted_at)
  end

  test "new_3: an unlink leaves a tombstone; a link made before it is refused, one after applied",
       %{token: token} do
    ws = "ops-new3e-" <> Integer.to_string(System.unique_integer([:positive]))
    {200, _} = ops(token, ws, [create("a", "a"), create("b", "b")])

    # Bob unlinks an edge the server never had (it reached him through git).
    {200, %{"results" => [r]}} = ops(token, ws, [unlink("a", "b", "2016-02-01T00:00:05Z")])
    assert r["result"] == "absent"

    # Alice's link, made before the unlink, replayed after it.
    {200, %{"results" => [r]}} = ops(token, ws, [link("a", "b", "2016-02-01T00:00:03Z")])
    assert r["result"] == "rejected", inspect(r)
    assert r["reason"] =~ "was unlinked on the server"

    [t] = export(token, ws)["edge_tombstones"]
    assert t["from_change_id"] == "a" and t["deleted_at"] =~ "2016-02-01T00:00:05"

    # A link made after the unlink is a relink, and removes the tombstone.
    {200, %{"results" => [r]}} = ops(token, ws, [link("a", "b", "2016-02-01T00:00:09Z")])
    assert r["result"] == "applied", inspect(r)
    assert export(token, ws)["edge_tombstones"] == []
  end

  test "new_3: an unlink older than the edge the server holds is refused", %{token: token} do
    ws = "ops-new3r-" <> Integer.to_string(System.unique_integer([:positive]))
    {200, _} = ops(token, ws, [create("a", "a"), create("b", "b")])

    {200, [_]} =
      then(ops(token, ws, [link("a", "b", "2016-02-01T00:00:09Z")]), fn {s, b} ->
        {s, b["results"]}
      end)

    {200, %{"results" => [r]}} = ops(token, ws, [unlink("a", "b", "2016-02-01T00:00:05Z")])
    assert r["result"] == "rejected", inspect(r)
    assert r["reason"] =~ "was linked again on the server"
    assert length(export(token, ws)["edges"]) == 1

    {200, %{"results" => [r]}} = ops(token, ws, [unlink("a", "b", "2016-02-01T00:00:10Z")])
    assert r["result"] == "applied"
    assert export(token, ws)["edges"] == []
    [t] = export(token, ws)["edge_tombstones"]
    assert t["deleted_at"] =~ "2016-02-01T00:00:10"
  end

  test "stack: an agent's unlink is in /export as an edge tombstone", %{token: token} do
    ws = "ops-stack-" <> Integer.to_string(System.unique_integer([:positive]))
    {200, _} = ops(token, ws, [create("a", "a"), create("b", "b")])
    {200, _} = ops(token, ws, [link("a", "b", "2016-02-01T00:00:01Z")])
    w = Repo.get_by!(DeciduousMcp.Schema.Workspace, name: ws)
    {:ok, _} = DeciduousMcp.Graph.Edges.delete_edge(node(w, "a").id, node(w, "b").id)

    [t] = export(token, ws)["edge_tombstones"]
    assert {t["from_change_id"], t["to_change_id"], t["edge_type"]} == {"a", "b", "leads_to"}
  end

  test "bridge_n4: attach, describe and detach reach the server as ops", %{token: token} do
    ws = "ops-n4-" <> Integer.to_string(System.unique_integer([:positive]))
    {200, _} = ops(token, ws, [create("n", "n")])
    hash = :crypto.hash(:sha256, "secret") |> Base.encode16(case: :lower)

    attach = %{
      op_id: Ecto.UUID.generate(),
      kind: "attach_document",
      change_id: "d1",
      node_change_id: "n",
      content_hash: hash,
      original_filename: "secret.txt",
      storage_filename: hash <> ".txt",
      mime_type: "text/plain",
      file_size: 6,
      description: nil,
      description_source: "none",
      attached_at: "2016-02-01T00:00:00Z"
    }

    {200, %{"results" => [r]}} = ops(token, ws, [attach])
    assert r["result"] == "applied", inspect(r)
    [d] = export(token, ws)["documents"]
    assert d["original_filename"] == "secret.txt"

    describe = fn new, was ->
      %{
        op_id: Ecto.UUID.generate(),
        kind: "describe_document",
        change_id: "d1",
        description: new,
        description_source: "user",
        was_description: was
      }
    end

    {200, %{"results" => [r]}} = ops(token, ws, [describe.("first", nil)])
    assert r["result"] == "applied", inspect(r)
    {200, %{"results" => [r]}} = ops(token, ws, [describe.("stale", nil)])
    assert r["result"] == "rejected"
    assert r["reason"] =~ ~s(the server has "first")

    detach = %{
      op_id: Ecto.UUID.generate(),
      kind: "detach_document",
      change_id: "d1",
      at: "2016-02-01T00:00:09Z"
    }

    {200, %{"results" => [r]}} = ops(token, ws, [detach])
    assert r["result"] == "applied", inspect(r)
    assert export(token, ws)["documents"] == []

    {200, %{"results" => [r]}} =
      ops(token, ws, [%{attach | op_id: Ecto.UUID.generate()}])

    assert r["result"] == "rejected"
    assert r["reason"] =~ "was detached on the server"
  end
end
