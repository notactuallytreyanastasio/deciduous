# One graph for every project

A single Postgres holds every repository's decision graph, one workspace per
repository, behind an MCP endpoint. Claude reaches it from any directory on the
machine; the CLI reaches it with `deciduous remote`.

The local `.deciduous/deciduous.db` is a **cache**. The server is the truth.

## Why a server

Agents write through the HTTP MCP server, so a project with no server is a
project whose agents have nowhere to write. Every project points at one, and
`deciduous init` does not finish until the project points at one that answers.

The server does not have to be somewhere else. By default it runs on this
machine, as a background service installed under `~/.config/deciduous/server/`
(a launchd LaunchAgent on macOS, a systemd user unit on Linux), and keeps its
data in a PostgreSQL on `localhost:5432`, connecting as the current OS user to
a database called `deciduous` that it creates and migrates itself. That is the
PostgreSQL a `brew install postgresql@17` or a distribution package already
runs. If nothing answers on 5432, `init` says so in the menu and prints what
to do:

```
  Nothing answers on localhost:5432, so choice 1 needs a PostgreSQL first:
    macOS:          brew install postgresql@17 && brew services start postgresql@17
    Debian/Ubuntu:  sudo apt install postgresql
  Or name another PostgreSQL (choice 2, --database-url), or let Docker run one
  for you (choice 3, --docker-postgres).
```

Any other PostgreSQL works if you give its URL. Docker can run both PostgreSQL
and the server, if you would rather install neither. A team can run one server
on a shared machine and hand out its URL and token; that runbook is
`deciduous_mcp/DEPLOY.md`, and the projects that use it take choice 4 below.

Wherever it runs, every repository gets its own workspace, and one query can
cross all of them. "What did I decide about auth, across every project I've
touched?" has an answer because the data is in one place.

## Why the cache/truth distinction matters

There are two writable stores, and they do not know about each other:

```
deciduous add …  (CLI)  ──►  .deciduous/deciduous.db   local SQLite, then an op in the log
add_node         (MCP)  ──►  deciduous                 the server's Postgres
```

