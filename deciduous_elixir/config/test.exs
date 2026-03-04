import Config

config :logger, level: :warning

# Raise instead of System.halt in tests so we can catch errors
config :deciduex, raise_on_exit: true
