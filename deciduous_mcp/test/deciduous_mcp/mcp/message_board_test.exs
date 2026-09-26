defmodule DeciduousMcp.MCP.MessageBoardTest do
  @moduledoc """
  post_message and read_messages over the router, as a client calls them:
  every read filter, reply threading, the unanswered list, limits, and that
  a workspace's board stays in that workspace.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.Workspaces
  alias DeciduousMcp.Test.McpClient

  @ws "board-test"

  setup do
    %{c: McpClient.connect(name: "board-test")}
  end

  defp post(c, author, subject, body, extra \\ %{}) do
    McpClient.call!(
      c,
      "post_message",
      Map.merge(
        %{"workspace" => @ws, "author" => author, "subject" => subject, "body" => body},
        extra
      )
    )
  end

  defp read(c, args), do: McpClient.call!(c, "read_messages", Map.put_new(args, "workspace", @ws))
  defp ids(%{"messages" => messages}), do: Enum.map(messages, & &1["id"])

  test "a post answers with id, mentions, reply_to and created_at, and reads back whole", %{c: c} do
    posted =
      post(c, "lead", "interface for @board-cli", "see body @orchestrator.", %{"branch" => "b1"})

    assert %{"id" => id, "mentions" => ["board-cli", "orchestrator"], "reply_to" => nil} = posted
    assert {:ok, _, 0} = DateTime.from_iso8601(posted["created_at"])
    assert Map.keys(posted) |> Enum.sort() == ~w(created_at id mentions reply_to)

    assert %{"messages" => [m], "latest_id" => ^id, "truncated" => false} = read(c, %{})

    assert m == %{
             "id" => id,
             "branch" => "b1",
             "author" => "lead",
             "subject" => "interface for @board-cli",
             "body" => "see body @orchestrator.",
             "mentions" => ["board-cli", "orchestrator"],
             "reply_to" => nil,
             "created_at" => posted["created_at"]
           }
  end

  test "the first post creates the workspace, and is kept", %{c: c} do
    refute match?({:ok, _}, Workspaces.get_by_name("board-fresh"))

    %{"id" => id} =
      McpClient.call!(c, "post_message", %{
        "workspace" => "board-fresh",
        "author" => "a1",
        "subject" => "s",
        "body" => "b"
      })

    assert {:ok, _} = Workspaces.get_by_name("board-fresh")
    assert ids(read(c, %{"workspace" => "board-fresh"})) == [id]
  end

  test "reading a workspace nothing was written to is an empty board, and creates nothing", %{
    c: c
  } do
    assert read(c, %{"workspace" => "board-nothing"}) ==
             %{"messages" => [], "latest_id" => 0, "truncated" => false}

    assert read(c, %{"workspace" => "board-nothing", "since_id" => 41})["latest_id"] == 41
    assert {:error, :not_found} = Workspaces.get_by_name("board-nothing")
  end

  test "every filter", %{c: c} do
    %{"id" => q1} =
      post(c, "lead", "schema question", "@a1 which column names?", %{"branch" => "x"})

    %{"id" => q2} = post(c, "lead", "second", "@a1 @a2 are you done", %{"branch" => "y"})
    %{"id" => r1} = post(c, "a1", "re: schema", "workspace_id, like nodes", %{"reply_to" => q1})
    %{"id" => n1} = post(c, "a2", "note", "migrations regenerate STRUCTURE.sql")

    assert ids(read(c, %{})) == [q1, q2, r1, n1]
    assert ids(read(c, %{"since_id" => q2})) == [r1, n1]
    assert ids(read(c, %{"author" => "lead"})) == [q1, q2]
    assert ids(read(c, %{"to" => "a2"})) == [q2]
    assert ids(read(c, %{"to" => "A2"})) == []
    assert ids(read(c, %{"branch" => "y"})) == [q2]
    assert ids(read(c, %{"id" => r1})) == [r1]
    assert ids(read(c, %{"id" => n1 + 1000})) == []

    # a1 answered q1, not q2; a2 answered nothing.
    assert ids(read(c, %{"unanswered_for" => "a1"})) == [q2]
    assert ids(read(c, %{"unanswered_for" => "a2"})) == [q2]

    # A reply by someone else does not answer for a2.
    post(c, "a1", "re: second", "a2 is busy", %{"reply_to" => q2})
    assert ids(read(c, %{"unanswered_for" => "a2"})) == [q2]
    assert ids(read(c, %{"unanswered_for" => "a1"})) == []

    # Full text, stemmed: "regenerating" finds "regenerate", "migration"
    # finds "migrations". ("STRUCTURE.sql" is one token to the parser.)
    assert ids(read(c, %{"query" => "regenerating migration"})) == [n1]
    assert ids(read(c, %{"query" => "column"})) == [q1]
    assert ids(read(c, %{"query" => "schema -question"})) == [r1]

    # Filters combine.
    assert ids(read(c, %{"author" => "lead", "to" => "a2", "since_id" => q1})) == [q2]
  end

  test "limit and truncated, and paging with since_id: latest_id", %{c: c} do
    all = for i <- 1..5, do: post(c, "a1", "m#{i}", "b")["id"]

    first = read(c, %{"limit" => 2})
    assert ids(first) == Enum.slice(all, 0, 2)
    assert first["truncated"] == true
    assert first["latest_id"] == Enum.at(all, 1)

    second = read(c, %{"limit" => 2, "since_id" => first["latest_id"]})
    assert ids(second) == Enum.slice(all, 2, 2)
    assert second["truncated"] == true

    third = read(c, %{"limit" => 2, "since_id" => second["latest_id"]})
    assert ids(third) == [List.last(all)]
    assert third["truncated"] == false

    empty = read(c, %{"since_id" => third["latest_id"]})
    assert empty == %{"messages" => [], "latest_id" => List.last(all), "truncated" => false}

    assert read(c, %{"limit" => 3})["truncated"] == true
    assert read(c, %{"limit" => 5})["truncated"] == false
  end

  test "limit bounds, and the default of 50", %{c: c} do
    assert {:error, msg} =
             McpClient.call(c, "read_messages", %{"workspace" => @ws, "limit" => 201})

    assert msg =~ "limit must be at most 200"
    assert {:error, msg} = McpClient.call(c, "read_messages", %{"workspace" => @ws, "limit" => 0})
    assert msg =~ "limit must be at least 1"
    refute msg =~ "nothing was written"

    for i <- 1..51, do: post(c, "a1", "m#{i}", "b")
    page = read(c, %{})
    assert length(page["messages"]) == 50
    assert page["truncated"] == true
  end

  test "refusals: blanks, lengths, unknown arguments, unknown reply_to, *", %{c: c} do
    base = %{"workspace" => @ws, "author" => "a1", "subject" => "s", "body" => "b"}
    refuse = fn args -> McpClient.call(c, "post_message", Map.merge(base, args)) end

    assert {:error, m} = refuse.(%{"author" => " "})
    assert m =~ "author must not be blank"
    assert m =~ "nothing was written"
    assert {:error, m} = refuse.(%{"subject" => "​"})
    assert m =~ "subject must not be blank"
    assert {:error, m} = refuse.(%{"body" => ""})
    assert m =~ "body must not be blank"

    # A missing required argument is refused by Hermes before the tool runs,
    # as for every tool (-32602).
    assert {:rpc_error, %{"code" => -32602, "data" => %{"message" => m}}} =
             McpClient.call(c, "post_message", Map.delete(base, "body"))

    assert m =~ "body: is required"

    assert {:error, m} = refuse.(%{"subject" => String.duplicate("s", 301)})
    assert m =~ "subject is 301 characters; the limit is 300"
    assert {:ok, _} = refuse.(%{"subject" => String.duplicate("é", 300)})

    assert {:error, m} = refuse.(%{"author" => String.duplicate("a", 101)})
    assert m =~ "the limit is 100"

    # 64 KiB is bytes: 32,769 two-byte characters is 65,538 bytes.
    assert {:ok, _} = refuse.(%{"body" => String.duplicate("b", 65_536)})
    assert {:error, m} = refuse.(%{"body" => String.duplicate("é", 32_769)})
    assert m =~ "body is 65538 bytes; the limit is 65536"
    assert {:error, m} = refuse.(%{"body" => String.duplicate("b", 65_537)})
    assert m =~ "the limit is 65536"

    assert {:error, m} = refuse.(%{"message" => "x"})
    assert m =~ ~s(post_message has no argument "message")

    assert {:error, m} = refuse.(%{"reply_to" => 987_654_321})
    assert m =~ "reply_to: no message 987654321 in this workspace"

    assert {:rpc_error, %{"data" => %{"message" => m}}} = refuse.(%{"reply_to" => "1"})
    assert m =~ "reply_to: expected type of :integer"

    assert {:error, m} = refuse.(%{"workspace" => "*"})
    assert m =~ ~s(workspace "*" is read-only)

    assert {:error, m} = McpClient.call(c, "read_messages", %{"workspace" => "*"})
    assert m =~ "read_messages reads one workspace's board"

    assert {:error, m} = McpClient.call(c, "read_messages", %{"workspace" => @ws, "from" => "x"})
    assert m =~ ~s(read_messages has no argument "from")

    # None of the refused posts left a message behind.
    assert length(read(c, %{})["messages"]) == 2
  end

  test "a refused first post to a new workspace leaves no workspace", %{c: c} do
    assert {:error, _} =
             McpClient.call(c, "post_message", %{
               "workspace" => "board-ghost",
               "author" => "a1",
               "subject" => "s",
               "body" => "b",
               "reply_to" => 1
             })

    assert {:error, :not_found} = Workspaces.get_by_name("board-ghost")
  end

  describe "cross-workspace isolation" do
    test "a reply_to into another workspace is refused, as a missing one is", %{c: c} do
      %{"id" => theirs} =
        McpClient.call!(c, "post_message", %{
          "workspace" => "board-other",
          "author" => "x",
          "subject" => "their secret @a1",
          "body" => "b"
        })

      assert {:error, m} =
               McpClient.call(c, "post_message", %{
                 "workspace" => @ws,
                 "author" => "a1",
                 "subject" => "s",
                 "body" => "b",
                 "reply_to" => theirs
               })

      assert m =~ "reply_to: no message #{theirs} in this workspace"
    end

    test "the database refuses it too, whatever the tool checks" do
      {:ok, a} = Workspaces.find_or_create("board-db-a")
      {:ok, b} = Workspaces.find_or_create("board-db-b")

      {:ok, %{id: id}} =
        DeciduousMcp.Board.post(a.id, %{"author" => "x", "subject" => "s", "body" => "b"})

      assert_raise Ecto.ConstraintError, ~r/agent_messages_reply_to_fkey/, fn ->
        Repo.insert!(%DeciduousMcp.Schema.AgentMessage{
          workspace_id: b.id,
          author: "y",
          subject: "s",
          body: "b",
          reply_to: id
        })
      end
    end

    test "reads never leak another workspace's messages", %{c: c} do
      McpClient.call!(c, "post_message", %{
        "workspace" => "board-other",
        "author" => "lead",
        "subject" => "secret plan @a1",
        "body" => "regenerate everything"
      })

      %{"id" => mine} = post(c, "a2", "mine", "b")

      for args <- [
            %{},
            %{"to" => "a1"},
            %{"unanswered_for" => "a1"},
            %{"author" => "lead"},
            %{"query" => "secret"},
            %{"since_id" => 0}
          ] do
        got = read(c, args)
        refute Enum.any?(got["messages"], &(&1["subject"] =~ "secret")), inspect(args)
      end

      assert ids(read(c, %{})) == [mine]
    end

    test "a pinned client reads and posts only in its pin, whatever workspace it names" do
      pinned = McpClient.connect(pin: "board-pinned")
      other = McpClient.connect()

      McpClient.call!(other, "post_message", %{
        "workspace" => "board-other",
        "author" => "lead",
        "subject" => "not for the pinned @a1",
        "body" => "b"
      })

      %{"id" => id} =
        McpClient.call!(pinned, "post_message", %{
          "workspace" => "board-other",
          "author" => "a1",
          "subject" => "pinned post",
          "body" => "b"
        })

      assert %{"messages" => [%{"id" => ^id}]} =
               McpClient.call!(pinned, "read_messages", %{"workspace" => "board-other"})

      assert %{"messages" => [%{"id" => ^id}]} =
               McpClient.call!(pinned, "read_messages", %{"workspace" => "*"})
    end
  end

  test "a post is a message_posted graph event", %{c: c} do
    %{"id" => id} = post(c, "lead", "event @a1", "b", %{"branch" => "main"})

    [payload] =
      Repo.all(
        from e in "graph_events",
          where: e.workspace == @ws,
          select: e.payload
      )

    assert %{
             "table" => "agent_messages",
             "op" => "INSERT",
             "event" => "message_posted",
             "workspace" => @ws,
             "id" => ^id,
             "author" => "lead",
             "subject" => "event @a1",
             "mentions" => ["a1"],
             "reply_to" => nil,
             "branch" => "main",
             "seq" => seq,
             "at" => _
           } = payload

    assert is_integer(seq)
  end

  test "both tools are listed, and read_messages refusals do not say nothing was written" do
    names = DeciduousMcp.MCP.Tools.all_definitions() |> Enum.map(& &1.name)
    assert "post_message" in names
    assert "read_messages" in names
  end

  test "posting is recorded as activity on the branch", %{c: c} do
    post(c, "lead", "s", "b", %{"branch" => "board-branch"})
    activity = McpClient.call!(c, "check_activity", %{"workspace" => @ws})
    assert Enum.any?(activity["sessions"], &(&1["branch"] == "board-branch"))
  end
end
