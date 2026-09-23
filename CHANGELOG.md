# Changelog

## [1.0.2] - 2026-09-23

The shared graph server can be installed without Erlang, Elixir or a hand-built database, and no MCP call can hang long enough for Claude Code to background it. Both packages move to 1.0.2 together; the release workflow refuses to publish a tag that Cargo.toml and deciduous_mcp/mix.exs do not both declare.

### Added
- **The server ships as a single executable.** Burrito wraps the release with ERTS and Elixir for macOS (arm64, x86_64), Linux (arm64, x86_64) and Windows (x86_64). A native install bootstraps an empty PostgreSQL from the release's `STRUCTURE.sql`.
- **One-command Docker setup.** `deciduous_mcp/scripts/setup.sh` generates private credentials, starts PostgreSQL 17 on a persistent volume, applies migrations and waits for readiness. It can use an existing database instead, and a repeat run keeps the saved token and password byte for byte.
- **`GET /ready`** answers 200 only when the database responds and every required migration is applied, and 503 until then. `/health` stays unauthenticated liveness.
- **Trigram indexes** (`pg_trgm` GIN) on node title, description and `metadata::text`. `query_nodes(search:)` and `ask_graph` were `ILIKE '%term%'` over every row: 84.5 ms on production across 30,249 nodes, now 3.0 ms, and 1.6 ms within one workspace.

### Fixed
- **A tool call that ran long was left to the client's timeout.** The transport waited up to four minutes, longer than Cloudflare's 100-second origin cut and Claude Code's 120-second background move. Hermes.Server.Base now stops any handler still running at 60 seconds, which rolls back its open transaction, and answers the request under its own id: `list_workspaces did not finish within 60s and was stopped; nothing it had not committed was kept`. The transport's own timeout is 90 seconds, as a backstop. The heaviest call measured on production is 7.2 seconds.
- **The server never logged its own failures.** `Hermes.Logging.should_log?/1` compared log levels backwards, so at `:info` every warning and error Hermes emitted (`request_handler_crashed`, `server_call_failed`) was dropped before it reached Logger.
- **`ask_graph` searched every workspace on the server.** Its search terms were added with `or_where`, which Ecto renders as `(workspace AND not deleted AND scope) OR term...`, so a question in one workspace returned nodes from others, deleted ones included.
- **`query_nodes` with `limit` below 1** reached Postgres as `LIMIT -5` and came back as a raw Postgrex error. It is now `limit must be at least 1, got -5`.
- **The events listener recovers after a database outage**, and `DB_SSL=true` now verifies the certificate and hostname by default.

### Not in this release
- Sessions still live in server memory, so a restart logs every client out. Claude Code reconnects and retries automatically, in about 0.3 s on production.
- The CLI still keeps a local SQLite cache. Removing it is next.

## [1.0.0] - 2026-09-22

The shared server is the product now. 0.19.0 put every project's graph in one Postgres; 1.0.0 is what happens when several agents write to it at once. The proof is the tetris arena: ten Claude Code sessions in ten git worktrees, one workspace, told to read each other's code and reasoning. Twenty-six minutes later there were ten playable games, 386 nodes, and 170 borrowed ideas with provenance. Write-up: https://notactuallytreyanastasio.github.io/tetris-arena/

