# Shared Postgres setup helpers

These helpers target the Postgres-backed Elixir server in `deciduous_mcp/`, not the separate Rust `deciduous serve --api` daemon. Read the guides before applying changes:

- [Local setup](../../docs/content/local-postgres.md)
- [Remote deployment outline](../../docs/content/remote-postgres.md)
- [Migration, upgrades, and restore](../../docs/content/upgrading.md)

Requires Python 3.9+ and, for the local stack, a running Docker daemon with Compose v2 (`docker compose` or `docker-compose`). No Python packages are required.

For a generated database-only project, without the MCP service:

```bash
python3 scripts/team-memory/postgres-only.py --output ./deciduous-postgres --port 55432
```

This writes a new directory with private credentials but starts nothing. It refuses existing paths and symlinks. The generated README explains starting Postgres and connecting a separately run MCP service. This is an alternative to the full stack below.

```bash
python3 scripts/team-memory/local-stack.py configure
python3 scripts/team-memory/local-stack.py configure --apply
python3 scripts/team-memory/local-stack.py up
python3 scripts/team-memory/local-stack.py up --apply
python3 scripts/team-memory/local-stack.py check
```

Stateful operations default to a plan. `check` performs server checks. `token` is the one operation that intentionally writes a secret to stdout; use it only in a pipe or command substitution to a trusted consumer. It refuses direct terminal output. The default private configuration is `~/.config/deciduous/team-memory/local.env`.

`migrate-sqlite.py` seeds one new workspace. It preserves the source and creates an independent SQLite snapshot before running a CLI export on a copy. It refuses visible target data and asks the operator to confirm a never-used workspace because server export omits deleted nodes. Its dry run is offline/read-only. Applying a migration can write blobs and records remotely before a later check fails; investigate the saved report instead of retrying blindly. There is no automatic destructive rollback.

The remote Compose file and Caddyfile are examples to review and adapt; local-stack.py operates only on compose.yaml. Never run a local upgrade command against a different deployment by guessing its project name.

## Tests

Offline safety regression tests:

```bash
python3 scripts/team-memory/tests/test_safety.py
python3 scripts/team-memory/tests/test_postgres_only.py
```

An explicit live test creates a unique workspace and retains its local fixtures. Run it only against an isolated server, using the matching CLI:

```bash
python3 scripts/team-memory/tests/live_migration.py \
  --url http://127.0.0.1:4400 \
  --cli target/debug/deciduous \
  --env-file /path/to/private-test.env
```

The test creates two local nodes, an edge and an attachment, verifies dry-run immutability, imports them, verifies the remote attachment checksum, checks that the original files are unchanged, refuses a repeat import into the populated workspace, and exercises CLI remote init/status/pull. It asserts the current limitation that pull does not populate local documents.

For hosts without Docker, `tests/native_server.py` can run the app against an already-created isolated native test database. It requires a loopback database URL on a non-default port and a database name ending `_test`. It runs migrations in that test database before starting the server; it is not a production deployment script.
