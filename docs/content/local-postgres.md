# Run shared memory on your machine

Run one Postgres-backed Deciduous server, then connect every agent to its HTTP MCP endpoint. Each repository gets a workspace. Agents in different worktrees use the same workspace name when they need to share decisions.

This setup runs on your laptop. Agents call `http://127.0.0.1:4000/mcp`; they do not connect to Postgres. Later, you can put the same server behind HTTPS and change the base URL. See [Remote Postgres](remote-postgres.md).

Only need Postgres? [Generate the database-only setup](#generate-just-postgres) without cloning Deciduous or installing Rust or the CLI.

## What you need

- macOS, Linux, or WSL with Python 3.9 or newer, Git, `unzip`, and Docker with Compose v2. The scripts rely on POSIX file permissions; native Windows is not covered. Docker Desktop includes Compose. On Linux, install the Docker Engine and Compose plugin using the [Docker installation instructions](https://docs.docker.com/engine/install/). Start the Docker daemon before continuing.
- A Deciduous checkout containing `deciduous_mcp/` and `scripts/team-memory/`. Use the same reviewed release for the CLI and server. The commands in these guides target the 1.0.0 source interface.
- Rust 1.85 or newer if you build the CLI from source. The containers build the Elixir server, so you do not need Elixir on your host.

## Get the matching source and setup helpers

Use a new directory so these commands cannot overwrite an existing checkout. The `v1.0.0` tag resolves to the source used by this guide. Until the new helper scripts are included in a GitHub release, download the published bundle from this site:

```bash
mkdir deciduous-setup
cd deciduous-setup
git clone --branch v1.0.0 --depth 1 \
  https://github.com/notactuallytreyanastasio/deciduous.git
curl --fail --location --output team-memory.zip \
  https://deciduous.dev/downloads/team-memory.zip
curl --fail --location --output team-memory.zip.sha256 \
  https://deciduous.dev/downloads/team-memory.zip.sha256
shasum -a 256 -c team-memory.zip.sha256
unzip -l team-memory.zip
cd deciduous
unzip -n ../team-memory.zip
```

On Linux, `sha256sum -c team-memory.zip.sha256` is an alternative checksum command. Check that the archive lists only files under `scripts/team-memory/`. `unzip -n` does not overwrite existing files. If a checksum fails or the listed paths differ, stop before extracting. Use a fresh checkout for a newer bundle instead of mixing helper versions.

From that Deciduous checkout:

```bash
docker version
docker compose version
python3 --version
cargo install --path . --locked
deciduous --version
deciduous remote --help
```

The helper also recognizes the standalone `docker-compose` v2 binary. If the first command cannot reach the daemon, start Docker Desktop or your chosen runtime. Do not continue with a stopped runtime.

Installing the CLI replaces the executable. It does not convert local graphs to Postgres. Existing users should read [Upgrading and migrating](upgrading.md) before moving data.

## Generate just Postgres

The database-only path needs Python 3.9+, Docker with Compose v2, and `unzip`, plus the standard download/checksum tools below. It does not need a Deciduous checkout, Rust, or the CLI. Start in a new empty folder:

```bash
mkdir deciduous-postgres-setup
cd deciduous-postgres-setup
curl --fail --location --output team-memory.zip \
  https://deciduous.dev/downloads/team-memory.zip
curl --fail --location --output team-memory.zip.sha256 \
  https://deciduous.dev/downloads/team-memory.zip.sha256
shasum -a 256 -c team-memory.zip.sha256
unzip -l team-memory.zip
```

Use `sha256sum -c team-memory.zip.sha256` if that is your system's checksum command. Stop if verification fails or the archive contains paths outside `scripts/team-memory/`. Then extract without overwriting and generate the database project:

```bash
unzip -n team-memory.zip
python3 scripts/team-memory/postgres-only.py \
  --output ./deciduous-postgres --port 55432
cd deciduous-postgres
docker compose config --quiet
docker compose up -d --build --wait
docker compose exec db psql -U deciduous -d deciduous -c 'SELECT 1;'
```

The generator creates `Dockerfile`, `compose.yaml`, `README.md`, `.gitignore`, and a private `.env` containing a fresh database password. It refuses an existing directory, file, or symlink and does not start Docker. The parent directory must already exist and must not be a symlink. Postgres uses a persistent named volume and binds only `127.0.0.1:55432`. Change `--port` if that host port is occupied.

New CLI builds with the generator provide the equivalent command:

```bash
deciduous setup --postgres --output ./deciduous-postgres --port 55432
```

The released `v1.0.0` tag above does not include that flag. Use the Python helper from the current download bundle until your installed CLI's `setup --help` lists `--postgres`. Choose a fresh output path; neither generator overwrites an earlier setup.

This generates a database only. Agents still need a separate Deciduous MCP service. For a service running on your host, construct its database URL from the generated credentials without printing them:

```bash
set -a
. ./.env
set +a
export DATABASE_URL="ecto://${POSTGRES_USER}:${POSTGRES_PASSWORD}@127.0.0.1:${POSTGRES_PORT}/${POSTGRES_DB}"
```

Pass `DATABASE_URL` to a host-run production release of `deciduous_mcp`, or a host process configured with `MIX_ENV=prod`. The current runtime reads this URL only in production mode; default development `mix run` uses the development database settings instead. Supply the service's own independent `DECIDUOUS_MCP_TOKEN` and run its Ecto migrations before starting it. Do not reuse the database password as an MCP token. This host URL does not work as a container's `127.0.0.1`; a container has its own network namespace. The full stack below handles container-to-container networking for you and creates its own private Postgres, so it is an alternative to this database-only project.

Do not enable shell tracing while loading secrets. Exported `POSTGRES_*` values override `.env` during Compose interpolation, so use a clean shell or unset unrelated values first. Changing the generated password in `.env` after initialization does not rotate the password stored in Postgres. Keep the volume, back it up, and read the generated README before changing credentials or versions.

You can stop here if you only wanted Postgres. To choose the full MCP-plus-Postgres stack instead, follow [Get the matching source and setup helpers](#get-the-matching-source-and-setup-helpers) and run the following steps from that Deciduous source checkout, not from the generated database directory.

## 1. Create credentials outside the repository

Preview the operation, then apply it:

```bash
python3 scripts/team-memory/local-stack.py configure
python3 scripts/team-memory/local-stack.py configure --apply
```

The script creates `~/.config/deciduous/team-memory/local.env` with owner-only permissions. It generates independent random values for the database password and the server's bearer token. It does not print either value or overwrite an existing file.

The default app port is 4000. To choose another port for a new installation, add `--port 4400` to both commands. For a separate installation, also pass a different `--env-file` and `--project` to every command. A Compose project name selects its database volume; changing it can make an existing graph appear to disappear because you started a different stack.

The server refuses to start without `DECIDUOUS_MCP_TOKEN`, or with a token shorter than 32 bytes. Treat it as access to all workspaces on this server. Workspace names and pinned headers are routing controls, not separate user permissions.

## 2. Start Postgres and the MCP server

```bash
python3 scripts/team-memory/local-stack.py up
python3 scripts/team-memory/local-stack.py up --apply
python3 scripts/team-memory/local-stack.py check
```

The first build can take several minutes. The stack uses `postgres:16-bookworm`, stores data in a named volume, and waits for Postgres to accept connections. The MCP container applies its Ecto migrations before starting the HTTP server. `check` then verifies a SQL query, `/health`, and an authenticated graph export. A health response alone does not prove that graph access works.

The networking is deliberate:

| Connection | Address |
| --- | --- |
| Agent to MCP | `http://127.0.0.1:4000/mcp` |
| CLI remote base URL | `http://127.0.0.1:4000` |
| MCP container to Postgres | `db:5432`, inside its Compose network |
| Host to Postgres | No published port |

The app's host port binds to loopback. Another computer cannot use this local URL. Do not change it to `0.0.0.0` to share it; use the [remote setup](remote-postgres.md) with HTTPS instead. The container-to-database connection uses the private Compose network without database TLS.

## 3. Give the CLI the server token

Run this from the Deciduous source checkout:

```bash
python3 scripts/team-memory/local-stack.py token \
  | deciduous remote login --url http://127.0.0.1:4000
```

The token goes through stdin, not through a command-line argument. `remote login` verifies it and stores it outside repositories in `~/.config/deciduous/credentials`, or under `$XDG_CONFIG_HOME/deciduous/credentials`. Do not run the token helper as an ordinary display command. It refuses a terminal, but piping it to `cat` would still expose the token.

The stored CLI credential does not configure an MCP client. Export the token into the process that launches your agent client, or put it in that client's secret store:

```bash
export DECIDUOUS_MCP_TOKEN="$(python3 scripts/team-memory/local-stack.py token)"
```

Do not turn on shell tracing (`set -x`) while handling credentials. Restart a client that was launched before the environment variable existed. See [Connect your agents](clients.md) for client-specific configuration.

## 4. Point one project at a named workspace

Open a terminal in the project your agents will work on. For a new Deciduous project:

```bash
deciduous init
deciduous remote init http://127.0.0.1:4000 --workspace example-app
deciduous remote status
```

`init` creates local integration files; inspect its Git diff. Skip it if your project is already initialized. `remote init` checks the server and token before writing the `[remote]` section of `.deciduous/config.toml`:

```toml
[remote]
url = "http://127.0.0.1:4000"
workspace = "example-app"
```

Use the same `example-app` workspace in each clone and worktree. Default workspace names derive from the local Git root directory, which may differ between teammates. The explicit name avoids splitting a team into separate graphs.

Run this first connection with one client before launching the team. The current server can return an HTTP 500 if several clients try to create the same new workspace at once. Once the workspace exists, connect the other agents. If a first connection failed in that race, retry it after the first client succeeds.

`remote init` does not redirect ordinary CLI writes. `deciduous add`, `deciduous link`, and the Rust `deciduous mcp` stdio server still use local SQLite. Team agents should write through the HTTP MCP endpoint. Use `deciduous remote pull` to refresh the CLI's node-and-edge cache for local inspection.

## 5. Connect two agents and verify the handoff

Give both agents the server's HTTP MCP URL, the token, and workspace `example-app`. Pin the workspace with `X-Deciduous-Workspace: example-app` where your client supports headers. Do not put a literal bearer token in a tracked config file.

Ask agent A to call `add_node` with `node_type: "goal"`, a short task title, and `workspace: "example-app"`. Keep the returned `change_id`. Ask agent B to query the same workspace, read that goal, and attach an observation with the first node's identifier as its parent through `add_edge`.

Then check the CLI cache:

```bash
deciduous remote status
deciduous remote pull
deciduous nodes
deciduous edges
deciduous remote watch
```

With the 1.0.0 CLI, `remote watch` streams server writes. Older versions can have different behavior; 0.19.0 prints a credential-bearing socket URL instead. Check your version before using it in a terminal or log. Equal counts from `remote status` are useful, but do not prove equal content: read the shared goal and its edge.

Continue with [Working as an agent team](teams.md) for task ownership, branches, locks, and recovery.

## Keep the graph safe

Create a backup before an upgrade:

```bash
python3 scripts/team-memory/local-stack.py backup
python3 scripts/team-memory/local-stack.py backup --apply
```

The script creates a private custom-format `pg_dump` archive and checks that `pg_restore` can read its table of contents. Store a copy on another device or encrypted backup service and rehearse restoring it. Postgres holds the shared graph and uploaded document blobs; a graph JSON export is not a full backup. See [the restore procedure](upgrading.md#restore-a-backup-without-overwriting-the-live-database).

To stop this installation without deleting its volume:

```bash
docker compose --project-name deciduous-team-memory \
  --env-file ~/.config/deciduous/team-memory/local.env \
  --file scripts/team-memory/compose.yaml stop
```

Run the helper's `up --apply` to start it again. Do not use volume-removal options or prune the stack's volume. None of the supplied scripts deletes a database or volume.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| Cannot connect to Docker | Start the Docker daemon; confirm `docker version` shows a server. |
| Port 4000 is occupied | Choose an unused loopback port in your private config, then update the client URL. |
| Health is 200, graph requests are 401 | The client and server must use the same bearer token. A stored CLI token is separate from the agent's environment. |
| HTTP 404 | The CLI uses the base URL; MCP uses the base URL plus `/mcp`. |
| Agent sees a different graph | Check the explicit workspace in both client headers and CLI config. |
| Local graph is behind | Use `remote pull`; SQLite does not receive server writes in the background. |
| Server fails after an update | Keep the backup and volume. Read app/migration logs before retrying; do not reinitialize the database. |

For logs, use the same Compose arguments as the stop command followed by `logs --tail 100 mcp`. Logs may contain task content; review them before sharing. Do not publish expanded `docker compose config` output because it contains credentials.
