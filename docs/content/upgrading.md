# Upgrade without losing the team's memory

There are three separate operations: replacing the CLI, moving a local SQLite graph into shared Postgres, and upgrading the shared server. Updating a binary does not move the graph. Keep the old database and attachments until you have verified the new workspace and a tested backup.

The helpers under `scripts/team-memory/` print a plan by default. State-changing operations require `--apply`. They do not delete databases, reset a schema, remove a volume, or upgrade a Postgres major version.

## Move an existing SQLite project to Postgres

Use this path to seed a new workspace. The migration script refuses a target with visible data. It does not merge an active server workspace with an old local copy: the current import endpoint replaces matching node fields without a newer-timestamp check, so a stale push can overwrite newer decisions.

### Prepare

Start and check your [local](local-postgres.md) or [remote](remote-postgres.md) shared server. Install the CLI from the same reviewed source release as the server; these guides use the 1.0.0 interface. `deciduous --version` and `deciduous remote --help` should match the documented commands.

Pause the agents writing to the local project. Reserve a new, unused workspace name and keep other agents out of it until verification finishes. An empty `/export` response is not enough to prove a workspace has never held data because that endpoint omits deleted nodes. The script asks you to confirm a new name for this reason.

Store your server token with `deciduous remote login --url <base-url>` or supply `DECIDUOUS_MCP_TOKEN` through your secret store. The script reads the same environment variable or private credentials file as the CLI. It does not put tokens into process arguments.

### Preview one project

Run from the Deciduous source checkout. Replace the project path with the repository that owns the SQLite graph, and choose a new backup directory outside that repository:

```bash
python3 scripts/team-memory/migrate-sqlite.py \
  --project /path/to/example-app \
  --url http://127.0.0.1:4000 \
  --workspace example-app \
  --backup-dir /path/to/private-backups/example-app-first-import
```

The preview reads SQLite through a private temporary copy of the database and any WAL, checks its integrity, and reports table counts. It leaves no retained artifacts or changes in the source project, makes no HTTP requests, and does not run a CLI export that might update an old schema. SQLite never opens the source file, so it cannot create a shared-memory sidecar there. The helper compares source and copied hashes and stops if it observes a change while copying. Keep writers paused for a stable snapshot.

Review the limitations below. Then apply the same command with the explicit acknowledgments:

```bash
python3 scripts/team-memory/migrate-sqlite.py \
  --project /path/to/example-app \
  --url http://127.0.0.1:4000 \
  --workspace example-app \
  --backup-dir /path/to/private-backups/example-app-first-import \
  --apply --writers-paused \
  --confirm-new-workspace example-app --accept-limitations
```

The script performs these steps:

1. Verify server access and refuse a populated target.
2. Preserve `.deciduous/` in an owner-only backup directory. The helper copies stable database and WAL bytes, checks their hashes, then uses SQLite's backup API on that private copy to include committed WAL data. The original project is not modified.
3. Run `deciduous graph` against a separate copy of that snapshot, allowing a compatible CLI to upgrade the copy without touching the preserved database.
4. Check node identities and edges. Reject self-loops, unresolved endpoints, duplicate edge signatures, and missing attachment bytes before import.
5. Upload referenced attachment bytes by their SHA-256 hash, then import graph metadata. Save the export and import report.
6. Read the workspace back, compare node identities and basic content, compare edges and document identities, and download each uploaded attachment to verify its checksum.

If an upload or import fails, the local project and backup remain intact. The server may already contain blobs or imported records. Stop and inspect the saved report; do not keep pushing into a partially populated workspace. There is no automatic remote rollback or deletion.

### Switch the team after verification

In the project repository:

```bash
deciduous remote init http://127.0.0.1:4000 --workspace example-app
deciduous remote status
```

Point each agent at the HTTP `/mcp` endpoint and pin the same workspace. Have one agent read an imported goal and another read its linked outcome. Retrieve a representative attachment. Only then resume work on the shared server.

Keep the local SQLite file and `.deciduous/documents/`. Ordinary CLI writes and `deciduous mcp` still write locally, so update the team's instructions to use HTTP MCP for shared writes. `remote pull` refreshes local nodes and edges; it is not a full replica or a complete rollback plan. See [Connect your agents](clients.md).

### What this migration does and does not preserve

