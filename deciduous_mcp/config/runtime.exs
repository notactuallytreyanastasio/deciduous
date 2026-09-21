import Config

# Read an env var, treating empty strings as unset — docker-compose's
# `${VAR:-}` defaulting sets variables to "" when they are absent from .env,
# and an empty token must look missing, not look like a token.
read_env = fn name ->
  case System.get_env(name) do
    nil -> nil
    "" -> nil
    v -> v
  end
end

config :deciduous_mcp, api_token: read_env.("DECIDUOUS_MCP_TOKEN")

if config_env() == :prod do
  database_url =
    read_env.("DATABASE_URL") ||
      raise """
      environment variable DATABASE_URL is missing.
      For example: ecto://USER:PASS@HOST/DATABASE
      """

  config :deciduous_mcp, DeciduousMcp.Repo,
    url: database_url,
    pool_size: String.to_integer(System.get_env("POOL_SIZE") || "10"),
    # The database lives on the same private docker network as this container
    # and is not reachable from outside it, so TLS to Postgres is off by
    # default here. Set DB_SSL=true if that ever stops being true.
    ssl: read_env.("DB_SSL") == "true",
    ssl_opts: [verify: :verify_none]
end
