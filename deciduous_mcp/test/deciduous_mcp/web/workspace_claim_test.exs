defmodule DeciduousMcp.Web.WorkspaceClaimTest do
  @moduledoc """
  A workspace belongs to the repository that first wrote to it.

  Workspace names are derived from a repository's directory name, lowercased.
  Two unrelated repositories called `bridge-api` (or `bridge-API`) therefore
  asked for the same workspace, and 1.0.7 gave it to both: `remote init` in
  the second printed "server holds 2 nodes" without a warning, and a pull
  imported the first project's goal into the second's committed graph
  (bridge finding C5).

  The CLI now sends the repository's root commit ids. The first repository to
  send them claims the workspace; a repository whose roots share none of them
  is refused, unless it asked for that workspace by name (`adopt`).
  """
  use DeciduousMcp.DataCase, async: false

  import Plug.Test
  import Plug.Conn

  alias DeciduousMcp.Web.Router

  @opts Router.init([])
  @a "1111111111111111111111111111111111111111"
  @b "2222222222222222222222222222222222222222"

  setup do
    %{token: Application.fetch_env!(:deciduous_mcp, :api_token)}
  end

  defp post(token, path, body) do
    conn =
      conn(:post, path, Jason.encode!(body))
      |> put_req_header("authorization", "Bearer " <> token)
      |> put_req_header("content-type", "application/json")
      |> Router.call(@opts)

    {conn.status, Jason.decode!(conn.resp_body)}
  end

  test "the first repository claims, the same one is verified, another is refused",
       %{token: token} do
    assert {200, %{"claim" => "claimed"}} =
             post(token, "/claim", %{workspace: "bridge-api", repo_roots: [@a]})

    assert {200, %{"claim" => "verified"}} =
             post(token, "/claim", %{workspace: "bridge-api", repo_roots: [@a]})

    assert {409, %{"error" => error}} =
             post(token, "/claim", %{workspace: "Bridge-API", repo_roots: [@b]})

    assert error =~ "another repository"
  end

  test "naming the workspace adopts it, and the adopter is verified after", %{token: token} do
    {200, _} = post(token, "/claim", %{workspace: "shared-by-choice", repo_roots: [@a]})

    assert {200, %{"claim" => "adopted"}} =
             post(token, "/claim", %{workspace: "shared-by-choice", repo_roots: [@b], adopt: true})

    assert {200, %{"claim" => "verified"}} =
             post(token, "/claim", %{workspace: "shared-by-choice", repo_roots: [@b]})
  end

  test "ops from another repository are refused before any is applied", %{token: token} do
    {200, _} = post(token, "/claim", %{workspace: "ops-claimed", repo_roots: [@a]})

    op = %{
      op_id: Ecto.UUID.generate(),
      kind: "create_node",
      change_id: "x1",
      node_type: "goal",
      title: "not yours",
      status: "pending",
      metadata: %{},
      created_at: "2026-09-23T00:00:00Z",
      updated_at: "2026-09-23T00:00:00Z"
    }

    assert {409, _} =
             post(token, "/ops", %{workspace: "ops-claimed", repo_roots: [@b], ops: [op]})

    assert Repo.aggregate(DeciduousMcp.Schema.Node, :count) == 0

    assert {200, %{"results" => [%{"result" => "applied"}]}} =
             post(token, "/ops", %{workspace: "ops-claimed", repo_roots: [@a], ops: [op]})
  end

  test "roots that are not commit ids are refused by name", %{token: token} do
    assert {422, %{"error" => error}} =
             post(token, "/claim", %{workspace: "bad-roots", repo_roots: ["main"]})

    assert error =~ "main"
  end
end
