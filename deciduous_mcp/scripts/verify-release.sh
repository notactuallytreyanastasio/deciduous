#!/usr/bin/env bash
# Repeatable fresh-install, real v1.0.0 upgrade, and Burrito acceptance checks.
# Only Docker, Bash, Git (for the baseline), and tar are needed on the host.
set -euo pipefail

project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repo_dir="$(cd "$project_dir/.." && pwd)"
otp_image="${OTP_IMAGE:-deciduous-release-otp:test}"
burrito_image="${BURRITO_IMAGE:-deciduous-release-burrito:test}"
baseline_image="${BASELINE_IMAGE:-deciduous-release-baseline:test}"
test_image="${TEST_IMAGE:-deciduous-release-tests:build}"
# Released v1.0.0, before the hot-path indexes/planner migrations and 1.0.1.
baseline_ref="${BASELINE_REF:-93145f245d9614964a775857b97a9b603efaef2c}"
postgres_image="${POSTGRES_IMAGE:-postgres:17.11}"
client_image="${CLIENT_IMAGE:-python:3.12-slim}"
test_id="deciduous-release-$(date +%s)-$$"
network="$test_id"
db="$test_id-db"
server="$test_id-server"
token="release-test-token-not-for-production-1234567890"
password="release-test-password-not-for-production"
build_dir="$(mktemp -d "${TMPDIR:-/tmp}/deciduous-release.XXXXXX")"
containers=()
volumes=()
network_created=0

cleanup() {
  result=$?
  trap - EXIT
  if [ "$result" -ne 0 ]; then
    for name in "${containers[@]}"; do
      docker logs --tail 60 "$name" >&2 2>/dev/null || true
    done
  fi
  for name in "${containers[@]}"; do docker rm -f "$name" >/dev/null 2>&1 || true; done
  for name in "${volumes[@]}"; do docker volume rm "$name" >/dev/null 2>&1 || true; done
  if [ "$network_created" -eq 1 ]; then docker network rm "$network" >/dev/null 2>&1 || true; fi
  # Only the directory this invocation created; no user data lives here.
  case "$build_dir" in */deciduous-release.*) rm -rf -- "$build_dir" ;; esac
  exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

docker info >/dev/null
if [ "${SKIP_BUILD:-0}" != 1 ]; then
  docker build --target runtime -t "$otp_image" "$project_dir"
  docker build --target compile -t "$test_image" "$project_dir"
  docker build -f "$project_dir/Dockerfile.burrito" --target burrito-runtime -t "$burrito_image" "$project_dir"
  git -C "$repo_dir" archive "$baseline_ref" deciduous_mcp | tar -x -C "$build_dir"
  docker build -t "$baseline_image" "$build_dir/deciduous_mcp"
fi
for image in "$otp_image" "$burrito_image" "$baseline_image" "$test_image"; do docker image inspect "$image" >/dev/null; done

docker network create "$network" >/dev/null
network_created=1
for name in "$test_id-postgres" "$test_id-state" "$test_id-certs" "$test_id-native"; do
  docker volume create "$name" >/dev/null
  volumes+=("$name")
done
printf '%s\n' 'Native executable: boot with an empty PATH and no network in a Python-only container...'
docker run --rm --user 0 --entrypoint /bin/cp -v "$test_id-native:/artifact" \
  "$burrito_image" /app/deciduous-mcp /artifact/deciduous-mcp
docker run --rm --network none -v "$test_id-native:/artifact:ro" \
  -v "$project_dir/test/release:/tests:ro" "$client_image" \
  python /tests/native_smoke.py /artifact/deciduous-mcp

docker run --rm --user 0 --entrypoint /bin/sh \
  -v "$test_id-certs:/certs" -v "$project_dir/test/release:/tests:ro" \
  "$otp_image" /tests/make-certs.sh
docker run --rm --user 0 --entrypoint /bin/sh -v "$test_id-certs:/certs" \
  "$postgres_image" -c 'chown postgres:postgres /certs/server.key'
containers+=("$db")
docker run -d --name "$db" --network "$network" --network-alias db --network-alias db-wrong \
  -e POSTGRES_PASSWORD="$password" -v "$test_id-postgres:/var/lib/postgresql/data" \
  -v "$test_id-certs:/certs:ro" "$postgres_image" \
  -c ssl=on -c ssl_cert_file=/certs/server.crt -c ssl_key_file=/certs/server.key >/dev/null

