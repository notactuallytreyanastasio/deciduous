#!/bin/sh
# Deliberately small Docker substitute for credential/idempotence tests.
set -eu
printf '%s\n' "$*" >> "$SETUP_TEST_CALLS"
case "${1:-}" in
  info) exit 0 ;;
  run)
    # Token, database password, and the randomly chosen host port. A fixed
    # port keeps the assertions deterministic.
    printf '%064d\n%064d\n%s\n' 1 2 21234
    exit 0 ;;
  inspect)
    printf '%s\n' "${SETUP_TEST_HEALTH:-healthy}"
    exit 0 ;;
  compose) shift ;;
  *) printf 'Unexpected Docker invocation: %s\n' "$*" >&2; exit 1 ;;
esac
for argument in "$@"; do
  case "$argument" in
    version) exit 0 ;;
    config|up)
      test -z "${DATABASE_URL:-}${DECIDUOUS_MCP_TOKEN:-}${COMPOSE_PROJECT_NAME:-}"
      exit 0 ;;
    ps) printf '%s\n' setup-test-server; exit 0 ;;
    port) printf '%s\n' 127.0.0.1:21234; exit 0 ;;
  esac
done
printf 'Unexpected Compose invocation: %s\n' "$*" >&2
exit 1
