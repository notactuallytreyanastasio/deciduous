#!/bin/sh
set -eu

# An explicit command is useful for release administration and diagnostics.
if [ "$#" -gt 0 ]; then
  exec "$@"
fi

# Fail before migrations if required runtime configuration is missing.
: "${DATABASE_URL:?DATABASE_URL must be set to your PostgreSQL connection URL}"
token_bytes=$(printf '%s' "${DECIDUOUS_MCP_TOKEN:-}" | wc -c)
if [ "$token_bytes" -lt 32 ]; then
  echo 'DECIDUOUS_MCP_TOKEN must contain at least 32 bytes. Generate one with: openssl rand -hex 32' >&2
  exit 1
fi

/app/bin/deciduous_mcp eval 'DeciduousMcp.Release.migrate()'
exec /app/bin/deciduous_mcp start