for attempt in {1..90}; do
  if docker exec "$db" pg_isready -U postgres >/dev/null 2>&1; then break; fi
  sleep 1
done
docker exec "$db" pg_isready -U postgres >/dev/null

sql() { docker exec "$db" psql -X -U postgres -d "$1" -v ON_ERROR_STOP=1 -Atc "$2"; }
client() {
  docker run --rm --network "$network" -e SERVER_URL=http://server:4000 \
    -e DECIDUOUS_MCP_TOKEN="$token" \
    -v "$project_dir/test/release:/tests:ro" -v "$test_id-state:/state" \
    "$client_image" python /tests/acceptance.py "$@"
}
schema_check() {
  docker run --rm --network "$network" -e PGHOST=db -e PGUSER=postgres \
    -e PGPASSWORD="$password" -e PGDATABASE="$1" -v "$project_dir:/source:ro" \
    "$postgres_image" bash /source/test/release/check_structure.sh --empty
}
start_server() {
  image=$1
  database=$2
  shift 2
  containers+=("$server")
  docker run -d --name "$server" --network "$network" --network-alias server \
    -e DATABASE_URL="ecto://postgres:$password@db:5432/$database" \
    -e DECIDUOUS_MCP_TOKEN="$token" "$@" "$image" >/dev/null
}
stop_server() { docker rm -f "$server" >/dev/null; }
# A bare `test a = b` under `set -e` exits without saying what it compared;
# in CI that read as a silent exit 1 between two PASS lines.
expect_eq() {
  if [ "$1" != "$2" ]; then
    printf 'FAIL %s: expected %s, got %s\n' "$3" "$2" "$1" >&2
    exit 1
  fi
}

# One checksum per table, over the tables the database had before the
# upgrade (upgrade_tables). A whole-database dump also changes when a
# migration adds an empty table, which is not data loss; per table, a
# failure names what changed. write_locks held 10-second write leases and is
# renamed to write_activity by 20260924110000, so it is not compared.
upgrade_tables() {
  sql "$1" "SELECT tablename FROM pg_tables WHERE schemaname = 'public'
    AND tablename NOT IN ('schema_migrations', 'write_locks') ORDER BY tablename"
}
data_fingerprint() {
  for table in $2; do
    printf '%s %s\n' "$table" "$(docker exec "$db" pg_dump -U postgres --data-only \
      --no-owner --no-privileges --table="public.$table" "$1" |
      sed '/^\\restrict /d; /^\\unrestrict /d' | cksum)"
  done
}

for database in fresh existing snapshot invalid; do sql postgres "CREATE DATABASE $database" >/dev/null; done

printf '%s\n' 'Running the server regression suite against isolated PostgreSQL...'
docker run --rm --network "$network" -e MIX_ENV=test -e PGHOST=db -e PGUSER=postgres \
  -e PGPASSWORD="$password" -e PGDATABASE=deciduous_release_unit_test \
  -e DECIDUOUS_MCP_TOKEN="$token" -e PORT=0 \
  -v "$project_dir/test:/app/test:ro" -v "$project_dir/config/test.exs:/app/config/test.exs:ro" \
  "$test_image" /bin/sh -ec 'mix deps.get --only test && mix hex.audit && mix test'

printf '%s\n' 'Checking invalid configuration fails before changing the database...'
if docker run --rm --network "$network" \
  -e DATABASE_URL="ecto://postgres:$password@db:5432/invalid" \
  -e DECIDUOUS_MCP_TOKEN=short "$otp_image"; then
  printf '%s\n' 'ERROR: short token was accepted' >&2; exit 1
fi
expect_eq "$(sql invalid "SELECT count(*) FROM pg_tables WHERE schemaname='public'")" 0 \
  "tables created by a server refused for a short token"
if docker run --rm --network "$network" \
  -e DATABASE_URL="ecto://postgres:$password@db:5432/invalid" \
  -e DECIDUOUS_MCP_TOKEN="$token" -e DB_SSL=TRUE "$otp_image"; then
  printf '%s\n' 'ERROR: invalid TLS configuration was accepted' >&2; exit 1
fi
expect_eq "$(sql invalid "SELECT count(*) FROM pg_tables WHERE schemaname='public'")" 0 \
  "tables created by a server refused for DB_SSL=TRUE"

