# Local Postgres for Deciduous

This directory contains a Postgres-only Docker setup. Nothing has started yet.
It does not contain the Deciduous MCP service or expose agent tools.

The Dockerfile uses PostgreSQL 16. Compose stores data in a named volume and
publishes port 55432 on `127.0.0.1` by default. The chosen port is in `.env`.
Database and user default to `deciduous`; `.env` contains the generated password
and was created with owner-only permissions. Do not commit, print, or share it.

## Start and check

Start Docker, then run from this directory:

```bash
docker compose config --quiet
docker compose up -d --build --wait
docker compose exec db psql -U deciduous -d deciduous -c 'SELECT 1;'
```

Use Compose v2. A standalone v2 `docker-compose` works with the same arguments.
Do not publish expanded `docker compose config` output because it includes the
password. Exported `POSTGRES_*` variables in your shell override `.env`; unset
unrelated values before running Compose, or use a clean shell. A port collision
requires a different local port, not removal of the existing database.

## Connect a separate MCP service

Agents connect to the MCP service, not to this Postgres port. A host-run MCP
service needs a database URL plus its own independent bearer token. In a shell
where tracing is disabled, load this generated file and construct the URL:

```bash
set -a
. ./.env
set +a
export DATABASE_URL="ecto://${POSTGRES_USER}:${POSTGRES_PASSWORD}@127.0.0.1:${POSTGRES_PORT}/${POSTGRES_DB}"
```

Pass that environment to a production release of `deciduous_mcp`, or a host
process configured with `MIX_ENV=prod`. The current runtime reads `DATABASE_URL`
only in production mode; a default development `mix run` ignores this URL.
Generate `DECIDUOUS_MCP_TOKEN` separately; the database password is not an MCP
token. Run the app's Ecto migrations before starting it. A Docker container has its
own loopback address, so this host URL is for a host-run app. Use the full
Compose setup when you want the MCP service and Postgres on one container
network with no published database port.

Full setup and MCP configuration:
https://deciduous.dev/tutorial/local-postgres.html

## Preserve the database

`docker compose stop` stops the container and keeps its data. Start it again
with `docker compose up -d`. Keep the same Compose project/directory name so
Compose selects the same named volume. Moving or renaming the directory can
select a different volume and make the old graph appear missing.

Changing `POSTGRES_PASSWORD`, `POSTGRES_USER`, or `POSTGRES_DB` in `.env` does
not change an initialized database. Those image settings initialize an empty
data directory. Rotate an existing database role's password through Postgres
and update app credentials in a coordinated change.

Create database backups, store them off this machine, and test a restore.
Do not use volume-removal or pruning options. Do not change the Dockerfile to
a new Postgres major version while reusing this data volume. Upgrade guide:
https://deciduous.dev/tutorial/upgrading.html

This is a local development configuration. The initialized role owns the
database and is a Postgres superuser. Review roles, private networking, backup
policy, and TLS before remote deployment:
https://deciduous.dev/tutorial/remote-postgres.html