A node the CLI adds is written locally and queued as an op that is sent to the
server before the command exits (see [How CLI writes reach the
server](#how-cli-writes-reach-the-server)). A node Claude writes through MCP
never reaches SQLite until `deciduous remote pull`. Left alone they drift, and
nothing in the graph tells you which half you are looking at.

Naming the server as the truth is what makes that tractable: local can always
be rebuilt from it, and `deciduous remote status` reports the gap rather than
letting you discover it months later.

## Setting up a repository

`deciduous init` (and `deciduous update`) in a project whose
`.deciduous/config.toml` has no `[remote]` asks, in a terminal, where the
project's graph should live:

```
Shared graph server
   This project has no server yet ([remote] in .deciduous/config.toml).

Where should this project's graph live?

  1) This machine: the deciduous server as a background service, using PostgreSQL on
     localhost:5432 as you (answering now)
  2) This machine: the server as a background service, using a PostgreSQL you name
     (any host, port, user, password)
  3) This machine, in Docker: a new PostgreSQL and the server in containers
  4) A server someone else runs: you need its URL and token

  1 and 2 install the release's server executable in /Users/you/.config/deciduous/server
  and start it at login. --docker runs that server in Docker instead.

Choose 1-4 [1]:
```

- **1**, the default. The first time on a machine, `init` downloads the server
  executable for its own version (one file with Erlang and Elixir inside),
  checks it against the release's `checksums.txt`, and runs it once to create
  the database `deciduous` and apply migrations. If PostgreSQL refuses (no
  such role, wrong password, no CREATEDB), you see its reason and the menu
  again. Then it installs the server as a user service that starts at login.
  It listens on `127.0.0.1` only.
- **2**. The same server, against a PostgreSQL you name:
  `postgres://USER@HOST:PORT/DATABASE`, with `?sslmode=require` or
  `verify-full` for TLS. The password is asked separately and not shown.
- **3**. Downloads the Docker bundle for its own version, checks it, and
  starts PostgreSQL 17 and the server in containers (a few minutes the first
  time). This is what 1.0.10 and earlier always did; an existing Docker
  install stays as it is.
- **4**. Asks for the URL and the token (typed without being shown), and checks
  both against the server before storing anything.

For 1 to 3 the server needs a port on this machine, and `init` asks for it
with a free one suggested from 20000-32767:

```
The server needs a port on this machine. 24871 is free right now; anything
already using the port you pick (a dev server on 4000, say) would stop this
server from starting.

Port [24871]:
```

The port is asked rather than fixed so that the server never squats one the
machine already wants. Choice 1 on a Mac prints, in order:

```
   Downloading https://github.com/notactuallytreyanastasio/deciduous/releases/download/v1.0.12/deciduous-mcp-darwin-arm64
   Installed /Users/you/.config/deciduous/server/bin/deciduous-mcp
   Preparing postgres://you@localhost:5432/deciduous (created if missing, then migrated; the first run unpacks the server)
   Created database deciduous
   Wrote /Users/you/.config/deciduous/server/.env (mode 600: database URL, token, port 24871)
   Started launchd agent dev.deciduous.server (/Users/you/Library/LaunchAgents/dev.deciduous.server.plist), logs in /Users/you/.config/deciduous/server/logs/server.log
   Ready http://127.0.0.1:24871/ready
   Stored token in /Users/you/.config/deciduous/credentials
   Wrote .deciduous/config.toml [remote] url = http://127.0.0.1:24871
   Connected http://127.0.0.1:24871 holds 0 nodes, 0 edges in workspace my-project
   Registered Claude Code: deciduous MCP at http://127.0.0.1:24871/mcp (user scope; restart Claude Code)

Done: this project writes to http://127.0.0.1:24871. Restart Claude Code so it picks up the server;
it sends the logging instructions when it connects.
```

On Linux the `Started` line names `deciduous-server.service` under
`~/.config/systemd/user/`, followed by a note that user services stop at
logout unless `loginctl enable-linger <user>` is on.

**Restart Claude Code.** When it connects, the server sends the logging
instructions (when to write, and how to write one step in one call). There is
no hook to install. If anything is missing (no PostgreSQL, a server that does
not answer, no token), `init` and `update` stop with exit 1 and say how to fix
it.

### What setup wrote

| File | Holds | Commit it? |
|---|---|---|
| `.deciduous/config.toml`, `[remote]` | the server's `url` and this repository's `workspace` name | yes: every clone and worktree finds the same workspace through it, and it holds no credential |
| `~/.config/deciduous/credentials` (mode 0600) | the token | no: it is outside every repository on purpose |
| `~/.config/deciduous/server/.env` (mode 600) | this machine's server: database URL, token, port, server mode and version | no |
| Claude Code's user-scope MCP list | `deciduous` at `<url>/mcp` with the bearer header | not a file in the repo |

```toml
[remote]
url = "http://127.0.0.1:24871"
workspace = "my-project"
```

The token is read from `DECIDUOUS_MCP_TOKEN` in the environment first, then
from the credentials file. The variable is for CI and one-off overrides; on a
developer machine nothing needs to be exported.

The Claude Code registration is this, run for you:

```bash
claude mcp add --scope user --transport http deciduous \
  http://127.0.0.1:24871/mcp \
  --header "Authorization: Bearer $DECIDUOUS_MCP_TOKEN"
```

An existing `deciduous` server in Claude Code is left alone (`Unchanged
Claude Code already has a deciduous MCP server`). If `claude` is not on
`PATH`, setup prints `Claude Code not found; register the server in your
client: <url>/mcp` and the line above is what to run, or what to translate
for another MCP client.

### The second project on this machine

Every later `deciduous init` finds the server that is already running, in
about a second, and skips the installation:

```
   Found local server at http://127.0.0.1:24871
   Wrote .deciduous/config.toml [remote] url = http://127.0.0.1:24871
   Connected http://127.0.0.1:24871 holds 0 nodes, 0 edges in workspace other-project
   Unchanged Claude Code already has a deciduous MCP server
```

`deciduous remote setup` asks the same question on its own, in a project that
already has `[remote]` too (it offers to keep the current server first). Once
this machine has a server, the menu shrinks to two choices: that server, or
one someone else runs. `deciduous update` also upgrades this machine's server
executable when it is older than the CLI.

### In a script

There is no terminal to ask, so give the answer instead; without one `init`
stops and prints these:

```bash
deciduous remote setup --local                  # this machine: a background service, PostgreSQL on localhost:5432
deciduous remote setup --database-url <url>     # this machine's server, a PostgreSQL you name
deciduous remote setup --docker-postgres        # this machine: PostgreSQL and the server in Docker
deciduous remote setup --url <url>              # a server someone else runs
```

`--docker` runs this machine's server in Docker against the PostgreSQL from
`--local` or `--database-url`. `--port <n>` or `DECIDUOUS_PORT` answers the
port question the first time; after that the port in
`~/.config/deciduous/server/.env` stands. `--url` takes its token from
`DECIDUOUS_MCP_TOKEN` or the stored one. `DATABASE_URL` in your environment
is never used unless you pass it with `--database-url`. Tests and CI in a
throwaway directory set `DECIDUOUS_NO_SERVER=1` to skip the step.

### The lower-level commands

Setup is built from three commands that still work on their own.

`deciduous remote login` stores a token by hand. It reads from stdin so the
token never lands in shell history or the process list, and with `--url` it
checks the token against that server before storing anything:

```
$ deciduous remote login --url https://example.com/deciduous-mcp
Token (read from stdin, not saved to shell history):
Token: stored in /Users/you/.config/deciduous/credentials
  mode 0600, outside every repository
```

`deciduous remote init <url>` points one repository at a server whose token is
already stored or in the environment. It verifies the server answers **and**
claims the workspace before writing; if either fails it says which and leaves
the project untouched. Then it sends the server the local history it lacks:

```
$ deciduous remote init https://example.com/deciduous-mcp
Remote: https://example.com/deciduous-mcp
  workspace: blog
  claim: verified (by this repository's root commit)
  server holds 719 nodes, 845 edges, 0 documents
  sent 12 local node(s), 9 edge(s) the server lacked

Written to .deciduous/config.toml. The token stays in ~/.config/deciduous/credentials (or DECIDUOUS_MCP_TOKEN), never here.
```

`--workspace <name>` overrides the derived name; see [How
workspaces are named](#how-workspaces-are-named).

`deciduous remote adopt <dir> --url <url>` walks a directory for projects that
already have a `.deciduous` and points each at the server. It never repoints a
project already aimed somewhere else; `--dry-run` first lists what it would
do.

## The commands

| Command | What it does |
|---|---|
| `deciduous remote setup` | Choose where this project's graph lives (the question `init` asks), set up this machine's server if that is the answer, store the token, write `[remote]`, register the server with Claude Code. `--local`, `--database-url`, `--docker-postgres`, `--docker`, `--url`, `--port` answer for scripts |
| `deciduous remote login` | Store a token by hand, read from stdin, outside every repository (mode 0600). `--url` verifies it first |
| `deciduous remote logout` | Forget the stored token |
| `deciduous remote init <url>` | Point this repo at a server; verifies and claims the workspace before writing, then sends local history the server lacks |
| `deciduous remote adopt <dir> --url <url>` | Point every project under a directory at a server; never repoints one already configured |
| `deciduous remote status` | Writes waiting in the log, and every node, edge and document that differs, field by field, each with the command that fixes it. Exits 1 when anything differs |
| `deciduous remote pull` | Send what is waiting, then refresh the local cache from the server (including its deletions) |
| `deciduous remote push` | Send what is waiting in the log. `--seed` also sends rows and documents (with their bytes) no op covers, such as history from before the remote. `--repair` makes the server's fields match this copy's for every node status lists as Different. `--drop-rejected` discards ops the server refused, `--retry-rejected` sends them again, `--drop <op id>` discards one op by the id `remote status` prints. Also sends again what this machine set aside, and exits 1 if anything is still set aside or a row could not be seeded |

`status` compares content, not counts. When nothing differs it says so:

```
Log: 0 write(s) waiting, 0 rejected  (.deciduous/remote-log.jsonl)

                 local    server
  nodes            719       719
  edges            845       845

In sync: no writes waiting, and every node, edge and document matches field by field.
Themes and tags are not sent to the server, so they are not compared.
```

Two graphs can hold the same number of nodes and different nodes, which is why
the table is not the verdict. When they differ, each side's extra rows are
listed by name:

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

Differs: `deciduous remote push` sends what is waiting; `--seed` adds what only
this copy has; `--repair` makes the server's fields match this copy's;
`deciduous remote pull` takes the server's side (newer edit wins per node).
```

A node listed as Different says which side edited it later. When this copy
did, the edit's op never reached the log. It may have been made before this
clone's config had a `[remote]`, or while the log could not be written.
`remote push --repair` sends it. A node an agent deleted is listed under
"Deleted on the server", and `remote pull` removes it here; `--seed` never
sends it back. Themes and tags are not sent to the server and are not
compared.

## How CLI writes reach the server

Every write the CLI makes to its local database (`add`, `link`, `unlink`,
`status`, `prompt`, `delete`, the `archaeology` commands, the local MCP and
HTTP API) also appends one operation to `.deciduous/remote-log.jsonl`:

```json
{"entry":"op","op_id":"b220f13c-…","at":"…","kind":"update_node","change_id":"d5508657-…","set":{"status":"completed"},"was":{"status":"pending"}}
```

An op names only what the write changed, and what each field held before
(`was`). The server writes a field only while it still holds that value. An
op that waited in the queue while an agent changed the same field is refused,
with both values, instead of putting the older one back. Before the command exits, the ops the
server has not acknowledged are sent, in order, to `POST /ops`, which applies
each one field by field and at most once (by `op_id`), and the answers are
appended to the log as acks. So:

- a status change does not resend the node, and cannot put back a title an
  agent changed in the meantime, or a status either;
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
printed on every replay, and are listed by `remote status`, each with the
start of its op id, until `deciduous remote push --drop-rejected` (all of
them), `--drop <op id>` (one) or `--retry-rejected` (send them again, once the
cause is fixed).

A write whose text holds a NUL character is refused before it is made, in a
project with a remote: the server cannot store one. `remote push --seed`
leaves out, by name, a row written earlier that holds one, and sends the rest.

If the server fails on a request (HTTP 500) instead of answering, the batch
is sent again one op at a time, and each failure once more at once. An op is
set aside only when the server fails on it, then answers another op, then
fails on it again: that is about the op, not the server, and it keeps one bad
op from holding back every write after it. The ops that write the same node
(or edge) after it are set aside with it, unsent, so their order holds. After
three ops in a row fail, or none is answered after a failure, the server is
failing: nothing is set aside and every unanswered op waits. What this machine
set aside is listed as "set aside", not "rejected": it is a write the server
never judged. `remote push` sends it again; `--drop-rejected` keeps it.

**Which project.** The log is sent as the project of the database it sits
beside: that `.deciduous/config.toml`'s url and workspace, and that
repository's root commits, also when `DECIDUOUS_DB_PATH` points at a database
outside the current directory. The url is read when the log is sent, not when
the op was written, so fixing a wrong url sends what queued while it was
wrong.

**Crashes.** A write's ops are stored in the database, in a `remote_outbox`
table, in the same transaction as the write, and moved to the log after the
commit. A process that dies between the two leaves them in the outbox, and
the next write (or the next command that opens the database) moves them to
the log, in order, before anything newer. So a write is never made here
without its op.

Each write holds the log's lock (an OS lock on
`remote-log.lock`, released when the process exits however it exits) from
before it changes the database until its op is appended, so the log's order
is the order of the writes and each update's `was` is the value it replaced.
If the lock cannot be had within 30 s, the write is refused and nothing is
written. A last line with no newline is an append that was cut short; the
next append moves it to `.deciduous/remote-log.unreadable` rather than
writing after it. Any other line that is not an entry is reported on every
replay and by `remote status`, is never sent, and is moved to the same file
when the log is compacted. Nothing unreadable is deleted: repair a line that
is a write you want sent and append it to the log again, then delete that
file. `remote status` exits 1 while it has lines in it.

**Local MCP server.** `deciduous mcp` sends the log on a thread of its own,
so no tool call waits on the network. That replay, like the one after a CLI
write, gets 10 s in all, the workspace lookup and every request included;
what it has not sent by then waits (`remote push` waits up to 120 s per
request). When stdin closes, the server gives its last replay 3 s and exits;
anything not sent stays in the log. A write that was made locally but could not be queued makes that
tool call an error that says so. Refusals found by the replay are added to
the next tool result.

## How workspaces are named

The **git repository root's directory name**, lowercased, decided once by
`remote init` and recorded in `.deciduous/config.toml`. The root rather than
the working directory, so running the CLI from a subdirectory cannot split one
project across two workspaces; the main working tree rather than a linked
worktree, so `git worktree add ../repo-feature` writes to `repo` like the
agents in it do. Recorded rather than re-derived, so renaming the directory or
cloning it under another name keeps writing to the same graph. A config
written by 1.0.7 (URL only) gets its name recorded the first time it is used.
The server is asked first (`POST /locate`) which workspace holds this
project's nodes, so a project renamed while on 1.0.7 keeps the graph it wrote
under its old name. Agents are told to read the same `workspace` from
`.deciduous/config.toml`.
Anything outside a git repository pools into `scratch` instead of minting a
workspace per temporary directory.

A workspace belongs to the repository that first wrote to it, identified by
its root commits. Two unrelated repositories that are both called `api` would
derive the same name. The second one's `remote init`, writes, seeds, pulls and
status are refused, and the error says how to name its own workspace. A
repository with no commit yet is refused from a claimed workspace until its
first commit. A shallow clone (CI's `--depth 1`) sends no roots, because its
oldest commit is not the root, and it is not checked. Neither is a 1.0.7 client.

Override the name when the directory name is not what the graph should be
called, or to share one workspace between repositories on purpose:

```bash
deciduous remote init <url> --workspace my-project
```

Setup registers the server with Claude Code at user scope, which is all one
machine needs. To pin a repository so that no tool call can write its nodes
anywhere else, add a header in a committed `.mcp.json`. The header beats the `workspace` argument, on reads
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

## Naming a node

A node has three ids, and the CLI takes all of them wherever it takes a node:

| Id | Looks like | Where you see it |
|----|-----------|------------------|
| local id | `12` (or `#12`) | the ID column of `deciduous nodes`; differs on every machine |
| change_id | `291cdb5a-...`, or 4+ hex characters of it | the CHANGE column; the same in every clone and on the server |
| server id | `56eb8923-...`, or a prefix of it | the `id` every MCP tool returns, and what agents quote |

A reference is tried as a local id and a change_id prefix first. Only when
neither matches, and the project has a `[remote]`, does the CLI ask the server
whether it is a server id, and it prints which node that turned out to be. A
server node this clone has not pulled yet is an error that says to run
`deciduous remote pull`; it is never taken as some other node.

Because a change_id prefix wins, a short server-id prefix that is also the
start of some local node's change_id names that local node, with no note: a
server id `0ccdf488-...` quoted as `0ccd` resolves to a teammate's node whose
change_id is `0ccd1111-...` once that node is imported. Quote server ids in
full, or at least 8 characters, when you link, delete or change status.

Digits are no exception: `89346034` is tried as local id 89346034 first, and
when there is no such node, as a change_id and then a server id prefix. A
zero-padded number such as `0012` is not taken as local id 12 (write `12` or `#12`).
The node arguments of `dot --nodes`, `writeup --roots`, `roadmap link` and
`events emit` take the same references; in `--nodes`, `3-7` is still a range
of local ids.

## Working alongside other agents

Two sessions writing to one workspace need to know the other exists and to
find out when it writes. The branch is the unit for both.

### No locks: every write is recorded, none is refused

Until 1.0.8 every write claimed an advisory lock on `(workspace, branch)` with
a ten-second lease, and a second session on the branch was refused with the
holder's name. It stopped the writer it should never stop and could not stop
the other one:

- A one-shot client (initialize, one call, exit) left its lease behind, and
  its own next process was refused for ten seconds: `locked by battery (0),
  session GNgCXo-N`. A server cannot tell that a session's process is gone.
- The CLI took no lock: its writes are replayed from `.deciduous/remote-log.jsonl`
  through `POST /ops`, possibly days after they were made.

And nothing needs the exclusion. Every write is atomic on its own: a node
insert, an update read `FOR UPDATE`, `capture_conversation_turn` and
`log_decision` in one transaction each, `/ops` compare-and-set field by field.
Two agents on one branch interleave their nodes, as two people on one branch
do, and no write is lost or torn.

So a write now records who wrote where: workspace, branch, session and
clientInfo for MCP; `deciduous CLI` and the repository's root commit for
`/ops`. It never refuses. Pass `branch` on every write so the record says
which one. The workspace setting `lock_scope` no longer does anything.

### check_activity

The read side. Call it before a burst of writes, and after every milestone,
instead of polling `query_nodes`.

| Field | What it holds |
|---|---|
| `sessions` | every session, MCP or CLI, that wrote in the last five minutes (`window_seconds`): `branch`, `client`, `client_version`, `session`, `is_you`, `first_seen_at`, `last_seen_at`, `seconds_ago` |
| `active_sessions` | how many distinct sessions that is |
| `branches` | the twenty most recently written branches: each with `last_node` (id, change_id, node_type, title, status, created_at) and `writers`, the sessions above that wrote it. `branches: N` for more; `branches_total` says how many the workspace has in all |

It used to list unexpired locks, which had always lapsed by the time anyone
looked, so a run of short-lived clients read `active_sessions: 0` throughout,
and CLI writes, which took no lock, never appeared at all.

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
- **Documents are not ops.** `remote push --seed` sends documents attached
  since the last seed, bytes included. Attaching one does not send it.
- **Self-loop edges are rejected** by the server's schema, and reported per
  project rather than dropped quietly.
- **The event stream is advisory.** Postgres does not queue a `NOTIFY` that
  fires while the listener's connection is down, and a reconnect races new
  `LISTEN`s against notifications issued at the same moment. A gap in the
  stream is not proof that nothing happened; call `check_activity` or
  `query_nodes` to catch up.
- **Activity is a record, not a lock.** Two sessions can write one branch at
  once; `check_activity` is how each learns of the other.
- **Documents live in Postgres**, keyed by sha256 and shared across workspaces.
  Five on this machine are referenced by rows whose bytes are gone; they answer
  `410 Gone`, not `404`.

## Deploying the server

See `deciduous_mcp/DEPLOY.md`.
