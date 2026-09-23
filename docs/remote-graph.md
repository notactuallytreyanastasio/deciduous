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
workspace = "my-project"
```

A URL and the workspace name, decided once, here. **The token is read from
`DECIDUOUS_MCP_TOKEN`**, so committing this file leaks a hostname and no
credential. That config *should* be committed — it is how every clone and
worktree of the repo finds the same workspace, whatever its directory is
called.

## The commands

| Command | What it does |
|---|---|
| `deciduous remote init <url>` | Point this repo at a server; verifies and claims the workspace before writing, then sends local history the server lacks |
| `deciduous remote status` | Writes waiting in the log, and every node and edge that differs, field by field |
| `deciduous remote pull` | Send what is waiting, then refresh the local cache from the server (including its deletions) |
| `deciduous remote push` | Send what is waiting in the log. `--seed` also sends rows no op covers (history from before the remote); `--drop-rejected` discards ops the server refused |

`status` compares content, not counts. Two graphs can hold the same number of
nodes and different nodes:

```
Log: 1 write(s) waiting, 0 rejected  (.deciduous/remote-log.jsonl)
  waiting   create goal 6645c7fc "bobs unpushed goal"

                 local    server
  nodes              2         2
  edges              0         0

Only here (1)
  goal 6645c7fc "bobs unpushed goal"  (waiting in the log)

Only on the server (1)
  goal 0e355b0b "agents goal"

Differs: `deciduous remote push` sends what is waiting; `deciduous remote pull`
takes the server's side (newer edit wins per node).
```

## How CLI writes reach the server

Every write the CLI makes to its local database (`add`, `link`, `unlink`,
`status`, `prompt`, `delete`, the `archaeology` commands, the local MCP and
HTTP API) also appends one operation to `.deciduous/remote-log.jsonl`:

```json
{"entry":"op","op_id":"b220f13c-…","at":"…","kind":"update_node","change_id":"d5508657-…","set":{"status":"completed"}}
```

An op names only what the write changed. Before the command exits, the ops the
server has not acknowledged are sent, in order, to `POST /ops`, which applies
each one field by field and at most once (by `op_id`), and the answers are
appended to the log as acks. So:

- a status change does not resend the node, and cannot put back a title an
  agent changed in the meantime;
- a write made while the server is down waits in the log and goes on the next
  write or `deciduous remote push`;
- deletes and unlinks are ops too, and reach the server;
- sending an op twice (a lost ack, two pushes at once) changes nothing the
  second time.

**Where it lives.** Beside the database, in `.deciduous/`, which the rules
`deciduous init` writes to `.gitignore` already ignore: it is this machine's
unsent writes, not something to share. A project without `[remote]` keeps no
log.

**Compaction.** After each replay the file is rewritten without the ops the
server acknowledged, so it holds only what is waiting and what was refused. It
is as long as the queue, not as long as the project.

**Refused ops.** The server refuses, with a reason, an op it cannot apply (an
edit to a node an agent deleted, for one). Refused ops stay in the log, are
printed on every replay, and are listed by `remote status` until
`deciduous remote push --drop-rejected`.

## How workspaces are named

The **git repository root's directory name**, lowercased, decided once by
`remote init` and recorded in `.deciduous/config.toml`. The root rather than
the working directory, so running the CLI from a subdirectory cannot split one
project across two workspaces; the main working tree rather than a linked
worktree, so `git worktree add ../repo-feature` writes to `repo` like the
agents in it do. Recorded rather than re-derived, so renaming the directory or
cloning it under another name keeps writing to the same graph. (A config
written by 1.0.7, URL only, gets the name recorded the first time it is used.)
Anything outside a git repository pools into `scratch` instead of minting a
workspace per temporary directory.

A workspace belongs to the repository that first wrote to it, identified by
its root commits. Two unrelated repositories that are both called `api` would
derive the same name; the second one's `remote init`, writes and pulls are
refused, and the error says how to name its own workspace.

Override the name when the directory name is not what the graph should be
called, or to share one workspace between repositories on purpose:

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
        "Authorization": "Bearer ${DECIDUOUS_MCP_TOKEN}",
        "X-Deciduous-Workspace": "my-project"
      }
    }
  }
}
```

**Use `${DECIDUOUS_MCP_TOKEN}`, never a literal token.** `.mcp.json` is a file
people commit — it is meant to be, that is how a team shares server config —
and Claude Code expands `${VAR}` from the environment when it connects. With
the variable unset it refuses and says so:

```
[Warning] [deciduous] mcpServers.deciduous: Missing environment variables: DECIDUOUS_MCP_TOKEN
```

A literal token here is a credential committed to a repository. Pasting one in
is the single easiest way to leak access to every graph on the server.

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

## Working alongside other agents

Two sessions writing to one workspace need three things: not to interleave,
to know the other exists, and to find out when it writes. The branch is the
unit for all three.

### Locks

Every write claims an advisory lock on `(workspace, branch)` with a ten second
lease. The same session writing again renews it, so one lease covers a run of
writes; a different session on the same branch is refused and told who holds
it:

```
workspace "blog" (branch "feat/auth") is locked by claude-code (2.1.278),
session GNeD3Qwg. Releases in 8s if that session goes idle, or finishes sooner.
Retry shortly, or write to a different branch.
```

