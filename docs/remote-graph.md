<!-- Generated from content/run-a-server.md by scripts/docs/build.mjs. Edit the source. -->

# Run a server for your team

A deciduous server is two containers, the server and PostgreSQL, on one machine behind an HTTPS proxy. You start it once, then give each person a URL and a token. Every developer's CLI and every agent's MCP client writes to it, one workspace per repository.

This is how the reference install runs: `https://bobbby.online/deciduous-mcp` is a Hetzner VM where Caddy proxies a path prefix to the server container. The steps below were run against the v1.1.0 release bundle on 2026-11-08, with the output shown.

## What you need

- A Linux VM you can SSH to, with Docker Engine and Compose v2. If the `docker compose` plugin is missing, the setup script falls back to `docker-compose`.
- A DNS name for it, such as `deciduous.example.com`.
- Ports 80 and 443 open to the people who will connect. Leave every other port closed. The server listens only on `127.0.0.1`, and PostgreSQL has no host port at all.
- Roughly 1 GB of RAM free while the image builds.

## 1. Download and check the release bundle

Use the bundle from the same release as the CLI your team runs:

```bash
VERSION=v1.1.0
curl -fLO https://github.com/notactuallytreyanastasio/deciduous/releases/download/$VERSION/deciduous-mcp-docker.tar.gz
curl -fLO https://github.com/notactuallytreyanastasio/deciduous/releases/download/$VERSION/checksums.txt
grep deciduous-mcp-docker.tar.gz checksums.txt | sha256sum -c -
tar -xzf deciduous-mcp-docker.tar.gz
cd deciduous-mcp-docker
```

`sha256sum -c` should print `deciduous-mcp-docker.tar.gz: OK`. On macOS, use `shasum -a 256 -c -` instead.

## 2. Start it

```bash
./scripts/setup.sh
```

The bundle contains source code, not an image. `setup.sh` builds the server image on the VM: it pulls the Elixir builder and Debian base images, compiles the release, then starts PostgreSQL 17.11 and the server. Expect it to take a while the first time. On a slow connection it took 30 minutes, almost all of it spent downloading base images. When it finishes:

```
Deciduous is ready: http://127.0.0.1:24555/mcp
Credentials are in /opt/deciduous-mcp-docker/.env (DECIDUOUS_MCP_TOKEN).
Rerun this command to rebuild and upgrade. Your settings and database volume are preserved.
```

`setup.sh` returns only once `/ready` reports that the database is reachable and migrated. It chooses a random port between 20000 and 32767 so that it never takes a port the machine already uses. To choose the port yourself, set `DECIDUOUS_PORT=24555 ./scripts/setup.sh` on the first run.

Everything that defines the install is in `.env`, which is created with mode 0600:

```
COMPOSE_PROJECT_NAME=deciduous
DECIDUOUS_SERVER_MODE=docker
DECIDUOUS_DATABASE_MODE=local
DECIDUOUS_BIND_ADDRESS=127.0.0.1
DECIDUOUS_PORT=24555
DECIDUOUS_MCP_TOKEN='<64 hex digits>'
POSTGRES_PASSWORD='<64 hex digits>'
DATABASE_URL='ecto://deciduous:<password>@db:5432/deciduous'
DB_SSL=false
DB_SSL_VERIFY=full
DB_SSL_CA_FILE=
POOL_SIZE=10
```

Back this file up together with the database. The token in it is what every client presents, and `COMPOSE_PROJECT_NAME` names the PostgreSQL volume (`<project>_postgres-data`).

Check the install from the VM:

```bash
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:24555/health   # 200
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:24555/ready    # 200
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:24555/mcp      # 401: no token
```

## 3. Put HTTPS in front of it

Clients on other machines need HTTPS. Here is a Caddy site on the VM that serves the server at the root of a domain:

```caddy
deciduous.example.com {
	reverse_proxy 127.0.0.1:24555 {
		flush_interval -1
	}
}
```

What the proxy must do is pass WebSocket upgrades through, because `deciduous remote watch` streams events over `/events`. Caddy does this by default. `flush_interval -1` is optional in 1.1.0. Earlier versions of this page said it was required for MCP streaming, but the 1.1.0 server does not stream: `POST /mcp` returns plain JSON with a `Content-Length`, and `GET /mcp` is refused with 405 (`this server does not offer a server-to-client stream`). Behind a Caddy without the setting, `initialize`, `add_node` and `remote watch` all worked. It is kept here because it costs nothing and protects you if a later server streams.

To share a domain with something else, serve the server under a path, as the reference install does. `handle_path` strips the prefix, so the server still sees `/mcp` at its own root:

```caddy
example.com {
	handle_path /deciduous-mcp/* {
		reverse_proxy 127.0.0.1:24555 {
			flush_interval -1
		}
	}
	handle {
		respond 404
	}
}
```

