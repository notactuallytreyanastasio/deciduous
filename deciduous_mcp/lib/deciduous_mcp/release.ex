defmodule DeciduousMcp.Release do
  @moduledoc """
  Release tasks, run with `bin/deciduous_mcp eval`.

  A release has no Mix, so `mix ecto.migrate` does not exist in the container.
  The entrypoint calls `DeciduousMcp.Release.migrate/0` before starting the
  server.
  """
  @app :deciduous_mcp

  def migrate do
    load_app()

    for repo <- repos() do
      {:ok, _, _} = Ecto.Migrator.with_repo(repo, &Ecto.Migrator.run(&1, :up, all: true))
    end
  end

  def rollback(repo, version) do
    load_app()
    {:ok, _, _} = Ecto.Migrator.with_repo(repo, &Ecto.Migrator.run(&1, :down, to: version))
  end

  defp repos do
    Application.fetch_env!(@app, :ecto_repos)
  end

  defp load_app do
    # `Application.ensure_loaded/1`, not `ensure_all_started/1`: migrations run
    # before the supervision tree, and starting the app here would boot the web
    # server against a database that has not been migrated yet.
    Application.ensure_loaded(@app)
  end
end
