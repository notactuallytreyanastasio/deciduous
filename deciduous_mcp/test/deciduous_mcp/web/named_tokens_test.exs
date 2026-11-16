defmodule DeciduousMcp.Web.NamedTokensTest do
  @moduledoc """
  A token per person: issued and revoked by the server's own token, accepted
  wherever that one is, and named beside what its holder writes.
  """
  use DeciduousMcp.DataCase, async: false

  import Plug.Test
  import Plug.Conn

  alias DeciduousMcp.{Activity, Tokens}
  alias DeciduousMcp.Graph.Workspaces
  alias DeciduousMcp.Web.Router

  @opts Router.init([])

  setup do
    %{master: Application.fetch_env!(:deciduous_mcp, :api_token)}
  end

  defp call(method, path, token, body \\ nil) do
    conn(method, path, body && Jason.encode!(body))
    |> put_req_header("authorization", "Bearer " <> token)
    |> put_req_header("content-type", "application/json")
    |> Router.call(@opts)
  end

  defp issue(master, name) do
    conn = call(:post, "/tokens", master, %{name: name})
    {conn.status, Jason.decode!(conn.resp_body)}
  end

  defp create_op(cid, branch) do
    %{
      op_id: Ecto.UUID.generate(),
      kind: "create_node",
      change_id: cid,
      node_type: "goal",
      title: cid,
      status: "pending",
      metadata: %{"branch" => branch},
      created_at: "2026-02-01T00:00:00Z",
      updated_at: "2026-02-01T00:00:00Z"
    }
  end

  test "an issued token is accepted, shown once, and stored only as a hash", %{master: master} do
    assert {201, %{"name" => "ada", "token" => "dcd_" <> _ = token}} = issue(master, "ada")
    assert call(:get, "/export?workspace=tokens-a", token).status == 200

    assert %{rows: [[hash]]} =
             Repo.query!("SELECT token_hash FROM api_tokens WHERE name = 'ada'")

    assert hash == :crypto.hash(:sha256, token)
    refute token =~ Base.encode16(hash, case: :lower)

    assert %{"tokens" => [%{"name" => "ada", "revoked_at" => nil} = row]} =
             call(:get, "/tokens", master).resp_body |> Jason.decode!()

    refute Map.has_key?(row, "token")
    refute Map.has_key?(row, "token_hash")
    assert is_binary(row["last_used_at"]), "the export above was a use"
  end

  test "a token that was never issued is refused, whatever it looks like", %{master: master} do
    {201, %{"token" => token}} = issue(master, "ada")

    for presented <- ["dcd_" <> String.duplicate("A", 43), token <> "x", "ada", ""] do
      assert call(:get, "/export?workspace=tokens-b", presented).status == 401, presented
    end
  end

  test "a revoked token stops working and its name is not issued again", %{master: master} do
    {201, %{"token" => token}} = issue(master, "grace")
    assert call(:delete, "/tokens/grace", master).status == 200
    assert call(:get, "/export?workspace=tokens-c", token).status == 401
    assert call(:delete, "/tokens/grace", master).status == 404

    assert {422, %{"error" => error}} = issue(master, "grace")
    assert error =~ "names are not reused"
    assert {422, _} = issue(master, "ada lovelace")
    assert {422, _} = issue(master, "")
  end

  test "a named token cannot issue, list or revoke tokens", %{master: master} do
    {201, %{"token" => ada}} = issue(master, "ada")
    assert call(:post, "/tokens", ada, %{name: "mallory"}).status == 403
    assert call(:get, "/tokens", ada).status == 403
    assert call(:delete, "/tokens/ada", ada).status == 403
    assert [%{name: "ada"}] = Tokens.list()
  end

  test "a write is recorded under the name of the token it came in on", %{master: master} do
    {201, %{"token" => ada}} = issue(master, "ada")
    {201, %{"token" => grace}} = issue(master, "grace")
    roots = ["1111111111111111111111111111111111111111"]

    for {token, cid} <- [{ada, "tok-ada"}, {grace, "tok-grace"}, {master, "tok-shared"}] do
      conn =
        call(:post, "/ops", token, %{
          workspace: "tokens-d",
          repo_roots: roots,
          ops: [create_op(cid, "main")]
        })

      assert conn.status == 200, conn.resp_body
    end

    {:ok, ws} = Workspaces.get_by_name("tokens-d")
    # Three clones of one repository, one branch: three rows, not one.
    actors = ws.id |> Activity.recent() |> Enum.map(& &1.actor) |> Enum.sort()
    assert actors == [nil, "ada", "grace"]
  end

  test "an MCP write carries the name too, and the next session does not inherit it", %{
    master: master
  } do
    alias DeciduousMcp.Test.McpClient
    {201, %{"token" => ada}} = issue(master, "ada")

    named = %{McpClient.connect(name: "named-client") | token: ada}

    assert {:ok, _} =
             McpClient.call(named, "add_node", %{
               "workspace" => "tokens-e",
               "node_type" => "goal",
               "title" => "by ada",
               "branch" => "a"
             })

    shared = McpClient.connect(name: "shared-client")

    assert {:ok, _} =
             McpClient.call(shared, "add_node", %{
               "workspace" => "tokens-e",
               "node_type" => "goal",
               "title" => "by nobody in particular",
               "branch" => "b"
             })

    {:ok, ws} = Workspaces.get_by_name("tokens-e")
    by_branch = ws.id |> Activity.recent() |> Map.new(&{&1.branch, &1.actor})
    assert by_branch == %{"a" => "ada", "b" => nil}

    assert {:ok, activity} =
             McpClient.call(shared, "check_activity", %{"workspace" => "tokens-e"})

    assert Enum.sort(Enum.map(activity["sessions"], & &1["actor"])) == [nil, "ada"]
  end
end
