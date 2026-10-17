# Run a shared Deciduous server

The server needs a PostgreSQL. It keeps graphs and document content there and
exposes an authenticated HTTP MCP endpoint. Use the PostgreSQL you already have
on localhost:5432, or any other you can name by URL; if you have none, Docker
can run one for you. There are two ways to run the server itself:

- **The native executable**, a single file with Erlang/OTP and Elixir inside,
  against a PostgreSQL you already have. This is what `deciduous init` and
  `deciduous remote setup` install on a developer machine by default, as a
  launchd or systemd user service. It is the recommended path and is
  described first below.
- **Docker with Compose.** Erlang, Elixir, Mix, and PostgreSQL tools run inside
  the containers, and setup can create the PostgreSQL too. `deciduous remote
  setup --docker-postgres` runs this path for you; `--docker` runs only the
  server container, against your database. Before 1.0.11 this was the only
  way, and existing Docker installs stay as they are.

No PostgreSQL on this machine? `brew install postgresql@17 && brew services
start postgresql@17` on macOS (Postgres.app works too), `sudo apt install
postgresql` on Debian/Ubuntu, or take the Docker path. The CLI prints the same
hint when nothing answers on localhost:5432.

## Run the native executable

Releases include `deciduous-mcp-<os>-<arch>` Burrito executables
(`deciduous-mcp-darwin-arm64`, `deciduous-mcp-darwin-amd64`,
`deciduous-mcp-linux-amd64`, `deciduous-mcp-linux-arm64`,
`deciduous-mcp-windows-amd64.exe`). They contain Erlang/OTP, Elixir, and the
application, so the machine running the server does not need a BEAM
installation.

`deciduous remote setup` (the default choice, or `--database-url`) installs one
as a user service under `~/.config/deciduous/server/`. What it does is what you
would do by hand:

1. Download `deciduous-mcp-<os>-<arch>` for the CLI's own version and check it
   against the release's `checksums.txt`. Install it as
   `~/.config/deciduous/server/bin/deciduous-mcp`, mode 755.
2. Choose a bearer token of at least 32 bytes (`openssl rand -hex 32`). The CLI
   reuses the token in `~/.config/deciduous/credentials` if there is one, so a
   machine never has two; otherwise it draws 64 hex digits from the OS random
   source.
