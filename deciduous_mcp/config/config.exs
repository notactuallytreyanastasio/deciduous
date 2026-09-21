import Config

config :deciduous_mcp,
  ecto_repos: [DeciduousMcp.Repo]

config :deciduous_mcp, DeciduousMcp.Repo,
  migration_primary_key: [type: :binary_id],
  migration_timestamps: [type: :utc_datetime_usec]

# PubSub for real-time collaboration
config :deciduous_mcp, DeciduousMcp.PubSub,
  adapter: Phoenix.PubSub.PG2

config :logger, :console,
  format: "$time $metadata[$level] $message\n",
  metadata: [:request_id, :workspace_id]

import_config "#{config_env()}.exs"
