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

  ssl? =
    case read_env.("DB_SSL") do
      nil -> false
      "false" -> false
      "true" -> true
      other -> raise "DB_SSL must be true or false, got: #{inspect(other)}"
    end

  ssl_opts =
    if ssl? do
      case read_env.("DB_SSL_VERIFY") || "peer" do
        "peer" ->
          host = URI.parse(database_url).host || raise "DATABASE_URL must include a host"

          trust =
            case read_env.("DB_SSL_CA_FILE") do
              nil -> [cacerts: :public_key.cacerts_get()]
              path -> [cacertfile: String.to_charlist(path)]
            end

          trust ++
            [
              verify: :verify_peer,
              server_name_indication: String.to_charlist(host),
              customize_hostname_check: [
                match_fun: :public_key.pkix_verify_hostname_match_fun(:https)
              ]
            ]

        "none" ->
          [verify: :verify_none]

        other ->
          raise "DB_SSL_VERIFY must be peer or none, got: #{inspect(other)}"
      end
    else
      []
    end

  config :deciduous_mcp, DeciduousMcp.Repo,
    url: database_url,
    pool_size: String.to_integer(System.get_env("POOL_SIZE") || "10"),
    # The database lives on the same private docker network as this container
    # and is not reachable from outside it, so TLS to Postgres is off by
    # default here. Set DB_SSL=true if that ever stops being true.
    # Postgrex 0.22 takes the TLS options as the value of :ssl; the separate
    # :ssl_opts key is deprecated and logged a warning on every connection.
    ssl: if(ssl?, do: ssl_opts, else: false)
end