### Added
- **`took_from` edge type.** A borrow is an edge, not a sentence: `from` is the node you took from, `to` is your node that used it, and it crosses branches on purpose. `log_observation` takes `took_from` (a node UUID or a full change_id) and `why` (the edge's rationale) and writes the observation and the edge in one call; `related_to` accepts a change_id too. `add_edge` and `deciduous link -t took_from` take it directly; the web viewer source draws it dash-dot, and the DOT export colours it violet. The viewer embedded in the binary is a separate build that has not been refreshed since 0.14.0; it draws the edge as a plain line until it is. In the arena zero of 471 edges crossed a branch, and the borrow counts in the write-up came from a regular expression over observation titles.
- **`check_activity` returns `branches`:** the last node on each of the twenty most recently written branches (type, title, change_id, created_at) with who holds its lock; `branches: N` widens it and `branches_total` counts them all. A workspace that has had sixty branches since 2025 should not hand all sixty to an agent asking what is happening now. The arena's agents polled `query_nodes` for decisions because the lock list was all the tool returned.
- **`deciduous remote watch` connects.** One line per write, quoting the branch, the operation, the node type and the title: `04:47:15  agent-7  observation  "Took the step-reset for lock delay from agent-8's decision, …"`. Edges, with `--edges`, print as `agent-3  edge chosen  a1b2c3d4 -> e5f6a7b8`. It reconnects with backoff when the socket closes. `--types`, `--branch`, `--json`; `--url` prints the socket URL for another client and `--claude-code` adds a `Monitor(...)` call to paste. It quotes rather than counts because a watcher in the arena kept a tally, counted updates as inserts, and reported a "second goal" convention that did not exist.
- **Event frames carry the node's `title` and `status`**, and edge frames carry the branch of their source node. Edges have no branch of their own, so before this a watcher could not tell whose edge it was looking at.

### Fixed
- **The events socket closed every subscriber after sixty seconds** with code 1002, whether or not any frame had been sent. `GraphSocket` was upgraded with no options, so Bandit's default idle timeout applied. The server now sends a ping every thirty seconds; the client's pong resets the timer, and a client that stops answering is still reaped.
- **`update_node`, `delete_node` and `delete_edge` never claimed the branch lock.** The 0.19 commit that said it closed this gap had added the `branch` argument to their schemas, but the tools called the graph directly and never went through `Scope.write_workspace_id/2`. They now resolve the node's workspace and claim the lock before touching anything.
- **Every MCP call from a long-lived Claude Code session hung for 300 seconds.** Hermes expires a session after thirty idle minutes, and a restart drops every session at once. The next call with the old `mcp-session-id` got a `200` carrying `"Server not initialized"` under a freshly generated id, not the request's id, so the client could never match it and waited until its own timeout. The client log says exactly this: `Received a response for an unknown message ID: {"id":"err_GNei6oQr…"}`. The server now answers a dead session with `404` and JSON-RPC error `-32001 Session not found` under the request's id, the shape the MCP spec and the reference SDK use, so the client fails in milliseconds and re-initializes. The idle timeout is 24 hours.
- **Every connection from Claude Code paid five seconds before its first call.** The client opens with a version-negotiation probe, a request named `server/discover` that Hermes does not know; Hermes stripped the method and answered `202` as if it were a notification, so the client waited five seconds for a reply to that id before falling back to the legacy handshake. Captured through a logging relay: probe at 19.68s, handshake at 24.69s. The server now answers any request whose method it does not implement with `-32601 Method not found` under the request's id, and the same client connects in 35 milliseconds.

### Changed
- The site leads with the multi-agent story alongside the two things it already did: memory that survives a session (`/recover`, the graph) and discovery over what was already decided (`ask_graph`, `query_nodes` across every workspace, `/decision-graph`). `docs/remote.html#alongside` documents locks, `check_activity`, events, `remote watch` and `took_from` together.
- The MCP server behind the shared graph reports version 1.0.0.

### Not in this release
- Nothing writes back from the server to SQLite except `remote pull`. The local database is a cache.
- The event stream is advisory: a `NOTIFY` that fires while the listener is down is lost, and a reconnect races new `LISTEN`s against notifications issued at the same moment. A gap is not proof of silence; `check_activity` and `query_nodes` are how a subscriber catches up.
- Locks are advisory too. A client that ignores the refusal and writes anyway still can.

## [0.19.0] - 2026-09-21

This entry was written for 1.0.0; 0.19.0 shipped with release notes in `src/changelog.rs` and none here.

### Added
- **`deciduous remote`** points a repository at a shared graph server. One Postgres holds every project's graph, one workspace per repository, and the local database becomes a cache of it. `remote init <url>` verifies the server and the token before writing anything; `remote status` reports which side has drifted; `remote pull` refreshes the cache; `remote push` seeds a workspace or carries history that predates the server.
- **`remote login`** stores the token at `~/.config/deciduous/credentials`, mode 0600, outside every repository. It is never written to `config.toml`; committing that file leaks a hostname and nothing else.
- **`remote adopt <dir>`** configures many projects at once and never repoints one already aimed somewhere else.
- **An MCP endpoint over HTTP** (`deciduous_mcp/`, Elixir and Postgres) serves the graph to Claude from any directory. Read tools accept `workspace: "*"` for the cross-project view; writes refuse it, because a node has to land somewhere. A repository can pin its workspace with an `X-Deciduous-Workspace` header and then no tool call can write its nodes anywhere else.
- **Documents live in Postgres**, keyed by sha256, so a file attached in several projects is stored once.
- **Advisory write locks** per `(workspace, branch)`, ten second lease renewed by every write from the same session, and `check_activity` to report the holders. Two agents on different branches never contend; a workspace can set `lock_scope: "workspace"` to make them.
- **Live events.** Postgres triggers fire `NOTIFY` on every node insert or update and edge insert; `GET /events` streams one JSON frame per write over a WebSocket, scoped to a workspace or `"*"`. Claude Code's `Monitor` tool can sit on it.
- **`deciduous remote watch`** printed the events URL with the token and workspace resolved. (In 1.0.0 it connects.)

### Fixed
- Edges resolve by their integer endpoint before the denormalized change_id. The copies go stale: 9,185 of 51,158 edges in one graph carried a change_id belonging to no node, and preferring the portable-looking identifier silently dropped 11% of an archive on import.
- `GET /mcp` returns 405. Cloudflare buffers SSE, so the server-to-client stream Claude Code waits on never opened, and every tool call hung for 300 seconds against a server answering the same POST in 88ms.

### Security
- Never put a literal token in `.mcp.json`. Use `${DECIDUOUS_MCP_TOKEN}`, which Claude Code expands from the environment and names when it is missing.

## [0.18.0] - 2026-09-18

### Changed
- **The shared graph is one file again: `.deciduous/graph.json`.** It holds every node, edge, theme, and tag, keyed by `change_id`. 0.17's `.deciduous/sync/` directory of per-record files is folded into it once by the next `sync`, `init`, or `update`, and then deleted — `git rm -r .deciduous/sync` afterwards. A record the file already holds in a *newer* version is not overwritten by the older file, so folding in a directory that arrived with a pull does not lose the pulled work.
- **Why the per-record store was undone.** The design was sound — two people adding records touch different files, so git merges them with no conflict — and the bill was 2,781 files and 11MB of directory overhead for a graph whose actual content is ~1MB. `git status` after a sync was a wall of paths, a PR touching the graph was hundreds of files, and a rename could put a record in a file named after a different one.
- **The merge driver is now the mechanism, not a nicety.** One file means every concurrent change conflicts in git, so `deciduous merge-record` runs on every merge. It merges the two documents record by record: a record only one side has survives (both people's new nodes are kept), and a record both sides changed still merges field by field with the ancestor as the tiebreak. `deciduous sync` repairs a file left with conflict markers by a clone that has no driver registered.
- `.gitignore` now tracks `.deciduous/graph.json` and `.gitattributes` routes it through `merge=deciduous`. `deciduous update` replaces the 0.17 rules rather than stacking on them.

### Fixed
- A `graph.json` that will not parse stops a sync instead of reading as an empty graph — which would have exported every local row over whatever was actually in the file.
- A sync that exports thousands of records writes the file once, not once per record.

### Added
- `RecordStore::batch` — apply many writes with a single write-through at the end.

## [0.17.1] - 2026-09-04

### Fixed
- `deciduous nodes` panicked on an observation whose description contains multibyte characters (an ellipsis, for instance).

## [0.17.0] - 2026-09-04

### Changed
- **One multi-user sync mechanism.** `.deciduous/sync/` held one JSON record per node, edge, theme, and tag, tracked in git. The event log, checkpoint, and patch export were removed. (Superseded by the single graph file in 0.18.0.)
- Records were written by the database layer, so CLI, MCP, and HTTP API all published automatically.
- `deciduous sync` reconciled both ways then exported `docs/graph-data.json`; `--check` exited 1 if anything was pending.
- Node references accept a `change_id` prefix everywhere (CLI and MCP).
- Concurrent edits of one record merge field by field through the `deciduous merge-record` git merge driver, registered by `init`/`update`/`sync`.
- Legacy JSONL event logs are imported once (tolerating corrupted lines) and removed; `deciduous events` became a deprecated alias.

## [0.16.0] - 2026-07-17

### Added
- **Multi-graph HTTP API daemon (`deciduous serve --api`)** — serves many independent decision graphs to remote clients over HTTP with bearer-token auth, one SQLite file per graph. This is what lets a graph live centrally and be written to by clients that have no local `.deciduous/` (e.g. a federated fleet all pointing at one daemon).
- **`GET /health`** — unauthenticated, side-effect-free liveness endpoint for reverse proxies and uptime probes.
- **`DECIDUOUS_API_DATA_DIR`** env var as an alternative to `--data-dir` (joining the existing `DECIDUOUS_API_TOKEN`), for clean container/systemd configuration.
- **Deployment artifacts** (`Dockerfile`, `deploy/`) — a container + Caddy auto-TLS reverse proxy, WAL-safe backup script, and a runbook for running the daemon behind a public subdomain.

### Security
- **The HTTP API is append-and-read only.** `delete_node`, `unlink_nodes`, `update_status`, and `update_prompt` are refused at the daemon (403), so a shared graph reachable by a bearer token cannot have its history erased or rewritten by any token holder.
- **`graph_id` is validated before any filesystem access**, closing a latent path-handling surface.

### Fixed
- **Cache/disk wedge** — a graph whose data directory was deleted under a running daemon left `PUT` answering 201 while every subsequent write 404'd forever. The filesystem is authoritative now: a vanished graph is re-created instead of wedging.


## [0.3.5] - 2025-12-10

### Fixed
- **Critical: Database path resolution now walks up directory tree** - Previously, `deciduous` used relative paths based on current working directory. Running commands from subdirectories or different directories would use/create a different database, making it appear like data was lost. Now `deciduous` walks up the directory tree to find `.deciduous/` folder, similar to how `git` finds `.git/`. This means:
  - Running `deciduous nodes` from `project/src/` correctly uses `project/.deciduous/deciduous.db`
  - Running commands from any subdirectory of an initialized project works correctly
  - No more "phantom" databases created in wrong directories

### Technical Details
- Modified `get_db_path()` in `src/db.rs` to traverse parent directories
- `DECIDUOUS_DB_PATH` env var still takes priority if set
- If no `.deciduous/` found anywhere up the tree, defaults to current directory (for `deciduous init`)

## [0.3.4] - 2025-12-10

### Added
- `deciduous sync` exports to `docs/graph-data.json` for GitHub Pages integration

## [0.3.3] - 2025-12-09

### Added
- `deciduous dot` command for DOT/PNG graph export
- `deciduous writeup` command for PR writeup generation
- `--auto` flag for branch-specific filenames

## [0.3.2] - 2025-12-09

### Added
- Initial public release
- Core decision graph functionality
- Web viewer with multiple visualization modes
- GitHub Pages deployment support