| Data | Treatment |
| --- | --- |
| Nodes and metadata exported by `deciduous graph` | Imported by `change_id` within the chosen workspace. The server assigns its own database IDs. |
| Edges | Resolve against exported integer IDs first, then change IDs. The helper rejects edges the server would omit or collapse. |
| Document metadata and local attachment bytes | The helper uploads bytes separately and verifies checksums after import. Missing bytes stop the migration. |
| Themes and node theme assignments | Not carried by the CLI graph export/import path. They remain in the SQLite backup. The shared server has no registered theme-editing MCP tool; keep this workflow local for now. |
| Command logs, audit history, QA records, and other local tables | Preserved in the original snapshot, not represented as a full server restore by graph import. |
| Deleted-history semantics | Do not assume a lossless transfer. The CLI graph and server export/import have different deletion behavior; retain the source backup. |
| Node IDs | Local integer IDs and server UUIDs differ. Share `change_id` references where supported. |

The server limits an import payload or blob to 64 MiB. Large graphs require a separate, reviewed migration plan. The helper refuses symlinks inside `.deciduous/` to avoid copying files outside the selected project.

`deciduous remote push` alone does not upload attachment bytes. `remote pull` imports nodes and edges only, not attachment files or theme assignments. `deciduous migrate` adds local change-ID support; it is not a SQLite-to-Postgres migration command. `remote adopt` writes remote URL configuration in discovered repositories; it does not migrate their history or configure their MCP clients.

## Upgrade the CLI and integration files

Keep a project backup before opening an old database with a new CLI. Build or install the reviewed CLI version, then check:

```bash
deciduous --version
deciduous remote --help
deciduous check-update
```

If you choose to run `deciduous update`, review its changes to generated agent instructions and hooks. That command updates integrations, not the Postgres service. Existing local-first instructions may still tell agents to use SQLite commands; make the shared-write path explicit in your project instructions. Do not run bulk adoption until each workspace name and cutover has been verified.

## Upgrade the local shared server

Check out the reviewed new source release in your server checkout. Review its Ecto migrations and dependency/security changes. Keep the current source revision and image available. Test the release against a restored backup before a production upgrade.

For the bundled local Compose stack:

```bash
python3 scripts/team-memory/local-stack.py upgrade
```

Pause all agents using the server, then:

```bash
python3 scripts/team-memory/local-stack.py upgrade --apply --writers-paused
python3 scripts/team-memory/local-stack.py check
```

The helper checks that the current app is running and Postgres is major version 16. It builds the new app, writes a custom-format database backup, checks that the archive is readable, and records the previous app image ID. Then it replaces only the MCP service. The app's entrypoint runs Ecto migrations before starting. The database container and its volume stay unchanged.

Afterward, verify one existing goal with its edges and an attachment, then test a new two-agent write/read handoff. Reconnect MCP clients after a server restart if their session has expired. Retain the backup until that verification passes.

The helper targets the bundled local Compose stack only. For the remote example or an existing deployment, perform the same reviewed sequence with that deployment's exact project, env file, and Compose file. Do not substitute a whole local deployment config for a live config you have not compared.

## Restore a backup without overwriting the live database

Restore into a new database on a separate test Postgres instance. Use a client version compatible with the archive. A custom archive uses `pg_restore`, not `psql`. The [PostgreSQL backup guide](https://www.postgresql.org/docs/16/backup-dump.html) describes both formats and notes that database dumps do not include cluster-wide roles or tablespaces.

On an isolated test instance, with connection details supplied through your approved credential mechanism:

```bash
createdb --template=template0 deciduous_restore_test
pg_restore --exit-on-error --single-transaction --no-owner --no-privileges \
  --dbname=deciduous_restore_test /path/to/deciduous-backup.dump
```

Set `PGHOST`, `PGPORT`, and `PGUSER` to that test instance before running these commands. Use a private `.pgpass` or your secret store, not a password argument. The commands deliberately avoid `--clean` and do not drop an existing database. Choose a fresh database name if the test name already exists.

Start the matching app version against the restored database on an isolated app port. Check nodes and edges and fetch an attachment. Recreate and verify the intended production roles/permissions before any cutover; `--no-owner --no-privileges` is for the test restore, not a permissions backup.

If an app upgrade fails after schema migrations ran, replacing the image alone may not restore compatibility. Keep writers paused. Restore the pre-upgrade dump into a new database, run the previous app against it, verify the result, then make a deliberate connection cutover. Keep the failed database for investigation. Account for any writes accepted after the backup before choosing a rollback.

## Upgrade Postgres separately

The scripts keep major version 16. A database major-version change needs its own tested `pg_upgrade` or dump/restore plan, rollback window, and compatibility checks. Never change the image to a new major version while reusing the same data volume and assume startup will migrate it. Patch releases within the selected major should also go through backup and verification.
