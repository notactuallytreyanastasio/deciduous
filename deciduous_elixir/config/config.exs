import Config

config :deciduex, ecto_repos: [Deciduex.Repo]

config :deciduex, Deciduex.Repo, database: ":memory:"

import_config "#{config_env()}.exs"