Sessions on different branches never contend. Pass `branch` on every write:
without it a session locks the empty-branch bucket, with everyone else who
forgot. A workspace can set `lock_scope: "workspace"` to make branches contend.

Fixed in 1.0: `update_node`, `delete_node` and `delete_edge` did not claim the
lock at all. The 0.19 commit that said it closed this gap had only added the
`branch` argument to their schemas; the tools never called the lock. They
resolve the node's workspace and claim it now.

### check_activity

The read side. Call it before a burst of writes.

| Field | What it holds |
|---|---|
| `sessions` | every unexpired lock: branch, client, version, whether it is you, seconds left |
| `branches` | the twenty most recently written branches: each with its last node (type, title, change_id, created_at) and who holds its lock. `branches: N` for more; `branches_total` says how many exist |

`branches` is new in 1.0. In the arena the agents polled `query_nodes` for
decisions because the lock list was all `check_activity` returned.

### Events

Postgres triggers call `pg_notify` on every insert or update to
`decision_nodes` and every insert to `decision_edges`. One listener bridges
that to a WebSocket at `GET /events?workspace=<name>` (or `*`). A frame is a
pointer with enough text to quote:

```json
{"table":"decision_nodes","op":"INSERT","workspace":"tetris-arena",
 "id":"…","change_id":"8d2f482a-…","node_type":"observation","status":"pending",
 "branch":"agent-7",
 "title":"Took the step-reset for lock delay from agent-8's decision, …"}
```

Edge frames carry `edge_type`, both endpoints' `change_id`, and the branch of
the source node. The handshake cannot carry an `Authorization` header from a
browser `WebSocket`, so `?token=` is accepted as a fallback. That is a bearer
token in a URL; keep it out of anything that logs.

### deciduous remote watch

Connects to that socket for this repository's workspace and prints one line
per write. It quotes; it does not count. It reconnects with backoff.

```
$ deciduous remote watch
Watching: tetris-arena
04:45:14  agent-9   observation  "Took 4 hidden rows from agent-3: SRS kicks can lift a piece 2 rows at the ceiling"
04:46:13  agent-10  observation  "Took 4 hidden rows (agent-9, from agent-3) and lock-out (agent-2, from agent-1); dropped my y<0 exemption"
04:47:15  agent-7   observation  "Took the step-reset for lock delay from agent-8's decision, and spawn-then-step-down and lock-out from agent-8 and agent-3"
```

| Flag | Effect |
|---|---|
| `--types outcome,observation` | only these node types |
| `--branch agent-3` | only this branch |
| `--edges` | include edge frames: `agent-3  edge chosen  a1b2c3d4 -> e5f6a7b8` |
| `--json` | the raw frame, one per line |
| `--url` | print the socket URL and exit, for another client |
| `--claude-code` | with `--url`: also print a `Monitor(...)` call for Claude Code |

Why quote rather than count: during the arena a session watching the raw
stream kept a per-branch tally and reported a "second goal" convention
spreading through the agents. The graph had ten goals. The tally had counted
updates as inserts.

### took_from

An edge type for a borrow: `from` is the node you took from, `to` is your node
that used it, crossing branches on purpose. `log_observation` takes
`took_from` (a node UUID or a full change_id) and `why` (the edge's rationale)
and writes the observation and the edge in one call; `add_edge` and `deciduous link -t took_from` take it directly.

```
log_observation({
  workspace: "tetris-arena", branch: "agent-7",
  title: "Took the step-reset for lock delay from agent-8's decision",
  took_from: "767cce30-9d0b-4e56-a4d8-2f0b1c8e7a11",
  why: "Resetting the lock timer on every step is what makes fast play at the floor feel fair."
})
```

In the arena zero of 471 edges crossed a branch. 89 of 91 observation titles
named another agent and none linked to the node, because the rules said to
link every node to its parent and did not say the node you borrowed from
counts. The borrow figures in the write-up come from a regular expression over
titles. `took_from` is where that information belongs.

## How pull works

`pull` does not reimplement merging. It writes the server's records into the
repository's `graph.json` store and hands them to the same `reconcile` that
`deciduous sync` uses between machines — newer `updated_at` wins, deletes are
tombstones, an edge waits until both endpoints exist. A separate remote-to-local
importer would have to re-earn all of that and would drift from it.

## What this does not do

- **A session that outlives the server is refused, not resumed.** Sessions
  expire after 24 idle hours, and a restart drops all of them. The next call
  on an old `mcp-session-id` gets `404` with JSON-RPC error `-32001 Session
  not found` under the request's own id, and the client starts a new session.
  Before 1.0 the server answered such a call with a `200` under a made-up id,
  and Claude Code waited 300 seconds for a reply it could never match, on
  every call, until the process was restarted.
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
- **The event stream is advisory.** Postgres does not queue a `NOTIFY` that
  fires while the listener's connection is down, and a reconnect races new
  `LISTEN`s against notifications issued at the same moment. A gap in the
  stream is not proof that nothing happened; call `check_activity` or
  `query_nodes` to catch up.
- **Locks are advisory too.** A client that ignores the refusal and writes
  anyway still can.
- **Documents live in Postgres**, keyed by sha256 and shared across workspaces.
  Five on this machine are referenced by rows whose bytes are gone; they answer
  `410 Gone`, not `404`.

## Deploying the server

See `deciduous_mcp/DEPLOY.md`.
