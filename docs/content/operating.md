# Backups, upgrades, tokens

There are two things to look after: the PostgreSQL database and the `.env` file next to it. The server itself holds no state, so if you rebuild it from the same `.env` against the same database, the graph comes back. Every command on this page was run against a v1.1.0 Docker install on 2026-11-08, with the output shown, except where a section says otherwise.

Run the commands from the directory that holds the install's `.env` and `compose.yaml`. A Docker install the CLI made (`remote setup --docker-postgres` on a developer machine) keeps them apart: the settings are in `~/.config/deciduous/server/.env` and the compose files in `~/.config/deciduous/server/bundle-1.1.0/deciduous-mcp-docker/`. Run the commands from that bundle directory and add `--env-file ~/.config/deciduous/server/.env` to each `docker compose` command. Each CLI version unpacks its own `bundle-<version>` directory, so the number in the path is the version of the CLI that last ran setup. A private database CA for that install goes in that bundle directory's `certs/`. If your machine has `docker-compose` rather than the `docker compose` plugin, use that; the arguments are the same.

## Back up

For the bundled PostgreSQL:

```bash
umask 077
docker compose -f compose.yaml -f compose.local.yaml exec -T db \
  pg_dump -U deciduous -d deciduous --format=custom > deciduous-$(date +%F).dump
```

The dump includes document contents (`document_blobs`), not only the graph. Copy it off the VM, together with `.env`. Without the token in `.env`, a restored server is one that no existing client can log in to.

For a managed database, use your provider's backups or `pg_dump` against `DATABASE_URL`.

## Prove the backup restores

A backup has not been tested until it has been restored. Restore it into a throwaway PostgreSQL with no network access, held in memory:

```bash
docker run -d --name deciduous-restore-check --network none \
  -e POSTGRES_USER=deciduous -e POSTGRES_DB=postgres \
  -e POSTGRES_HOST_AUTH_METHOD=trust \
  --tmpfs /var/lib/postgresql/data postgres:17.11-alpine
until docker exec deciduous-restore-check pg_isready -h 127.0.0.1 -U deciduous -d postgres; do sleep 1; done

docker exec -i deciduous-restore-check pg_restore -U deciduous \
  --create --exit-on-error --dbname=postgres < deciduous-2026-11-08.dump
docker exec deciduous-restore-check psql -X -U deciduous -d deciduous -tA \
  -c 'SELECT count(*) FROM decision_nodes;' -c 'SHOW random_page_cost;'
docker rm -f deciduous-restore-check
```

Use `-h 127.0.0.1` in the wait. Over the Unix socket, `pg_isready` already reports the image's temporary initialisation server as ready, which shuts down a moment later; a restore started then fails with `terminating connection due to administrator command`. The temporary server does not listen on TCP, so the TCP check waits for the real one.

The test install printed `3` and `1.1`: three nodes, and the database's planner setting. Use `--create`. Without it, the restore lands in a database that already exists and skips the database-level settings the migrations made, even though the migration ledger says they ran.

## Upgrade the server

