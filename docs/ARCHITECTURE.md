# Deciduous Architecture

One graph per repository: goals, options, decisions, actions, outcomes,
observations and revisits, joined by typed edges. Three things write it (the
CLI, the MCP tools an agent calls while it works, and since 1.1.0 an import of
the Claude Code transcript after the fact), and the whole design question is
where the graph lives and how the copies agree. This page is that story for
1.1.0.

---

## Where the graph lives

`deciduous init` asks. The answer decides which of two models a project runs
under, and most of the architecture follows from it.

```
Where should this project's graph live?

  1) This machine: the deciduous server as a background service, using PostgreSQL on
     localhost:5432 as you (answering now)
  2) This machine: the server as a background service, using a PostgreSQL you name
     (any host, port, user, password)
  3) This machine, in Docker: a new PostgreSQL and the server in containers
  4) A server someone else runs: you need its URL and token
```

| Store | Path | Tracked in git | Role |
|---|---|---|---|
| SQLite database | `.deciduous/deciduous.db` | no | A per-machine cache. Every CLI command reads and writes it. Holds local-only data too: the agent board (local model), roadmap state, Q&A history. |
| Graph file | `.deciduous/graph.json` | yes, in the git model | The shared graph for projects with no `[remote]`: every node, edge, theme and tag, keyed by `change_id`. Every database write is mirrored into it at once; a `git checkout`, `pull`, `reset` or `stash` is followed by the next command. `deciduous sync` reconciles the two. |
| Shared server | PostgreSQL behind an Elixir MCP endpoint, `[remote] url` in `.deciduous/config.toml` | the config is; the data is not | The truth for projects with a `[remote]`. One workspace per repository. Agents write to it over HTTP; the CLI writes locally and replays each write as an op. |
| Op log | `.deciduous/remote-log.jsonl` | no | This machine's writes on their way to the server, one op per field change, replayed through `POST /ops` after each write and by `remote push`. Only kept when the project has a `[remote]`. |
| Documents | `.deciduous/documents/<name>.<hash8>` | no | Files attached to nodes, named by original filename plus the first eight hex digits of their SHA-256. With a `[remote]`, `remote push --seed` sends them, bytes included. |
| Token | `~/.config/deciduous/credentials` (mode 0600) | outside every repository | The server's bearer token. Also read from `DECIDUOUS_MCP_TOKEN`. Never in `config.toml`. |
| This machine's server | `~/.config/deciduous/server/` | no | The release's single-file server executable, its `.env` (database URL, token, port, `DECIDUOUS_SERVER_MODE`), logs, and a launchd or systemd user service. |

How the roles shifted, release by release, matters because older pages still
tell older stories:

- **0.17** kept one JSON file per record under `.deciduous/sync/`; **0.18**
  folded that into the single `graph.json` and made the git merge driver the
  mechanism (`deciduous merge-record`, registered by `init`, `update` and
  `sync`). A `.deciduous/sync/` directory today is a leftover `sync` folds in
  once and deletes.
- **0.19 and 1.0.0** added the shared server. "Nothing writes back from the
  server to SQLite except `remote pull`. The local database is a cache."
- **1.0.5** removed the GitHub Pages export: `deciduous sync` only reconciles
  `graph.json` with the database and no longer writes `docs/graph-data.json`.
- **1.0.8** replaced snapshot pushes with the op log, so a status change
  cannot put back a title an agent changed meanwhile.
- **1.0.11** made the local database follow `graph.json` on every checkout,
  so a node added on one branch no longer appears on every branch, and made
  "this machine's server" a native user service against PostgreSQL on 5432
  instead of Docker.
- **1.0.12** made `update` keep every section and command file the user
  edited, and give projects with a `[remote]` remote-aware templates.

In the remote model `init` and `update` skip `.gitignore`, `.gitattributes`,
the merge driver and `graph.json`: the graph lives on the server, and a
worktree whose config lacks `[remote]` sends to its main checkout's server.
Themes and tags are not sent to the server; the board is never in `graph.json`
or any export.

