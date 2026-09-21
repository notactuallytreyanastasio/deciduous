import Config

if config_env() == :prod do
  database_url =
    System.get_env("DATABASE_URL") ||
      raise """
      environment variable DATABASE_URL is missing.
      For example: ecto://USER:PASS@HOST/DATABASE
      """

  config :deciduous_mcp, DeciduousMcp.Repo,
    url: database_url,
    pool_size: String.to_integer(System.get_env("POOL_SIZE") || "10"),
    ssl: String.to_existing_atom(System.get_env("DB_SSL") || "true"),
    ssl_opts: [verify: :verify_none]
end

# Workspace config (optional default workspace)
config :deciduous_mcp,
  default_workspace_name: System.get_env("DECIDUOUS_WORKSPACE") || "default",
  deciduous_project_dir: System.get_env("DECIDUOUS_PROJECT_DIR")
