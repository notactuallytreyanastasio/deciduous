defmodule DeciduousMcp.Web.OpsClaimNoGhostTest do
  @moduledoc """
  SERVER-N6: POST /ops {workspace: new, ops: []} answered 200 and left a
  workspace behind, and so did a batch whose every op was rejected;
  Ops.run called find_or_create before it looked at the ops. /claim did
  the same with no roots to record. Chapter 22 said neither a read nor a
  failed write creates a workspace. A bad name came back as
  {"error":":global"}, the inspected atom. /claim stored 20,000 roots.

  SERVER-N7: GET /export with X-Deciduous-Repo-Roots recorded the roots on
  an unclaimed workspace, so the first repository of that name to run
  `remote status` or `pull` owned it, and the real one then got 409 on
  its own workspace. A read checks a claim; it does not make one.
  """
  use DeciduousMcp.DataCase, async: false

  import Plug.Test
  import Plug.Conn

  alias DeciduousMcp.Graph.Workspaces
  alias DeciduousMcp.Web.Router

  @opts Router.init([])
  @a "1111111111111111111111111111111111111111"
  @b "3333333333333333333333333333333333333333"

  defp token, do: Application.fetch_env!(:deciduous_mcp, :api_token)

  defp post(path, body) do
    conn =
      conn(:post, path, Jason.encode!(body))
      |> put_req_header("authorization", "Bearer " <> token())
      |> put_req_header("content-type", "application/json")
      |> Router.call(@opts)

    {conn.status, Jason.decode!(conn.resp_body)}
  end

  defp exists?(name), do: match?({:ok, _}, Workspaces.get_by_name(name))

  defp update_op(cid) do
    %{
      op_id: Ecto.UUID.generate(),
      kind: "update_node",
      change_id: cid,
      set: %{status: "active"},
      was: %{status: "pending"}
    }
  end

  defp create_op(cid, extra \\ %{}) do
    Map.merge(
      %{
        op_id: Ecto.UUID.generate(),
        kind: "create_node",
        change_id: cid,
        node_type: "goal",
        title: cid,
        status: "pending"
      },
      extra
    )
  end

  test "SERVER-N6: /ops that writes nothing creates no workspace" do
    assert {200, %{"results" => []}} = post("/ops", %{workspace: "n6-empty", ops: []})
    refute exists?("n6-empty")

    assert {200,
            %{
              "results" => [
                %{"result" => "rejected"},
                %{"result" => "absent"},
                %{"result" => "rejected"}
              ]
            }} =
             post("/ops", %{
               workspace: "n6-rejected",
               ops: [
                 update_op("nope"),
                 %{op_id: Ecto.UUID.generate(), kind: "delete_node", change_id: "nope"},
                 create_op("bad", %{title: String.duplicate("t", 10_001)})
               ]
             })

    refute exists?("n6-rejected")
  end

  test "SERVER-N6: /ops that writes a node creates the workspace and claims it" do
    assert {200, %{"results" => [%{"result" => "rejected"}, %{"result" => "applied"}]}} =
             post("/ops", %{
               workspace: "n6-real",
               repo_roots: [@a],
               ops: [update_op("x"), create_op("y")]
             })

    assert {:ok, ws} = Workspaces.get_by_name("n6-real")
    assert ws.settings["repo_roots"] == [@a]
  end

  test "SERVER-N6: /claim with nothing to record creates no workspace; with roots it claims" do
    assert {200, %{"claim" => "unchecked"}} = post("/claim", %{workspace: "n6-claim"})

    assert {200, %{"claim" => "unchecked"}} =
             post("/claim", %{workspace: "n6-claim", repo_roots: []})

    refute exists?("n6-claim")

    assert {200, %{"claim" => "claimed"}} =
             post("/claim", %{workspace: "n6-claim", repo_roots: [@a]})

    assert exists?("n6-claim")
  end

  test "SERVER-N6: a bad workspace name is refused with a sentence on /ops and /claim" do
    for path <- ["/ops", "/claim"],
        {name, says} <- [
          {"*", "the global view"},
          {"  ", "is blank"},
          {"a\u0007b", "control or formatting"},
          {42, "is not a string"}
        ] do
      assert {422, %{"error" => error}} =
               post(path, %{workspace: name, ops: [], repo_roots: [@a]})

      assert error =~ "invalid workspace name", "#{path} #{inspect(name)}: #{error}"
      assert error =~ says
    end
  end

  test "SERVER-N6: repo_roots is bounded" do
    roots = for i <- 1..20_000, do: String.pad_leading(Integer.to_string(i, 16), 40, "0") |> String.downcase()
    assert {422, %{"error" => error}} = post("/claim", %{workspace: "n6-many", repo_roots: roots})
    assert error =~ "20000"
    refute exists?("n6-many")
  end

  test "SERVER-N7: GET /export with repo roots does not claim an unclaimed workspace" do
    {200, _} = post("/ops", %{workspace: "n7-ws", ops: [create_op("n7")]})

    conn =
      conn(:get, "/export?workspace=n7-ws")
      |> put_req_header("authorization", "Bearer " <> token())
      |> put_req_header("x-deciduous-repo-roots", @b)
      |> Router.call(@opts)

    assert conn.status == 200
    assert {:ok, ws} = Workspaces.get_by_name("n7-ws")
    assert (ws.settings || %{})["repo_roots"] in [nil, []]

    # The real repository can still claim it.
    assert {200, %{"claim" => "claimed"}} =
             post("/claim", %{workspace: "n7-ws", repo_roots: [@a]})

    # And once it is claimed, the export by another repository is refused.
    conn =
      conn(:get, "/export?workspace=n7-ws")
      |> put_req_header("authorization", "Bearer " <> token())
      |> put_req_header("x-deciduous-repo-roots", @b)
      |> Router.call(@opts)

    assert conn.status == 409
  end

  # Verification of SERVER-N6, the claim half: on a workspace that exists
  # and is unclaimed (every workspace an agent makes over MCP), Ops.run
  # claimed it for the batch's repo_roots before applying anything. A batch
  # that wrote nothing then owned the workspace, and the real repository's
  # next push was 409 claimed_by_other_repository.
  test "SERVER-N6 claim: an /ops batch that writes nothing does not claim the workspace" do
    {:ok, ws} = Workspaces.find_or_create("n6-claim")
    {:ok, _} = DeciduousMcp.Graph.Nodes.create_node(ws.id, %{node_type: "goal", title: "by mcp"})

    for {what, ops} <- [
          {"empty", []},
          {"all rejected", [update_op("no-such-node")]},
          {"delete of an absent node",
           [%{op_id: Ecto.UUID.generate(), kind: "delete_node", change_id: "absent"}]}
        ] do
      assert {200, _} = post("/ops", %{workspace: "n6-claim", ops: ops, repo_roots: [@a]}), what
      {:ok, ws} = Workspaces.get_by_name("n6-claim")
      assert (ws.settings || %{})["repo_roots"] in [nil, []], "#{what}: #{inspect(ws.settings)}"
    end

    # The real repository's push is not locked out, and it is the one that claims.
    assert {200, %{"results" => [%{"result" => "applied"}]}} =
             post("/ops", %{workspace: "n6-claim", ops: [create_op("real")], repo_roots: [@b]})

    {:ok, ws} = Workspaces.get_by_name("n6-claim")
    assert ws.settings["repo_roots"] == [@b]

    # And once claimed, another repository is still refused, before it writes.
    assert {409, %{"reason" => "claimed_by_other_repository"}} =
             post("/ops", %{workspace: "n6-claim", ops: [create_op("other")], repo_roots: [@a]})

    refute DeciduousMcp.Repo.get_by(DeciduousMcp.Schema.Node, change_id: "other")
  end
end