## What writes the graph

```
you / the agent
  │
  ├─ deciduous add | link | status | prompt | delete | unlink | doc | tag | archaeology …
  │      → SQLite, mirrored to graph.json (git model) or logged as an op and sent (remote model)
  │
  ├─ MCP tools, two servers with different tool sets:
  │      deciduous mcp         stdio, 34 tools, writes the local database like the CLI
  │      the shared server     HTTP, 21 tools (add_node, capture_conversation_turn, ask_graph,
  │                            post_message, check_activity …), writes PostgreSQL directly
  │
  └─ deciduous transcript import <session>      (1.1.0)
         → reads ~/.claude/projects/<slug>/<session>.jsonl, derives goals from prompts,
           actions from edits and commits, outcomes from test runs, each with
           metadata.evidence = proven | inferred
```

The transcript path exists because an agent that logs its own decisions is an
agent that can forget to. Up to 1.0.2 deciduous tried to force it with a hook
that denied the agent's next tool call until it wrote to the graph; 1.0.3
removed it, because a rule that blocks work has to be one the agent can
satisfy every time and this one could not (a graph write sent beside the next
action did not count, and an environment variable typed into the session never
reached the hook). The hooks `init` installs now, `version-check.sh` and
`board-mentions.sh`, never block. Logging is encouraged by the `CLAUDE.md`
section, the MCP tools' own descriptions and replies, and the instructions the
shared server sends on `initialize`; and whatever the agent did not log, the
transcript import recovers afterwards with the evidence labelled.

A transcript-derived action, from a scratch project:

```
$ deciduous show 13

Node #13 action
────────────────────────────────────────────────────────────
Title: Edit src/lib.rs, src/widget.rs
Description: Reading the module, then adding the widget.
Status: pending
Change ID: 09d8fc49-fbc7-8448-83b2-6286a557c26a
Created: 2026-10-01T10:00:08Z
Updated: 2026-10-01T10:00:08Z

Metadata
  Branch: feat/widget
  Files: src/lib.rs (git:3048f12804ac39bd0b0adee36ab1ba9b09b4b151), src/widget.rs (git:d456351e4e617162562a1093bcee361646b4c8ee)
  Consulted: src/lib.rs
  Evidence: proven transcript, session 0f1e2d3c, 2 tool call(s)

Connections
  Incoming (1):
    #12 ─[leads_to]→ here: files changed in this turn
  Outgoing (1):
    here ─[leads_to]→ #14: command run in this turn
```

Decisions and options are not derived from transcripts; see
[transcripts.md](transcripts.md).

## What reads it

| Reader | What it gives |
|---|---|
| `deciduous nodes`, `edges`, `show <id>`, `graph` | The local database as text or JSON. `show` prints evidence, consulted files and file hashes; `show --json` prints `metadata_json` raw. |
| `deciduous serve` | The embedded React viewer on `http://localhost:3000`, reading the local database through `/api/graph`. See [WEB_VIEWER.md](WEB_VIEWER.md). |
| `deciduous dot`, `deciduous writeup` | Graphviz DOT (and PNG with graphviz installed) and a PR body from a node range. |
| `deciduous pulse`, `narratives pivots`, `archaeology timeline` | Health, pivot chains and a chronological listing. |
| `ask_graph`, `query_nodes`, `show_node` (shared server) | Retrieval across one workspace or every workspace (`workspace: "*"`), with the route each answer took. |
| `deciduous remote status`, `remote watch` | Drift between this copy and the server, field by field; a live line per write. |

## The model, in one screen

`goal -> options -> decision -> actions -> outcomes`. Observations attach
anywhere; a revisit marks a pivot and leads to the new decision. Edge types:
`leads_to`, `requires`, `chosen`, `rejected`, `blocks`, `enables`, `took_from`
(a borrow, usually across branches). Statuses: `pending`, `active`,
`completed`, `rejected`, `superseded`, `abandoned`. Every node has a local `id`
(different on every machine), a `change_id` (the same everywhere), and on the
server a server id; the CLI takes all three. The full treatment is
[CONCEPTS.md](CONCEPTS.md).

