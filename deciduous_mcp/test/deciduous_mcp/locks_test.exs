defmodule DeciduousMcp.LocksTest do
  use DeciduousMcp.DataCase, async: true

  alias DeciduousMcp.Locks

  setup do
    {:ok, workspace: create_test_workspace("locks-test")}
  end

  test "acquiring a free key succeeds", %{workspace: ws} do
    assert {:ok, lock} = Locks.acquire(ws.id, "main", "sess-a", "claude-code", "1.0")
    assert lock.session_id == "sess-a"
    assert lock.lock_key == "main"
  end

  test "a second session cannot claim a key someone else holds", %{workspace: ws} do
    assert {:ok, _} = Locks.acquire(ws.id, "main", "sess-a", "claude-code", "1.0")

    assert {:error, holder} = Locks.acquire(ws.id, "main", "sess-b", "claude-code", "1.0")
    assert holder.session_id == "sess-a"
  end

  test "the same session renews its own lock instead of conflicting with itself", %{
    workspace: ws
  } do
    assert {:ok, first} = Locks.acquire(ws.id, "main", "sess-a", "claude-code", "1.0")
    # A tiny sleep so a renewal's expires_at is observably later than the
    # first acquire's, proving this took the renew path and not a no-op.
    Process.sleep(5)
    assert {:ok, second} = Locks.acquire(ws.id, "main", "sess-a", "claude-code", "1.0")

    assert DateTime.compare(second.expires_at, first.expires_at) == :gt
  end

  test "acquired_at does not move on renewal, only expires_at does", %{workspace: ws} do
    assert {:ok, first} = Locks.acquire(ws.id, "main", "sess-a", "claude-code", "1.0")
    Process.sleep(5)
    assert {:ok, second} = Locks.acquire(ws.id, "main", "sess-a", "claude-code", "1.0")

    # A "how long has this session been active" reading should reflect when
    # it first showed up, not the most recent write.
    assert DateTime.compare(second.acquired_at, first.acquired_at) == :eq
  end

  test "a different session takes over once the lease has expired", %{workspace: ws} do
    assert {:ok, _} = Locks.acquire(ws.id, "main", "sess-a", "claude-code", "1.0", 0)
    # lease_seconds: 0 means it is already expired by the time this returns.
    assert {:ok, lock} = Locks.acquire(ws.id, "main", "sess-b", "claude-code", "1.0")
    assert lock.session_id == "sess-b"
  end

  test "different branches do not contend by default", %{workspace: ws} do
    assert {:ok, _} = Locks.acquire(ws.id, "feature-x", "sess-a", "claude-code", "1.0")
    assert {:ok, _} = Locks.acquire(ws.id, "feature-y", "sess-b", "claude-code", "1.0")
  end

  test "lock_key_for uses the branch by default", %{workspace: ws} do
    assert Locks.lock_key_for(ws, "feature-x") == "feature-x"
    assert Locks.lock_key_for(ws, nil) == ""
  end

  test "lock_key_for collapses every branch to one key when configured workspace-wide" do
    ws = create_test_workspace("locks-workspace-wide")

    {:ok, ws} =
      DeciduousMcp.Repo.update(
        Ecto.Changeset.change(ws, settings: %{"lock_scope" => "workspace"})
      )

    assert Locks.lock_key_for(ws, "feature-x") == "*"
    assert Locks.lock_key_for(ws, "feature-y") == "*"
  end

  test "active/1 lists only unexpired locks, most recent first", %{workspace: ws} do
    {:ok, _} = Locks.acquire(ws.id, "old", "sess-old", "claude-code", "1.0", 0)
    Process.sleep(5)
    {:ok, _} = Locks.acquire(ws.id, "new", "sess-new", "claude-code", "1.0")

    keys = ws.id |> Locks.active() |> Enum.map(& &1.lock_key)
    assert keys == ["new"]
  end

  test "active/1 on a workspace with no locks is an empty list", %{workspace: ws} do
    assert Locks.active(ws.id) == []
  end

  test "holder/2 returns nil once a lease has expired", %{workspace: ws} do
    {:ok, _} = Locks.acquire(ws.id, "main", "sess-a", "claude-code", "1.0", 0)
    assert Locks.holder(ws.id, "main") == nil
  end
end
