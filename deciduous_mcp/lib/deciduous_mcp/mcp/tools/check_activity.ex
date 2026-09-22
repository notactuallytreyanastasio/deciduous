defmodule DeciduousMcp.MCP.Tools.CheckActivity do
  @moduledoc """
  MCP Tool: check whether another session is actively writing to this
  workspace right now, and on what branch.

  Read-only — it reports the same lock state `add_node` and friends contend
  for, but taking this call never claims or renews anything. An agent that
  starts a fresh branch off main and wants to know if it's about to collide
  with someone else can call this before writing a single node.
  """
  use DeciduousMcp.MCP.Component, type: :tool

  alias DeciduousMcp.Locks
  alias DeciduousMcp.MCP.Scope

  def definition do
    %{
      name: "check_activity",
      description:
        "List active write sessions in a workspace — which branches have a " <>
          "session actively writing right now, which client, and whether it's " <>
          "this session. Call this before a burst of writes to see if another " <>
          "agent, possibly on a different branch off main, is already in here.",
      input_schema: %{
        type: "object",
        properties: %{}
      }
    }
    |> Scope.with_workspace_arg()
  end

  def call(%{arguments: args, server: frame}) do
    case Scope.read_scope(frame, args) do
      {:ok, :global} ->
        {:error, %{code: -1, message: "check_activity looks at one workspace; pass its name."}}

      {:ok, workspace_id} ->
        do_call(workspace_id, frame)

      {:error, message} ->
        {:error, %{code: -1, message: message}}
    end
  end

  defp do_call(workspace_id, frame) do
    my_session = session_id(frame)
    locks = Locks.active(workspace_id)

    result = %{
      active_sessions: length(locks),
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

  defp session_id(frame) do
    frame.private
    |> Map.new()
    |> Map.get(:session_id)
  end
end
