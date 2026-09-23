defmodule DeciduousMcp.Web.ImportRaceTest do
  @moduledoc """
  SERVER-N4 through /import: /ops creates took turns with each other and
  with add_node, but POST /import upserted beside them without asking.
  15 rounds of 4 /ops create_node racing 4 /import of the same change_id
  gave 14 ops answered "rejected: ...: workspace_id has already been
  taken", the false alarm SERVER-N4 was about, which the CLI keeps in its
  log for good.

  Real transactions and parallel connections.
  """
  use DeciduousMcp.RealDbCase, async: false

  alias DeciduousMcp.Test.McpHttp

  defp post(path, body) do
    {status, _, resp} =
      McpHttp.request("POST", path, Jason.encode!(body), [{"content-type", "application/json"}])

    {status, Jason.decode!(resp)}
  end

  defp create_op(cid) do
    %{
      op_id: Ecto.UUID.generate(),
      kind: "create_node",
      change_id: cid,
      node_type: "goal",
      title: cid
    }
  end

  defp import_body(ws, nodes, edges \\ []) do
    %{workspace: ws, graph: %{nodes: nodes, edges: edges}}
  end

  test "SERVER-N4: /ops create_node racing /import of one change_id is never rejected", ctx do
    ws = unique(ctx, "n4-import")
    # The workspace exists first, so the race is on the node, not the name.
    {200, _} = post("/ops", %{workspace: ws, ops: [create_op("seed")]})

    for round <- 1..15 do
      cid = "n4i-#{round}"

      results =
        (List.duplicate(:ops, 4) ++ List.duplicate(:import, 4))
        |> Enum.shuffle()
        |> Enum.map(fn
          :ops ->
            Task.async(fn -> post("/ops", %{workspace: ws, ops: [create_op(cid)]}) end)

          :import ->
            Task.async(fn ->
              post(
                "/import",
                import_body(ws, [%{"change_id" => cid, "node_type" => "goal", "title" => cid}])
              )
            end)
        end)
        |> Enum.map(&Task.await(&1, 30_000))

      for {status, body} <- results do
        assert status == 200, "round #{round}: #{status} #{inspect(body)}"

        case body do
          %{"results" => [r]} ->
            assert r["result"] in ["applied", "exists"], "round #{round}: #{inspect(r)}"

          %{"nodes" => _} ->
            :ok
        end
      end
    end
  end

  test "SERVER-N4: /ops create_edge racing /import of the same edge is never rejected", ctx do
    ws = unique(ctx, "n4-import-edge")

    for round <- 1..10 do
      a = "n4ie-a#{round}"
      b = "n4ie-b#{round}"
      {200, _} = post("/ops", %{workspace: ws, ops: [create_op(a), create_op(b)]})

      edge_op = fn ->
        post("/ops", %{
          workspace: ws,
          ops: [
            %{
              op_id: Ecto.UUID.generate(),
              kind: "create_edge",
              from_change_id: a,
              to_change_id: b,
              edge_type: "leads_to"
            }
          ]
        })
      end

      import = fn ->
        post(
          "/import",
          import_body(
            ws,
            [
              %{"id" => 1, "change_id" => a, "node_type" => "goal", "title" => a},
              %{"id" => 2, "change_id" => b, "node_type" => "goal", "title" => b}
            ],
            [%{"from_node_id" => 1, "to_node_id" => 2, "edge_type" => "leads_to"}]
          )
        )
      end

      results =
        [edge_op, import, edge_op, import, edge_op, import]
        |> Enum.map(&Task.async/1)
        |> Enum.map(&Task.await(&1, 30_000))

      for {200, body} <- results, %{"results" => [r]} <- [body] do
        assert r["result"] in ["applied", "exists"], "round #{round}: #{inspect(r)}"
      end

      assert Enum.all?(results, &match?({200, _}, &1)), inspect(results)
    end
  end
end