[Take a backup](#back-up) first. Putting the old binary back does not undo a migration, so rolling back means restoring the pre-upgrade dump.

Extract the new release bundle into a new directory next to the one [Run a server](run-a-server.md) created (`deciduous-mcp-docker`). Copy your `.env` into it, plus `certs/` if you use a private database CA, and run setup there. From the directory that holds `deciduous-mcp-docker`, as the user who owns it:

```bash
mkdir deciduous-mcp-1.2.0
tar -xzf deciduous-mcp-docker.tar.gz -C deciduous-mcp-1.2.0 --strip-components=1
cp -p deciduous-mcp-docker/.env deciduous-mcp-1.2.0/
cd deciduous-mcp-1.2.0 && ./scripts/setup.sh
```

Keep `COMPOSE_PROJECT_NAME` the same: it is how setup finds the running containers and the PostgreSQL volume. The new server runs pending migrations before it listens, and setup waits for `/ready`.

Rerunning setup on the same release was tested and took 8 seconds, because the image was already built. Settings, volume and graph all survived. A real version change was not run for this page; the procedure is the one in `deciduous_mcp/DEPLOY.md`.

## Upgrade the CLI

People upgrade their own CLI however they installed it (`brew upgrade deciduous`, `cargo install deciduous`, or a release binary). The CLI and server versions do not have to match: a 1.1.0 CLI works against the 1.0.12 reference server. After upgrading, run `deciduous update` in each repository. It refreshes the Claude Code, OpenCode and Windsurf files that `init` wrote, for whichever of `.claude/`, `.opencode/` and `.windsurf/` the repository has. It upgrades a server only when that server is a native one on the same machine; a Docker install on a VM is never touched.

## Rotate the token

For a Docker install made by hand, set a new token in `.env` and rerun setup. These commands are for the Linux VM; on macOS, `sed -i` needs `''` after it. Keep the single quotes around the value, as `setup.sh` writes it: tools that read the token back out of `.env`, including the one-liner in `deciduous_mcp/DEPLOY.md`, expect them.

```bash
NEW=$(openssl rand -hex 32)
sed -i "s/^DECIDUOUS_MCP_TOKEN=.*/DECIDUOUS_MCP_TOKEN='$NEW'/" .env
./scripts/setup.sh
```

For the other two kinds of install (not run for this page):

- A native server the CLI installed: edit `DECIDUOUS_MCP_TOKEN` in `~/.config/deciduous/server/.env`, then restart it with `launchctl kickstart -k gui/$(id -u)/dev.deciduous.server` on macOS or `systemctl --user restart deciduous-server.service` on Linux. The CLI on that machine reuses the token in `~/.config/deciduous/credentials`, so store the new one there too with `deciduous remote login --url http://127.0.0.1:<port>`, which checks the new token against the restarted server before storing it.
- A Docker install the CLI made: edit the same `.env`, then rerun its setup with `DECIDUOUS_ENV_FILE=~/.config/deciduous/server/.env ~/.config/deciduous/server/bundle-1.1.0/deciduous-mcp-docker/scripts/setup.sh`, and `deciduous remote login --url http://127.0.0.1:<port>` as above.

Once it restarts, the old token gets 401 and the new one works. Rotation logs out every client at once, but most commands do not say so. `remote status`, `remote pull`, `remote push` and `remote watch` fail and exit 1:

```
Error: reached https://deciduous.example.com but the server rejected the token (401). Check DECIDUOUS_MCP_TOKEN.
```

(`pull` and `push` print it without the `reached <url> but` part.) A write succeeds locally and warns, as below. Reads of the local cache, such as `deciduous nodes`, print nothing and exit 0.

Each person stores the new token with `deciduous remote login --url <base>`. They also need to update the `Authorization` header in their agent's MCP registration (see [Connect your agents](clients.md)), which `remote login` does not touch. A CLI write made with a rejected token is not lost. It is saved locally, queued, and the command still exits 0 after printing this warning:

```
Warning: the local write succeeded but the server refused it: the server rejected the token (401). Check DECIDUOUS_MCP_TOKEN.
1 write(s) wait in /home/you/my-app/.deciduous/remote-log.jsonl, and every later write will be refused the same way until that is fixed. `deciduous remote status` lists them.
```

Once the new token is stored, the next write sends the queued one along with it. `deciduous remote push` sends the queue on its own. Because the exit code is 0, a script cannot tell that the write never reached the server unless it also runs `deciduous remote status`, which exits 1 while anything is waiting.

## Remove a document that should never have been attached

`deciduous doc detach` does not take a document off the server. In 1.1.0 the detached document no longer appears in `get_graph`, but `show_node` still lists it and `GET /documents/<id>` still returns its bytes. To remove a leaked file, delete it from the database. Documents are addressed by the SHA-256 of their contents, so hash the same file:

```bash
HASH=$(sha256sum leaked.env | cut -d' ' -f1)
docker compose -f compose.yaml -f compose.local.yaml exec -T db \
  psql -X -U deciduous -d deciduous -v ON_ERROR_STOP=1 -v h="$HASH" <<'EOF'
BEGIN;
DELETE FROM node_documents WHERE content_hash = :'h';
DELETE FROM document_blobs WHERE content_hash = :'h';
COMMIT;
EOF
```

On the test install, `GET /documents/<hash>` went from 200 to 404, and the attacher's `remote status` still reported in sync. This deletes the bytes from every workspace that attached the same file, which is what you want for a secret. Two copies remain that the database cannot reach: the attacher's `.deciduous/documents/`, and every backup taken since the file was attached. Rotate the secret itself as well.

## Remove a node's text for good

Deleting a node does not remove its text either. `deciduous delete` and `delete_node` set `deleted_at` on the row: the title, description and prompt stay in `decision_nodes`, and the write is also recorded, title included, in `graph_events`, the log `remote watch` replays from. After `deciduous delete` on a node whose description held a pasted secret, both tables still had it. To purge it, take the node's server id (`id` from `show_node`, or `deciduous show` once pulled) and delete both:

```bash
docker compose -f compose.yaml -f compose.local.yaml exec -T db \
  psql -X -U deciduous -d deciduous -v ON_ERROR_STOP=1 -v id=<server id> <<'EOF'
BEGIN;
DELETE FROM graph_events WHERE payload->>'id' = :'id';
DELETE FROM decision_nodes WHERE id = :'id';
COMMIT;
EOF
```

Its edges go with it. On the test install that printed `DELETE 2` and `DELETE 1`, a `pg_dump --data-only` no longer contained the text, and the deleting clone's `remote status` still said `In sync`. Every clone that pulled before the delete still has the node's text in its `.deciduous/deciduous.db`, and backups keep it too.

## Watch what is happening

```bash
docker compose -f compose.yaml -f compose.local.yaml ps
docker compose -f compose.yaml -f compose.local.yaml logs --tail=100 server
```

For an external database, leave out `-f compose.local.yaml`. `/health` returns 200 whenever the process is running. `/ready` also checks the database and migrations, and returns 503 until both are fine; point uptime checks at `/ready`.

From any client, `deciduous remote watch` prints one line for each node write to that project's workspace as it lands, from every agent and every CLI. Edge writes are shown only with `--edges`:

```
Watching: my-app
13:45:15  main      action      "Written while watching"
```

## When a client and the server disagree

`deciduous remote status` compares one repository's local cache with its workspace on the server. It exits 1 when there is any difference. It also exits 0, printing `Local only`, in a project with no `[remote]`, so a script should check for the `In sync` line as well:

```
                 local    server
  nodes              3         3
  edges              0         0

In sync: no writes waiting, and every node, edge and document matches field by field.
Themes and tags are not sent to the server, so they are not compared.
```

`deciduous remote pull` sends anything waiting, then refreshes the cache from the server. That is the fix for "only on the server". `deciduous remote push` resends queued writes after an outage. Its repair flags (`--seed`, `--repair`, `--overwrite-server`, `--drop-rejected`, `--retry-rejected`, `--drop <op>`, `--overwrite`) are for a cache that has drifted in ways no queued write explains. Read `deciduous remote push --help` before using them: `--overwrite` replaces the server's copy of every row with yours. Common cases are listed in [Troubleshooting](troubleshooting.md).

## Stop or remove

`docker compose ... down` stops the install and keeps the database volume. `down --volumes` deletes the database. Do not run it unless that is what you want and you have a dump you have restored.
