#!/usr/bin/env bash
# Prove a custom-format backup restores the full application and DB defaults.
# Both PostgreSQL servers are disposable, isolated containers with no host ports.
set -euo pipefail

project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
otp_image="${OTP_IMAGE:-deciduous-release-otp:test}"
postgres_image="${POSTGRES_IMAGE:-postgres:17.11}"
client_image="${CLIENT_IMAGE:-python:3.12-slim}"
test_id="deciduous-restore-$(date +%s)-$$"
network="$test_id"
source_db="$test_id-source"
target_db="$test_id-target"
server="$test_id-server"
state_volume="$test_id-state"
database=deciduous_restore_proof
token=restore-test-token-not-for-production-1234567890
password=restore-test-password-not-for-production
backup_dir="$(mktemp -d "${TMPDIR:-/tmp}/deciduous-restore.XXXXXX")"
containers=()
network_created=0
volume_created=0

cleanup() {
  result=$?
  trap - EXIT
  if [ "$result" -ne 0 ]; then
    for name in "${containers[@]}"; do
      docker logs --tail 60 "$name" >&2 2>/dev/null || true
    done
  fi
  for name in "${containers[@]}"; do docker rm -f "$name" >/dev/null 2>&1 || true; done
  if [ "$volume_created" -eq 1 ]; then docker volume rm "$state_volume" >/dev/null 2>&1 || true; fi
  if [ "$network_created" -eq 1 ]; then docker network rm "$network" >/dev/null 2>&1 || true; fi
  rm -f "$backup_dir/deciduous-backup.dump"
  rmdir "$backup_dir"
  exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

docker info >/dev/null
docker image inspect "$otp_image" >/dev/null
docker network create "$network" >/dev/null
network_created=1
docker volume create "$state_volume" >/dev/null
volume_created=1

start_database() {
  container=$1
  alias=$2
  containers+=("$container")
  docker run -d --name "$container" --network "$network" --network-alias "$alias" \
    -e POSTGRES_PASSWORD="$password" --tmpfs /var/lib/postgresql/data \
    "$postgres_image" >/dev/null
  for attempt in {1..90}; do
    if docker exec "$container" pg_isready -U postgres >/dev/null 2>&1; then break; fi
    sleep 1
  done
  docker exec "$container" pg_isready -U postgres >/dev/null
}

start_server() {
  containers+=("$server")
  docker run -d --name "$server" --network "$network" --network-alias server \
    -e DATABASE_URL="ecto://postgres:$password@$1:5432/$database" \
    -e DECIDUOUS_MCP_TOKEN="$token" "$otp_image" >/dev/null
}

client() {
  docker run --rm --network "$network" -e SERVER_URL=http://server:4000 \
    -e DECIDUOUS_MCP_TOKEN="$token" -v "$project_dir/test/release:/tests:ro" \
    -v "$state_volume:/state" "$client_image" python /tests/acceptance.py "$@"
}

data_fingerprint() {
  docker exec "$1" pg_dump -U postgres --data-only --no-owner --no-privileges \
    "$database" | sed '/^\\restrict /d; /^\\unrestrict /d' | cksum
}

start_database "$source_db" source-db
start_database "$target_db" restore-db
docker exec "$source_db" createdb -U postgres "$database"
start_server source-db
client wait
client seed --case restore
docker stop "$server" >/dev/null
docker rm "$server" >/dev/null

printf '%s\n' 'Backup: capture the existing graph, binary document, schema and database settings...'
before_restore="$(data_fingerprint "$source_db")"
umask 077
docker exec "$source_db" pg_dump -U postgres -d "$database" --format=custom \
  > "$backup_dir/deciduous-backup.dump"

# --create restores the archive's original database name AND its DATABASE
# PROPERTIES. Restoring into an existing differently named database skips those
# settings while restoring schema_migrations, so migrations would not fix them.
docker exec -i "$target_db" pg_restore -U postgres --create --exit-on-error \
  --dbname=postgres < "$backup_dir/deciduous-backup.dump"
after_restore="$(data_fingerprint "$target_db")"
if [ "$before_restore" != "$after_restore" ]; then
  printf '%s\n' 'ERROR: restored database rows differ from the backup source' >&2
  exit 1
fi

docker run --rm --network "$network" -e PGHOST=restore-db -e PGUSER=postgres \
  -e PGPASSWORD="$password" -e PGDATABASE="$database" -v "$project_dir:/source:ro" \
  "$postgres_image" bash /source/test/release/check_structure.sh

start_server restore-db
client wait
client verify --case restore --replaced
printf '%s\n' 'PASS: backup restored into a separate PostgreSQL server; all rows, document bytes, schema, migration ledger and database defaults preserved.'
