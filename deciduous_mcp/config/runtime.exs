import Config

# The settings file `deciduous remote setup` writes for a server it runs as a
# background service (~/.config/deciduous/server/.env). launchd has no
# environment-file directive, and putting the database password and token in
# the LaunchAgent plist would copy them into a second file, so the service
# passes only this path and the server reads the file itself. Lines are
# KEY=value or KEY='value'; the file is parsed, never executed.
#
# When it is set, the file is authoritative over the process environment, the
# same rule scripts/setup.sh applies: a DATABASE_URL left in some shell must
# not silently point the service at another database.
settings_file =
  case System.get_env("DECIDUOUS_ENV_FILE") do
    path when path in [nil, ""] ->
      %{}

    path ->
      text =
        case File.read(path) do
          {:ok, text} ->
            text

          {:error, reason} ->
            raise "DECIDUOUS_ENV_FILE=#{path} cannot be read (#{:file.format_error(reason)})."
        end

      for line <- String.split(text, ["\r\n", "\n"]),
          line = String.trim(line),
          line != "" and not String.starts_with?(line, "#"),
          [key, value] <- [String.split(line, "=", parts: 2)],
          into: %{} do
        {String.trim(key), value |> String.trim() |> String.trim("'")}
      end
  end

# Read a setting, treating empty strings as unset — docker-compose's
# `${VAR:-}` defaulting sets variables to "" when they are absent from .env,
# and an empty token must look missing, not look like a token.
read_env = fn name ->
  case Map.get(settings_file, name) || System.get_env(name) do
    nil -> nil
    "" -> nil
    v -> v
  end
end

config :deciduous_mcp, api_token: read_env.("DECIDUOUS_MCP_TOKEN")

# The settings file names the port DECIDUOUS_PORT (the host port, in Docker
# terms); a container or a hand-run server sets PORT.
port_text = Map.get(settings_file, "DECIDUOUS_PORT") || read_env.("PORT") || "4000"

config :deciduous_mcp,
  http_port:
    (case Integer.parse(port_text) do
       {p, ""} -> p
       _ -> raise "PORT must be an integer, got: #{inspect(port_text)}"
     end)

# Which address to listen on. Unset means every interface, which is right
# inside a container (Docker publishes the port on 127.0.0.1 only). A server
# run directly on the machine binds loopback, or it would offer every graph to
# the local network.
case read_env.("DECIDUOUS_BIND_ADDRESS") do
  nil ->
    :ok

  address ->
    case :inet.parse_address(String.to_charlist(address)) do
      {:ok, ip} ->
        config :deciduous_mcp, bind_ip: ip

      {:error, _} ->
        raise "DECIDUOUS_BIND_ADDRESS must be an IP address, got: #{inspect(address)}"
    end
end

# A server run as a service creates its database if it is missing and runs
# migrations before it listens. The container's entrypoint migrates with
# `bin/deciduous_mcp eval`, which the single-file executable has no way to
# call. DECIDUOUS_MCP_COMMAND=prepare-database does only that step and exits:
# `deciduous remote setup` runs it once to learn, in the database's own words,
# whether the URL it was given works.
config :deciduous_mcp,
  prepare_database_at_start: read_env.("DECIDUOUS_PREPARE_DATABASE") == "true",
  command: System.get_env("DECIDUOUS_MCP_COMMAND")

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

  # TLS to Postgres, in libpq's terms (sslmode). DB_SSL=true turns it on;
  # DB_SSL_VERIFY picks how much of the server is checked:
  #
  #   full  (default; also "peer", "verify-full")  chain AND hostname: the
  #         certificate must name the host in DATABASE_URL, as a DNS name or,
  #         for an IP address, as an IP subjectAltName.
  #   ca    (also "verify-ca")  chain only: the certificate must come from
  #         the trusted CA, whatever name it carries. For a database reached
  #         by an address its certificate does not name.
  #   none  (also "require")  encrypted, server not authenticated. Logged as
  #         a warning at every boot.
  #
  # DB_SSL_CA_FILE trusts a private CA instead of the system store. It is
  # checked here, at boot: a missing or non-certificate file used to leave
  # the server running with every connection failing (a missing file said
  # nothing at all; a wrong file said only `unknown_ca`, forever).
  {ssl_opts, tls_description} =
    if ssl? do
      host = URI.parse(database_url).host || raise "DATABASE_URL must include a host"

      {trust, trust_name} =
        case read_env.("DB_SSL_CA_FILE") do
          nil ->
            {[cacerts: :public_key.cacerts_get()], "the system CA store"}

          path ->
            pem =
              case File.read(path) do
                {:ok, pem} ->
                  pem

                {:error, reason} ->
                  raise "DB_SSL_CA_FILE=#{path} cannot be read (#{:file.format_error(reason)}). " <>
                          "Point it at the PEM file of the CA that signed the database's certificate."
              end

            certs = for {:Certificate, der, _} <- :public_key.pem_decode(pem), do: der

            if certs == [] do
              raise "DB_SSL_CA_FILE=#{path} contains no PEM certificate " <>
                      "(expected -----BEGIN CERTIFICATE-----)."
            end

            {[cacerts: certs], "DB_SSL_CA_FILE (#{path}, #{length(certs)} certificate(s))"}
        end

      case read_env.("DB_SSL_VERIFY") || "full" do
        mode when mode in ["full", "peer", "verify-full"] ->
          {trust ++
             [
               verify: :verify_peer,
               server_name_indication: String.to_charlist(host),
               customize_hostname_check: [
                 match_fun: :public_key.pkix_verify_hostname_match_fun(:https)
               ]
             ], "verify-full: chain against #{trust_name}, certificate must name #{host}"}

        mode when mode in ["ca", "verify-ca"] ->
          # No SNI, so no hostname check; the chain is still verified.
          {trust ++ [verify: :verify_peer, server_name_indication: :disable],
           "verify-ca: chain against #{trust_name}, hostname not checked"}

        mode when mode in ["none", "require"] ->
          {[verify: :verify_none], :unverified}

        other ->
          raise "DB_SSL_VERIFY must be full, ca or none " <>
                  "(or libpq's verify-full, verify-ca, require), got: #{inspect(other)}"
      end
    else
      {[], "off (DB_SSL unset or false)"}
    end

  config :deciduous_mcp, db_tls: tls_description

  config :deciduous_mcp, DeciduousMcp.Repo,
    url: database_url,
    pool_size: String.to_integer(read_env.("POOL_SIZE") || "10"),
    # The database lives on the same private docker network as this container
    # and is not reachable from outside it, so TLS to Postgres is off by
    # default here. Set DB_SSL=true if that ever stops being true.
    # Postgrex 0.22 takes the TLS options as the value of :ssl; the separate
    # :ssl_opts key is deprecated and logged a warning on every connection.
    ssl: if(ssl?, do: ssl_opts, else: false)
end