Keep the final `handle` if nothing else is served on that domain. Without it, Caddy answers every other path with an empty 200. A developer who leaves the prefix off the URL then gets `the server's response was not a graph: Failed to read JSON: EOF while parsing` instead of the CLI's clear `no such endpoint (404). Is the URL right, including any path prefix?`. Both behaviours were tested. If the domain already serves something else, that site answers instead, and the error depends on what it returns.

If Caddy runs in a container, join it to the bundle's Compose network and proxy to `server:4000`. Port 4000 is the port inside the network; `DECIDUOUS_PORT` applies only on the host. The path-prefix form was tested exactly this way: health, readiness, CLI writes, an MCP `add_node`, and `remote watch` all went through Caddy.

From your laptop:

```bash
curl -s -o /dev/null -w '%{http_code}\n' https://deciduous.example.com/health  # 200
curl -s -o /dev/null -w '%{http_code}\n' https://deciduous.example.com/mcp     # 401
```

`/health` and `/ready` are public on purpose, so that proxies and uptime checks can use them without a token. Every other route requires the token.

## 4. Hand out the URL and the token

Each person needs two things:

- the base URL: `https://deciduous.example.com`, or `https://example.com/deciduous-mcp` for the path form. This is a base URL, without `/mcp`. The CLI adds the paths it needs, and only the agent's MCP client is given `<base>/mcp`.
- the token: `DECIDUOUS_MCP_TOKEN` from `.env`, sent over a channel you trust.

From there, each person follows [Join a server](content/quickstart.md). There is one token for the whole server. Read [what that means](#what-one-token-means) before you hand it out.

## Use a managed PostgreSQL instead

If your provider runs PostgreSQL for you, start only the server container and point it at that database. You need a role that owns the database. Migrations run `ALTER DATABASE` to set planner defaults, so owning the schema alone is not enough.

```bash
export DATABASE_URL='ecto://deciduous:URL_ENCODED_PASSWORD@db.internal.example.com:5432/deciduous'
export DECIDUOUS_MCP_TOKEN="$(openssl rand -hex 32)"
export DB_SSL=true            # TLS to the database
./scripts/setup.sh --external-database
unset DATABASE_URL DECIDUOUS_MCP_TOKEN
```

The server runs the pending migrations before it starts listening, and setup remembers that this install uses an external database. `DB_SSL_VERIFY` controls how much of the database server's certificate is checked:

| `DB_SSL_VERIFY` | libpq equivalent | What is checked |
|---|---|---|
| `full` (default) | `verify-full` | The certificate chain, and that the certificate names the host in `DATABASE_URL`. |
| `ca` | `verify-ca` | The chain only. Use this when you connect by an address the certificate does not name. |
| `none` | `require` | Nothing. The connection is encrypted but the server is not authenticated, and a warning is logged at every boot. |

For a private CA, put its PEM file at `certs/postgres-ca.pem` and set `DB_SSL_CA_FILE=/app/certs/postgres-ca.pem`. The server refuses to start if that file is missing or empty. Each boot logs the mode in effect:

```
Database TLS: verify-full: chain against DB_SSL_CA_FILE (/app/certs/postgres-ca.pem, 1 certificate(s)), certificate must name db.internal
```

Pages written for deciduous 1.0.4 said that the server never verified the database certificate. That stopped being true when `DB_SSL_VERIFY` was added. 1.1.0 verifies the chain by default (`deciduous_mcp/config/runtime.exs`).

The managed-database path was not run end to end for this page. The settings above are read from `runtime.exs` and `scripts/setup.sh`.

## Without Docker

The release also ships native server executables (`deciduous-mcp-linux-amd64` and `-linux-arm64`) with Erlang and Elixir built in. On a VM you can run one under systemd against any PostgreSQL. `deciduous_mcp/DEPLOY.md` in the repository walks through it, including the unit file. The CLI's `remote setup --local` installs this same executable, but as a service bound to loopback on a developer's machine. That case is covered in [Without a shared server](content/local.md).

## What one token means

The bearer token is the server's only access control:

- **It covers every workspace on the server.** `X-Deciduous-Workspace` and the `workspace` argument route a request to a workspace; they do not authorize it. Anyone with the token can read and write every project on the server. If two teams must not see each other's graphs, run two servers.
- **There are no per-user identities.** A node on the server records the branch its writer reported, and nothing about who wrote it. Checked with `/export`: the metadata holds `branch` and `confidence`, and there is no author field. Use branch names to tell agents apart (see [Many agents, one graph](content/teams.md)).
- **`/events` also accepts the token as a `?token=` query parameter**, as a fallback for WebSocket clients that cannot set headers. Query strings end up in access logs, so turn off query logging on your proxy, or redact it.
- **Rotating the token logs out everyone at once.** See [Rotate the token](content/operating.md#rotate-the-token).

Backups, upgrades and rotation are covered in [Backups, upgrades, tokens](content/operating.md).