## Agent coordination

Agents working in parallel on one repository talk on the message board
(`deciduous board post|read|show`, `post_message` and `read_messages` on both
MCP servers), not in a scratch file. Locally it lives in the main worktree's
database, so every worktree reads one board; with a `[remote]`, in the
server's `agent_messages` table. `@label` addresses a message, `--reply-to`
answers one, and `board read --unanswered <label>` is what a session reads at
start. On the server, `check_activity` reports who has written to which branch
in the last five minutes, and `GET /events` streams every write.

## Source layout

```
src/
├── main.rs            CLI: clap definitions and command dispatch
├── lib.rs             crate exports
├── db.rs              SQLite via Diesel: nodes, edges, documents, themes, board, mirror digest
├── schema.rs          Diesel tables (decision_nodes, decision_edges, node_documents, themes,
│                      node_themes, agent_messages, command_log, roadmap_*, qa_interactions …)
├── config.rs          .deciduous/config.toml: [branch], [github], [hooks], [remote]
├── records.rs         graph.json: the record store, reconcile, follow-on-checkout, merge driver
├── oplog.rs           .deciduous/remote-log.jsonl: ops queued for the server, acks, compaction
├── remote.rs          client of the shared server: setup, init, status, push, pull, adopt
├── server.rs          installing and checking "this machine's server" from init and update
├── local_server.rs    the pure decisions behind that: database URL, mode, .env, service units
├── watch.rs           `remote watch`: the GET /events WebSocket client
├── board.rs           the message board, local and remote backends
├── transcript.rs      `transcript import` and `transcript list` (1.1.0)
├── mcp/               `deciduous mcp`, the stdio server
│   ├── protocol.rs    JSON-RPC and MCP message types
│   ├── tools.rs       the 34 tool definitions and dispatch
│   ├── handlers.rs    tool handlers over Database
│   └── query.rs       pure graph analysis used by the reporting tools
├── serve.rs           `deciduous serve`: the viewer's HTTP API, embeds viewer.html
├── api.rs             `deciduous serve --api`: the multi-graph bearer-token daemon
├── export.rs          DOT export and PR writeups
├── archaeology.rs     pivot, timeline, supersede
├── narratives.rs      .deciduous/narratives.md and pivot chains
├── pulse.rs           the health report
├── hooks.rs           hooks listed in config.toml (deciduous ships none since 1.0.3)
├── init/
│   ├── mod.rs         init and update
│   ├── templates.rs   CLAUDE.md section, commands, skills, hook scripts (git model)
│   ├── remote_templates.rs   the same for projects with a [remote]
│   ├── guard.rs       how update replaces only what deciduous wrote
│   └── known_templates.rs    every template text ever shipped, for recognising old installs
├── opencode.rs        OpenCode integration (.opencode/, AGENTS.md)
├── roadmap.rs, github.rs   ROADMAP.md and GitHub Issues sync through `gh`
├── events.rs          reader for the pre-0.17 JSONL event log, migration only
├── changelog.rs       release notes shown by `update`
├── demo_swarm/        `deciduous demo-swarm` (hidden): the multi-agent Tetris demonstration
└── viewer.html        the built viewer, generated from web/

deciduous_mcp/         the shared server: Elixir on Bandit, PostgreSQL;
                       21 MCP tools, /ops, /import, /export, /events, /messages
web/                   the viewer's source (React, Vite); web/src/App.tsx is most of it
```

Not in 1.1.0: a terminal UI (`deciduous tui` is not a command), GitHub Pages
export from `sync`, `diff export`/`diff apply` patches, `.deciduous/sync/`,
and blocking hooks.

## Tests