3. Test the database and prepare it. With `DECIDUOUS_MCP_COMMAND=prepare-database`
   the executable connects, creates the database if PostgreSQL says it does
   not exist (the role needs `CREATEDB` for that), runs every pending
   migration, prints `database ready` and exits 0; otherwise it prints
   PostgreSQL's reason (`role "x" does not exist`, `password authentication
   failed`, `permission denied to create database`) and exits 1. No `psql` is
   needed, and PostgreSQL 16 works (the migrations were run against Homebrew
   16).

   ```bash
   cd ~/.config/deciduous/server
   DECIDUOUS_MCP_COMMAND=prepare-database \
   DATABASE_URL='ecto://USER:URL_ENCODED_PASSWORD@localhost:5432/deciduous' \
   DB_SSL=false DB_SSL_VERIFY=full DECIDUOUS_MCP_TOKEN='<token>' \
   DECIDUOUS_MCP_INSTALL_DIR=~/.config/deciduous/server/runtime \
   POOL_SIZE=2 PORT=0 ./bin/deciduous-mcp
   ```

   The default database is `deciduous` on localhost:5432 as the OS user with
   no password: Homebrew and Postgres.app create a superuser named after the
   OS user and trust local connections, and Debian-style packages
   authenticate local users by name.
4. Pick a port on 127.0.0.1. The CLI asks, suggesting a free one from the same
   20000-32767 range the Docker setup draws from (see [First install](#first-install)
   for why), and refuses a port that is already bound. Port 0 is refused too:
   the URL written to each project's config would not survive a restart.
5. Write the settings file, `~/.config/deciduous/server/.env`, mode 600. Every
   key the server reads is in it, so nothing depends on the environment of
   whoever starts it:

   ```
   DECIDUOUS_SERVER_MODE=native
   DECIDUOUS_SERVER_VERSION=1.0.12
   DECIDUOUS_BIND_ADDRESS=127.0.0.1
   DECIDUOUS_PORT=24987
   DECIDUOUS_MCP_TOKEN='<at least 32 bytes>'
   DATABASE_URL='ecto://USER:URL_ENCODED_PASSWORD@localhost:5432/deciduous'
   DB_SSL=false
   DB_SSL_VERIFY=full
   DB_SSL_CA_FILE=
   POOL_SIZE=10
   DECIDUOUS_PREPARE_DATABASE=true
   ```

   `DB_SSL`, `DB_SSL_VERIFY` and `DB_SSL_CA_FILE` are the TLS settings
   described under [Use an existing PostgreSQL database](#use-an-existing-postgresql-database);
   `?sslmode=require|verify-ca|verify-full` on the URL given to
   `--database-url` fills them in. `DECIDUOUS_SERVER_MODE` is how the CLI
   tells a native install from a Docker one (a file without it predates
   native servers and is Docker). `DECIDUOUS_SERVER_VERSION` is what
   `deciduous update` compares against its own version.
6. Run the executable with `DECIDUOUS_ENV_FILE` pointing at that file and
   `DECIDUOUS_MCP_INSTALL_DIR` at a directory it may unpack its runtime into.
   The file is parsed, not executed, and wins over the process environment.
   `DECIDUOUS_PREPARE_DATABASE=true` repeats step 3 at every start, so an
   upgraded executable migrates before it listens. `DECIDUOUS_BIND_ADDRESS`
   keeps it on loopback; unset, it listens on every interface. In a terminal:

   ```bash
   DECIDUOUS_ENV_FILE=~/.config/deciduous/server/.env \
   DECIDUOUS_MCP_INSTALL_DIR=~/.config/deciduous/server/runtime \
   ~/.config/deciduous/server/bin/deciduous-mcp
   ```

   The CLI instead starts it as a user service that runs at login and is
   restarted whenever it exits, with output in
   `~/.config/deciduous/server/logs/server.log`. On macOS that is a
   LaunchAgent at `~/Library/LaunchAgents/dev.deciduous.server.plist`:

   ```xml
   <?xml version="1.0" encoding="UTF-8"?>
   <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
   <plist version="1.0">
   <dict>
     <key>Label</key>
     <string>dev.deciduous.server</string>
     <key>ProgramArguments</key>
     <array>
       <string>/Users/you/.config/deciduous/server/bin/deciduous-mcp</string>
     </array>
     <key>EnvironmentVariables</key>
     <dict>
       <key>DECIDUOUS_ENV_FILE</key>
       <string>/Users/you/.config/deciduous/server/.env</string>
       <key>DECIDUOUS_MCP_INSTALL_DIR</key>
       <string>/Users/you/.config/deciduous/server/runtime</string>
     </dict>
     <key>WorkingDirectory</key>
     <string>/Users/you/.config/deciduous/server</string>
     <key>RunAtLoad</key>
     <true/>
     <key>KeepAlive</key>
     <true/>
     <key>ThrottleInterval</key>
     <integer>10</integer>
     <key>StandardOutPath</key>
     <string>/Users/you/.config/deciduous/server/logs/server.log</string>
     <key>StandardErrorPath</key>
     <string>/Users/you/.config/deciduous/server/logs/server.log</string>
   </dict>
   </plist>
   ```

   loaded with `launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/dev.deciduous.server.plist`.
   launchd has no environment-file directive, and putting the database
   password and token in the plist would copy them into a second file, so
   the service passes only the path and the server reads the file itself.
   On Linux it is a systemd user unit at
   `~/.config/systemd/user/deciduous-server.service`:

   ```ini
   [Unit]
   Description=deciduous graph server
   After=network.target

   [Service]
   Environment="DECIDUOUS_ENV_FILE=/home/you/.config/deciduous/server/.env"
   Environment="DECIDUOUS_MCP_INSTALL_DIR=/home/you/.config/deciduous/server/runtime"
   WorkingDirectory="/home/you/.config/deciduous/server"
   ExecStart="/home/you/.config/deciduous/server/bin/deciduous-mcp"
   Restart=always
   RestartSec=5
   StandardOutput=append:/home/you/.config/deciduous/server/logs/server.log
   StandardError=append:/home/you/.config/deciduous/server/logs/server.log

   [Install]
   WantedBy=default.target
   ```

   enabled with `systemctl --user daemon-reload && systemctl --user enable
   deciduous-server.service && systemctl --user restart deciduous-server.service`.
   User services stop at logout unless lingering is on:
   `loginctl enable-linger $USER`.
7. Wait for `http://127.0.0.1:<port>/ready` to answer 200. It checks the
   database and migrations; the first start unpacks the embedded runtime, so
   allow two minutes. The CLI then stores the token in
   `~/.config/deciduous/credentials` (mode 0600), writes `[remote] url =
   http://127.0.0.1:<port>` to the project's `.deciduous/config.toml`, and
   registers the server with Claude Code at user scope.

