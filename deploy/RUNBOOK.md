# Deploying deciduous as party_line's federated memory

One `deciduous serve --api` daemon at `https://deciduous.bobbby.online`, and
every party_line instance points at it, so room and chat histories federate
into one place: a graph per room, many authors writing in.

Nothing here deploys itself. These are the steps a human runs on a VM they
control. The artifacts are in this directory (`Dockerfile` is one level up).

## Architecture

```
  party_line (many instances)
        │  HTTPS + Authorization: Bearer <token>
        ▼
  deciduous.bobbby.online  ── Caddy (auto Let's Encrypt TLS) ─┐
                                                              │  plain HTTP,
                                                              │  private network,
                                                              ▼  never public
                                            deciduous serve --api  (loopback-only)
                                                              │
                                                              ▼
                                    /data/graphs/<room-id>/deciduous.db   (one per room)
```

TLS stops at Caddy. The daemon speaks plain HTTP on a private compose network
and is never published to the host — from the internet it is reachable only
through the proxy.

## Runbook

1. **DNS** — A record `deciduous.bobbby.online` → the VM's public IPv4 (AAAA
   too if it has IPv6), TTL 300. Confirm `dig +short deciduous.bobbby.online`
   resolves *before* first boot — Caddy's ACME challenge fails otherwise.
2. **Firewall** — allow inbound 80 and 443. Do **not** open 4141. (The daemon
   isn't published, but firewall it anyway.)
3. **Host** — install Docker Engine + the compose plugin. Put the repo (or at
   least `Dockerfile`, `.dockerignore`, and `deploy/`) at `/opt/deciduous`.
4. **Secret** — `cd /opt/deciduous/deploy && cp .env.example .env`, set
   `DECIDUOUS_API_TOKEN=$(openssl rand -hex 32)`, `chmod 600 .env`. Store it in
   your password manager — every party_line instance must send this exact value.
5. **First boot** — `cd /opt/deciduous/deploy && docker compose up -d --build`.
   Watch `docker compose logs -f caddy` until it logs a cert obtained, and
   `docker compose ps` shows `deciduous` as `healthy`.
6. **Verify TLS** — `curl -sS https://deciduous.bobbby.online/health` → `ok`
   with a valid cert (no `-k`).
7. **Verify auth** — `TOKEN=<value>; curl -sS -H "Authorization: Bearer $TOKEN"
   https://deciduous.bobbby.online/api/v1/graphs` → `{"ok":true,...}`; the same
   call with no token → 401.
8. **Verify the destroy boundary** — with a graph created, confirm a destructive
   tool is refused at the daemon:
   `curl -sS -X POST -H "Authorization: Bearer $TOKEN"
   https://deciduous.bobbby.online/api/v1/graphs/smoketest/tools/delete_node -d
   '{"node_id":1}'` → `403 tool not permitted over the API`.
9. **Wire party_line** — set on each instance's environment and restart:
   ```
   DECIDUOUS_URL=https://deciduous.bobbby.online
   DECIDUOUS_TOKEN=<the same token>
   DECIDUOUS_ENABLED=true
   ```
   Then drive a room and confirm a write landed:
   `curl -sS -X POST -H "Authorization: Bearer $TOKEN"
   https://deciduous.bobbby.online/api/v1/graphs/room-default/tools/list_nodes -d '{}'`.
10. **Backups** — `mkdir -p /opt/deciduous/backups`, install the cron line in
    `backup.sh`, run it once by hand, confirm a `<graph>-<ts>.db.gz` appears,
    and **test a restore into a scratch graph before trusting it.**
11. **Rollback (code)** — `git checkout <prev-ref> && docker compose up -d
    --build`. `docker compose down` stops everything but keeps the named
    volumes (data + certs).
12. **Restore (data)** — stop the daemon first (`docker compose stop
    deciduous`), replace `/data/graphs/<id>/deciduous.db` in the
    `deciduous-data` volume from a gunzipped backup, then `start`. Swapping a
    SQLite file under an open connection risks corruption even with the cache
    fix, so stop-first is the safe rule.

## Security status

The workflow ran an adversarial review of exposing a token-authed graph DB to
the internet. Here's where each finding stands.

**Closed (in this branch):**

- **No TLS** → Caddy terminates TLS; daemon is loopback-only behind it.
- **Destroy reachable by any token holder** → the daemon now enforces
  append-and-read (`tool_is_allowed`); `delete_node`/`unlink_nodes`/`update_*`
  return 403. This is the authoritative boundary — party_line's Broker mirrors
  it but a leaked token bypasses the client, not the daemon. Verified 403 live.
- **Cache/disk wedge** → filesystem is authoritative; a vanished graph
  re-creates instead of 404-ing forever. Verified live.
- **Token exposure** → env secret from a gitignored `chmod 600` `deploy/.env`,
  never `--token`, never an image layer, not given to Caddy.
- **No `/health`** → added, unauthenticated, side-effect free, pre-auth.
- **No backup** → `backup.sh` uses SQLite online `.backup` (WAL-safe).
- **`graph_id` validation ordering** → validated before any path join.
- **party_line loopback timeouts** → widened + retry, 404 still surfaces.

**You must decide (not closed):**

- **One shared global token, no per-graph scoping.** Anyone with the token can
  read/write **every** room's graph — there is no tenant isolation. Acceptable
  only if the token is treated as a fully-trusted secret and every party_line
  instance is trusted. Real isolation means per-graph capability tokens — a
  deciduous feature that does not exist yet. **Decide whether one shared token
  is acceptable for your trust model before going public.**
- **No rate limiting.** Caddy caps body size (2 MB), but there's no per-IP rate
  limit, and a token holder can create unbounded graphs (disk/fd/memory). Add a
  Caddy rate-limit plugin or a daemon-side cap if the token is ever shared
  beyond instances you fully control.
- **`POST /graphs/<id>/query` runs arbitrary read-only SQL** with no statement
  timeout — a token holder can dump every graph and burn CPU. party_line never
  uses it. Consider disabling the `/query` route in the daemon if nothing you
  run needs it.

**Minor (accepted):** error envelopes echo some internal detail; token-length
timing side channel in `constant_time_eq`. Low sensitivity given the trust model.
