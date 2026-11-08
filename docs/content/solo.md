# Solo and offline use

The Rust CLI can keep a graph in `.deciduous/deciduous.db` without a running server. Use this for an independent offline project or a local copy you want to inspect. For agents that must read one another's current decisions, use [shared Postgres](local-postgres.md) and HTTP MCP instead.

## Start a local graph

Install the current CLI and initialize the repository:

```sh
brew tap notactuallytreyanastasio/tap
brew install deciduous
deciduous init
deciduous add goal "Make retries bounded" -p "Make retries stop after a bounded delay."
deciduous nodes
deciduous serve
```

`init` creates local state and agent integration files. Review generated instructions before using them; a local CLI workflow and a shared-server workflow have different write destinations. Keep the SQLite database out of Git.

Use `deciduous add --help`, `link --help`, and other command help from your installed version for details. Local node references can use an integer ID or a supported `change_id` prefix. Server MCP tools generally require the server's full UUID instead.

## Feature boundaries

| Capability | Shared HTTP MCP / Postgres | Local Rust CLI / stdio MCP |
| --- | --- | --- |
| Read and write decisions | Shared tools such as `add_node`, `query_nodes`, `add_edge` | Local commands and a separate stdio tool set |
| Workspace-wide cross-branch reads | Yes, without copying between agent machines | Reads the local database's current contents |
| Activity and branch leases | `check_activity`, short MCP write leases | No equivalent shared-service coordination |
| Live writes | `/events` and `remote watch` | Local viewer reads local data; not the remote event stream |
| Borrowing provenance | `took_from` edges; `log_observation` helper | `deciduous link ... -t took_from` in the local graph |
| Documents | Stored metadata and uploaded bytes; HTTP import/upload/read endpoints | `doc` commands and stdio attachment tools |
| Themes and assignments | Schema/export support, but no registered theme-editing MCP tools | Theme/tag commands and stdio tools |
| DOT images and PR writeups | No registered export-DOT or writeup tools | `dot`, `writeup`, and corresponding stdio tools |
| Explicit local conversation sessions | Not the HTTP MCP session model | Local stdio session tools and `.deciduous/active_session` |
| Offline edits | No automatic queue for the shared server | Available locally; later transfer needs review |
| Git graph synchronization | Not the Postgres write path | `.deciduous/graph.json` and `deciduous sync` |

The shared server and local stdio MCP do not promise feature parity. Discover the tools on the connection you are using. An imported field appearing in a graph snapshot does not imply that a dedicated shared tool can edit it.

## Inspect a local copy of the shared graph

After configuring the CLI remote, you can pull nodes and edges and open the local viewer:

```sh
deciduous remote status
deciduous remote pull
deciduous serve
```

This is a partial local copy. Pull does not download attachment bytes or reproduce all server deletions, and it does not keep running in the background. The local viewer is therefore not proof of the server's current state. Query the HTTP tools for an authoritative team read.

Treat the copy as inspection data. A local `add`, `link`, or status update still writes to SQLite. Do not bulk-push those local changes into a busy workspace to make them visible; the import path can replace newer server fields. Record new shared work through HTTP MCP.

## Optional Git-based collaboration

For a team that chooses the local/Git model instead of a shared service, `.deciduous/graph.json` holds the records exchanged through Git. Local mutations write the record store, and `deciduous sync` reconciles it with SQLite.

```sh
git pull
deciduous sync
deciduous sync --check
git status --short
```

Review the graph file before staging it by name. It can contain user prompts, paths, descriptions, and metadata. Keep private records out of a public repository. `sync --check` uses a nonzero exit status to report pending reconciliation; read its output before treating that as a command failure.

Local numeric IDs differ between machines. Use change IDs for durable references. The merge driver and reconciliation use record timestamps, so inspect conflicts involving concurrent edits to the same reasoning. Document bytes and local session/command records are not a complete part of this Git exchange.

Git-based synchronization does not contact the shared Postgres server. Avoid running the two collaboration models as if they were one automatic replication system.

## Move to the shared service

Keep a backup of the local database and document files, choose a workspace name the whole team will use, and follow [upgrading and migration](upgrading.md). The supplied procedure checks the target and handles content uploads separately from graph metadata.

After migration, connect agents to the shared HTTP MCP endpoint and replace local-write instructions in their project guidance. Retain the old database as a migration backup. Do not delete it merely because the new workspace has matching node counts.
