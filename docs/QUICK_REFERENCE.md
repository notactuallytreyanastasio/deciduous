<!-- Generated from content/reference.md by scripts/docs/build.mjs. Edit the source. -->

# Shared graph reference

Use the HTTP MCP server for an agent team's day-to-day graph reads and writes. The server stores the shared graph in Postgres. The Rust CLI manages local tools and explicit transfers between that server and a local SQLite database.

This reference describes Deciduous 1.0. Start with [local Postgres setup](content/local-postgres.md), then [connect your agents](content/clients.md). The [MCP guide](content/mcp.md) explains the transport and tool differences.

## Which command reaches which database

| Interface | Storage it reads or writes | Intended use |
| --- | --- | --- |
| HTTP MCP at `/mcp` | Shared Postgres | Team reasoning, cross-branch reads, borrowing, handoffs |
| `deciduous remote status`, `push`, `pull`, `watch` | The configured HTTP server; some commands also use local SQLite | Check connectivity, migrate history, refresh a local graph, watch writes |
| `deciduous add`, `link`, `nodes`, `show`, `status`, `graph` | Local SQLite | Offline work or inspection of a pulled copy |
| `deciduous mcp` | Local SQLite over stdio | Optional local-only MCP integration |
| `deciduous serve` | Local SQLite | Local graph viewer |
| `deciduous serve --api` | Per-graph SQLite stores | Separate Rust HTTP API; not the Postgres service |
| `deciduous sync` | Local SQLite and `.deciduous/graph.json` | Optional Git-based sharing, not remote Postgres synchronization |

Configuring `[remote]` does not reroute ordinary CLI commands or `deciduous mcp`. A successful local `deciduous add` is not evidence that your teammates can see the node.

## Setup commands in newer source builds

The onboarding change in the current source checkout adds `deciduous setup`. It is not included in the published `v1.0.0` binary. Check whether your build supports it with `deciduous setup --help`.

```sh
deciduous setup
```

With no flags, this prints the shared Postgres setup and upgrade guide to stdout. It does not open a local graph, write configuration, generate credentials, or start a service.

To generate a standalone database project instead:

```sh
deciduous setup --postgres --output ./deciduous-postgres --port 55432
```

This creates `Dockerfile`, `compose.yaml`, `README.md`, `.gitignore`, and a private `.env` in a new directory. It refuses existing output paths and a symlink parent. The parent must already exist. Omit `--output` and `--port` to use `./deciduous-postgres` and port `55432`.

Generation does not start Docker or alter a database. The generated Compose project runs Postgres only, with loopback access and a persistent named volume. It does not include the MCP service; agents still need that separate service.