On a machine with no launchd and no `systemctl --user` (a container, a bare
shell on a server) the CLI offers to run the server in Docker instead;
declined, it does steps 1-5 and prints the foreground command from step 6 for
you to run by hand, then stops until the server answers.

`deciduous update` in a project that writes to this machine's native server
replaces an executable older than the CLI (verified like the first), rewrites
`DECIDUOUS_SERVER_VERSION`, and restarts the service; the server migrates as it
starts. Docker installs are not upgraded by `update`. The settings file is
authoritative once written: `--docker` or `--docker-postgres` on a machine with
a native install, or `--database-url` on a Docker one, refuses rather than
replacing it, and `--local` uses whichever is there. To remove a native
install:

```bash
launchctl bootout gui/$(id -u)/dev.deciduous.server      # macOS
rm ~/Library/LaunchAgents/dev.deciduous.server.plist
systemctl --user disable --now deciduous-server.service  # Linux
rm -r ~/.config/deciduous/server                          # settings and executable; the database stays
```

Instead of step 3 you can load the schema snapshot. For a fresh database, download the binary and `deciduous-mcp-structure.sql`
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

`psql` is needed only for that bootstrap. For upgrades that need migrations,
start the new executable once with `DECIDUOUS_MCP_COMMAND=prepare-database`
(or with `DECIDUOUS_PREPARE_DATABASE=true`). Run the appropriate binary name
for your platform; on Windows it ends in `.exe`. Run this way, with no
settings file, the server reads `PORT` and listens on every interface unless
`DECIDUOUS_BIND_ADDRESS` says otherwise.

For maintainers, regenerate the checked-in snapshot after changing migrations,
using a disposable PostgreSQL 17 database with all repository migrations
applied and PostgreSQL 17 client tools. Do not use a production database for
this step:

```bash
DATABASE_URL='ecto://USER:PASS@HOST/DATABASE' ./scripts/dump_structure.sh
```

The script verifies the migration ledger and exports the schema without user
data. Review and commit the migration and `STRUCTURE.sql` changes together.

## Run it in Docker

`deciduous remote setup --docker-postgres` (choice 3 in the `init` menu)
downloads the release's `deciduous-mcp-docker.tar.gz`, checks it, and runs the
setup below for you with the settings file at
`~/.config/deciduous/server/.env`; `--docker` does the same with
`--external-database` against the PostgreSQL from choice 1 or 2, rewriting
`localhost` to `host.docker.internal`. The rest of this section is the same
install by hand.

### First install

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
port. The server listens on `127.0.0.1` only, so it is reachable only from this
machine by default, on a port chosen at random from 20000-32767 — above the
registered range, below the ephemeral range the host draws outgoing ports from.
The port is random rather than fixed so that an install never squats a port the
machine already wants: a server sitting on 4000 breaks a Phoenix dev server, and
two installs sharing a default collide. Setup writes the chosen port to `.env`
as `DECIDUOUS_PORT` and prints the URL.

