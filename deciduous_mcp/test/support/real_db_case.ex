defmodule DeciduousMcp.RealDbCase do
  @moduledoc """
  For tests about concurrency: no sandbox, every process gets its own
  connection and its own transaction, as on a real server.

  The sandbox exists to isolate tests, and it does that by funnelling every
  process through one connection inside one transaction. That serializes
  exactly the interleavings a race test is written to provoke: two inserts
  of the same workspace name can never both pass a check-then-insert when
  they run one after the other on one connection.

  Anything a test creates must be named with `unique/1`; the prefix is
  deleted (cascading to nodes, edges and locks) when the test ends.
  """
  use ExUnit.CaseTemplate

  using do
    quote do
      import DeciduousMcp.RealDbCase
    end
  end

  setup do
    prefix = "realdb-#{System.unique_integer([:positive])}-"
    Ecto.Adapters.SQL.Sandbox.mode(DeciduousMcp.Repo, :auto)

    on_exit(fn ->
      # Edges first: deleting one writes a tombstone (chapter 29's trigger),
      # which fails its foreign key once the workspace row is gone, as it
      # is while a workspace delete cascades to the edges.
      DeciduousMcp.Repo.query!(
        "DELETE FROM decision_edges WHERE workspace_id IN (SELECT id FROM workspaces WHERE name LIKE $1)",
        [prefix <> "%"]
      )

      DeciduousMcp.Repo.query!("DELETE FROM workspaces WHERE name LIKE $1", [prefix <> "%"])
      Ecto.Adapters.SQL.Sandbox.mode(DeciduousMcp.Repo, :manual)
    end)

    %{prefix: prefix}
  end

  def unique(%{prefix: prefix}, name), do: prefix <> name
end
