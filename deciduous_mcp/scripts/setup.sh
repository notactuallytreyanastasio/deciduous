#!/bin/sh
# First install and repeatable upgrades. Requires Docker and a Compose command.
set -eu

usage() {
  printf '%s\n' \
    'Usage: scripts/setup.sh [--external-database]' \
    '' \
    'Default: create a private PostgreSQL container and start Deciduous.' \
    'Existing database: set DATABASE_URL and optionally DECIDUOUS_MCP_TOKEN,' \
    'then use --external-database on first setup. Subsequent runs remember it.' \
    '' \
    'DECIDUOUS_ENV_FILE selects a settings file (default: deciduous_mcp/.env).' \
    'COMPOSE_PROJECT_NAME and DECIDUOUS_PORT select an independent installation.'
}

requested_mode=
case "${1:-}" in
  '') ;;
  --external-database) requested_mode=external ;;
  -h|--help) usage; exit 0 ;;
  *) usage >&2; exit 2 ;;
esac
[ "$#" -le 1 ] || { usage >&2; exit 2; }

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_dir=$(CDPATH= cd -- "$script_dir/.." && pwd)
env_file=${DECIDUOUS_ENV_FILE:-"$project_dir/.env"}
case "$env_file" in
  /*) ;;
  *) env_file="$PWD/$env_file" ;;
esac

command -v docker >/dev/null 2>&1 || {
  printf '%s\n' 'Install Docker with Docker Compose, then rerun this command.' >&2
  exit 1
}
if docker compose version >/dev/null 2>&1; then
  compose_command=plugin
elif command -v docker-compose >/dev/null 2>&1; then
  compose_command=standalone
else
  printf '%s\n' 'Docker Compose is missing. Install the Compose plugin and rerun.' >&2
  exit 1
fi
docker info >/dev/null 2>&1 || {
  printf '%s\n' 'Docker is not running. Start Docker and rerun this command.' >&2
  exit 1
}

if [ ! -e "$env_file" ]; then
  mode=${requested_mode:-local}
  if [ "$mode" = external ] && [ -z "${DATABASE_URL:-}" ]; then
    printf '%s\n' 'Set DATABASE_URL to your existing PostgreSQL database before using --external-database.' >&2
    exit 1
  fi
  if [ "$mode" = local ] && [ -n "${DATABASE_URL:-}" ]; then
    printf '%s\n' 'DATABASE_URL is set. Use --external-database to connect to that database, or unset it for a fresh local install.' >&2
    exit 1
  fi

  # The database image supplies the random source and utilities. No host
  # Erlang, Elixir, OpenSSL, psql, or language runtime is needed.
  secrets=$(docker run --rm --entrypoint sh postgres:17.11-alpine -eu -c '
    od -An -N32 -tx1 /dev/urandom | tr -d " \n"
    printf "\n"
    od -An -N32 -tx1 /dev/urandom | tr -d " \n"
    printf "\n"
  ')
  token=${DECIDUOUS_MCP_TOKEN:-$(printf '%s\n' "$secrets" | sed -n '1p')}
  password=$(printf '%s\n' "$secrets" | sed -n '2p')
  database_url=${DATABASE_URL:-"ecto://deciduous:$password@db:5432/deciduous"}
  [ "${#token}" -ge 32 ] || {
    printf '%s\n' 'DECIDUOUS_MCP_TOKEN must contain at least 32 bytes.' >&2
    exit 1
  }
  # Compose single quotes keep dollar signs literal. Reject delimiters and
  # line breaks rather than silently changing a supplied credential.
  case "$token$database_url" in
    *"'"*|*'
'*|*"$(printf '\r')"*)
      printf '%s\n' 'Credentials cannot contain quotes or line breaks. URL-encode special characters in DATABASE_URL.' >&2
      exit 1 ;;
  esac
  settings_tmp=$(umask 077; mktemp "$env_file.tmp.XXXXXX")
  trap 'rm -f "$settings_tmp"' EXIT HUP INT TERM
  (
    umask 077
    {
      printf 'COMPOSE_PROJECT_NAME=%s\n' "${COMPOSE_PROJECT_NAME:-deciduous}"
      printf 'DECIDUOUS_DATABASE_MODE=%s\n' "$mode"
      printf 'DECIDUOUS_BIND_ADDRESS=%s\n' "${DECIDUOUS_BIND_ADDRESS:-127.0.0.1}"
      printf 'DECIDUOUS_PORT=%s\n' "${DECIDUOUS_PORT:-4000}"
      printf "DECIDUOUS_MCP_TOKEN='%s'\n" "$token"
      printf "POSTGRES_PASSWORD='%s'\n" "$password"
      printf "DATABASE_URL='%s'\n" "$database_url"
      printf 'DB_SSL=%s\n' "${DB_SSL:-false}"
      printf 'DB_SSL_VERIFY=%s\n' "${DB_SSL_VERIFY:-full}"
      printf 'DB_SSL_CA_FILE=%s\n' "${DB_SSL_CA_FILE:-}"
      printf 'POOL_SIZE=%s\n' "${POOL_SIZE:-10}"
    } > "$settings_tmp"
  ) || {
    printf '%s\n' 'Could not write the settings file.' >&2
    exit 1
  }
  # A hard link publishes the complete file atomically and refuses to replace
  # an existing path, including one created by another setup process.
  ln "$settings_tmp" "$env_file" || {
    printf '%s\n' 'Another setup created the settings file. Rerun this command.' >&2
    exit 1
  }
  rm -f "$settings_tmp"
  trap - EXIT HUP INT TERM
  printf 'Created %s with private credentials.\n' "$env_file"
fi
if [ ! -f "$env_file" ]; then
  printf '%s\n' 'The settings path must be a regular file.' >&2
  exit 1
fi
chmod 600 "$env_file"

# Read only this non-secret selector; never execute .env as a shell script.
mode=$(sed -n 's/^DECIDUOUS_DATABASE_MODE=//p' "$env_file")
case "$mode" in
  local|external) ;;
  *) printf '%s\n' 'Set DECIDUOUS_DATABASE_MODE=local or external in the settings file.' >&2; exit 1 ;;
esac
if [ -n "$requested_mode" ] && [ "$mode" != "$requested_mode" ]; then
  printf '%s\n' 'The settings file belongs to a local installation. Use a separate DECIDUOUS_ENV_FILE and COMPOSE_PROJECT_NAME for an external database.' >&2
  exit 1
fi

# The saved installation is authoritative on later runs. Compose normally lets
# the shell override .env, which could silently switch databases or tokens.
unset DATABASE_URL DECIDUOUS_MCP_TOKEN POSTGRES_PASSWORD COMPOSE_PROJECT_NAME
unset DECIDUOUS_BIND_ADDRESS DECIDUOUS_PORT DB_SSL DB_SSL_VERIFY DB_SSL_CA_FILE POOL_SIZE

compose_cli() {
  if [ "$compose_command" = plugin ]; then
    docker compose "$@"
  else
    docker-compose "$@"
  fi
}

compose() {
  if [ "$mode" = local ]; then
    compose_cli --env-file "$env_file" -f "$project_dir/compose.yaml" -f "$project_dir/compose.local.yaml" "$@"
  else
    compose_cli --env-file "$env_file" -f "$project_dir/compose.yaml" "$@"
  fi
}

# Keep secrets out of stdout, including Compose's fully expanded config.
compose config --quiet
compose up -d --build

# Poll the container healthcheck, which checks /ready (including PostgreSQL).
# This works with both Compose v1 and v2 and never needs a TTY or host curl.
attempt=0
while [ "$attempt" -lt 90 ]; do
  container_id=$(compose ps -q server)
  if [ -n "$container_id" ]; then
    health=$(docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' "$container_id")
    case "$health" in
      healthy)
        address=$(compose port server 4000)
        printf '\nDeciduous is ready: http://%s/mcp\n' "$address"
        printf 'Credentials are in %s (DECIDUOUS_MCP_TOKEN).\n' "$env_file"
        printf '%s\n' 'Rerun this command to rebuild and upgrade. Your settings and database volume are preserved.'
        exit 0 ;;
      unhealthy|exited|dead) break ;;
    esac
  fi
  attempt=$((attempt + 1))
  sleep 2
done
printf '%s\n' 'Deciduous did not become ready. Check the server logs with the Compose commands in DEPLOY.md.' >&2
compose ps >&2
exit 1
