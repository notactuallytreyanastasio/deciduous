import Config

config :deciduous_mcp, DeciduousMcp.Repo,
  username: "postgres",
  password: "postgres",
  hostname: "localhost",
  database: "deciduous_mcp_dev",
  stacktrace: true,
  show_sensitive_data_on_connection_error: true,
  pool_size: 10

config :logger, level: :debug