Setup returns success only after `/ready` confirms database access. It prints
the MCP URL but keeps the token out of terminal output. Open `.env` to get
`DECIDUOUS_MCP_TOKEN` for your client. Keep this file private and back it up
alongside the database. `.env.example` lists the settings.

Rerunning setup preserves `.env` and the named database volume, including its
port: `DECIDUOUS_PORT` is read on the first run only. To choose the port instead
of letting setup pick one:

```bash
DECIDUOUS_PORT=4010 ./scripts/setup.sh
```

To change the port of an installation that already exists, edit `DECIDUOUS_PORT`
in `.env`, rerun setup, and update the URL in `[remote]` in each project's
`.deciduous/config.toml` (or rerun `deciduous remote init <url>` there).

### Use an existing PostgreSQL database

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
inside the server container, not at your host's PostgreSQL. Use
`host.docker.internal` for a host database (`deciduous remote setup --docker`
rewrites localhost to it for you). Docker Desktop routes that name to the
host's loopback. On Linux, compose.yaml maps it to the host gateway (the
`docker0` address), where a stock PostgreSQL does not listen: add that address
to `listen_addresses` and a `pg_hba.conf` line for the Docker networks
(172.16.0.0/12), or use a reachable private address, or attach the server to
the database's Docker network. URL-encode reserved characters in the password.
None of this applies to the native executable, which reaches localhost
directly.

For a TLS database connection, set `DB_SSL=true` before first setup or in
`.env`. `DB_SSL_VERIFY` says how much of the server is checked, in libpq's
terms:

| `DB_SSL_VERIFY` | libpq `sslmode` | Checks |
|---|---|---|
| `full` (default; `peer` still accepted) | `verify-full` | the chain, and that the certificate names the host in `DATABASE_URL`: a DNS name, or for an IP address an IP subjectAltName |
| `ca` | `verify-ca` | the chain only. For a database reached by an address its certificate does not name |
| `none` | `require` | nothing: encrypted, server not authenticated. Logged as a warning at every boot |

The chain is checked against the system certificate authorities, or against a
private CA: put its PEM file at `certs/postgres-ca.pem` and set
`DB_SSL_CA_FILE=/app/certs/postgres-ca.pem` (compose mounts the directory
read-only). The server reads that file at boot and refuses to start if it is
missing or holds no certificate, rather than failing every connection later.
Each boot logs one line saying which mode is in effect:

    Database TLS: verify-full: chain against DB_SSL_CA_FILE (/app/certs/postgres-ca.pem, 1 certificate(s)), certificate must name db.internal

A 1.0.1 deployment with `DB_SSL=true` was not verifying anything. Upgrading:
with a public CA and the right hostname, nothing to do; with a private CA, set
`DB_SSL_CA_FILE`; with a certificate that does not name the address you
connect to, also set `DB_SSL_VERIFY=ca`. Reach for `none` only when no CA
file can be had.

Subsequent `./scripts/setup.sh` calls remember that this is an external
database. Do not apply `STRUCTURE.sql` to an existing database. Its migration
ledger is a bootstrap for empty databases, not an upgrade script.

For two installations on one machine, give each a different
`COMPOSE_PROJECT_NAME`, `DECIDUOUS_ENV_FILE`, and `DECIDUOUS_PORT`. These can be
passed as environment variables on the first setup run. The project name and
port are saved in that installation's settings file; use the same
`DECIDUOUS_ENV_FILE` for later runs.

## Connect your client

`deciduous init` and `deciduous remote setup` do this for you: they store the
token in `~/.config/deciduous/credentials`, write `[remote] url` to the
project's `.deciduous/config.toml`, and register the server with Claude Code
at user scope, which is the by-hand line below. An existing `deciduous`
registration in Claude Code is left as it is.

```bash
claude mcp add --scope user --transport http deciduous http://127.0.0.1:<DECIDUOUS_PORT>/mcp \
  --header "Authorization: Bearer $DECIDUOUS_MCP_TOKEN"
```

