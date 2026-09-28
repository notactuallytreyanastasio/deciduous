defmodule DeciduousMcp.Web.HistoryDatesTest do
  @moduledoc """
  A node's created_at is when the decision was made, and every copy of a
  graph that reaches the server (`remote push --seed`, `--overwrite`, a
  replayed create op) must carry it through: a customer case study found
  about 38% of one graph's dates were the times of imports and backfills.

  Over the router, as the CLI sends it.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.Workspaces
  alias DeciduousMcp.Schema.{Edge, Node}
  alias DeciduousMcp.Test.McpClient

  setup do
    %{client: McpClient.connect()}
  end

  defp import!(client, graph) do
    {status, report} =
      McpClient.post_json(client, "/import", %{"workspace" => "dates-ws", "graph" => graph})

    assert status == 200, inspect(report)
    report
  end

  defp ops!(client, ops) do
    {status, body} =
      McpClient.post_json(client, "/ops", %{"workspace" => "dates-ws", "ops" => ops})

    assert status == 200, inspect(body)
    body["results"]
  end

  defp node!(cid) do
    {:ok, ws} = Workspaces.get_by_name("dates-ws")
    Repo.one!(from(n in Node, where: n.workspace_id == ^ws.id and n.change_id == ^cid))
  end

  defp at(iso) do
    {:ok, dt, _} = DateTime.from_iso8601(iso)
    %{dt | microsecond: {elem(dt.microsecond, 0), 6}}
  end

  defp node(cid, created, extra \\ %{}) do
    Map.merge(
      %{
        "id" => System.unique_integer([:positive]),
        "change_id" => cid,
        "node_type" => "goal",
        "title" => cid,
        "created_at" => created,
        "updated_at" => "2026-09-01T00:00:00-04:00"
      },
      extra
    )
  end

  test "an import keeps every node's and edge's created_at, offsets and all", %{client: c} do
    a = node("d-a", "2019-03-04T05:06:07.123456-05:00")
    b = node("d-b", "2021-01-01T00:00:00+09:00")

    report =
      import!(c, %{
        "nodes" => [a, b],
        "edges" => [
          %{
            "from_node_id" => a["id"],
            "to_node_id" => b["id"],
            "edge_type" => "leads_to",
            "created_at" => "2021-01-02T03:04:05-04:00"
          }
        ]
      })

    assert report["nodes"]["dated_on_arrival"] == 0
    assert DateTime.compare(node!("d-a").inserted_at, at("2019-03-04T10:06:07.123456Z")) == :eq
    assert DateTime.compare(node!("d-b").inserted_at, at("2020-12-31T15:00:00Z")) == :eq

    [edge] = Repo.all(from(e in Edge, where: e.from_node_id == ^node!("d-a").id))
    assert DateTime.compare(edge.inserted_at, at("2021-01-02T07:04:05Z")) == :eq
  end

  test "a naive created_at (old `add --date`) is read as UTC, not dated on arrival",
       %{client: c} do
    import!(c, %{"nodes" => [node("d-naive", "2018-06-01 12:00:00")]})
    assert DateTime.compare(node!("d-naive").inserted_at, at("2018-06-01T12:00:00Z")) == :eq
  end

  test "a created_at that is not a time is refused whole", %{client: c} do
    {status, body} =
      McpClient.post_json(c, "/import", %{
        "workspace" => "dates-ws",
        "graph" => %{
          "nodes" => [node("d-ok", "2020-01-01T00:00:00Z"), node("d-bad", "yesterday")]
        }
      })

    assert status in [400, 422], inspect(body)
    assert inspect(body) =~ "nodes[1].created_at"
    assert inspect(body) =~ "nothing was imported"
    assert Workspaces.get_by_name("dates-ws") == {:error, :not_found}
  end

  test "without created_at a node takes its updated_at, and without either the report says so",
       %{client: c} do
    report =
      import!(c, %{
        "nodes" => [
          node("d-upd", nil, %{"updated_at" => "2017-02-03T04:05:06Z"})
          |> Map.delete("created_at"),
          %{"change_id" => "d-none", "node_type" => "goal", "title" => "none"}
        ]
      })

    assert report["nodes"]["dated_from_updated_at"] == 1
    assert report["nodes"]["dated_on_arrival"] == 1
    assert DateTime.compare(node!("d-upd").inserted_at, at("2017-02-03T04:05:06Z")) == :eq
  end

  test "a re-import keeps the earlier created_at, whichever copy carries it", %{client: c} do
    import!(c, %{"nodes" => [node("d-re", "2026-09-20T10:00:00Z")]})
    # The true date arrives later (a seed from a clone that kept it).
    import!(c, %{"nodes" => [node("d-re", "2016-01-01T00:00:00Z")]})
    assert DateTime.compare(node!("d-re").inserted_at, at("2016-01-01T00:00:00Z")) == :eq
    # A re-stamped copy after it does not move it forward again.
    import!(c, %{"nodes" => [node("d-re", "2026-09-28T00:00:00Z")]})
    assert DateTime.compare(node!("d-re").inserted_at, at("2016-01-01T00:00:00Z")) == :eq
  end

  test "a re-imported edge keeps the earlier created_at", %{client: c} do
    a = node("d-ea", "2016-01-01T00:00:00Z")
    b = node("d-eb", "2016-01-01T00:00:00Z")

    e = fn made ->
      %{
        "from_node_id" => a["id"],
        "to_node_id" => b["id"],
        "edge_type" => "leads_to",
        "created_at" => made
      }
    end

    import!(c, %{"nodes" => [a, b], "edges" => [e.("2026-09-20T00:00:00Z")]})
    import!(c, %{"nodes" => [a, b], "edges" => [e.("2016-02-02T00:00:00Z")]})
    import!(c, %{"nodes" => [a, b], "edges" => [e.("2026-09-28T00:00:00Z")]})
    [edge] = Repo.all(from(x in Edge, where: x.from_node_id == ^node!("d-ea").id))
    assert DateTime.compare(edge.inserted_at, at("2016-02-02T00:00:00Z")) == :eq
  end

  describe "/ops create_node" do
    defp create(cid, fields) do
      Map.merge(
        %{
          "op_id" => Ecto.UUID.generate(),
          "kind" => "create_node",
          "change_id" => cid,
          "node_type" => "goal",
          "title" => cid
        },
        fields
      )
    end

    test "keeps the op's created_at", %{client: c} do
      [r] = ops!(c, [create("o-a", %{"created_at" => "2015-05-05T05:05:05-07:00"})])
      assert r["result"] == "applied", inspect(r)
      assert DateTime.compare(node!("o-a").inserted_at, at("2015-05-05T12:05:05Z")) == :eq
    end

    test "without created_at, dates by updated_at, then by the op's at", %{client: c} do
      [r1, r2] =
        ops!(c, [
          create("o-u", %{"updated_at" => "2014-04-04T00:00:00Z"}),
          create("o-at", %{"at" => "2013-03-03T00:00:00Z"})
        ])

      assert {r1["result"], r2["result"]} == {"applied", "applied"}, inspect({r1, r2})
      assert DateTime.compare(node!("o-u").inserted_at, at("2014-04-04T00:00:00Z")) == :eq
      assert DateTime.compare(node!("o-at").inserted_at, at("2013-03-03T00:00:00Z")) == :eq
    end

    test "a create of a node already here moves its date earlier, never later", %{client: c} do
      import!(c, %{"nodes" => [node("o-x", "2026-09-20T10:00:00Z")]})

      [r] = ops!(c, [create("o-x", %{"created_at" => "2012-12-12T00:00:00Z"})])
      assert r["result"] == "exists"
      assert DateTime.compare(node!("o-x").inserted_at, at("2012-12-12T00:00:00Z")) == :eq

      [r] = ops!(c, [create("o-x", %{"created_at" => "2026-09-28T00:00:00Z"})])
      assert r["result"] == "exists"
      assert DateTime.compare(node!("o-x").inserted_at, at("2012-12-12T00:00:00Z")) == :eq
    end
  end
end
