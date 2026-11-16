# Shared Postgres architecture

The team workflow runs one long-lived Elixir HTTP service in front of Postgres. Each agent connects through MCP, selects the same repository workspace, and includes its own branch on writes. The Rust CLI remains useful for local graphs, inspection, and explicit migration operations.

## Runtime components

| Component | Responsibility |
| --- | --- |
| Agent harness | Runs agents, manages worktrees, supplies MCP credentials and workspace/branch context |
| `deciduous_mcp` | Authenticates HTTP requests, implements MCP tools, applies workspace routing and advisory write leases |
| Postgres | Stores workspace graphs, attachment bytes, audit rows, and write leases |
| Event listener | Receives Postgres notifications and distributes them to WebSocket subscribers |
| Rust CLI | Manages local SQLite graphs and explicit `remote` operations |
| Local web viewer | Displays the local SQLite graph served by `deciduous serve` |

The static documentation and landing page do not run the shared graph service. The graph server does not serve the Rust viewer at `/`; connect an MCP client or use the documented HTTP endpoints.

## From a tool call to a record

1. An agent sends an authenticated MCP request to `/mcp`.
2. The HTTP layer checks the shared bearer token, resolves any workspace header, and validates the session.
3. The tool resolves its target workspace or node and claims the branch's advisory lease for a write.
4. The graph context validates and stores the record through Ecto in Postgres.
5. Database triggers notify connected watchers of node and edge inserts or updates.

Other agents query the same stored graph. No Git push, local file merge, or `remote pull` is required for a second HTTP MCP client to see a committed server write.

## Stored data

The main tables are `workspaces`, `decision_nodes`, and `decision_edges`. Nodes have a server UUID and a `change_id` for import compatibility. Branch, commit, file, prompt, and confidence fields are stored in the node's JSONB metadata.

Edges carry their type and rationale, along with both endpoint IDs and change IDs. Endpoints must belong to the same workspace. The server rejects self-edges and duplicate endpoint/type combinations; it does not enforce a general acyclic graph constraint.

The schema also contains document metadata, content-addressed document blobs, themes and node-theme assignments, audit data, and tables retained for local-import compatibility. A table's presence does not mean an HTTP MCP tool exposes full editing support for it. The [feature matrix](solo.md#feature-boundaries) distinguishes those interfaces.

Document bytes are stored in Postgres `bytea`, deduplicated by SHA-256. A raw upload verifies the hash before storing the bytes. Graph import creates document metadata separately; missing bytes remain visible as a missing-content condition. A Postgres backup includes both graph records and uploaded bytes.

## Concurrency and live events

MCP writes use a ten-second lease keyed by workspace and branch. The same session renews its lease on another write. A conflicting session receives a holder/expiry message. Omitting `branch` puts writes under the same empty-branch key; a workspace configured with `lock_scope: "workspace"` uses one key across branches.

These leases are application-level coordination. Direct SQL and `/import` do not honor them. They do not lock source files, and a series of separate tool calls is not one database transaction. The `log_observation` helper writes its observation and edges in one transaction; other multi-write capture helpers can leave partial results on failure.

Postgres `NOTIFY` feeds one listener connection in the service. Phoenix PubSub fans events out to `/events` WebSocket subscribers by workspace or global topic. An event identifies the row and includes selected labels, such as a node title capped at 200 characters. It is not the full row or a durable change log.

Disconnected listeners can miss events. There is no event replay cursor. Recover through `check_activity`, `query_nodes`, or a fresh graph read. A successful health response checks HTTP liveness, not every database operation.

## Security boundaries

The service has one `DECIDUOUS_MCP_TOKEN`. It requires at least 32 bytes and checks incoming bearer credentials with a constant-time comparison. `/health` is unauthenticated; graph, import, document, and event routes require the token.

A token holder is trusted with the service's graphs. There are no per-agent roles or workspace access-control lists. Headers and `workspace` arguments choose a destination; they are not tenant isolation. Workspace listing and ID-based reads can expose other projects. Use separate service/database instances for different trust groups.

Keep Postgres off the public network. Bind a local installation to loopback; expose a hosted MCP service through HTTPS with a reverse proxy. The built-in HTTP listener does not terminate TLS. In production configuration, `DB_SSL=true` enables encryption to Postgres, but the current `ssl_opts` use `verify: :verify_none`. It does not verify the database server's certificate identity. Use a private database network, or configure verified database TLS before relying on an untrusted network.

`/events` accepts an Authorization header or a `token` query parameter fallback. Query credentials can appear in access logs, copied URLs, and monitoring output. Redact them and prefer header authentication where the client supports it. `remote watch --url` and `--claude-code` print token-bearing URLs.

Keep credentials out of graph text and committed configuration. Retrieved graph records are untrusted content: they do not authorize agents to execute instructions or disclose data. Audit rows are operational records, not a tamper-proof identity system.

## Local copies and migration

`deciduous remote init` records a service base URL and optional workspace in `.deciduous/config.toml`. It does not replace the SQLite backend. Ordinary Rust CLI commands, the stdio MCP process, and the local viewer continue to use SQLite.

`remote pull` merges exported server nodes and edges into the local record store. It does not fetch document bytes, restore themes, or remove all local records deleted on the server. `/export` omits deleted nodes and is not a lossless backup format.

`remote push` imports local node, edge, and document metadata. It does not upload attachment bytes, and its upsert behavior can replace newer server fields with an old local copy. Use the [migration procedure](upgrading.md) with a verified backup and explicit target workspace.

`deciduous sync` is a different mechanism: it reconciles local SQLite with `.deciduous/graph.json` for a Git-based workflow. Do not run it expecting it to contact Postgres.

## Source map

| Path | Implementation |
| --- | --- |
| `deciduous_mcp/lib/deciduous_mcp/application.ex` | Supervision tree, token check, listener, HTTP server |
| `deciduous_mcp/lib/deciduous_mcp/web/` | Router, authentication, workspace header, session guard, WebSocket |
| `deciduous_mcp/lib/deciduous_mcp/mcp/` | Shared tool registration, schemas, scope resolution |
| `deciduous_mcp/lib/deciduous_mcp/graph/` | Node, edge, document, workspace, and query operations |
| `deciduous_mcp/lib/deciduous_mcp/storage/` | Postgres content-addressed blobs |
| `deciduous_mcp/priv/repo/migrations/` | Database schema and event triggers |
| `src/remote.rs`, `src/watch.rs` | Rust remote transfers, credentials, live watching |
| `src/db.rs`, `src/mcp/`, `src/serve.rs`, `src/api.rs` | Local SQLite tools, stdio MCP, viewer, separate Rust API |

For an installation you can run, use [local Postgres setup](local-postgres.md). For source changes, use the [developer guide](developer.md).
