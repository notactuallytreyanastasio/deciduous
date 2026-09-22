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

**Check for drift first.** The live `/opt/blog/docker-compose.yml` is not
always what is in `~/code/blog`: at time of writing the box had a
`marginalia_drafts` volume and `DRAFT_REPO_ROOT` that the repo did not, holding
each Marginalia draft's git history. Rsyncing the repo copy over it would have
destroyed that. Edit the live file in place and backport the change.


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

## 4. Add the Caddy route

Deployed as a **path on the existing domain**, not a subdomain:
`deciduous-mcp.bobbby.online` has no DNS record and there is no wildcard, so a
subdomain would need a Cloudflare record before Caddy could get a certificate.
A path needs neither. Inside the `bobbby.online` block, **before** the
catch-all `handle` that sends everything to Phoenix:

```
	handle_path /deciduous-mcp/* {
		reverse_proxy deciduous-mcp:4000 {
			# MCP streams responses over SSE; without this Caddy buffers them
			# and the client waits for a response already written.
			flush_interval -1
		}
	}
```

`handle_path` strips the prefix, so the app sees `/mcp`, `/import`,
`/blob/:hash` and `/documents/:id` at its own root.

To move to a subdomain later, add the Cloudflare record and swap this for its
own site block; nothing in the app changes.

Reload rather than restart Caddy — `docker compose exec caddy caddy reload
--config /etc/caddy/Caddyfile` — so the other dozen sites on that box do not
blink.

## 5. Ship it

The Dockerfile's three ARGs must name a tag that actually exists on Docker Hub
— hexpm publishes only specific elixir/erlang/debian combinations, and a
plausible-looking guess fails at `load metadata` with "not found". Check before
building:

```bash
curl -s "https://hub.docker.com/v2/repositories/hexpm/elixir/tags?page_size=100&name=1.18.4-erlang-28" \
  | python3 -c "import json,sys;[print(r['"'"'name'"'"']) for r in json.load(sys.stdin)['"'"'results'"'"']]"
```

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

It runs in two phases. First every file under any `.deciduous/documents/` is
uploaded under its own computed sha256 — a content-addressed sweep, not a
per-database walk, because files drift: `deep-squishing-sparrow.md` sits in
njlegalize-me's documents directory while the only row referencing it lives in
a different graph whose documents directory does not exist. Then the graphs
import, and each document row records whether its bytes actually arrived.

Idempotent on `[workspace_id, change_id]` and on content hash, so re-running
refreshes rather than duplicating. Locally this imports 85 graphs into 73
workspaces: 29,838 nodes, 80,672 edges, 85 document rows and 71 blobs.

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
- **Documents are imported**, content and all: 71 blobs, 22MB, stored in
  Postgres `document_blobs` keyed by sha256 so a file attached in several
  projects is stored once. `GET /documents/:id` serves them behind the same
  bearer token. Five files are referenced by live rows but their bytes are gone
  from this machine entirely; those rows import as history and answer 410.
- **46 self-loop edges** across all graphs are rejected by the schema's
  no-self-loop rule and reported per project, not written.
- **Nothing writes back to SQLite.** This is one-way: local graphs push up.
  `Sync.Bridge.export_checkpoint/2` still exists but targets a checkpoint
  format the current CLI no longer reads.
