# Deploy the shared graph service

The Elixir service in this directory exposes HTTP MCP backed by Postgres.
Agents connect to its `/mcp` endpoint with a bearer token; only the service
needs database credentials. The Rust `deciduous serve --api` daemon is a
different deployment and does not use this setup.

Start with the public guides:

- [Local Postgres and MCP setup](../docs/content/local-postgres.md), including
  a database-only generator if you already run the app.
- [Remote deployment](../docs/content/remote-postgres.md), with an HTTPS proxy
  and a private database network.
- [Migration, upgrades, and restore](../docs/content/upgrading.md), including
  the data the SQLite import does not carry over.

The matching templates and helpers live in
[`scripts/team-memory/`](../scripts/team-memory/README.md). The full local
Compose stack builds this directory's Dockerfile, keeps Postgres off host
ports, and publishes MCP only on loopback. The remote Compose and Caddy files
are examples to review for your own host; they are not production credentials
or a complete access-control system.

The service reads `DATABASE_URL` in production mode and requires an independent
`DECIDUOUS_MCP_TOKEN` of at least 32 bytes. A release must apply Ecto migrations
before serving requests; this Dockerfile's entrypoint does that. Store secrets
outside source control and verify authenticated graph access as well as the
public `/health` endpoint.

One bearer token grants access across workspaces. Workspace headers select a
graph; they do not enforce per-user authorization. `DB_SSL=true` currently
uses `verify: :verify_none`, so it encrypts the database connection without
verifying the server certificate. Use a trusted private network or an
authenticated tunnel, or implement and test certificate verification before
relying on public database transport.

Back up Postgres, including document blobs, and test a restore into a separate
database before an upgrade. Keep existing SQLite files and attachments during
migration. Do not replace a live deployment configuration with an example or
point a new Postgres major version at an existing data volume.
