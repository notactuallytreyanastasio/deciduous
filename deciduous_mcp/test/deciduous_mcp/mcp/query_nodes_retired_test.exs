defmodule DeciduousMcp.MCP.QueryNodesRetiredTest do
  @moduledoc """
  query_nodes can leave out retired notes (observations whose status is
  `superseded` or `abandoned`) with `exclude_retired: true`, before `limit`.
  A retired decision, action or any other type is kept. Without the argument,
  and without the server setting that changes the default, it returns
  everything as it always did. An explicit `status` always wins.

  Through the whole router, so the argument schema is exercised.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.{Nodes, Workspaces}
  alias DeciduousMcp.Test.McpClient

  @ws "qn-retired"

  setup do
    {:ok, ws} = Workspaces.find_or_create(@ws)
    files = %{"files" => ["src/a.rs"]}

    # The retired notes are created last, so they are the newest and a limit
    # applied before the filter would hand them back.
    for {type, title, status} <- [
          {"goal", "live goal", "active"},
          {"outcome", "done outcome", "completed"},
          {"option", "rejected option", "rejected"},
          {"decision", "old decision", "superseded"},
          {"action", "dropped action", "abandoned"},
          {"observation", "live note", "completed"},
          {"observation", "old note", "superseded"},
          {"observation", "scratch note", "abandoned"}
        ] do
      {:ok, _} =
        Nodes.create_node(ws.id, %{
          node_type: type,
          title: title,
          status: status,
          metadata:
            if(title in ["live goal", "old decision", "dropped action", "old note"],
              do: files,
              else: %{}
            )
        })
    end

    %{client: McpClient.connect()}
  end

  defp query(client, args),
    do: McpClient.call!(client, "query_nodes", Map.put(args, "workspace", @ws))

  defp titles(answer), do: answer["nodes"] |> Enum.map(& &1["title"]) |> Enum.sort()

  defp put_defaults(settings) do
    previous = Application.fetch_env(:deciduous_mcp, :read_defaults)
    Application.put_env(:deciduous_mcp, :read_defaults, settings)

    on_exit(fn ->
      case previous do
        {:ok, value} -> Application.put_env(:deciduous_mcp, :read_defaults, value)
        :error -> Application.delete_env(:deciduous_mcp, :read_defaults)
      end
    end)
  end

  test "default_returns_retired", %{client: client} do
    answer = query(client, %{})

    assert answer["count"] == 8
    assert "old note" in titles(answer)
    assert "scratch note" in titles(answer)
    assert answer["exclude_retired"] == false
    assert query(client, %{"exclude_retired" => false}) == answer
  end

  test "exclude_retired_drops_superseded_and_abandoned_notes", %{client: client} do
    answer = query(client, %{"exclude_retired" => true})

    # The old decision and the dropped action are retired too, and stay.
    assert titles(answer) ==
             [
               "done outcome",
               "dropped action",
               "live goal",
               "live note",
               "old decision",
               "rejected option"
             ]

    assert answer["exclude_retired"] == true

    # Before the limit: six came back because six nodes are kept, not because
    # the six newest happened to be (the two newest are hidden notes).
    limited = query(client, %{"exclude_retired" => true, "limit" => 6})
    assert titles(limited) == titles(answer)

    # The server's setting does the same with no argument, and an argument
    # that says false still beats it.
    put_defaults(detail: "full", hide_retired: true)
    assert titles(query(client, %{})) == titles(answer)
    assert query(client, %{"exclude_retired" => false})["count"] == 8
  end

  test "explicit_status_wins", %{client: client} do
    answer = query(client, %{"exclude_retired" => true, "status" => "superseded"})
    assert titles(answer) == ["old decision", "old note"]

    put_defaults(detail: "full", hide_retired: true)
    assert titles(query(client, %{"status" => "abandoned"})) == ["dropped action", "scratch note"]
  end

  test "file_branch_honours_exclude_retired", %{client: client} do
    all = query(client, %{"file" => "src/a.rs"})
    assert titles(all) == ["dropped action", "live goal", "old decision", "old note"]
    assert all["exclude_retired"] == false

    live = query(client, %{"file" => "src/a.rs", "exclude_retired" => true})
    assert titles(live) == ["dropped action", "live goal", "old decision"]
    assert live["exclude_retired"] == true
    # What names the file is counted before any filter, as it says.
    assert live["file_node_count"] == 4

    explicit =
      query(client, %{"file" => "src/a.rs", "exclude_retired" => true, "status" => "superseded"})

    assert titles(explicit) == ["old decision", "old note"]
  end
end
