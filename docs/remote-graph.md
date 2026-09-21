# One graph for every project

A single Postgres holds every repository's decision graph, one workspace per
repository, behind an MCP endpoint. Claude reaches it from any directory on the
machine; the CLI reaches it with `deciduous remote`.

The local `.deciduous/deciduous.db` becomes a **cache**. The server is the
truth.

## Why that distinction matters

Before the server existed there was one writable store. Now there are two, and
they do not know about each other:

```
deciduous add …  (CLI)  ──►  .deciduous/deciduous.db   local SQLite
add_node         (MCP)  ──►  deciduous_mcp_prod        remote Postgres
```

A node added with the CLI never reaches the server on its own. A node Claude
writes through MCP never reaches SQLite. Left alone they drift silently, and
nothing in the graph tells you which half you are looking at.

Naming the server as the truth is what makes that tractable: local can always
be rebuilt from it, and `deciduous remote status` reports the gap rather than
letting you discover it months later.

## Setting up a repository

The server is registered once, globally — every directory already has it:

```bash
claude mcp add --scope user --transport http deciduous \
  https://<host>/deciduous-mcp/mcp \
  --header "Authorization: Bearer $DECIDUOUS_MCP_TOKEN"
```

Then, inside the repository:

```bash
export DECIDUOUS_MCP_TOKEN=<token>        # never goes in the repo
deciduous remote init https://<host>/deciduous-mcp
```

`init` verifies the server answers **and** that the token works before writing
anything. If either fails it says which, and leaves the project untouched — a
saved URL that does not answer is worse than no URL, because the failure only
surfaces later, on some unrelated command.

It writes one table to `.deciduous/config.toml`:

```toml
[remote]
url = "https://<host>/deciduous-mcp"
```

A URL and nothing else. **The token is read from `DECIDUOUS_MCP_TOKEN`**, so
committing this file leaks a hostname and no credential. That config *should*
be committed — it is how every clone of the repo finds the same workspace.

## The commands

| Command | What it does |
|---|---|
| `deciduous remote init <url>` | Point this repo at a server; verifies before writing |
| `deciduous remote status` | Local vs remote counts, and which side is ahead |
| `deciduous remote pull` | Refresh the local cache from the server |
| `deciduous remote push` | Send the local graph up (seeding and backfill) |

`status` names the direction, because the fix differs:

```
              local    remote
  nodes           2         3
  edges           1         1

Drift: the server holds more than this machine. `deciduous remote pull` to refresh.
```

Equal counts print `counts match`, not `in sync` — two graphs can hold the same
number of nodes and different nodes.

## How workspaces are named

The **git repository root's directory name**, lowercased. The root rather than
the working directory, so running the CLI from a subdirectory cannot split one
project across two workspaces. Anything outside a git repository pools into
`scratch` instead of minting a workspace per temporary directory.

Override it when the directory name is not what the graph should be called:

```bash
deciduous remote init <url> --workspace my-project
```

To pin a repository so that no tool call can write its nodes anywhere else, add
a header in `.mcp.json`. The header beats the `workspace` argument, on reads
and writes alike:

```json
{
  "mcpServers": {
    "deciduous": {
      "type": "http",
      "url": "https://<host>/deciduous-mcp/mcp",
      "headers": {
        "Authorization": "Bearer …",
        "X-Deciduous-Workspace": "my-project"
      }
    }
  }
}
```

## Asking across every project

Read tools take `workspace: "*"`:

```
query_nodes  workspace="my-project"   → just that repo
query_nodes  workspace="*"            → every repo, each node tagged with its own
list_workspaces                       → the index: what exists, how big
```

Writes refuse `"*"` — a node has to land somewhere specific:

```
add_node workspace="*"
→ workspace "*" is read-only: a node must be written to one project.
```

A repo pinned by header ignores `"*"` on reads too, so a project that opted
into isolation cannot be made to read its neighbours.

## How pull works

`pull` does not reimplement merging. It writes the server's records into the
repository's `graph.json` store and hands them to the same `reconcile` that
`deciduous sync` uses between machines — newer `updated_at` wins, deletes are
tombstones, an edge waits until both endpoints exist. A separate remote-to-local
importer would have to re-earn all of that and would drift from it.

## What this does not do

- **Themes do not transfer.** `deciduous graph` exports nodes, edges and
  documents; the SQLite `themes`/`node_themes` tables and the matching Postgres
  columns both exist, but nothing carries the assignments across.
- **`push` is not the normal write path.** It exists to seed a workspace and to
  carry history that predates the server. Routine writes should go through MCP.
- **There is no offline queue.** Without the network you have a local cache and
  a CLI that can still write to it; those writes reach the server on the next
  `push`, and until then the two differ.
- **Self-loop edges are rejected** by the server's schema, and reported per
  project rather than dropped quietly.
- **Documents live in Postgres**, keyed by sha256 and shared across workspaces.
  Five on this machine are referenced by rows whose bytes are gone; they answer
  `410 Gone`, not `404`.

## Deploying the server

See `deciduous_mcp/DEPLOY.md`.