For released `v1.0.0` installations, use the current Python helper from the [Postgres-only setup guide](content/local-postgres.md#generate-just-postgres). Both generators use the same templates. Follow the generated README before starting containers or connecting an app.

## Workspace and branch arguments

Use one stable workspace name for the repository, such as `example-app`, across machines and worktrees. Pass the actual Git branch on writes, such as `agent-api` or `agent-tests`.

Scoped HTTP tools resolve the workspace in this order:

1. The `X-Deciduous-Workspace` request header, if present.
2. The tool's `workspace` argument.
3. `scratch`, if neither is present.

Names are trimmed and lowercased. Path separators and names longer than 128 characters are rejected. Unknown names are created, so a typo can create a second workspace.

`query_nodes`, `get_graph`, `find_orphans`, and `ask_graph` accept `workspace: "*"` for cross-project reads when the client is not pinned. `check_activity` requires one workspace. Scoped writes reject `"*"`.

The header is a routing safeguard, not an access-control boundary. The shared bearer token grants access to the service's data. `list_workspaces` and ID-based reads are not confined by a workspace header. See [security boundaries](content/architecture.md#security-boundaries).

## Shared HTTP MCP tools

The server registers 18 tools. Discover their current schemas with your MCP client's `tools/list`; the local stdio server exposes a different set.

### Read and recover

| Tool | Arguments | Result |
| --- | --- | --- |
| `list_workspaces` | None | Workspace names and live node/edge counts |
| `check_activity` | `workspace`; optional `branches` from 0 to 200, default 20 | Active write leases and the newest created node on each returned branch |
| `query_nodes` | `workspace`; optional `type`, `status`, `branch`, `search`, `limit` | Matching live node summaries, newest first; default limit 100 |
| `show_node` | `node_id` | Full node metadata, descriptions, incoming/outgoing edges, documents, themes |
| `get_graph` | `workspace`; optional `branch` | Graph snapshot with nodes, edges, document metadata, themes, and node-theme links |
| `find_orphans` | `workspace` | Non-goal nodes with no incoming edge |
| `get_ancestors` | `node_id` | Walk incoming edges, including the starting node |
| `get_descendants` | `node_id` | Walk outgoing edges, including the starting node |
| `ask_graph` | `question`, `workspace`; optional `scope`, `include_context` | Heuristic search results and optional connected-node context |

`query_nodes.search` matches titles and descriptions. The MCP schema does not expose an offset. Narrow the filters or use `get_graph` when you need the complete workspace. A branch-filtered graph omits edges whose other endpoint is outside the returned branch; read the workspace graph to inspect cross-branch borrowing.

The traversal tools currently stop after a budget of 50 visited nodes. They are useful for context recovery, not an exhaustive export of an arbitrarily large graph.

`ask_graph` uses text matching and inferred type/status filters. It does not call a language model, produce a verified answer, or perform semantic vector search. Its `scope` values are `all`, `active`, `decisions`, `goals`, `observations`, and `recent`. Read the returned records before drawing a conclusion.

### Write and connect

| Tool | Required arguments | Useful optional arguments |
| --- | --- | --- |
| `add_node` | `node_type`, `title` | `workspace`, `branch`, `description`, `status`, `confidence`, `commit`, `prompt`, `files` |
| `add_edge` | `from_node_id`, `to_node_id` | `workspace`, `branch`, `edge_type`, `rationale` |
| `update_node` | `node_id` | `title`, `description`, `status`, `metadata`, `branch` |
| `delete_node` | `node_id` | `branch` |
| `delete_edge` | `from_node_id`, `to_node_id` | `edge_type`, `branch` |

`add_node` returns both `id` and `change_id`. Use the returned server UUID `id` in ordinary node and edge tools. Local integer IDs and local change-ID prefixes are not interchangeable with server IDs.

`update_node.metadata` replaces the metadata map. Read it first and preserve fields you still need. Its top-level `branch` selects the write lease; changing the node's recorded branch requires `metadata.branch`.

`delete_node` sets a deletion timestamp. `delete_edge` removes the edge row. Prefer `superseded` or `abandoned` for a decision that remains useful history. A routine export is not a backup of deleted records.

### Capture helpers

| Tool | Required arguments | Purpose |
| --- | --- | --- |
| `log_observation` | `title` | Add an observation, optionally connected through `related_to` and `took_from`; also accepts `description`, `why`, `tags`, `workspace`, `branch` |
| `log_decision` | `title`, `chosen_option` | Add a decision with a chosen option and optional rejected options, rationale, parent, confidence, workspace, branch |
| `capture_conversation_turn` | `summary` | Add supplied goal, observations, options, decision, action, and outcome structures; optional parent, workspace, branch, confidence |
| `close_thread` | `title` | Add an outcome; optional parent, goal to complete, lessons, follow-up goals, success, workspace, branch |

`log_observation.related_to` and `took_from` accept a full server node UUID or full `change_id` in the same workspace. `why` becomes the borrowing edge's rationale. The observation and its edges are written in one transaction.

The other capture helpers perform multiple writes. Their current implementations do not wrap the entire operation in one transaction. After an error, inspect the graph for partial results before retrying. None of these helpers is a general idempotency endpoint.

`log_decision` draws `chosen` and `rejected` edges from the decision to its option nodes. Use the lower-level tools if your team requires a different connection pattern. `close_thread` does not finish an MCP session or release a lease through a separate unlock operation.

## Vocabulary

| Node type | Record |
| --- | --- |
| `goal` | The requested result and relevant user prompt |
| `option` | An approach worth considering |
| `decision` | The choice and its rationale |
| `action` | Work planned or performed, with evidence |
| `outcome` | What happened after the action |
| `observation` | A fact learned during the work |
| `revisit` | A decision being reconsidered |

Normal tool statuses are `pending`, `active`, `completed`, `rejected`, `superseded`, and `abandoned`. Imports also preserve legacy `feedback` nodes and `done` statuses; new work should use the documented vocabulary above. Confidence is an optional value from 0 to 100, not a measured probability.

Edge types are `leads_to`, `chosen`, `rejected`, `requires`, `blocks`, `enables`, and `took_from`. Both endpoints must exist in the same workspace. They may be on different branches. A `took_from` edge runs from the source idea to the node that reused it.

## CLI remote commands

Use the service's base URL for the CLI, without `/mcp`:

```sh
deciduous remote login --url http://127.0.0.1:4000
deciduous remote init http://127.0.0.1:4000 --workspace example-app
deciduous remote status
deciduous remote watch
```

Run repository commands from an initialized project. `login` reads the token from standard input and keeps it outside the repository. A non-empty `DECIDUOUS_MCP_TOKEN` environment value takes precedence over the stored credential.

| Command | Behavior |
| --- | --- |
| `remote login [--url URL]` | Store a token; optional URL verifies it first |
| `remote logout` | Remove the stored credential; does not clear a token already set in the environment |
| `remote init URL [--workspace NAME]` | Check health/authentication and save `[remote]` configuration |
| `remote status` | Compare local and server node and edge counts; matching counts do not prove identical content |
| `remote pull` | Merge server nodes and edges into the local graph; does not retrieve attachment bytes or provide a complete server backup |
| `remote push` | Bulk-import the local graph into the server; a controlled migration/backfill operation, not routine synchronization |
| `remote adopt ROOT --url URL --dry-run` | Preview configuration of existing `.deciduous` projects below a directory; omit `--dry-run` only after reviewing |
| `remote watch` | Connect to the event stream, print changes, and reconnect with backoff |

In 1.0, `remote watch` supports `--types outcome,observation`, repeated `--branch NAME`, `--edges`, and `--json`. `--url` and `--claude-code` print connection information instead of watching; that output contains a token-bearing URL. Keep it out of logs, screenshots, and the decision graph. Older 0.19 builds print a URL by default, so check `deciduous --version` and `deciduous remote watch --help`.

`remote push` can overwrite newer server fields with a stale local copy. `remote pull` does not reconcile server deletions into a faithful local mirror. Read [upgrading and migrating](content/upgrading.md) before using either on valuable history.

## HTTP endpoints

| Method and path | Purpose |
| --- | --- |
| `GET /health` | Unauthenticated liveness response, `ok` |
| `POST /mcp` | Authenticated MCP Streamable HTTP requests |
| `GET /export?workspace=example-app` | Authenticated graph snapshot; metadata, not attachment bytes |
| `POST /import` | Authenticated bulk import of a local-export-shaped graph |
| `PUT /blob/:sha256` | Authenticated raw document upload, verified against its SHA-256 hash |
| `GET /documents/:id` | Authenticated document bytes by document ID or content hash |
| `GET /events?workspace=example-app` | Authenticated WebSocket upgrade for write notifications |

`GET /mcp` returns 405 because the server does not offer a server-to-client SSE stream. Tool responses arrive through the MCP POST transport. Use `/events` for live write notifications.

Imports and blob uploads have a 64 MiB request-body limit. Document reads return 404 for an unknown attachment and 410 when the metadata exists but content is missing. The event stream can miss notifications during disconnects; query the graph after reconnecting.

See [architecture](content/architecture.md) for persistence and security limits, or [solo and offline use](content/solo.md) for the local CLI and Git-based workflow.
