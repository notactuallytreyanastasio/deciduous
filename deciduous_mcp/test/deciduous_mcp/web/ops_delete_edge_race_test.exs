defmodule DeciduousMcp.Web.OpsDeleteEdgeRaceTest do
  @moduledoc """
  SERVER-N8: /ops delete_node(a) racing create_edge(a -> b), both answered
  `applied` in 3 of 170 rounds, leaving an edge row out of a deleted node.

  An edge row out of a deleted node is also what deleting a node that
  already has edges leaves (a delete is soft; the edges stay for the
  tombstone and are dropped from every read). So "both applied" is right
  when the edge came first, and wrong only if the edge was checked against
  a live node and inserted after the delete committed. Edges.create_edge
  reads both ends FOR SHARE and Nodes.delete_node takes the row FOR
  UPDATE, so the two serialise; this checks, from the timestamps each one
  took under its lock, that every round is one of the two serial orders.
  """
  use DeciduousMcp.RealDbCase, async: false

  alias DeciduousMcp.Test.McpHttp

  defp post_ops(ws, ops) do
    {200, _, body} =
      McpHttp.request("POST", "/ops", Jason.encode!(%{workspace: ws, ops: ops}), [
        {"content-type", "application/json"}
      ])

    Jason.decode!(body)["results"]
  end

  defp create(cid) do
    %{
      op_id: Ecto.UUID.generate(),
      kind: "create_node",
      change_id: cid,
      node_type: "goal",
      title: cid
    }
  end

  test "SERVER-N8: delete_node racing create_edge is always one serial order", ctx do
    ws = unique(ctx, "n8")

    tally =
      for round <- 1..60, reduce: %{} do
        acc ->
          a = "n8-a#{round}"
          b = "n8-b#{round}"
          [_, _] = post_ops(ws, [create(a), create(b)])

          delete = %{op_id: Ecto.UUID.generate(), kind: "delete_node", change_id: a}

          edge = %{
            op_id: Ecto.UUID.generate(),
            kind: "create_edge",
            from_change_id: a,
            to_change_id: b,
            edge_type: "leads_to"
          }

          # The delete is the cheaper op and wins every round when both
          # start together; a jitter lets the edge win some.
          [[d], [e]] =
            [{delete, :rand.uniform(6) - 1}, {edge, 0}]
            |> Enum.map(fn {op, lag} ->
              Task.async(fn ->
                Process.sleep(lag)
                post_ops(ws, [op])
              end)
            end)
            |> Enum.map(&Task.await(&1, 30_000))

          assert d["result"] == "applied"

          %{rows: [[deleted_at]]} =
            DeciduousMcp.Repo.query!(
              "SELECT n.deleted_at FROM decision_nodes n JOIN workspaces w ON w.id = n.workspace_id WHERE w.name = $1 AND n.change_id = $2",
              [ws, a]
            )

          case e["result"] do
            "applied" ->
              %{rows: [[inserted_at]]} =
                DeciduousMcp.Repo.query!(
                  "SELECT e.inserted_at FROM decision_edges e JOIN workspaces w ON w.id = e.workspace_id WHERE w.name = $1 AND e.from_change_id = $2",
                  [ws, a]
                )

              assert NaiveDateTime.compare(inserted_at, deleted_at) == :lt,
                     "round #{round}: edge inserted at #{inserted_at}, after the delete at #{deleted_at}"

            "rejected" ->
              assert e["reason"] =~ "deleted"
          end

          Map.update(acc, e["result"], 1, &(&1 + 1))
      end

    # Both orders should turn up over 60 rounds; if one never does, the
    # race is not being exercised and the test proves nothing.
    assert map_size(tally) == 2, inspect(tally)
  end
end
