<!-- Generated from content/remote-postgres.md by scripts/docs/build.mjs. Edit the source. -->

# Share one server across machines

Keep the same workspace names and put the MCP server behind HTTPS. Agents connect to the MCP service; only that service connects to Postgres. You can run both containers on a private network in one VM or connect the app to a managed database through a private network.

Complete the [local setup](content/local-postgres.md) first. The remote example is a deployment outline, not a hardened hosting platform or a managed service.

## A small VM setup

The repository includes `scripts/team-memory/compose.remote.example.yaml` and `Caddyfile.example`. It is a separate stack, not an overlay for the local Compose file. It publishes only Caddy on ports 80 and 443. Neither Postgres nor the app has a host port.

1. Provision a VM and point a DNS name such as `memory.example.com` at it. Limit SSH access; allow HTTPS and the HTTP port used for certificate issuance. Do not expose ports 4000 or 5432.
2. Install Docker and Compose v2. Copy a reviewed Deciduous release to the VM. Review the server's dependency advisories and pin deployment images to reviewed digests before exposing it to an untrusted network.
3. Create an owner-only env file outside the checkout. Include independent 64-character hex values for `POSTGRES_PASSWORD` and `DECIDUOUS_MCP_TOKEN`, plus `MCP_DOMAIN=memory.example.com`. The local `configure` helper can generate the secrets in a chosen file; add the domain with your editor. Do not print the file or commit it.
4. Review the example, then start it from the checkout, substituting your env-file path:

```bash
docker compose --project-name deciduous-memory \
  --env-file /secure/path/deciduous.env \
  --file scripts/team-memory/compose.remote.example.yaml config --quiet

docker compose --project-name deciduous-memory \
  --env-file /secure/path/deciduous.env \
  --file scripts/team-memory/compose.remote.example.yaml up -d --build

curl --fail https://memory.example.com/health
```

Caddy obtains and renews the public certificate. Its reverse proxy handles WebSocket upgrades; the example disables response buffering for streaming responses. Preserve Caddy's certificate volume. See [Caddy's reverse proxy documentation](https://caddyserver.com/docs/caddyfile/directives/reverse_proxy).

From each workstation, store the token with `deciduous remote login --url https://memory.example.com`, then configure the project:

```bash
deciduous remote init https://memory.example.com --workspace example-app
deciduous remote status
```

Use `https://memory.example.com/mcp` in the agent client. Verify both authorized graph access and an unauthenticated request returning 401. Keep the MCP token out of command arguments, source control, screenshots, and logs. `/health` is public by design.

## If Postgres is managed elsewhere

Set the app's `DATABASE_URL` to the private database address. Allow connections only from the app, use a database-specific role, and arrange migrations with the required schema privileges. The local example's database owner is convenient for development, not a least-privilege production-role design.

The current server supports `DB_SSL=true`, but its runtime configuration uses `verify: :verify_none`. That encrypts traffic without verifying the database server's certificate. Do not treat it as authenticated TLS to an arbitrary public database endpoint. Use a trusted private network or authenticated tunnel, or add and test CA/hostname verification in the server before relying on public database transport. A URL parameter alone does not fix this configuration.

## Operational limits to plan for

The shared bearer token grants access across workspaces. A workspace header pins routing; it is not per-user authorization. Run separate deployments for teams that must not share access, or put a reviewed identity/access layer in front of the service.

`/events` accepts the bearer token in a URL query for WebSocket clients. Redact query tokens at every proxy and observability service before enabling access logs. Rotate a leaked token on the server and reconnect all clients.

Back up the whole Postgres database, including document blobs, and store encrypted copies off the VM. Test a restore into a separate database. Keep role/permission definitions and secret recovery procedures alongside the database backup. Use [the upgrade procedure](content/upgrading.md) before replacing the app; do not repoint a Postgres major-version image at an existing volume.
