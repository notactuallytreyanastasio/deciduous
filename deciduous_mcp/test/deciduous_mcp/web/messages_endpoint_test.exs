defmodule DeciduousMcp.Web.MessagesEndpointTest do
  @moduledoc """
  POST /messages and GET /messages: the board for `deciduous board` in
  remote mode. Same arguments and answers as the tools, same auth and
  pinning as /export and /ops, errors as `{"error": "..."}`.

  Run with BOARD_EXAMPLES=1 to print each exchange, the examples the CLI
  builds against.
  """
  use DeciduousMcp.DataCase, async: false

  import Plug.Test
  import Plug.Conn

  alias DeciduousMcp.Graph.Workspaces
  alias DeciduousMcp.Web.Router

  @opts Router.init([])
  @ws "board-http"

  setup do
    %{token: Application.fetch_env!(:deciduous_mcp, :api_token)}
  end

  defp request(method, path, token, body, headers) do
    conn =
      case body do
        nil -> conn(method, path)
        body when is_binary(body) -> conn(method, path, body)
        body -> conn(method, path, Jason.encode!(body))
      end

    conn =
      if token, do: put_req_header(conn, "authorization", "Bearer " <> token), else: conn

    conn =
      headers
      |> Enum.reduce(put_req_header(conn, "content-type", "application/json"), fn {k, v}, c ->
        put_req_header(c, k, v)
      end)
      |> Router.call(@opts)

    decoded = Jason.decode(conn.resp_body)

    if System.get_env("BOARD_EXAMPLES") do
      IO.puts(
        "\n#{String.upcase(to_string(method))} #{path}" <>
          if(body, do: "\n" <> if(is_binary(body), do: body, else: Jason.encode!(body)), else: "") <>
          "\n-> #{conn.status} #{conn.resp_body}"
      )
    end

    {conn.status, elem(decoded, 1)}
  end

  defp post_msg(token, body, headers \\ []), do: request(:post, "/messages", token, body, headers)

  defp get_msgs(token, query, headers \\ []),
    do: request(:get, "/messages?" <> query, token, nil, headers)

  test "POST answers 201 with the post_message output; GET answers the read_messages output",
       %{token: token} do
    {201, posted} =
      post_msg(token, %{
        workspace: @ws,
        author: "lead",
        subject: "interface for @board-cli",
        body: "see post 16 @orchestrator.",
        branch: "main"
      })

    assert %{"id" => id, "mentions" => ["board-cli", "orchestrator"], "reply_to" => nil} = posted
    assert Map.keys(posted) |> Enum.sort() == ~w(created_at id mentions reply_to)

    {201, reply} =
      post_msg(token, %{
        workspace: @ws,
        author: "board-cli",
        subject: "re",
        body: "ok",
        reply_to: id
      })

    assert reply["reply_to"] == id

    {200, read} = get_msgs(token, "workspace=#{@ws}")
    assert %{"latest_id" => latest, "truncated" => false, "messages" => [first, second]} = read
    assert latest == reply["id"]

    assert first == %{
             "id" => id,
             "branch" => "main",
             "author" => "lead",
             "subject" => "interface for @board-cli",
             "body" => "see post 16 @orchestrator.",
             "mentions" => ["board-cli", "orchestrator"],
             "reply_to" => nil,
             "created_at" => posted["created_at"]
           }

    assert second["reply_to"] == id
  end

  test "workspace from ?workspace= when the body names none", %{token: token} do
    {201, %{"id" => id}} =
      request(
        :post,
        "/messages?workspace=#{@ws}",
        token,
        %{author: "a1", subject: "s", body: "b"},
        []
      )

    {200, %{"messages" => [%{"id" => ^id}]}} = get_msgs(token, "workspace=#{@ws}")
  end

  test "every GET filter", %{token: token} do
    {201, %{"id" => q}} =
      post_msg(token, %{
        workspace: @ws,
        author: "lead",
        subject: "q",
        body: "@a1 which columns?",
        branch: "x"
      })

    {201, %{"id" => r}} =
      post_msg(token, %{workspace: @ws, author: "a1", subject: "re", body: "these", reply_to: q})

    {201, %{"id" => q2}} =
      post_msg(token, %{workspace: @ws, author: "lead", subject: "q2", body: "@a1 regenerating?"})

    ids = fn query ->
      {200, %{"messages" => ms}} = get_msgs(token, "workspace=#{@ws}&" <> query)
      Enum.map(ms, & &1["id"])
    end

    assert ids.("since_id=#{q}") == [r, q2]
    assert ids.("author=a1") == [r]
    assert ids.("to=a1") == [q, q2]
    assert ids.("unanswered_for=a1") == [q2]
    assert ids.("query=regenerate") == [q2]
    assert ids.("query=%22which%20columns%22") == [q]
    assert ids.("id=#{r}") == [r]
    assert ids.("branch=x") == [q]
    assert ids.("limit=1") == [q]

    {200, page} = get_msgs(token, "workspace=#{@ws}&limit=2")
    assert page["truncated"] == true
    assert page["latest_id"] == r
  end

  test "auth: no token and a wrong token are 401, on both", %{token: _token} do
    {401, _} = post_msg(nil, %{workspace: @ws, author: "a", subject: "s", body: "b"})
    {401, _} = post_msg("wrong", %{workspace: @ws, author: "a", subject: "s", body: "b"})
    {401, _} = get_msgs(nil, "workspace=#{@ws}")
    {401, _} = get_msgs("wrong", "workspace=#{@ws}")
    assert {:error, :not_found} = Workspaces.get_by_name(@ws)
  end

  test "4xx answers are {\"error\": sentence}", %{token: token} do
    base = %{workspace: @ws, author: "a1", subject: "s", body: "b"}

    assert {400, %{"error" => "invalid json"}} = post_msg(token, "{not json")
    assert {400, %{"error" => e}} = post_msg(token, "[1]")
    assert e =~ "JSON object"

    assert {422, %{"error" => e}} = post_msg(token, Map.put(base, :message, "x"))
    assert e =~ ~s(post_message has no argument "message")
    assert {422, %{"error" => e}} = post_msg(token, Map.put(base, :subject, "  "))
    assert e =~ "subject must not be blank"
    assert {422, %{"error" => e}} = post_msg(token, Map.delete(base, :author))
    assert e =~ "author is required"

    assert {422, %{"error" => e}} =
             post_msg(token, Map.put(base, :body, String.duplicate("é", 32_769)))

    assert e =~ "body is 65538 bytes; the limit is 65536"
    assert {422, %{"error" => e}} = post_msg(token, Map.put(base, :reply_to, 424_242))
    assert e =~ "reply_to: no message 424242 in this workspace"
    assert {422, %{"error" => e}} = post_msg(token, Map.put(base, :workspace, "*"))
    assert e =~ ~s(workspace "*" is read-only)

    # None of those created the workspace.
    assert {:error, :not_found} = Workspaces.get_by_name(@ws)

    assert {400, %{"error" => e}} = get_msgs(token, "workspace=#{@ws}&since_id=abc")
    assert e =~ "since_id must be an integer"
    assert {422, %{"error" => e}} = get_msgs(token, "workspace=#{@ws}&limit=500")
    assert e =~ "limit must be at most 200"
    assert {422, %{"error" => e}} = get_msgs(token, "workspace=#{@ws}&from=lead")
    assert e =~ ~s(read_messages has no argument "from")
    assert {422, %{"error" => e}} = get_msgs(token, "workspace=*")
    assert e =~ "read_messages reads one workspace's board"
    assert {422, %{"error" => e}} = get_msgs(token, "workspace=#{@ws}&author=")
    assert e =~ "author must not be blank"
  end

  test "an unknown workspace reads as an empty board and is not created", %{token: token} do
    assert {200, %{"messages" => [], "latest_id" => 7, "truncated" => false}} =
             get_msgs(token, "workspace=board-http-none&since_id=7")

    assert {:error, :not_found} = Workspaces.get_by_name("board-http-none")
  end

  describe "pinning, as /export and /ops" do
    test "a body naming another workspace than the pin is 403; none goes to the pin",
         %{token: token} do
      pin = [{"x-deciduous-workspace", "board-pin"}]
      base = %{author: "a1", subject: "s", body: "b"}

      assert {403, %{"error" => e}} =
               post_msg(token, Map.put(base, :workspace, "board-else"), pin)

      assert e =~ ~s(pinned to workspace "board-pin")

      {201, %{"id" => id}} = post_msg(token, base, pin)

      {201, _} = post_msg(token, Map.put(base, :workspace, "board-else"))

      # A pinned read gets the pin, whatever it names.
      {200, %{"messages" => [%{"id" => ^id}]}} = get_msgs(token, "workspace=board-else", pin)
      {200, %{"messages" => [%{"id" => ^id}]}} = get_msgs(token, "workspace=*", pin)
    end

    test "a reply_to into another workspace is refused", %{token: token} do
      {201, %{"id" => theirs}} =
        post_msg(token, %{workspace: "board-theirs", author: "x", subject: "s", body: "b"})

      assert {422, %{"error" => e}} =
               post_msg(token, %{
                 workspace: @ws,
                 author: "a",
                 subject: "s",
                 body: "b",
                 reply_to: theirs
               })

      assert e =~ "no message #{theirs} in this workspace"
    end
  end

  test "an HTTP post is recorded as the CLI's activity", %{token: token} do
    {201, _} =
      post_msg(
        token,
        %{workspace: @ws, author: "a1", subject: "s", body: "b", branch: "cli-branch"},
        [
          {"x-deciduous-repo-roots", "0123456789abcdef0123"}
        ]
      )

    {:ok, ws} = Workspaces.get_by_name(@ws)
    [seen] = DeciduousMcp.Activity.recent(ws.id)
    assert seen.branch == "cli-branch"
    assert seen.client_name == "deciduous CLI"
    assert seen.session_id == "cli:0123456789ab"
  end
end
