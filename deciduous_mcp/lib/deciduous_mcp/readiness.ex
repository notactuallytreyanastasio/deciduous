defmodule DeciduousMcp.Readiness do
  @moduledoc "Checks whether the database has the migrations required by this release."

  alias DeciduousMcp.Repo

  # The migration files are shipped in both the OTP and Burrito releases.
  # Baking their versions into the module keeps each probe to one DB query.
  @migration_pattern Path.expand("../../priv/repo/migrations/*.exs", __DIR__)
  @migration_files Path.wildcard(@migration_pattern)
  if @migration_files == [], do: raise("release must include database migrations")

  for file <- @migration_files do
    @external_resource file
  end

  @versions Enum.map(@migration_files, fn file ->
              file
              |> Path.basename()
              |> String.split("_", parts: 2)
              |> hd()
              |> String.to_integer()
            end)

  # @external_resource detects edits to known files; the glob also needs to
  # invalidate this module when a migration is added or removed.
  def __mix_recompile__?, do: Path.wildcard(@migration_pattern) != @migration_files

  # How long /ready may take before it answers "not ready". The probe runs in
  # a task so it can wait its turn in a busy pool (a burst of tool calls
  # holding every connection is a busy server, not an unready one) while the
  # endpoint still answers promptly when the database is gone: with no live
  # connection the checkout queue is not bounded by the query's :timeout,
  # and CI's native smoke test caught /ready never answering at all.
  @budget_ms 1_500

  def check do
    task = Task.async(&probe/0)

    case Task.yield(task, @budget_ms) || Task.shutdown(task, :brutal_kill) do
      {:ok, result} -> result
      _ -> :unavailable
    end
  end

  defp probe do
    case Ecto.Adapters.SQL.query(Repo, "SELECT version FROM schema_migrations", [],
           timeout: 1_000,
           log: false
         ) do
      {:ok, %{rows: rows}} ->
        applied = MapSet.new(rows, &hd/1)
        if Enum.all?(@versions, &MapSet.member?(applied, &1)), do: :ok, else: :unavailable

      {:error, _} ->
        :unavailable
    end
  rescue
    _ -> :unavailable
  catch
    :exit, _ -> :unavailable
  end
end
