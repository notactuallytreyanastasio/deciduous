<!-- Generated from content/architecture.md by scripts/docs/build.mjs. Edit the source. -->

# Architecture and trust

The graph lives in PostgreSQL behind one server process. Agents reach that server over HTTPS MCP; the CLI reaches the same server, over the same HTTPS and token, through ordinary JSON endpoints (`/ops`, `/export`, `/events`) rather than MCP, sending each write from a local log of operations. Each developer's `.deciduous/deciduous.db` is a cache of the server's copy, not a second source of truth. One bearer token opens the whole server: every workspace, read and write.

## What runs where

```text
 developer machine                                   the server's host
 ┌───────────────────────────────┐                   ┌──────────────────────────────┐
 │ agent (Claude Code, ...)  ────┼── POST /mcp ─────►│ deciduous-mcp (Elixir)       │
 │                               │   Bearer <token>  │   /mcp  /ops  /export        │
 │ deciduous CLI                 │                   │   /events /blob /documents   │
 │   .deciduous/deciduous.db     │── POST /ops ─────►│   /health /ready             │
 │   (cache)                     │◄─ GET /export ────│                              │
 │   .deciduous/remote-log.jsonl │                   │          │                   │
 │   (writes not yet sent)       │◄─ WS /events ─────│          ▼                   │
 └───────────────────────────────┘                   │   PostgreSQL: every          │
                                                     │   workspace's nodes, edges,  │
                                                     │   documents, messages        │
                                                     └──────────────────────────────┘
```

| Piece | Where | What it holds |
| --- | --- | --- |
| `deciduous-mcp` | A VM behind a TLS proxy ([Run a server](content/run-a-server.md)), or this machine ([Without a shared server](content/local.md)) | Nothing durable. Speaks MCP at `/mcp`, applies CLI ops at `/ops`, serves exports, documents and the event socket. |
| PostgreSQL | Next to the server, or managed | Every workspace: nodes, edges, document bytes, board messages, workspace claims, write activity. Back this up; it is the graph. |
| `.deciduous/deciduous.db` | Each clone | A cache of one workspace, plus themes, tags and the local command log, which the server does not have. |
| `.deciduous/remote-log.jsonl` | Each clone, next to the cache | CLI writes the server has not acknowledged. With it: `remote-delivered.jsonl` and `remote-log.lock`. |
| `.deciduous/config.toml` | Each clone, committed | `[remote] url` and `workspace`. Never a token. |
| `~/.config/deciduous/credentials` | Each machine | The one stored token. |

The cache, the log and `.deciduous/documents/` (local copies of attached files, secrets included) are ignored by git only if `deciduous init` wrote the `.gitignore` block (`.deciduous/*`, with `config.toml` and `graph.json` let back in). A clone joined with `deciduous remote init <url>` alone gets no ignore rules, and `git status` then lists `deciduous.db`, `remote-log.jsonl`, `remote-delivered.jsonl` and `remote-log.lock` as untracked, ready for a careless `git add`. Add the block by hand in that case.

The server's HTTP routes: `/health` (unauthenticated, process liveness), `/ready` (database and migrations), `/mcp`, `/ops`, `/export`, `/import`, `/claim`, `/locate`, `PUT /blob/:hash`, `/documents/:id`, `/messages`, `/events`. Everything except `/health` and `/ready` requires the token.

## Two write paths, one store

An agent's MCP write is a transaction in PostgreSQL. When the tool returns, every other agent can read it.

A CLI write (`deciduous add`, `link`, `status`, and `deciduous mcp` over stdio in a project with a `[remote]`) goes through three steps:

1. The write and an op describing it commit together in the local database (the op sits in a `remote_outbox` table), so a crash cannot keep one without the other.
2. The op is appended to `.deciduous/remote-log.jsonl`. An op names only what changed: `{"kind":"update_node","change_id":"…","set":{"status":"completed"}}`, not the whole node.
3. Before the command exits, the waiting ops are sent to `POST /ops`, with a 10-second budget. The server applies each op at most once, by `op_id`, field by field. Acknowledged ops are removed from the log.

