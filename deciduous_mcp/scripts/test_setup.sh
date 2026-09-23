#!/bin/sh
# Exercise setup without network access or a Docker daemon. Real container
# readiness/data preservation is covered by the deployment smoke tests.
set -eu
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_dir=$(CDPATH= cd -- "$script_dir/.." && pwd)
test_dir=$(mktemp -d "${TMPDIR:-/tmp}/deciduous-setup-test.XXXXXX")
trap 'rm -rf "$test_dir"' EXIT HUP INT TERM
mkdir "$test_dir/bin"
cp "$project_dir/test/support/setup_docker_stub.sh" "$test_dir/bin/docker"
chmod +x "$test_dir/bin/docker"

run_setup() {
  env -i PATH="$test_dir/bin:/usr/bin:/bin" \
    SETUP_TEST_CALLS="$test_dir/calls" \
    DECIDUOUS_ENV_FILE="$test_dir/installation.env" \
    "$@" "$script_dir/setup.sh"
}

run_setup > "$test_dir/first.log"
cp "$test_dir/installation.env" "$test_dir/original.env"
test "$(wc -l < "$test_dir/installation.env" | tr -d ' ')" = 11
test "$(find "$test_dir/installation.env" -perm 600 | wc -l | tr -d ' ')" = 1
grep -q '^DECIDUOUS_DATABASE_MODE=local$' "$test_dir/installation.env"
grep -q 'compose.local.yaml' "$test_dir/calls"
grep -q '^Deciduous is ready:' "$test_dir/first.log"
if grep -q '0000000000000000' "$test_dir/first.log"; then
  printf '%s\n' 'FAIL: setup printed a credential.' >&2
  exit 1
fi

# The port is chosen, not fixed: nothing in the tree may pin it to a default a
# second installation would collide with.
port=$(sed -n 's/^DECIDUOUS_PORT=//p' "$test_dir/installation.env")
case "$port" in
  ''|*[!0-9]*) printf 'FAIL: no numeric DECIDUOUS_PORT, got %s\n' "$port" >&2; exit 1 ;;
esac
if [ "$port" -lt 20000 ] || [ "$port" -gt 32767 ]; then
  printf 'FAIL: chosen port %s is outside 20000-32767.\n' "$port" >&2
  exit 1
fi

run_setup > "$test_dir/second.log"
cmp "$test_dir/original.env" "$test_dir/installation.env"
test "$(grep -c '^run ' "$test_dir/calls")" = 1

# Publishing a fresh .env uses an exclusive link; existing settings are
# reused, even when the caller supplies a different token on a later run.
run_setup DECIDUOUS_MCP_TOKEN=do-not-replace-the-existing-token > "$test_dir/third.log"
cmp "$test_dir/original.env" "$test_dir/installation.env"

# A server that fails readiness must never produce a success message.
if run_setup SETUP_TEST_HEALTH=unhealthy > "$test_dir/unhealthy.log" 2>&1; then
  printf '%s\n' 'FAIL: an unhealthy server was reported ready.' >&2
  exit 1
fi
if grep -q '^Deciduous is ready:' "$test_dir/unhealthy.log"; then
  exit 1
fi

# A caller's port is honoured on a fresh installation, and a nonsense one stops
# setup instead of reaching Compose.
env -i PATH="$test_dir/bin:/usr/bin:/bin" SETUP_TEST_CALLS="$test_dir/chosen.calls" \
  DECIDUOUS_ENV_FILE="$test_dir/chosen.env" DECIDUOUS_PORT=4010 \
  "$script_dir/setup.sh" > "$test_dir/chosen.log"
grep -q '^DECIDUOUS_PORT=4010$' "$test_dir/chosen.env"
if env -i PATH="$test_dir/bin:/usr/bin:/bin" SETUP_TEST_CALLS="$test_dir/bad.calls" \
  DECIDUOUS_ENV_FILE="$test_dir/bad.env" DECIDUOUS_PORT=eighty \
  "$script_dir/setup.sh" > "$test_dir/bad.log" 2>&1; then
  printf '%s\n' 'FAIL: setup accepted a non-numeric DECIDUOUS_PORT.' >&2
  exit 1
fi
test ! -e "$test_dir/bad.env"

# External setup requires an explicit database and does not add a local DB.
if env -i PATH="$test_dir/bin:/usr/bin:/bin" SETUP_TEST_CALLS="$test_dir/external.calls" \
  DECIDUOUS_ENV_FILE="$test_dir/external.env" "$script_dir/setup.sh" \
  --external-database > "$test_dir/external-missing.log" 2>&1; then
  printf '%s\n' 'FAIL: external setup accepted a missing DATABASE_URL.' >&2
  exit 1
fi
test ! -e "$test_dir/external.env"
env -i PATH="$test_dir/bin:/usr/bin:/bin" SETUP_TEST_CALLS="$test_dir/external.calls" \
  DECIDUOUS_ENV_FILE="$test_dir/external.env" \
  DATABASE_URL='ecto://user:password@existing.example.test/database' \
  DECIDUOUS_MCP_TOKEN=preserve-the-existing-server-token \
  "$script_dir/setup.sh" --external-database > "$test_dir/external.log"
grep -q '^DECIDUOUS_DATABASE_MODE=external$' "$test_dir/external.env"
grep -q "^DECIDUOUS_MCP_TOKEN='preserve-the-existing-server-token'$" "$test_dir/external.env"
if grep -q 'compose.local.yaml' "$test_dir/external.calls"; then
  printf '%s\n' 'FAIL: external setup started a local database.' >&2
  exit 1
fi
printf '%s\n' 'PASS: setup preserves credentials, keeps secrets private, and gates success on readiness.'