For another client, or a per-project registration, use
`http://127.0.0.1:<DECIDUOUS_PORT>/mcp` — the URL setup printed, with the
port from `.env` — with the HTTP/Streamable HTTP transport and a bearer
authorization header. For Claude Code, put this in your project's
`.mcp.json`. Claude Code expands the token from its process environment; the
configuration can be shared without committing the secret. The workspace
header pins calls to one project:

```json
{
  "mcpServers": {
    "deciduous": {
      "type": "http",
      "url": "http://127.0.0.1:24871/mcp",
      "headers": {
        "Authorization": "Bearer ${DECIDUOUS_MCP_TOKEN}",
        "X-Deciduous-Workspace": "my-project"
      }
    }
  }
}
```

Before starting Claude Code, load only the token from the settings file created
by setup. For a server the CLI set up, native or Docker, that file is
`~/.config/deciduous/server/.env`; for a Docker install run by hand it is
`.env` in the extracted bundle:

```bash
export DECIDUOUS_MCP_TOKEN="$(sed -n "s/^DECIDUOUS_MCP_TOKEN='\(.*\)'$/\1/p" ~/.config/deciduous/server/.env)"
claude
```

The JSON file does not load `.env` itself. Keep the `${DECIDUOUS_MCP_TOKEN}`
reference in the file and the actual value in your client process environment.
See [Claude Code's environment expansion documentation](https://code.claude.com/docs/en/mcp#environment-variable-expansion-in-mcp-json).
For another MCP client, use its supported secret or header configuration;
`${...}` expansion is client-specific. The CLI itself reads
`DECIDUOUS_MCP_TOKEN` from the environment ahead of the credentials file, which
is how CI passes a token.

For clients on other machines, put a TLS reverse proxy in front of the server
and use its HTTPS URL. Workspaces organize projects; the shared bearer token
grants access across the server. Use separate deployments when teams need
separate access boundaries.

## Back up and upgrade

For a native server, or a Docker server on an external database, the database
is your PostgreSQL: use its own `pg_dump` (`pg_dump -d deciduous
--format=custom > deciduous-backup.dump` for the default install) or your
provider's backup procedure. Back up `~/.config/deciduous/server/.env` with it;
the token in it is what every client presents. Upgrading a native server is
`deciduous update` in any project that uses it, or by hand: install the new
executable over `bin/deciduous-mcp`, set `DECIDUOUS_SERVER_VERSION` in `.env`,
and restart the service. `DECIDUOUS_PREPARE_DATABASE=true` makes the new
executable migrate before it listens.

For the bundled PostgreSQL service, run these commands from your installation
directory. If your system uses `docker-compose`, substitute it for
`docker compose` in the examples:

```bash
umask 077
docker compose -f compose.yaml -f compose.local.yaml exec -T db \
  pg_dump -U deciduous -d deciduous --format=custom > deciduous-backup.dump
```

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

To upgrade a Docker install, extract the new release into a separate
directory, copy your `.env` into it, then run `./scripts/setup.sh` there. Keep
`COMPOSE_PROJECT_NAME` unchanged: it identifies the running services and
persistent PostgreSQL volume. Copy any custom CA from `certs/` as well. Setup
builds the new server, applies pending migrations, and waits for readiness.
Keep the old release and backup until you have checked your clients. Reverting
the binary alone does not undo a database migration; restore the pre-upgrade
backup to a separate database if rollback is needed.

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
    reverse_proxy 127.0.0.1:24871 {
        flush_interval -1
    }
}
```

If Caddy is another container, put both services on the same Docker network and
proxy to `server:4000` (4000 is the port inside the container; the host port
from `.env` does not apply on the Docker network). Leave PostgreSQL unexposed. `flush_interval -1` keeps MCP
streamed responses from waiting in the proxy's buffer. `/health` checks process
liveness; `/ready` also checks the database and is appropriate for deployment
readiness.
