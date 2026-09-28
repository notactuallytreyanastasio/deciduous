defmodule DeciduousMcp.NativeServiceTest do
  @moduledoc """
  What a server run as a background service (no container) depends on: the
  settings file read through DECIDUOUS_ENV_FILE, and a database that is
  created and checked with PostgreSQL's own error messages.
  """
  use ExUnit.Case, async: false

  alias DeciduousMcp.Release

  @runtime Path.expand("../../config/runtime.exs", __DIR__)

  defp prod_config(settings, env \\ %{}) do
    dir = Path.join(System.tmp_dir!(), "deciduous-settings-#{System.unique_integer([:positive])}")
    File.mkdir_p!(dir)
    path = Path.join(dir, ".env")
    File.write!(path, settings)

    names = ["DECIDUOUS_ENV_FILE" | Map.keys(env)]
    saved = Map.new(names, &{&1, System.get_env(&1)})

    try do
      System.put_env("DECIDUOUS_ENV_FILE", path)
      Enum.each(env, fn {k, v} -> System.put_env(k, v) end)
      Config.Reader.read!(@runtime, env: :prod, target: :host)[:deciduous_mcp]
    after
      Enum.each(saved, fn
        {k, nil} -> System.delete_env(k)
        {k, v} -> System.put_env(k, v)
      end)

      File.rm_rf!(dir)
    end
  end

  @settings """
  DECIDUOUS_SERVER_MODE=native
  DECIDUOUS_BIND_ADDRESS=127.0.0.1
  DECIDUOUS_PORT=24123
  DECIDUOUS_MCP_TOKEN='#{String.duplicate("a", 64)}'
  DATABASE_URL='ecto://bg@localhost:5432/deciduous'
  DB_SSL=false
  POOL_SIZE=3
  DECIDUOUS_PREPARE_DATABASE=true
  """

  test "the settings file supplies the database, token, port, bind address and pool" do
    app = prod_config(@settings)

    assert app[:api_token] == String.duplicate("a", 64)
    assert app[:http_port] == 24123
    assert app[:bind_ip] == {127, 0, 0, 1}
    assert app[:prepare_database_at_start]
    assert app[DeciduousMcp.Repo][:url] == "ecto://bg@localhost:5432/deciduous"
    assert app[DeciduousMcp.Repo][:pool_size] == 3
  end

  test "the settings file wins over a DATABASE_URL left in the environment" do
    app = prod_config(@settings, %{"DATABASE_URL" => "ecto://x@elsewhere/other", "PORT" => "9"})

    assert app[DeciduousMcp.Repo][:url] == "ecto://bg@localhost:5432/deciduous"
    assert app[:http_port] == 24123
  end

  test "a bind address that is not an IP address stops the boot" do
    assert_raise RuntimeError, ~r/DECIDUOUS_BIND_ADDRESS must be an IP address/, fn ->
      prod_config(String.replace(@settings, "127.0.0.1", "localhost"))
    end
  end

  test "a settings file that cannot be read stops the boot and says which" do
    System.put_env("DECIDUOUS_ENV_FILE", "/nonexistent/deciduous/.env")

    try do
      assert_raise RuntimeError, ~r{/nonexistent/deciduous/.env cannot be read}, fn ->
        Config.Reader.read!(@runtime, env: :prod, target: :host)
      end
    after
      System.delete_env("DECIDUOUS_ENV_FILE")
    end
  end

  defp test_db_config(overrides) do
    DeciduousMcp.Repo.config()
    |> Keyword.drop([:pool])
    |> Keyword.merge(overrides)
  end

  test "an unreachable server is reported with its address and the refusal, no password" do
    config = test_db_config(hostname: "127.0.0.1", port: 1, password: "sekrit-pw")

    assert {:error, message} = Release.ensure_database(config)
    assert message =~ "cannot connect to"
    assert message =~ "127.0.0.1:1/"
    assert message =~ "refused"
    refute message =~ "sekrit-pw"
  end

  test "a missing database is created" do
    name = "deciduous_prepare_test_#{System.unique_integer([:positive])}"
    config = test_db_config(database: name)

    try do
      assert :ok = Release.ensure_database(config)
      assert :ok = Release.ensure_database(config), "a second run finds it and does nothing"
    after
      Ecto.Adapters.Postgres.storage_down(config)
    end
  end
end
