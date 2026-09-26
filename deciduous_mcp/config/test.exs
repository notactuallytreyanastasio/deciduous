import Config

config :deciduous_mcp, DeciduousMcp.Repo,
  username: System.get_env("PGUSER") || "postgres",
  password: System.get_env("PGPASSWORD") || "postgres",
  hostname: System.get_env("PGHOST") || "localhost",
  database:
    System.get_env("PGDATABASE") || "deciduous_mcp_test#{System.get_env("MIX_TEST_PARTITION")}",
  pool: Ecto.Adapters.SQL.Sandbox,
  pool_size: System.schedulers_online() * 2

config :logger, level: :warning

# The app's Events.Listener would run its timed catch-up pass through
# whichever test owns the shared sandbox connection. Tests that need the
# timer start their own listener.
config :deciduous_mcp, :events_catch_up_every_ms, nil