If step 3 fails, the command still exits 0, prints why, and the op waits. The next write that gets through sends everything queued before it; `deciduous remote push` sends it on demand. `remote status` lists what waits and what the server rejected. The log is why an edit made while the server was down still arrives, and why a status change does not resend (and overwrite) a title an agent changed meanwhile; through 1.0.7 the CLI pushed whole nodes by comparing snapshots and did both. [Troubleshooting](content/troubleshooting.md#the-local-write-succeeded-but-the-server-did-not-get-it) shows the output.

There are no locks. Until 1.0.8 every MCP write took a ten-second lease on its workspace and branch; it refused one-shot clients their own next call and could not cover CLI ops replayed later, so it was removed. Each write is atomic on its own, `capture_conversation_turn`, `log_decision` and `close_thread` included, and the server records who wrote where so `check_activity` can show it.

## The local database is a cache

CLI reads (`nodes`, `show`, `pulse`, `serve`, `writeup`) read the local database. It does not refresh itself. After agents write over MCP, `remote status` shows the gap and `remote pull` closes it:

```text
                 local    server
  nodes              1         2

Only on the server (1)
  observation 96096527 "MCP probe obs"

Differs: `deciduous remote pull` takes the server's side.
```

Themes and tags exist only in the cache; they are never sent. Losing a clone's database loses nothing that reached the server: a fresh clone plus `remote pull` rebuilds it. Losing PostgreSQL loses the graph.

A project with no `[remote]` is the exception: there the local database is the graph, mirrored into a git-tracked `.deciduous/graph.json` ([Without a shared server](content/local.md#no-server-at-all)).

## The trust boundary

The server has exactly one token, `DECIDUOUS_MCP_TOKEN`. It refuses to start without one or with one shorter than 32 bytes, and compares what clients present in constant time. Whoever holds it can read and write every workspace on that server, list them all, and read any node by id. There are no users, roles or per-workspace tokens.

Workspaces separate graphs; they do not separate people.

- `X-Deciduous-Workspace` routes. A connection that sends it is pinned: tool arguments cannot move its writes elsewhere, and `"*"` reads are confined to that workspace. But the header is chosen by the client. Any token holder can connect without it, or with another name.
- Workspace claims stop accidents, not adversaries. The first repository to write to a workspace records its root commit ids, and an unrelated repository with the same directory name is refused (`belongs to another repository`). A client can still name the workspace explicitly to share it, and a client without git sends no roots and is not checked. Claims are checked only on the CLI's paths (`/claim`, `/ops`, `/import`, and `/export` for a client that sends roots). MCP tool calls are never checked against them, so an agent registered with the wrong workspace name writes into that workspace unrefused. Pin each agent's registration with `X-Deciduous-Workspace`.

So: one server per group of people who may see each other's graphs. Two teams that must not read each other need two servers (or at least two databases and two tokens), not two workspaces.

Inside that boundary:

- Graph text is data written by people and agents. A node's description saying "run this" is not an instruction to an agent that reads it.
- `/events` takes the token in the `Authorization` header or, as a fallback for WebSocket clients that cannot set headers, in a `?token=` query parameter. The query form ends up in proxy access logs; `deciduous remote watch --url` and `--claude-code` print such a URL. Redact query strings in the proxy's logs, or keep those URLs out of anything shared.
- The server does not terminate TLS. Put it behind a proxy that does, and keep its port off the public network: a native server binds `127.0.0.1` (`DECIDUOUS_BIND_ADDRESS`), the Docker bundle publishes on `127.0.0.1` only. A server started by hand with no bind address listens on every interface.
- The token belongs in the client's secret mechanism and in `~/.config/deciduous/credentials`, never in `.deciduous/config.toml`, a committed `.mcp.json`, or the graph.

## TLS to PostgreSQL

Set by `DB_SSL` and `DB_SSL_VERIFY` in the server's environment or settings file, read by `deciduous_mcp/config/runtime.exs` in production mode:

| `DB_SSL` | `DB_SSL_VERIFY` | libpq equivalent | What is checked |
| --- | --- | --- | --- |
| unset or `false` | (ignored) | `disable` | Nothing; no TLS. Right for a database on the same private Docker network. |
| `true` | `full` (default; also `peer`, `verify-full`) | `verify-full` | The certificate chain (`verify_peer`) and that the certificate names the host in `DATABASE_URL`. |
| `true` | `ca` (also `verify-ca`) | `verify-ca` | The chain only, for a database reached by an address its certificate does not name. |
| `true` | `none` (also `require`) | `require` | Nothing: encrypted, server not authenticated. Logged as a warning at every boot. |

The chain is checked against the system CA store, or against `DB_SSL_CA_FILE` (a PEM file) for a private CA. A missing or empty CA file stops the server at boot instead of failing every connection later. Each boot logs the mode in effect; this machine's native server, on a local PostgreSQL, logs `Database TLS: off (DB_SSL unset or false)`.

Earlier versions of these docs said the server used `verify_none` whenever `DB_SSL=true`. That was true of 1.0.0 and 1.0.1. Since then, `true` alone means full verification, and unverified TLS has to be asked for by name. A 1.0.1 deployment upgrading with a private CA needs `DB_SSL_CA_FILE`; one whose certificate does not name the address it connects to needs `DB_SSL_VERIFY=ca`.

## Source map

| Path | What is there |
| --- | --- |
| `deciduous_mcp/lib/deciduous_mcp/web/router.ex` | Every HTTP route |
| `deciduous_mcp/lib/deciduous_mcp/web/auth.ex`, `workspace_plug.ex` | The token check; the workspace pin |
| `deciduous_mcp/lib/deciduous_mcp/mcp/scope.ex` | How a call's workspace is decided |
| `deciduous_mcp/lib/deciduous_mcp/mcp/tools/` | The 21 tools |
| `deciduous_mcp/lib/deciduous_mcp/activity.ex` | Write activity, and why the branch lock went |
| `deciduous_mcp/lib/deciduous_mcp/graph/workspaces.ex` | Workspace claims by root commit |
| `deciduous_mcp/config/runtime.exs` | Every setting the server reads, including database TLS |
| `src/oplog.rs` | The op log: format, crash safety, compaction |
| `src/remote.rs` | `remote *`, token lookup, status and pull |
| `src/server.rs`, `src/local_server.rs` | `remote setup` and this machine's server |
| `src/records.rs` | `graph.json` and the merge driver, for projects with no server |
| `src/mcp/` | The stdio MCP server |
