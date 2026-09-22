# Deploying the shared graph server

The server joins the existing `blog` compose stack on the Hetzner box
(`root@5.161.181.91`, `/opt/blog`) the same way `marginalia`, `blinks` and
`nathan` did: its own database on the shared `db` service, its own container on
the internal network, its own Caddy subdomain. Postgres is never published to
the host or the internet.

## 1. Create the database

`init-db.sh` only runs when the `pgdata` volume is first created, so it will
not run again on the live box. Create the database by hand:

```bash
ssh root@5.161.181.91
docker compose -f /opt/blog/docker-compose.yml exec db \
  psql -U blog -d blog_prod -c "CREATE DATABASE deciduous_mcp_prod;"
```

Add it to `init-db.sh` too, so a rebuilt volume still gets it:

```bash
SELECT 'CREATE DATABASE deciduous_mcp_prod'
WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'deciduous_mcp_prod')\gexec
```

## 2. Generate the token

```bash
openssl rand -hex 32
```

Add to `/opt/blog/.env`:

```
DECIDUOUS_MCP_TOKEN=<the 32-byte hex>
```

The app refuses to boot without it, and refuses a token shorter than 32 bytes.
There is no default and no fallback: this process exposes every decision graph
on the machine over HTTP, and the one failure worth making impossible is
starting it unauthenticated because a variable was missing.

## 3. Add the compose service

In `/opt/blog/docker-compose.yml`, alongside `marginalia`:

```yaml
  # Shared decision graph. One Postgres database behind an MCP endpoint, with
  # one workspace per project — see deciduous_mcp/DEPLOY.md.
  deciduous-mcp:
    build:
      context: /opt/deciduous-mcp
    restart: always
    depends_on:
      db:
        condition: service_healthy
    environment:
      DATABASE_URL: ecto://blog:${DB_PASSWORD}@db/deciduous_mcp_prod
      DECIDUOUS_MCP_TOKEN: ${DECIDUOUS_MCP_TOKEN}
      PORT: "4000"
      POOL_SIZE: "10"
    # Not exposed to host - Caddy proxies to port 4000 via Docker network
```

## 4. Add the Caddy site

In `/opt/blog/Caddyfile`:

```
deciduous-mcp.bobbby.online {
	encode gzip zstd

	header {
		X-Forwarded-Proto {scheme}
		alt-svc "clear"
	}

	reverse_proxy deciduous-mcp:4000 {
		header_up X-Real-IP {http.request.header.CF-Connecting-IP}
		header_up X-Forwarded-For {http.request.header.CF-Connecting-IP}
		# MCP streams responses over SSE; without this Caddy buffers them and
		# the client waits for a response that has already been written.
		flush_interval -1
	}
}
```

A DNS record for `deciduous-mcp.bobbby.online` has to exist first (Cloudflare,
same as the other subdomains).

## 5. Ship it

```bash
rsync -az --delete --exclude _build --exclude deps \
  ~/code/deciduous/deciduous_mcp/ root@5.161.181.91:/opt/deciduous-mcp/
ssh root@5.161.181.91 'cd /opt/blog && docker compose up -d --build deciduous-mcp && docker compose restart caddy'
curl https://deciduous-mcp.bobbby.online/health     # -> ok
```

Migrations run from the container's entrypoint before the server starts.

## 6. Load the graphs

```bash
export DECIDUOUS_MCP_URL=https://deciduous-mcp.bobbby.online
export DECIDUOUS_MCP_TOKEN=<the token>
~/code/deciduous/deciduous_mcp/scripts/import_all.sh -n   # dry run first
~/code/deciduous/deciduous_mcp/scripts/import_all.sh
```

Idempotent on `[workspace_id, change_id]`, so re-running refreshes rather than
duplicating. Locally this imports 85 graphs into 73 workspaces: 29,838 nodes
and 80,672 edges.

## 7. Point Claude at it

One user-scope registration, so the server is present in every directory on the
machine:

```bash
claude mcp add --scope user --transport http deciduous \
  https://deciduous-mcp.bobbby.online/mcp \
  --header "Authorization: Bearer $DECIDUOUS_MCP_TOKEN"
```

A repo that wants its workspace pinned regardless of what any tool call says
adds a `.mcp.json` with an extra header:

```json
{
  "mcpServers": {
    "deciduous": {
      "type": "http",
      "url": "https://deciduous-mcp.bobbby.online/mcp",
      "headers": {
        "Authorization": "Bearer ...",
        "X-Deciduous-Workspace": "epstein"
      }
    }
  }
}
```

## What is not covered

- **Themes.** `deciduous graph` exports `nodes`, `edges` and `documents` only,
  though SQLite has `themes`/`node_themes` and Postgres has the columns. Theme
  assignments do not survive the import.
- **Document files.** Document rows are not imported at all; the blobs live in
  each repo's `.deciduous/documents/`.
- **46 self-loop edges** across all graphs are rejected by the schema's
  no-self-loop rule and reported per project, not written.
- **Nothing writes back to SQLite.** This is one-way: local graphs push up.
  `Sync.Bridge.export_checkpoint/2` still exists but targets a checkpoint
  format the current CLI no longer reads.
