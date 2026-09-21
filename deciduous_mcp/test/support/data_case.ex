defmodule DeciduousMcp.DataCase do
  @moduledoc """
  Test case template for tests that require database access.
  Uses Ecto.Adapters.SQL.Sandbox for test isolation.
  """
  use ExUnit.CaseTemplate

  using do
    quote do
      alias DeciduousMcp.Repo
      import Ecto
      import Ecto.Changeset
      import Ecto.Query
      import DeciduousMcp.DataCase
    end
  end

  setup tags do
    pid = Ecto.Adapters.SQL.Sandbox.start_owner!(DeciduousMcp.Repo, shared: not tags[:async])
    on_exit(fn -> Ecto.Adapters.SQL.Sandbox.stop_owner(pid) end)

    :ok
  end

  @doc """
  Creates a workspace for testing. Returns the workspace.
  """
  def create_test_workspace(name \\ "test-workspace") do
    {:ok, workspace} = DeciduousMcp.Graph.Workspaces.find_or_create(name)
    workspace
  end
end
