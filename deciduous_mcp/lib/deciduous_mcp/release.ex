defmodule DeciduousMcp.Release do
  @moduledoc """
  Release tasks.

  A release has no Mix, so `mix ecto.migrate` does not exist in the container.
  The container's entrypoint calls `migrate/0` with `bin/deciduous_mcp eval`
  before starting the server.

  The single-file executable has no `eval`. A server run as a service on the
  machine sets `DECIDUOUS_PREPARE_DATABASE=true` and calls
  `prepare_database/0` from `DeciduousMcp.Application.start/2` instead, and
  `deciduous remote setup` runs it alone (`DECIDUOUS_MCP_COMMAND=prepare-database`)
  to check a database URL before installing anything.
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

  @doc """
  Connects, creates the database if it does not exist, and migrates.

  Returns `:ok` or `{:error, message}` where the message names the database
  (never its password) and carries PostgreSQL's own reason: a refused
  connection, a failed password, a missing role, no CREATEDB.
  """
  def prepare_database do
    load_app()

    Enum.reduce_while(repos(), :ok, fn repo, :ok ->
      with :ok <- ensure_database(repo.config()),
           :ok <- migrate_repo(repo) do
        {:cont, :ok}
      else
        error -> {:halt, error}
      end
    end)
  end

  @doc """
  Connects with `config` (a repo's configuration) and creates the database
  when PostgreSQL says it does not exist. `:ok` or `{:error, message}`.
  """
  def ensure_database(config) do
    database = Keyword.fetch!(config, :database)

    case probe(config) do
      :ok ->
        :ok

      {:error, %Postgrex.Error{postgres: %{code: :invalid_catalog_name}}} ->
        # Connects to the `postgres` maintenance database to run CREATE
        # DATABASE, which needs the CREATEDB privilege.
        case Ecto.Adapters.Postgres.storage_up(config) do
          :ok ->
            IO.puts("created database #{database}")
            :ok

          {:error, :already_up} ->
            :ok

          {:error, reason} ->
            {:error,
             "database #{inspect(database)} does not exist on #{describe(config)}, " <>
               "and creating it failed: #{format(reason)}\n" <>
               "Create it as a role that may (createdb #{database}), " <>
               "or grant this role CREATEDB, and run setup again."}
        end

      {:error, error} ->
        {:error, "cannot connect to #{describe(config)}: #{format(error)}"}
    end
  end

  # One connection, made synchronously, so the answer is the server's own
  # reason ("password authentication failed", "database ... does not exist",
  # "connection refused") rather than a pool that retries and then reports
  # only that it timed out. A pooled Postgrex connection that fails with
  # backoff_type: :stop takes its pool down as `:killed`, losing the reason;
  # Postgrex.Notifications with sync_connect returns it from start_link.
  defp probe(config) do
    {:ok, _} = Application.ensure_all_started(:postgrex)

    opts =
      config
      |> Keyword.drop([:name, :log, :pool, :pool_size])
      |> Keyword.merge(sync_connect: true, auto_reconnect: false)

    task =
      Task.async(fn ->
        # init's {:stop, reason} also arrives as an exit signal.
        Process.flag(:trap_exit, true)

        case Postgrex.Notifications.start_link(opts) do
          {:ok, conn} ->
            GenServer.stop(conn)
            :ok

          {:error, reason} ->
            {:error, reason}
        end
      end)

    case Task.yield(task, 20_000) || Task.shutdown(task, :brutal_kill) do
      {:ok, result} -> result
      nil -> {:error, "no answer within 20 seconds"}
    end
  end

  defp migrate_repo(repo) do
    case Ecto.Migrator.with_repo(repo, &Ecto.Migrator.run(&1, :up, all: true)) do
      {:ok, _, _} ->
        :ok

      {:error, reason} ->
        {:error, "migrating #{describe(repo.config())} failed: #{format(reason)}"}
    end
  rescue
    e -> {:error, "migrating #{describe(repo.config())} failed: #{Exception.message(e)}"}
  end

  defp describe(config) do
    user = Keyword.get(config, :username) || System.get_env("USER")
    host = Keyword.get(config, :hostname) || Keyword.get(config, :socket_dir) || "localhost"
    port = Keyword.get(config, :port, 5432)
    "#{user}@#{host}:#{port}/#{Keyword.get(config, :database)}"
  end

  defp format(reason) when is_binary(reason), do: reason
  defp format(%{__exception__: true} = e), do: Exception.message(e)
  defp format(reason), do: inspect(reason)

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