```bash
cargo test            # unit and CLI tests against temporary databases
cargo clippy          # warnings denied in CI; the toolchain is pinned to 1.92.0 by rust-toolchain.toml
```

### The integration battery

`tests/e2e_*.rs` drive real processes across real boundaries: the built
binary, `deciduous mcp` over stdio, `deciduous serve --api` over TCP, git
clones of a bare origin with the merge driver, and a shared graph server over
HTTP MCP. A plain `cargo test` skips all of it (each test prints one
`skipped:` line), so the normal suite stays fast and needs no network.

| File | What it holds to account |
|------|--------------------------|
| `e2e_bridge.rs` | CLI writes reaching the server: offline replay, deletes, no stale overwrites, `remote status` |
| `e2e_server.rs` | The HTTP MCP server: workspace pins, atomicity, validation, protocol edges, auth on every route |
| `e2e_gitsync.rs` | Teammates through git: merge driver failures, clock skew, `sync --check` |
| `e2e_conflicts.rs` | Three clones, one server, some offline: stale ops refused, edits arriving through git and the server in both orders |
| `e2e_stdio.rs` | The stdio server and API daemon: id range, path confinement, hangs, error ids |
| `e2e_concurrency.rs` | Many processes on one project: every acknowledged write lands, no "database is locked" |
| `e2e_model.rs` | Seeded random interleavings of every surface, checked against a model at each sync |
| `e2e_fuzz.rs` | Malformed, huge and invalid-UTF-8 input to both MCP servers |
| `e2e_swarm.rs` | `demo-swarm`'s trust-gated start with a stub `claude` (macOS) |

Local-only parts (no server):

```bash
DECIDUOUS_E2E=1 cargo test --test e2e_gitsync --test e2e_stdio \
  --test e2e_concurrency --test e2e_model --test e2e_fuzz --test e2e_swarm
```

Everything, against a throwaway server built from your checkout (never
production; every test writes its own fresh workspaces, but they stay):

```bash
cd deciduous_mcp
COMPOSE_PROJECT_NAME=dx-e2e DECIDUOUS_PORT=4816 DECIDUOUS_IMAGE=deciduous-mcp:e2e \
  DECIDUOUS_ENV_FILE=/tmp/dx-e2e.env sh scripts/setup.sh
cd ..
export DECIDUOUS_E2E=1 DECIDUOUS_E2E_SERVER=http://127.0.0.1:4816
export DECIDUOUS_E2E_TOKEN=$(sed -n "s/^DECIDUOUS_MCP_TOKEN='\(.*\)'$/\1/p" /tmp/dx-e2e.env)
cargo test --test 'e2e_*'
docker compose -p dx-e2e down -v && docker image rm deciduous-mcp:e2e
```

The randomized tests print their seed first and again in any failure.
`DECIDUOUS_E2E_SEED=<seed>` replays that run exactly; `DECIDUOUS_E2E_STEPS=<n>`
makes it longer. CI runs the whole battery on every pull request
(`.github/workflows/integration.yml`) against a server built from the same
commit, plus `mix test` for the server itself.

## Development workflow

```bash
cargo build --release
./target/release/deciduous <command>

# The viewer: after any change under web/src
cd web && npm run build && cd ..
cp web/dist/index.html src/viewer.html        # embedded by serve.rs
cp web/dist/index.html docs/demo/index.html   # the static demo; the two files are identical
cargo build --release
```

`src/viewer.html` is generated; do not hand-edit it.

## Known gaps in 1.1.0

- The viewer does not show evidence kind, consulted files or file hashes;
  `show` and `show --json` do.
- `deciduous commands` prints "No commands logged." in a fresh 1.1.0 project:
  nothing in the CLI writes `command_log` any more.
- `deciduous add --help` lists six node types and `status --help` four
  statuses; the binary accepts seven and six (`revisit`; `superseded`,
  `abandoned`).
- A write made on a branch while a server is configured is on the server for
  every branch: the server has no branches.
- Themes and tags are not sent to the server.
