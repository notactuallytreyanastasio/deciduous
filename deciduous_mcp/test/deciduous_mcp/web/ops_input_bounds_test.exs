defmodule DeciduousMcp.Web.OpsInputBoundsTest do
  @moduledoc """
  SERVER-N3: POST /ops held a create or an update to none of the bounds
  MCP holds the same write to. A 1,000,000-character title and a
  20,000,000-character description were applied (MCP refuses them with
  "limit is 10000" and "limit is 262144"), and query_nodes then returned
  the megabyte title whole. created_at "99999-..." or 12345 silently
  became the arrival time, and "1900-01-01" was stored.
  """
  use DeciduousMcp.DataCase, async: false

  import Plug.Test
  import Plug.Conn

  alias DeciduousMcp.Schema.Node
  alias DeciduousMcp.Web.Router

  @opts Router.init([])

  defp ops(ops) do
    token = Application.fetch_env!(:deciduous_mcp, :api_token)

    conn =
      conn(:post, "/ops", Jason.encode!(%{workspace: "ops-bounds", ops: ops}))
      |> put_req_header("authorization", "Bearer " <> token)
      |> put_req_header("content-type", "application/json")
      |> Router.call(@opts)

    {conn.status, Jason.decode!(conn.resp_body)}
  end

  defp create(cid, extra) do
    Map.merge(
      %{
        "op_id" => Ecto.UUID.generate(),
        "kind" => "create_node",
        "change_id" => cid,
        "node_type" => "goal",
        "title" => "t",
        "status" => "pending",
        "metadata" => %{"branch" => "main"},
        "created_at" => "2016-02-01T00:00:00-05:00",
        "updated_at" => "2016-02-01T00:00:00-05:00"
      },
      extra
    )
  end

  defp one(op) do
    {200, %{"results" => [r]}} = ops([op])
    r
  end

  defp exists?(cid), do: Repo.get_by(Node, change_id: cid) != nil

  test "SERVER-N3: create_node is held to MCP's sizes, by field" do
    for {cid, extra, says} <- [
          {"n3-title", %{"title" => String.duplicate("t", 1_000_000)},
           "title is 1000000 characters; the limit is 10000"},
          {"n3-desc", %{"description" => String.duplicate("d", 300_000)},
           "description is 300000 characters; the limit is 262144"},
          {"n3-blank", %{"title" => " ​ "}, "title must not be blank"},
          {"n3-branch", %{"metadata" => %{"branch" => String.duplicate("b", 600)}},
           "metadata.branch is 600 characters; the limit is 512"},
          {"n3-meta", %{"metadata" => %{"prompt" => String.duplicate("p", 300_000)}},
           "metadata.prompt is 300000 characters; the limit is 262144"}
        ] do
      r = one(create(cid, extra))
      assert r["result"] == "rejected", "#{cid}: #{inspect(r)}"
      assert r["reason"] =~ says
      assert r["reason"] =~ cid
      refute exists?(cid)
    end
  end

  test "SERVER-N3: update_node set is held to the same sizes and to the status vocabulary" do
    assert %{"result" => "applied"} = one(create("n3-u", %{}))

    for {set, says} <- [
          {%{"title" => String.duplicate("t", 10_001)}, "set.title is 10001 characters"},
          {%{"status" => "done"}, "set.status must be one of"},
          {%{"status" => "bogus"}, "set.status must be one of"}
        ] do
      was = Map.new(set, fn {k, _} -> {k, if(k == "status", do: "pending", else: "t")} end)

      r =
        one(%{
          "op_id" => Ecto.UUID.generate(),
          "kind" => "update_node",
          "change_id" => "n3-u",
          "set" => set,
          "was" => was
        })

      assert r["result"] == "rejected", inspect(r)
      assert r["reason"] =~ says
    end

    assert %Node{title: "t", status: "pending"} = Repo.get_by!(Node, change_id: "n3-u")
  end

  test "SERVER-N3: a created_at that is not a time is refused, not replaced by the arrival time" do
    for {cid, at} <- [
          {"n3-at1", "99999-01-01T00:00:00Z"},
          {"n3-at2", 12345},
          {"n3-at3", "yesterday"}
        ] do
      r = one(create(cid, %{"created_at" => at}))
      assert r["result"] == "rejected", "#{inspect(at)}: #{inspect(r)}"
      assert r["reason"] =~ "created_at"
      refute exists?(cid)
    end
  end

  test "SERVER-N3: the CLI's own shapes still apply: RFC 3339 with an offset, and a legacy naive time" do
    assert %{"result" => "applied"} = one(create("n3-ok1", %{}))

    assert %{"result" => "applied"} =
             one(
               create("n3-ok2", %{
                 "created_at" => "2025-03-04 05:06:07",
                 "updated_at" => "2025-03-04 05:06:07"
               })
             )

    assert %Node{inserted_at: at} = Repo.get_by!(Node, change_id: "n3-ok2")
    assert DateTime.to_iso8601(at) =~ "2025-03-04T05:06:07"
  end

  test "SERVER-N3 (verification): create_edge is held to the bounds add_edge is" do
    assert %{"result" => "applied"} = one(create("n3-ea", %{}))
    assert %{"result" => "applied"} = one(create("n3-eb", %{}))

    edge = fn extra ->
      Map.merge(
        %{
          "op_id" => Ecto.UUID.generate(),
          "kind" => "create_edge",
          "from_change_id" => "n3-ea",
          "to_change_id" => "n3-eb",
          "edge_type" => "leads_to"
        },
        extra
      )
    end

    for {extra, says} <- [
          {%{"rationale" => String.duplicate("r", 2_000_000)},
           "rationale is 2000000 characters; the limit is 262144"},
          {%{"rationale" => %{"a" => 1}}, "rationale must be a string"},
          {%{"weight" => -1}, "weight must be at least 0"},
          {%{"weight" => "heavy"}, "weight must be a number"},
          {%{"edge_type" => "bogus"}, "edge_type must be one of"}
        ] do
      r = one(edge.(extra))
      assert r["result"] == "rejected", "#{inspect(Map.keys(extra))}: #{inspect(r)}"
      assert r["reason"] =~ "create_edge n3-ea -> n3-eb"
      assert r["reason"] =~ says
    end

    assert Repo.aggregate(DeciduousMcp.Schema.Edge, :count) == 0
    assert %{"result" => "applied"} = one(edge.(%{"rationale" => "why", "weight" => 1.0}))
  end

  test "SERVER-N3 (verification): metadata holds files, prompt and commit to MCP's types" do
    for {cid, meta, says} <- [
          {"n3-files", %{"files" => "notalist"}, "metadata.files must be an array"},
          {"n3-files2", %{"files" => [1]}, "metadata.files[0] must be a string"},
          {"n3-prompt", %{"prompt" => %{"a" => 1}}, "metadata.prompt must be a string"},
          {"n3-commit", %{"commit" => 7}, "metadata.commit must be a string"}
        ] do
      r = one(create(cid, %{"metadata" => meta}))
      assert r["result"] == "rejected", "#{cid}: #{inspect(r)}"
      assert r["reason"] =~ says
      refute exists?(cid)
    end

    # Keys of the node's own, which the CLI carries from whatever wrote them.
    assert %{"result" => "applied"} =
             one(create("n3-own", %{"metadata" => %{"files" => ["a.rs"], "sequence" => 3}}))
  end

  test "new (low): a change_id, op_id or kind that cannot be stored is refused, with no 500 and no workspace" do
    long = String.duplicate("c", 300)

    token = Application.fetch_env!(:deciduous_mcp, :api_token)

    post = fn ws, ops ->
      conn =
        conn(:post, "/ops", Jason.encode!(%{workspace: ws, ops: ops}))
        |> put_req_header("authorization", "Bearer " <> token)
        |> put_req_header("content-type", "application/json")
        |> Router.call(@opts)

      {conn.status, conn.resp_body}
    end

    for {ws, op, says} <- [
          {"vg-cid300", create(long, %{}), "change_id is 300 characters; the limit is 255"},
          {"vg-cidnul", create("a\u0000b", %{}), "change_id contains a NUL"},
          {"vg-opid300", create("ok-cid", %{"op_id" => long}), "op_id is 300 characters"},
          {"vg-opidnul", create("ok-cid", %{"op_id" => "a\u0000b"}), "op_id contains a NUL"},
          {"vg-kind300", Map.put(create("ok-cid", %{}), "kind", long), "unknown op kind"}
        ] do
      {status, body} = post.(ws, [op])
      assert status in [200, 422], "#{ws}: #{status} #{inspect(body)}"
      assert body =~ says, "#{ws}: #{body}"
      assert {:error, :not_found} = DeciduousMcp.Graph.Workspaces.get_by_name(ws), ws
    end
  end
end
