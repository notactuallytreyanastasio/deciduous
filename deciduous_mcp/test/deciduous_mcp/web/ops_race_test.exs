defmodule DeciduousMcp.Web.OpsRaceTest do
  @moduledoc """
  SERVER-N4: eight /ops requests at once, each creating the same change_id
  (or the same edge) under its own op id, answered most of them
  "rejected: workspace_id has already been taken" (or "from_node_id has
  already been taken"): the loser of the unique index fell into the
  changeset error path and never looked again. The CLI keeps a rejected op
  and prints it until someone deals with it, so a benign race (two clones
  seeding, a retry racing its original) left a permanent false alarm that
  named the wrong field. Each of them should be `exists`.

  Real transactions, real HTTP, parallel connections.
  """
  use DeciduousMcp.RealDbCase, async: false

  alias DeciduousMcp.Test.McpHttp

  defp post_ops(ws, ops) do
    {200, _, body} =
      McpHttp.request(
        "POST",
        "/ops",
        Jason.encode!(%{workspace: ws, ops: ops}),
        [{"content-type", "application/json"}]
      )

    Jason.decode!(body)["results"]
  end

  defp create(cid) do
    %{
      op_id: Ecto.UUID.generate(),
      kind: "create_node",
      change_id: cid,
      node_type: "goal",
      title: cid,
      status: "pending",
      metadata: %{},
      created_at: "2026-01-01T00:00:00Z",
      updated_at: "2026-01-01T00:00:00Z"
    }
  end

  defp at_once(n, fun) do
    1..n
    |> Enum.map(fn _ -> Task.async(fun) end)
    |> Enum.flat_map(&Task.await(&1, 30_000))
  end

  test "SERVER-N4: racing creates of one change_id: one applied, the rest exists", ctx do
    ws = unique(ctx, "n4-node")

    for round <- 1..10 do
      cid = "n4-#{round}"
      results = at_once(8, fn -> post_ops(ws, [create(cid)]) end)
      tally = Enum.frequencies_by(results, & &1["result"])

      assert tally == %{"applied" => 1, "exists" => 7},
             "round #{round}: #{inspect(tally)} #{inspect(Enum.find(results, &(&1["result"] == "rejected")))}"
    end
  end

  test "SERVER-N4: racing creates of one edge: one applied, the rest exists", ctx do
    ws = unique(ctx, "n4-edge")

    for round <- 1..10 do
      a = "n4e-a#{round}"
      b = "n4e-b#{round}"
      [_, _] = post_ops(ws, [create(a), create(b)])

      edge = fn ->
        %{
          op_id: Ecto.UUID.generate(),
          kind: "create_edge",
          created_at: DateTime.to_iso8601(DateTime.utc_now()),
          from_change_id: a,
          to_change_id: b,
          edge_type: "leads_to",
          created_at: "2026-01-01T00:00:00Z"
        }
      end

      results = at_once(8, fn -> post_ops(ws, [edge.()]) end)
      tally = Enum.frequencies_by(results, & &1["result"])

      assert tally == %{"applied" => 1, "exists" => 7},
             "round #{round}: #{inspect(tally)} #{inspect(Enum.find(results, &(&1["result"] == "rejected")))}"
    end
  end
end
