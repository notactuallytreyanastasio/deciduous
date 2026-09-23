defmodule DeciduousMcp.MCP.Tools.CheckActivity do
  @moduledoc """
  MCP Tool: who else is writing to this workspace, on what branch, and what
  did everyone just do.

  Read-only. `sessions` lists every session, MCP or CLI, that wrote to the
  workspace in the last five minutes (`DeciduousMcp.Activity`), with its
  branch and client, and whether it is this session. `branches` lists every
  branch with the most recent node on it and who has been writing it. In the
  first arena run the agents polled `query_nodes` for decisions because that
  was the only way to see what was new; this is the one call that replaces
  that poll.

  It used to list unexpired branch locks: ten-second leases that had always
  lapsed by the time anyone asked, so a run of short-lived clients read
  "0 active sessions" throughout, and CLI writes, which took no lock, never
  appeared at all (team probe T10).
  """
  use DeciduousMcp.MCP.Component, type: :tool

  alias DeciduousMcp.Activity
  alias DeciduousMcp.Graph.Nodes
  alias DeciduousMcp.MCP.Scope

  def definition do
    %{
      name: "check_activity",
      description:
        "Who has been writing to this workspace in the last five minutes: each session's " <>
          "branch, client (MCP or the deciduous CLI) and whether it is this session; and " <>
          "the most recent node on every branch, so one call shows what everyone else " <>
          "just did. Writes are never refused because someone else is writing; this is " <>
          "how you see them. Call it before a burst of writes and after every milestone, " <>
          "instead of polling query_nodes.",
      input_schema: %{
        type: "object",
        properties: %{
          branches: %{
            type: "integer",
            minimum: 0,
            maximum: 200,
            description:
              "How many branches to list under `branches`, most recently written first " <>
                "(default 20). `branches_total` says how many the workspace has in all."
          }
        }
      }
    }
    |> Scope.with_workspace_arg()
  end

  def call(%{arguments: args, server: frame}) do
    case Scope.read_scope(frame, args) do
      {:ok, :global} ->
        {:error, %{code: -1, message: "check_activity looks at one workspace; pass its name."}}

      {:ok, workspace_id} ->
        do_call(workspace_id, frame, branches_limit(args))

      {:error, message} ->
        {:error, %{code: -1, message: message}}
    end
  end

  defp branches_limit(args) do
    case Map.get(args, "branches") do
      n when is_integer(n) -> n |> max(0) |> min(200)
      _ -> 20
    end
  end

  defp do_call(workspace_id, frame, limit) do
    my_session = session_id(frame)
    now = DateTime.utc_now()
    seen = Activity.recent(workspace_id)
    by_branch = Enum.group_by(seen, & &1.branch)
    {recent, total} = Nodes.latest_per_branch(workspace_id, limit: limit)

    writer = fn a ->
      %{
        client: a.client_name,
        client_version: a.client_version,
        session: short_session(a.session_id),
        is_you: a.session_id == my_session,
        first_seen_at: DateTime.to_iso8601(a.first_seen_at),
        last_seen_at: DateTime.to_iso8601(a.last_seen_at),
        seconds_ago: max(DateTime.diff(now, a.last_seen_at, :second), 0)
      }
    end

    result = %{
      window_seconds: Activity.window_seconds(),
      active_sessions: seen |> Enum.map(& &1.session_id) |> Enum.uniq() |> length(),
      branches_total: total,
      branches:
        Enum.map(recent, fn node ->
          branch = get_in(node.metadata, ["branch"]) || ""

          %{
            branch: if(branch == "", do: nil, else: branch),
            last_node: %{
              id: node.id,
              change_id: node.change_id,
              node_type: node.node_type,
              title: node.title,
              status: node.status,
              created_at: DateTime.to_iso8601(node.inserted_at)
            },
            writers: Enum.map(Map.get(by_branch, branch, []), writer)
          }
        end),
      sessions:
        Enum.map(seen, fn a ->
          Map.put(writer.(a), :branch, if(a.branch == "", do: nil, else: a.branch))
        end)
    }

    {:ok, Jason.encode!(result)}
  end

  defp short_session(id) do
    id
    |> String.replace_prefix("session_", "")
    |> String.slice(0, 8)
  end

  defp session_id(frame) do
    frame.private
    |> Map.new()
    |> Map.get(:session_id)
  end
end
