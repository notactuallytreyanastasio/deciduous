# Run a shared Deciduous server

The server stores graphs and document content in PostgreSQL and exposes an
authenticated HTTP MCP endpoint. Docker with Compose is enough to run it;
Erlang, Elixir, Mix, and PostgreSQL tools run inside the containers.

## First install

Download `deciduous-mcp-docker.tar.gz` and `checksums.txt` from the same
[release](https://github.com/notactuallytreyanastasio/deciduous/releases).
Verify the archive's SHA-256 against `checksums.txt`, then extract it. On Linux:

```bash
sha256sum --ignore-missing --check checksums.txt
tar -xzf deciduous-mcp-docker.tar.gz
cd deciduous-mcp-docker
./scripts/setup.sh
```

On macOS, compare `shasum -a 256 deciduous-mcp-docker.tar.gz` with the matching
line in `checksums.txt`. From a source checkout, use `cd deciduous_mcp` and the
same setup command. Windows users can run these commands in WSL with Docker
Desktop's WSL integration enabled.

The first build downloads dependencies and can take several minutes. Setup
generates a bearer token and database password, saves them in `.env` with mode
`600`, and starts PostgreSQL 17.11 and the server. PostgreSQL has no published
port. The server listens on `127.0.0.1:4000`, so it is reachable only from this
machine by default.

Setup returns success only after `/ready` confirms database access. It prints
the MCP URL but keeps the token out of terminal output. Open `.env` to get
`DECIDUOUS_MCP_TOKEN` for your client. Keep this file private and back it up
alongside the database. `.env.example` lists the settings.

Rerunning setup preserves `.env` and the named database volume. To use a
different local port on the first run:

```bash
DECIDUOUS_PORT=4010 ./scripts/setup.sh
```

## Connect your client

Use `http://127.0.0.1:4000/mcp` with the HTTP/Streamable HTTP transport and a
bearer authorization header. For Claude Code, put this in your project's
`.mcp.json`. Claude Code expands the token from its process environment; the
configuration can be shared without committing the secret. The workspace
header pins calls to one project:

```json
{
  "mcpServers": {
    "deciduous": {
      "type": "http",
      "url": "http://127.0.0.1:4000/mcp",
      "headers": {
        "Authorization": "Bearer ${DECIDUOUS_MCP_TOKEN}",
        "X-Deciduous-Workspace": "my-project"
      }
    }
  }
}
```

Before starting Claude Code, load only the token from the settings file created
by setup. Replace the path below with your installation's `.env` path:

```bash
export DECIDUOUS_MCP_TOKEN="$(sed -n "s/^DECIDUOUS_MCP_TOKEN='\(.*\)'$/\1/p" /path/to/deciduous-mcp-docker/.env)"
claude
```

The JSON file does not load `.env` itself. Keep the `${DECIDUOUS_MCP_TOKEN}`
reference in the file and the actual value in your client process environment.
See [Claude Code's environment expansion documentation](https://code.claude.com/docs/en/mcp#environment-variable-expansion-in-mcp-json).
For another MCP client, use its supported secret or header configuration;
`${...}` expansion is client-specific.

For clients on other machines, put a TLS reverse proxy in front of the server
and use its HTTPS URL. Workspaces organize projects; the shared bearer token
grants access across the server. Use separate deployments when teams need
separate access boundaries.

## Use an existing PostgreSQL database

Keep the existing token so clients can continue to connect. Before upgrading,
take a database backup and test the release against a restored copy. Use the
database owner role, with permission to change the Deciduous schema. Migrations
also run `ALTER DATABASE` to set planner defaults, so schema ownership alone is
not sufficient.

In a newly extracted release directory:

```bash
export DATABASE_URL='ecto://USER:URL_ENCODED_PASSWORD@HOST:5432/DATABASE'
export DECIDUOUS_MCP_TOKEN='<your existing token, at least 32 bytes>'
./scripts/setup.sh --external-database
unset DATABASE_URL DECIDUOUS_MCP_TOKEN
```

Setup saves this configuration and starts only the server. It does not create
a second database. The container applies pending Ecto migrations before
starting the HTTP listener; migrations already recorded in `schema_migrations`
are skipped. This path also works with a new, empty database created by your
database administrator.

The database host must be reachable from the container. `localhost` points
inside the server container, not at your host's PostgreSQL. On Docker Desktop,
use `host.docker.internal` for a host database; on a Linux server, use a
reachable private address or attach the server to the database's Docker network.
URL-encode reserved characters in the password.

For a TLS database connection, set `DB_SSL=true` before first setup or in
`.env`. The server verifies the database certificate using system certificate
authorities by default. For a private CA, put its PEM file at
`certs/postgres-ca.pem` and set
`DB_SSL_CA_FILE=/app/certs/postgres-ca.pem`. Compose mounts this directory
read-only. `DB_SSL_VERIFY=none` explicitly disables certificate verification;
use it only for a legacy database on a trusted private network.

Subsequent `./scripts/setup.sh` calls remember that this is an external
database. Do not apply `STRUCTURE.sql` to an existing database. Its migration
ledger is a bootstrap for empty databases, not an upgrade script.

For two installations on one machine, give each a different
`COMPOSE_PROJECT_NAME`, `DECIDUOUS_ENV_FILE`, and `DECIDUOUS_PORT`. These can be
passed as environment variables on the first setup run. The project name and
port are saved in that installation's settings file; use the same
`DECIDUOUS_ENV_FILE` for later runs.

## Back up and upgrade

For the bundled PostgreSQL service, run these commands from your installation
directory. If your system uses `docker-compose`, substitute it for
`docker compose` in the examples:

```bash
umask 077
docker compose -f compose.yaml -f compose.local.yaml exec -T db \
  pg_dump -U deciduous -d deciduous --format=custom > deciduous-backup.dump
```

For an external database, use your provider's backup or `pg_dump` procedure.
A backup is only proven when you have restored it to a separate database and
verified the graph and document content there.

To check the bundled database's backup, start a temporary PostgreSQL instance
with no network access. Its data lives in memory and disappears when you remove
the container:

```bash
docker run -d --name deciduous-restore-check --network none \
  -e POSTGRES_USER=deciduous -e POSTGRES_DB=postgres \
  -e POSTGRES_HOST_AUTH_METHOD=trust \
  --tmpfs /var/lib/postgresql/data postgres:17.11
docker exec deciduous-restore-check pg_isready -U deciduous -d postgres
```

Once `pg_isready` reports that PostgreSQL accepts connections, restore the
archive. `--create` restores its original database name and database settings,
including `random_page_cost` and `work_mem`. A restore into an already-created
database skips those settings even though it restores the migration ledger.

```bash
docker exec -i deciduous-restore-check pg_restore -U deciduous \
  --create --exit-on-error --dbname=postgres < deciduous-backup.dump
docker exec deciduous-restore-check psql -X -U deciduous -d deciduous \
  -v ON_ERROR_STOP=1 -c 'SELECT count(*) FROM decision_nodes; SELECT count(*) FROM document_blobs; SHOW random_page_cost; SHOW work_mem;'
```

Compare the counts with the backup source. For an application-level check,
run a separate server against a restored copy and verify its graphs and
document downloads. The repository's `scripts/verify-release.sh` includes this
round trip with a synthetic 300-node graph and a binary document. After your
restore check, remove only the temporary copy:

```bash
docker rm -f deciduous-restore-check
```

Extract the new release into a separate directory, copy your `.env` into it,
then run `./scripts/setup.sh` there. Keep `COMPOSE_PROJECT_NAME` unchanged: it
identifies the running services and persistent PostgreSQL volume. Copy any
custom CA from `certs/` as well. Setup builds the new server, applies pending
migrations, and waits for readiness. Keep the old release and backup until you
have checked your clients. Reverting the binary alone does not undo a database
migration; restore the pre-upgrade backup to a separate database if rollback is
needed.

The bundled database stays on PostgreSQL major version 17. Review the
[17.11 upgrade notes](https://www.postgresql.org/docs/release/17.11/) if an
existing database uses custom logical-decoding plugins, PGP encryption, or
`btree_gist`/`ltree` indexes; these can need additional configuration or repair.

For a local database, these commands show status and recent logs:

```bash
docker compose -f compose.yaml -f compose.local.yaml ps
docker compose -f compose.yaml -f compose.local.yaml logs --tail=100 server
```

With an external database, omit `-f compose.local.yaml`. For a custom settings
file, add `--env-file /path/to/installation.env` to the Compose command.

`docker compose ... down` stops the installation while preserving its database
volume. Do not use `down --volumes` unless you intend to delete the database.
Changing `POSTGRES_PASSWORD` in `.env` does not rotate an existing PostgreSQL
role's password; rotate it in PostgreSQL and update `DATABASE_URL` together.

## Verify a release before deployment

From a repository checkout, run the acceptance suite:

```bash
cd deciduous_mcp
./scripts/verify-release.sh
```

It builds the current server and the released v1.0.0 baseline, then uses
isolated PostgreSQL containers to check a fresh install and an upgrade with
existing graphs and documents. It also checks native Burrito startup without
system BEAM tools, authentication, MCP calls, live events, database recovery,
and TLS verification. The fixtures contain synthetic data; the suite does not
connect to your configured production database.

To run just the downloadable bundle checks:

```bash
./scripts/verify-setup.sh
```

This packages and extracts the same archive uploaded by release CI, runs setup,
seeds a graph and binary document, reruns setup, and restarts the server. It
checks that credentials, graph IDs, document bytes, and the PostgreSQL volume
survive, then repeats the checks against an existing database through
`--external-database`. Both commands clean up the containers and volumes they
create. Docker, Bash, and tar are required; the full suite also uses Git to
retrieve the baseline revision.

## Expose the server through HTTPS

The default loopback binding works with a reverse proxy on the same host. For
example, a host-installed Caddy can proxy a domain:

```caddy
deciduous.example.com {
    reverse_proxy 127.0.0.1:4000 {
        flush_interval -1
    }
}
```

If Caddy is another container, put both services on the same Docker network and
proxy to `server:4000`. Leave PostgreSQL unexposed. `flush_interval -1` keeps MCP
streamed responses from waiting in the proxy's buffer. `/health` checks process
liveness; `/ready` also checks the database and is appropriate for deployment
readiness.

## Run the native binary

Releases include `deciduous-mcp-*` Burrito executables for Linux, macOS, and
Windows. They contain Erlang/OTP, Elixir, and the application, so the machine
running the server does not need a BEAM installation.

For a fresh database, download the binary and `deciduous-mcp-structure.sql`
from the same release and verify their checksums. This snapshot requires
PostgreSQL 17 or later and `psql` 17 or later; the release tests use PostgreSQL
17. Create an empty database owned by the connecting role, set a random bearer
token of at least 32 bytes, then bootstrap once:

```bash
export DATABASE_URL='ecto://USER:URL_ENCODED_PASSWORD@HOST:5432/DATABASE'
export DECIDUOUS_MCP_TOKEN='<a random token of at least 32 bytes>'

psql "postgres:${DATABASE_URL#ecto:}" -X --single-transaction \
  -v ON_ERROR_STOP=1 -f deciduous-mcp-structure.sql
chmod +x deciduous-mcp-linux-amd64
./deciduous-mcp-linux-amd64
```

`psql` is needed only for that bootstrap. Use the Docker path for upgrades that
need migrations. Run the appropriate binary name for your platform; on
Windows it ends in `.exe`.

For maintainers, regenerate the checked-in snapshot after changing migrations,
using a disposable PostgreSQL 17 database with all repository migrations
applied and PostgreSQL 17 client tools. Do not use a production database for
this step:

```bash
DATABASE_URL='ecto://USER:PASS@HOST/DATABASE' ./scripts/dump_structure.sh
```

The script verifies the migration ledger and exports the schema without user
data. Review and commit the migration and `STRUCTURE.sql` changes together.
