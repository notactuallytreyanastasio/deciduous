#!/usr/bin/env bash
# Run with PostgreSQL 17 client tools and DATABASE_URL or PG* environment values.
# The release container suite calls this for databases built by both migrations
# and STRUCTURE.sql. --empty additionally verifies no application data shipped.
set -euo pipefail

if [ "$#" -gt 1 ] || { [ "$#" -eq 1 ] && [ "$1" != "--empty" ]; }; then
  printf '%s\n' 'Usage: check_structure.sh [--empty]' >&2
  exit 1
fi

project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
check_tmp="$(mktemp "${TMPDIR:-/tmp}/deciduous-schema-check.XXXXXX")"
trap 'rm -f "$check_tmp"' EXIT

bash "$project_dir/scripts/dump_structure.sh" "$check_tmp"
if ! diff -u "$project_dir/STRUCTURE.sql" "$check_tmp"; then
  printf '%s\n' 'STRUCTURE.sql differs from the migrated database. Regenerate it with scripts/dump_structure.sh.' >&2
  exit 1
fi

case "${1:-}" in
  --empty)
    if [ -n "${DATABASE_URL:-}" ]; then
      case "$DATABASE_URL" in
        ecto://*) export PGDATABASE="postgres:${DATABASE_URL#ecto:}" ;;
        *) export PGDATABASE="$DATABASE_URL" ;;
      esac
    fi

    psql -X --no-password -v ON_ERROR_STOP=1 --quiet <<'SQL'
DO $$
DECLARE
  candidate record;
  populated boolean;
BEGIN
  FOR candidate IN
    SELECT tablename FROM pg_tables
    WHERE schemaname = 'public' AND tablename <> 'schema_migrations'
  LOOP
    EXECUTE format('SELECT EXISTS (SELECT 1 FROM public.%I)', candidate.tablename) INTO populated;
    IF populated THEN
      RAISE EXCEPTION 'Fresh bootstrap unexpectedly contains rows in %', candidate.tablename;
    END IF;
  END LOOP;
END
$$;
SQL
    ;;
  '') ;;
esac

printf '%s\n' 'Schema, migration ledger, and database defaults match STRUCTURE.sql.'
