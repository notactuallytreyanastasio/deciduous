import Config

config :deciduous_mcp, DeciduousMcp.Repo,
  username: "postgres",
  password: "postgres",
  hostname: "localhost",
  database: "deciduous_mcp_test#{System.get_env("MIX_TEST_PARTITION")}",
  pool: Ecto.Adapters.SQL.Sandbox,
  pool_size: System.schedulers_online() * 2

config :logger, level: :warning