printf '%s\n' 'Fresh install: initialize empty PostgreSQL using the packaged OTP release...'
start_server "$otp_image" fresh
client wait
schema_check fresh
test "$(docker exec "$server" id -u)" != 0
client seed --case fresh
client verify --case fresh
docker restart "$server" >/dev/null
client wait
client verify --case fresh --replaced
stop_server

printf '%s\n' "Existing install: seed actual v1.0.0 ($baseline_ref), then upgrade in place..."
start_server "$baseline_image" existing
client wait --path /health
expect_eq "$(sql existing 'SELECT count(*) FROM schema_migrations')" 15 "migrations applied by v1.0.0"
client seed --case upgrade
fingerprint_tables="$(upgrade_tables existing)"
before_upgrade="$(data_fingerprint existing "$fingerprint_tables")"
stop_server
start_server "$otp_image" existing
client wait
# Every migration in this checkout, not a hardcoded count: 17 was right for
# 1.0.1 and silently wrong the moment 1.0.2 added TrigramSearch.
expected_migrations="$(find "$project_dir/priv/repo/migrations" -name '[0-9]*_*.exs' | wc -l | tr -d ' ')"
expect_eq "$(sql existing 'SELECT count(*) FROM schema_migrations')" "$expected_migrations" \
  "migrations applied after upgrading v1.0.0 in place"
after_upgrade="$(data_fingerprint existing "$fingerprint_tables")"
if [ "$after_upgrade" != "$before_upgrade" ]; then
  printf '%s\n' 'FAIL data across the upgrade; tables whose data changed:' >&2
  diff <(printf '%s\n' "$before_upgrade") <(printf '%s\n' "$after_upgrade") >&2 || true
  exit 1
fi
client verify --case upgrade --replaced
docker restart "$server" >/dev/null
client wait
client verify --case upgrade --replaced
stop_server

printf '%s\n' 'Native install: bootstrap STRUCTURE.sql and start Burrito in a container without system BEAM...'
docker exec -i "$db" psql -X -U postgres -d snapshot -v ON_ERROR_STOP=1 --single-transaction \
  < "$project_dir/STRUCTURE.sql" >/dev/null
schema_check snapshot
start_server "$burrito_image" snapshot
client wait
test "$(docker exec "$server" id -u)" != 0
docker exec "$server" /bin/sh -eu -c 'for tool in erl elixir mix; do if command -v "$tool"; then exit 1; fi; done'
client seed --case burrito
client verify --case burrito

printf '%s\n' 'Readiness rejects a missing migration; startup survives a database outage...'
sql snapshot 'DELETE FROM schema_migrations WHERE version=20260922200100' >/dev/null
client wait --status 503
sql snapshot 'INSERT INTO schema_migrations (version, inserted_at) VALUES (20260922200100, NOW())' >/dev/null
client wait
stop_server
docker stop "$db" >/dev/null
start_server "$burrito_image" snapshot
client wait --path /health
client wait --status 503
docker start "$db" >/dev/null
client wait
client verify --case burrito --replaced
stop_server

printf '%s\n' 'TLS: verify the database CA and hostname, and reject untrusted/mismatched peers...'
start_server "$burrito_image" snapshot -e DB_SSL=true \
  -e DB_SSL_CA_FILE=/certs/ca.crt -v "$test_id-certs:/certs:ro"
client wait
client verify --case burrito --replaced
stop_server
start_server "$burrito_image" snapshot -e DB_SSL=true
client wait --path /health
client wait --status 503
stop_server
start_server "$burrito_image" snapshot -e DB_SSL=true \
  -e DATABASE_URL="ecto://postgres:$password@db-wrong:5432/snapshot" \
  -e DB_SSL_CA_FILE=/certs/ca.crt -v "$test_id-certs:/certs:ro"
client wait --path /health
client wait --status 503
stop_server

printf '%s\n' 'PASS: empty install, schema bootstrap, real v1.0.0 upgrade, persistence, MCP, auth, documents, events, outage recovery, and verified TLS.'

bash "$project_dir/scripts/verify-setup.sh"

OTP_IMAGE="$otp_image" POSTGRES_IMAGE="$postgres_image" CLIENT_IMAGE="$client_image" \
  bash "$project_dir/scripts/verify-restore.sh"
