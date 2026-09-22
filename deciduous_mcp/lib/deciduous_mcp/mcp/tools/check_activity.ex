defmodule DeciduousMcp.MCP.Tools.CheckActivity do
  @moduledoc """
  MCP Tool: check whether another session is actively writing to this
  workspace right now, and on what branch.

  Read-only — it reports the same lock state `add_node` and friends contend
  for, but taking this call never claims or renews anything. An agent that
  starts a fresh branch off main and wants to know if it's about to collide
  with someone else can call this before writing a single node.

  It also answers the question the locks alone did not: what did everyone
  just do. `branches` lists every branch in the workspace with the most
  recent node on it, lock or no lock. In the first arena run the agents
  polled `query_nodes` for decisions because that was the only way to see
  what was new; this is the one call that replaces that poll.
  """
  use DeciduousMcp.MCP.Component, type: :tool

  alias DeciduousMcp.Graph.Nodes
  alias DeciduousMcp.Locks
  alias DeciduousMcp.MCP.Scope

  def definition do
    %{
      name: "check_activity",
      description:
        "List active write sessions in a workspace — which branches have a " <>
          "session actively writing right now, which client, and whether it's " <>
          "this session — and the most recent node on every branch, so one call " <>
          "shows what everyone else just did. Call this before a burst of writes, " <>
          "and after every milestone, instead of polling query_nodes.",
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
    locks = Locks.active(workspace_id)
    lock_by_branch = Map.new(locks, &{&1.lock_key, &1})
    {recent, total} = Nodes.latest_per_branch(workspace_id, limit: limit)

    result = %{
      active_sessions: length(locks),
      branches_total: total,
      branches:
        Enum.map(recent, fn node ->
          branch = get_in(node.metadata, ["branch"]) || ""
          # Under lock_scope "workspace" every branch shares the one key "*",
          # so a branch row looked up by its own name would show nobody
          # holding anything while every write is blocked.
          lock = lock_by_branch[branch] || lock_by_branch["*"]

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
            locked_by:
              if lock do
                %{
                  client: lock.client_name,
                  session: short_session(lock.session_id),
                  is_you: lock.session_id == my_session
                }
              end
          }
        end),
      sessions:
        Enum.map(locks, fn l ->
          %{
            branch: if(l.lock_key in ["", "*"], do: nil, else: l.lock_key),
            workspace_wide_lock: l.lock_key == "*",
            client: l.client_name,
            client_version: l.client_version,
            is_you: l.session_id == my_session,
            started_at: DateTime.to_iso8601(l.acquired_at),
            expires_in_seconds: max(DateTime.diff(l.expires_at, DateTime.utc_now(), :second), 0)
          }
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
