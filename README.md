# Deciduous

**A decision graph for AI-assisted development.** One graph per repository: what was asked, which approaches were considered, which one was chosen and why, what was changed, what happened. It survives the end of the session, the compaction of the context window and the departure of the person who knew.

[![Crates.io](https://img.shields.io/crates/v/deciduous.svg)](https://crates.io/crates/deciduous)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)

---

## See It In Action

**[Browse the Live Decision Graph](https://deciduous.dev/demo/)** — the graph of building deciduous itself: 1,465 nodes and 1,313 edges in `docs/graph-data.json`, from the export of 2026-09-04

**[Interactive Tutorial](https://deciduous.dev/tutorial/)** — the workflow in 15 minutes

**[Watch the Demo](https://asciinema.org/a/761574)** — a full session

---

## What the graph is

Seven node types, one flow:

```
goal → options → decision → actions → outcomes
```

Goals lead to options; options lead to a decision (which names what it beat); the decision leads to actions; actions lead to outcomes. Observations attach anywhere. A revisit connects an approach that turned out wrong to the one that replaced it. This is what a small one looks like from the CLI:

```
$ deciduous nodes
6 nodes:
ID    CHANGE    TYPE         STATUS     TITLE
------------------------------------------------------------------------------
1     c2c70542  goal         pending    Add rate limiting to the API
2     fa8118b2  option       pending    Redis sliding window
3     c4e588ed  option       pending    In-process token bucket
4     5bcfb238  decision     pending    Redis sliding window over in-process bucket
5     81b03384  action       pending    Add RateLimiter middleware
6     d556d253  outcome      pending    cargo test: 48 passed, 0 failed

$ deciduous show 4
Node #4 decision
Title: Redis sliding window over in-process bucket
Status: pending
Change ID: 5bcfb238-a946-4281-8354-5484b2ec5589

Metadata
  Confidence: 85%
  Branch: main

Connections
  Incoming (2):
    #2 ─[chosen]→ here: survives several API instances
    #3 ─[rejected]→ here: per-process counts diverge behind a load balancer
  Outgoing (1):
    here ─[leads_to]→ #5: implementation
```

Every node has a local integer id and a `change_id` (UUID) that is the same on every machine; edges carry a type (`leads_to`, `requires`, `chosen`, `rejected`, `blocks`, `enables`, `took_from`) and a rationale. Nodes record the branch they were made on, a confidence, the files they touch, the commit they belong to, and for goals the user's prompt verbatim.

## Three ways the graph gets written

1. **The agent, through MCP, as it works.** Claude Code connects to the shared graph server and receives the logging instructions with the connection: a goal when you ask for something (your message verbatim), options when there is more than one way, a decision, an action per change, an outcome with the number or the error. `capture_conversation_turn` writes one step's goal, decision, action and outcome in one call; `add_node` with `parent_id` creates and links one node. See [MCP](#mcp-servers).
2. **The CLI.** `deciduous add`, `link`, `status`, `doc attach`, `archaeology pivot`. The example above was six `add` and six `link` commands. See [The Workflow](#the-workflow).
3. **The transcript, after the fact.** `deciduous transcript import` reads the JSONL transcript Claude Code wrote for a session and derives the graph from it, with evidence on every node. Nothing has to be logged for a session to be in the graph. Next section.

---

## The transcript is the log

An agent that logs its own decisions is an agent that can forget to. The
transcript cannot. Claude Code writes every session to a JSONL file under
`~/.claude/projects/<slug>/`; since 1.1.0, `deciduous transcript import`
derives the graph from it: every prompt a goal (verbatim), every edit run and
`git commit` an action naming the files, every build, test or lint run an
outcome with its exit and summary, the assistant's closing report an outcome
too.

```bash
deciduous transcript list              # this project's sessions, and which are imported
deciduous transcript import 328f8485   # by session id prefix, or a path to the .jsonl
```

```
Imported Remote postgres code upstream (328f8485, 68 prompts)
  goals 68 · actions 119 · outcomes 67 proven, 41 inferred · observations 7 · edges 301
  50 files hashed · 52 deciduous MCP writes skipped (already in the graph)
```

That session was three weeks of work. The agent had logged nine nodes through
MCP while it ran.

Every derived node says what it is backed by. `metadata.evidence.kind` is
`proven` (a tool call and its result, or your own words, with the transcript
uuids) or `inferred` (the assistant said so). The assistant's "all tests pass"
and the `cargo test` result that says `12 passed, 1 failed` sit side by side,
labelled, and `deciduous show` prints which is which:

```
$ deciduous show 42
Node #42 outcome
Title: cargo test passed
Description: 212 passed, 0 failed
  Evidence: proven transcript, session 328f8485, 1 tool call(s), exit ok

$ deciduous show 67
Node #67 outcome
Title: Both built, tested against the live server, and shipped.
  Evidence: inferred transcript, session 328f8485
```

Each action lists the files the agent `Read` before it (`metadata.consulted`)
and carries git blob hashes of the files it touched (`metadata.file_hashes`,
the id `git log --find-object` takes). Importing the same transcript twice
adds nothing, on any machine: change ids are derived from the session and
entry uuids, so two people importing one transcript get the same nodes.

What is not derived: decisions and options (a choice between approaches is a
judgement about prose; that pass belongs to an agent reading the imported
graph), subagent transcripts (counted and skipped), and anything but Claude
Code's format. The web viewer does not yet show evidence or hashes; `show`
does. [docs/transcripts.md](docs/transcripts.md) has the full derivation table
and a `SessionEnd` hook that imports at the end of every session.

---

## Several agents, one graph

Ten Claude Code sessions, started at the same second in ten git worktrees of one repository, each building a Tetris from nothing, all logging to one shared deciduous workspace and told to read each other's code and reasoning. Twenty-six minutes later: ten playable games, 386 graph nodes, 170 borrowed ideas with provenance. One finding (some SRS kicks lift a piece two rows, so two hidden rows refuse a legal rotation at the ceiling) reached eight of the nine other branches in eighteen minutes.

**[The arena write-up](https://notactuallytreyanastasio.github.io/tetris-arena/)** · **[the repository](https://github.com/notactuallytreyanastasio/tetris-arena)** · **[how it works](https://deciduous.dev/remote.html#alongside)**

What made it work is one server, reachable over MCP from every directory, with the branch as the unit of coordination:

- **Activity.** Every write records who wrote which branch; none is refused because someone else is writing. `check_activity` lists who wrote in the last five minutes and the last node on each recent branch.
- **Events.** Postgres triggers push every write over a WebSocket as it lands. `deciduous remote watch` prints one quoted line per write; Claude Code's `Monitor` tool can sit on the same URL.
- **Provenance.** A `took_from` edge records a borrow across branches; `log_observation` writes the observation and the edge in one call. In the arena the borrows lived in observation titles and had to be recovered with a regular expression, which is why the edge type exists.
- **The message board.** Agents post interface changes, questions and answers with `post_message`, address each other with `@label`, answer with `reply_to`, and read what is waiting for them with `read_messages` `unanswered_for`. See [The Message Board](#the-message-board).

---

## First-time setup

Every project writes to a shared graph server, and `deciduous init` does not
finish until the project points at one that answers.

**1. Install the binary**, and PostgreSQL if the graph will live on this machine:

```bash
brew install notactuallytreyanastasio/tap/deciduous   # Homebrew, macOS or Linux
# or
cargo install deciduous --locked                      # needs a Rust toolchain

deciduous --version
which -a deciduous    # with both installed, the first one on PATH runs
```

A graph on this machine is kept in PostgreSQL on localhost:5432 by default:
the one you already have, or `brew install postgresql@17 && brew services start
postgresql@17`, or your distribution's package. Any other PostgreSQL works if
you give its URL. Docker is optional: it can run PostgreSQL and the server for
you instead (choice 3 below).

**2. Set up a project:**

```bash
cd your-project
deciduous init            # Claude Code; --opencode, --windsurf or --both for others
```

`init` writes the slash commands, skills, hooks and a `CLAUDE.md` section. Then,
if the project has no server yet, it asks where its graph should live:

```
   This project has no server yet ([remote] in .deciduous/config.toml).

Where should this project's graph live?

  1) This machine: the deciduous server as a background service, using PostgreSQL on
     localhost:5432 as you (answering now)
  2) This machine: the server as a background service, using a PostgreSQL you name
     (any host, port, user, password)
  3) This machine, in Docker: a new PostgreSQL and the server in containers
  4) A server someone else runs: you need its URL and token

Choose 1-4 [1]:

The server needs a port on this machine. 24871 is free right now; anything
already using the port you pick (a dev server on 4000, say) would stop this
server from starting.

Port [24871]:
```

- **1 and 2.** The first time on a machine, `init` downloads the server
  executable for its own version (one file with Erlang and Elixir inside),
  checks it against the release's checksums, and runs it once to create the
  database (`deciduous` for choice 1) and apply migrations. If PostgreSQL
  refuses (no such role, wrong password, no CREATEDB), you see its reason and
  the menu again. Then it installs the server under
  `~/.config/deciduous/server/` as a user service that starts at login: a
  LaunchAgent on macOS, a systemd user unit on Linux. It listens on
  127.0.0.1 only. Every later project finds the running server in about a
  second. For choice 2 the password is asked separately and not shown.
- **3.** Downloads the Docker bundle for its own version, checks it, and
  starts PostgreSQL 17 and the server with Docker (a few minutes). This is
  what 1.0.10 and earlier always did, and existing Docker installs stay as
  they are.
- **4.** Asks for the URL and the token (typed without being shown), and checks
  them against the server before storing anything.

Either way it stores the token in `~/.config/deciduous/credentials` (mode
0600), writes `[remote]` to `.deciduous/config.toml`, and registers the server
with Claude Code. `deciduous update` asks the same question in a project that
has no server, and upgrades this machine's server executable when it is older
than the CLI. `deciduous remote setup` asks it on its own. In a script there
is no terminal to ask, so give the answer instead:

```bash
deciduous remote setup --local                  # 1: PostgreSQL on localhost:5432 as $USER
deciduous remote setup --database-url postgres://USER:PASSWORD@HOST:5432/deciduous   # 2
deciduous remote setup --docker-postgres        # 3
deciduous remote setup --docker                 # the server in Docker, PostgreSQL from 1 or 2
deciduous remote setup --url <url>              # 4
```

`DATABASE_URL` in your environment is never used unless you pass it with
`--database-url`. Commit the project files (by path, not `git add -A`) so
every clone gets them. `DECIDUOUS_NO_SERVER=1` skips the server step, for
tests and CI.

**3. Restart Claude Code.** When it connects, the server sends the logging
instructions. There is no hook to install. Agents write through the MCP
tools. `deciduous add` from the CLI writes to the local database and queues
the change in `.deciduous/remote-log.jsonl`, which is sent to the server
before the command exits (or, if the server is down, on the next write or
`deciduous remote push`).

If anything is missing (no PostgreSQL, a server that does not answer, no token),
`init` and `update` stop with exit 1 and say how to fix it. Upgrading later is
`brew upgrade deciduous` or the same `cargo install` line, then
`deciduous update` in each project (`deciduous update --all ~/code` for many).

**4. See several agents work together** (macOS, in iTerm2 or Ghostty):

```bash
deciduous demo-swarm --preview   # the walkthrough, in this terminal; nothing is started
deciduous demo-swarm --dry-run   # builds the arena and the window, starts no agents
deciduous demo-swarm             # one Opus lead and four Sonnet workers on one graph
```

`demo-swarm` is hidden from `--help`. After `deciduous update`, Claude Code
also has it as `/demo-swarm`.

Written for the agent doing the setup rather than for you:
[deciduous.dev/agents/](https://deciduous.dev/agents/index.md) and
[deciduous.dev/llms.txt](https://deciduous.dev/llms.txt).

---

## Installation

### Homebrew

```bash
brew tap notactuallytreyanastasio/tap
brew install deciduous
```

### Pre-built binaries

From [GitHub Releases](https://github.com/notactuallytreyanastasio/deciduous/releases):

| Platform | Binary |
|----------|--------|
| Linux (x86_64) | `deciduous-linux-amd64` |
| Linux (ARM64) | `deciduous-linux-arm64` |
| macOS (Intel) | `deciduous-darwin-amd64` |
| macOS (Apple Silicon) | `deciduous-darwin-arm64` |
| Windows | `deciduous-windows-amd64.exe` |

```bash
curl -LO https://github.com/notactuallytreyanastasio/deciduous/releases/latest/download/deciduous-darwin-arm64
chmod +x deciduous-darwin-arm64
sudo mv deciduous-darwin-arm64 /usr/local/bin/deciduous
```

### Cargo, or from source

```bash
cargo install deciduous --locked

git clone https://github.com/notactuallytreyanastasio/deciduous.git
cd deciduous && cargo build --release     # target/release/deciduous; toolchain pinned by rust-toolchain.toml
```

### Self-host the shared graph server

The server needs a PostgreSQL. On a developer machine you do not install it by
hand: `deciduous init` (or `deciduous remote setup`) downloads the release's
`deciduous-mcp-*` executable, one file with Erlang/OTP and Elixir inside, and
runs it as a user service against the PostgreSQL on localhost:5432. The server
creates its database if it is missing and migrates at every start, so a
machine with PostgreSQL needs nothing else.

For a server a team shares, run the same executable on the host, against the
PostgreSQL there or any other by URL. The [deployment
guide](deciduous_mcp/DEPLOY.md) walks through the settings file, the service
unit, TLS to the database, a reverse proxy, backups and upgrades.

Without a PostgreSQL to point at, Docker can run one. Download and extract
`deciduous-mcp-docker.tar.gz` from the same release, then:

```bash
cd deciduous-mcp-docker
./scripts/setup.sh
```

Setup creates private credentials, starts PostgreSQL 17 with persistent
storage, applies migrations, and waits until the server can query the
database. It picks a free port in 20000-32767 unless `DECIDUOUS_PORT` says
otherwise (4000 is the port inside the container only), and prints the
resulting MCP endpoint; the port and bearer token are in `.env`. Running setup
again keeps the same credentials, port and database. `deciduous remote setup
--docker-postgres` does all of this for you.

---

## Where the graph lives

| File | Role |
|------|------|
| `.deciduous/config.toml` | `[remote]` names the server URL and workspace; `[branch]` controls branch tagging |
| `.deciduous/deciduous.db` | SQLite, ignored by git. With a `[remote]`, this machine's cache of the server's graph; without one, the graph itself |
| `.deciduous/remote-log.jsonl` | CLI writes waiting for the server, one op each; `deciduous remote push` replays them, each applied at most once |
| `.deciduous/graph.json` | The git-tracked graph for a project with no server: `deciduous sync` reconciles it with the database both ways, `init` registers a merge driver that merges it record by record. A project with a `[remote]` has no graph.json to commit, and `sync` is not its step |
| `.deciduous/documents/` | Attached files, content-hash named |
| `~/.config/deciduous/credentials` | The server token, mode 0600, never in a repository |

The server holds one workspace per repository; a git worktree uses its main checkout's. `deciduous remote status` lists what differs between this copy and the server, field by field, and exits 1 if anything does; `deciduous remote pull` sends what is waiting and refreshes the cache.

---

## The Workflow

```
BEFORE you do something → Log what you're ABOUT to do
AFTER it succeeds/fails → Log the outcome
CONNECT immediately    → Link every node to its parent
```

### Example session, from the CLI

```bash
deciduous add goal "Add rate limiting to the API" -c 90 --prompt-stdin < prompt.txt   # the user's words, verbatim
deciduous add option "Redis sliding window" -c 80
deciduous add option "In-process token bucket" -c 70
deciduous link 1 2 -r "possible approach"
deciduous link 1 3 -r "possible approach"

deciduous add decision "Redis sliding window over in-process bucket" -c 85
deciduous link 2 4 -t chosen -r "survives several API instances"
deciduous link 3 4 -t rejected -r "per-process counts diverge behind a load balancer"

deciduous add action "Add RateLimiter middleware" -c 85 -f src/middleware/rate_limit.rs
deciduous link 4 5 -r implementation
deciduous doc attach 5 docs/rate-limit.png -d "request path with the limiter"

deciduous add outcome "cargo test: 48 passed, 0 failed" -c 95 --commit HEAD
deciduous link 5 6
```

`add` warns when a prompt looks like a summary rather than the user's message; `--prompt-stdin` takes a long one. `--commit` stores the full hash and refuses a rev git cannot resolve.

### When an approach turns out wrong

`archaeology pivot` writes the observation, the revisit and the new decision, links them, and marks the old approach superseded, in one transaction:

```
$ deciduous archaeology pivot 4 "Redis adds a network hop on every request; p99 rose 8 ms" \
    "Local token bucket with Redis for the shared quota" -c 80 --dry-run
=== PIVOT (dry run) ===

  ?? [observation] "Redis adds a network hop on every request; p99 rose 8 ms"
    └── linked from #4 "Redis sliding window over in-process bucket"
  ?? [revisit] "Reconsidering: Redis sliding window over in-process bucket"
    └── linked from ?? [observation]
  ?? [decision] "Local token bucket with Redis for the shared quota"
    └── linked from ?? [revisit]

  Superseded: #4 "Redis sliding window over in-process bucket" -> status: superseded
```

### Types, statuses, edges

| Node type | Example |
|-----------|---------|
| `goal` | "Add rate limiting to the API" |
| `option` | "Redis sliding window" |
| `decision` | "Redis sliding window over in-process bucket" |
| `action` | "Add RateLimiter middleware" |
| `outcome` | "cargo test: 48 passed, 0 failed" |
| `observation` | "p99 rose 8 ms" |
| `revisit` | "Reconsidering: Redis sliding window" |

Statuses: `pending`, `active`, `completed`, `rejected`, `superseded`, `abandoned` (`deciduous status <id> <status>`). `active` is current truth; `superseded` and `abandoned` are history that stays queryable.

Edge types: `leads_to` (the default), `requires`, `chosen`, `rejected`, `blocks`, `enables`, `took_from` (this node borrowed from that one, usually across branches).

### Documents

```bash
deciduous doc attach 1 docs/architecture.png -d "System architecture"
deciduous doc attach 1 screenshot.png --ai-describe    # description from the claude CLI
deciduous doc list 1
deciduous doc open 3
deciduous doc detach 3          # soft-delete
deciduous doc gc --dry-run      # orphaned files that gc would remove
```

Files go in `.deciduous/documents/` under their content hash, so the same file attached twice is stored once. The web viewer shows them in the node's detail panel.

### Recovering context

```bash
deciduous pulse            # active state, gaps, recent nodes
deciduous nodes -b <branch>
deciduous show <id>
deciduous commands         # what the CLI did recently
```

Or ask the graph in plain English from the web viewer's Q&A panel, or through the shared server's `ask_graph` tool.

---

## MCP servers

There are two.

**The shared server** (Elixir, PostgreSQL, over HTTP) is what `deciduous init` sets up and registers with Claude Code. Its 21 tools are where the multi-agent features live: `capture_conversation_turn`, `add_node` (with `parent_id`), `add_edge`, `update_node`, `delete_node`, `delete_edge`, `log_decision`, `log_observation` (with `took_from`), `close_thread`, `show_node`, `query_nodes`, `get_graph`, `get_ancestors`, `get_descendants`, `find_orphans`, `ask_graph`, `consolidation_report`, `check_activity`, `list_workspaces`, `post_message`, `read_messages`. It sends the logging instructions in the `initialize` result, so every client gets them with no project configuration. Reads accept `workspace: "*"` for the view across every project; writes refuse it. [docs/remote-graph.md](docs/remote-graph.md) and [deciduous.dev/remote.html](https://deciduous.dev/remote.html).

**The local server** (`deciduous mcp`, stdio) exposes this project's database with 34 tools, for a client that has no shared server, or for Claude Desktop:

```bash
claude mcp add deciduous -- deciduous mcp              # Claude Code, this project
claude mcp add deciduous --scope user -- deciduous mcp  # every project
```

Claude Code reads MCP servers from `claude mcp add` or a project `.mcp.json`, not from `.claude/settings.local.json`. For Claude Desktop, in `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS) or `%APPDATA%\Claude\claude_desktop_config.json` (Windows):

```json
{
  "mcpServers": {
    "deciduous": {
      "command": "/Users/you/.cargo/bin/deciduous",
      "args": ["mcp"],
      "env": { "DECIDUOUS_DB_PATH": "/path/to/your/project/.deciduous/deciduous.db" }
    }
  }
}
```

Use the absolute path to the binary (`which deciduous`); Claude Desktop does not inherit your shell `PATH`.

| Category | Tools |
|----------|-------|
| Graph | `add_node`, `link_nodes`, `unlink_nodes`, `delete_node`, `update_status`, `update_prompt` |
| Query | `list_nodes`, `search_nodes`, `show_node`, `list_edges`, `get_graph` |
| Analysis | `trace_chain`, `get_node_context`, `get_pulse`, `find_orphans`, `get_timeline`, `get_branch_summary` |
| Sessions | `start_session`, `end_session`, `resume_session`, `get_session`, `list_sessions` (kept in a file next to the database, so a session survives a server restart) |
| Documents, themes | `attach_document`, `list_documents`, `create_theme`, `list_themes`, `tag_node`, `untag_node` |
| Export, sync | `export_dot`, `generate_writeup`, `sync`, `sync_status` |
| Message board | `post_message`, `read_messages` |

Writes made through `deciduous mcp` in a project with a `[remote]` are queued for the server like CLI writes. [deciduous.dev/mcp.html](https://deciduous.dev/mcp.html) has the tool reference.

---

## The Message Board

When more than one agent works at once, they coordinate through deciduous, never through a scratch or markdown file.

```bash
$ deciduous board post --as lead -s "port for the api?" -m "@w1 @w2 which port, 4000?"
Posted #1, to w1, w2
$ deciduous board post --as w1 -s "re port" -m "4000. @lead" --reply-to 1
Posted #2 (reply to #1), to lead
$ deciduous board read --unanswered w2
#1  lead -> w1, w2  2026-09-26T17:39:56.706026Z
    port for the api?
    | @w1 @w2 which port, 4000?
$ deciduous board read --unanswered w1
Nothing addressed to w1 is waiting for an answer.
```

`@label` anywhere in the subject or body addresses a message (an email address is not a mention). **Unanswered** means: mentions the label, and no message by that label has `reply_to` its id. Read it at start, before touching a file another agent may be changing, and before finishing.

Where it lives: on the server when the project has a `[remote]` (server 1.0.9 or later; an older one is an error that says so, never a quiet fall back); otherwise in the `agent_messages` table of the **main worktree's** `.deciduous/deciduous.db`, so every worktree of a repository shares one board. Never in `graph.json`, never exported, never synced.

Set `DECIDUOUS_AGENT_LABEL` in a session's environment and `board post` uses it for `--as`, and the `board-mentions.sh` hook `init` installs shows that label's unanswered messages at session start and as new ones arrive. Without the variable the hook prints nothing.

---

## Viewing the graph

```bash
deciduous serve --port 3000      # web viewer
deciduous dot --auto --png       # docs/decision-graph-<branch>.png (needs graphviz)
deciduous writeup --auto -t "My PR" --nodes 1-11   # PR body with the graph embedded
```

The viewer has six views (DAG, Chains, Timeline, Graph, Roadmap, Story), branch filtering, search, and a Q&A panel: `POST /api/ask` sends your question with the relevant nodes and edges to the `claude` CLI, and every answer is kept and searchable (`GET /api/qa/search?q=auth`). It does not yet show the evidence kind or file hashes a transcript import writes; `deciduous show` does.

`dot --auto` and `writeup --auto` name the file after the branch, so two open PRs do not fight over one PNG. The `cleanup-decision-graphs.yml` workflow `init` writes removes the PNGs after a PR merges.

---

## Skills: pulse, narratives, archaeology, decision-graph

`init` installs three skills and a command for working on history:

- **/pulse** maps the current design as decisions: the agent reads the code, names the design questions it answers, and logs them as goals and options. `deciduous pulse` is the CLI view of the graph's active state.
- **/narratives** writes `.deciduous/narratives.md`: how a subsystem evolved, where it pivoted, what it connects to. `deciduous narratives init` seeds it from the active goals; `narratives pivots` lists every revisit with its chain.
- **/archaeology** turns narratives into graph structure: observation → revisit → new decision, old decision superseded. `deciduous archaeology pivot` is the one-command form shown above; `archaeology supersede --cascade` and `archaeology timeline` go with it.
- **/decision-graph** builds a graph for a repository that was not using deciduous from the start, from commit history, code structure and (with `gh`) PR descriptions and review threads.

---

## Keeping the integration updated

```bash
deciduous check-update     # exit 1 if .deciduous/.version is behind the binary
deciduous update           # this project; --all <dir> for every project under it
```

`update` auto-detects which assistants are installed (`.claude/`, `.opencode/`, `.windsurf/`) and rewrites what deciduous wrote: the slash commands (`/decision`, `/recover`, `/work`, `/document`, `/build-test`, `/serve-ui`, `/decision-graph`, `/sync`, `/demo-swarm`), the skills, `version-check.sh` and `board-mentions.sh`, `.claude/agents.toml`, and the deciduous section of `CLAUDE.md` or `AGENTS.md`; for Windsurf, `.windsurf/hooks.json`, `.windsurf/hooks/version-check.sh` and `.windsurf/rules/deciduous.md`; for OpenCode, `.opencode/plugins/`, `commands/`, `agents/`, `tools/` and `opencode.json`.

**Kept:** anything you wrote. A section or command file you edited is left alone on every project, and the template it would have written goes to `.deciduous/update-templates/<path>` for you to `diff`. Everything `update` changes is first copied to `.deciduous/update-backups/<time>/`. In `.claude/settings.json` it only removes the logging hooks earlier versions installed and adds the board-mentions entries if missing. It does not touch `.deciduous/config.toml` or `docs/`; with a `[remote]` it also leaves `.gitignore`, `.gitattributes`, `.git/config` and `graph.json` alone. A project with a `[remote]` gets templates that say `remote pull` and `remote status` instead of `sync`.

`version-check.sh` asks crates.io for the latest version once per 24 hours with a 3-second timeout, caches it in `.deciduous/.latest_version`, and never blocks. `deciduous auto-update off` turns it off.

## Why there are no logging hooks

Up to 1.0.2, deciduous installed a hook that denied an agent's next tool call
until it wrote to the graph. It could leave a session unable to do anything:
a graph write sent beside the next action did not count, a session whose
settings changed underneath it could not reset the count, and an environment
variable typed into the session never reached the hook. 1.0.3 removed it, and
`deciduous update` takes it out of projects that have it. The hooks left are
the once-a-day version check and, since 1.0.9, the board-mentions hook, which
prints a labelled session's unanswered messages. Neither ever blocks.

Logging is encouraged by what the agent reads anyway: the `CLAUDE.md` section
and the instructions the server sends on every connection. And since 1.1.0 the
transcript import means a session that logged nothing is still in the graph.

---

## Environment variables

| Variable | Effect |
|----------|--------|
| `DECIDUOUS_DB_PATH` | Use this SQLite file instead of walking up to `.deciduous/deciduous.db` (worktrees, Claude Desktop) |
| `DECIDUOUS_MCP_TOKEN` | The shared server's bearer token, for scripts; `remote login` stores it in `~/.config/deciduous/credentials` instead |
| `DECIDUOUS_AGENT_LABEL` | This session's label on the message board; `board post --as` default, and what `board-mentions.sh` reads for |
| `DECIDUOUS_PORT` | Port for this machine's server when it is first set up (otherwise asked, a free one suggested) |
| `DECIDUOUS_NO_SERVER` | `init` and `update` skip the server step (tests, CI) |
| `DECIDUOUS_QUIET_LOCAL_ONLY` | Silence the one-line stderr note a write prints when no server is configured |
| `DECIDUOUS_OPS_TIMEOUT` | Per-request timeout when replaying queued writes to the server |

---

## The Premises

1. **Decisions are the unit of institutional knowledge.** Code tells you *what*, but decisions tell you *why*. Six months from now, you won't remember why you chose Redis over Postgres for that cache. The graph will.

2. **Structured thinking produces better outcomes.** The act of logging a decision—naming it, assigning confidence, connecting it to goals—forces you to think it through.

3. **Real-time capture beats retroactive documentation.** By the time you write post-hoc docs, you've forgotten the options you rejected. The transcript is captured in the moment too, which is why deriving the graph from it works.

4. **Graphs beat documents.** Goals spawn decisions, decisions spawn actions, actions produce outcomes. A graph captures these relationships. You can trace any outcome to its origin.

5. **Complex PRs tell a story.** A 50-file diff is incomprehensible. A decision graph shows the goal, the key decisions, the rejected approaches, and how each change connects to purpose.

6. **Context loss is inevitable.** Sessions end. Memory compacts. The graph survives.

7. **The graph is a shared workspace.** Decisions flow between sessions, between humans and AI, between teammates. The graph doesn't care who's typing—it preserves the reasoning.

---

## Commands Reference

Every command and flag below is in `deciduous --help` for 1.1.0.

```bash
# Set up
deciduous init [--claude] [--opencode] [--windsurf] [--both] [--no-auto-update]
deciduous update [--all <dir>]            # update integration files; keeps what you wrote
deciduous check-update                    # exit 1 if integration files are behind the binary
deciduous auto-update on|off              # the once-a-day crates.io check
deciduous integration                     # Claude Code hooks, commands and skills present?
deciduous completion bash|zsh|fish|powershell|elvish

# Write
deciduous add <goal|option|decision|action|outcome|observation|revisit> "Title"
    -d "description" -c <0-100> -p "prompt" | --prompt-stdin
    -f "a.rs,b.rs" --commit <HEAD|branch|tag|hash> -b <branch> | --no-branch
    --date "YYYY-MM-DD"                   # backdate (archaeology)
deciduous link <from> <to> -r "rationale" -t leads_to|requires|chosen|rejected|blocks|enables|took_from
deciduous unlink <from> <to> [-t <type>]  # -t required when two edges join the nodes
deciduous delete <id> [--dry-run]
deciduous status <id> pending|active|completed|rejected|superseded|abandoned
deciduous prompt <id> ["prompt"]          # stdin when omitted
# <id> anywhere is a local id, a change_id prefix, or a server id

# Read
deciduous nodes [-b <branch>] [-t <type>] [--theme <name>]
deciduous edges
deciduous show <id> [--json]              # prints evidence for transcript-derived nodes
deciduous graph                           # full graph as JSON
deciduous pulse [-b <branch>] [-r <n>] [--summary] [--json]
deciduous commands [-l <n>]               # recent command log

# From a Claude Code transcript (docs/transcripts.md)
deciduous transcript list [--dir <dir>] [--json]
deciduous transcript import <path|session-id>... [--dry-run] [--no-hash] [--json]

# History
deciduous archaeology pivot <from_id> "observation" "new approach" [-c <n>] [-r "reason"] [--dry-run]
deciduous archaeology supersede <id> [--cascade] [--dry-run]
deciduous archaeology timeline [-l <n>] [-t <type>] [-b <branch>] [--json]
deciduous narratives init [-o <path>] [--force]
deciduous narratives show [-p <path>]
deciduous narratives pivots [-b <branch>] [--json]
deciduous audit [--associate-commits] [--min-score <0-100>] [--dry-run] [--yes]

# Documents
deciduous doc attach <node_id> <file> [-d "..."] [--ai-describe]
deciduous doc list [<node_id>] [--include-detached] [--json]
deciduous doc show <doc_id> [--json]
deciduous doc describe <doc_id> ["text"] [--ai]
deciduous doc open <doc_id>
deciduous doc detach <doc_id>
deciduous doc gc [--dry-run]

# Themes
deciduous themes create <name> [-c "#hex"] [-d "..."]
deciduous themes list
deciduous themes delete <name>
deciduous tag add|remove <node_id> <theme>
deciduous tag list <node_id>
deciduous tag suggest [<node_id>] [--apply]
deciduous tag confirm <node_id> <theme>

# Message board (coordination, not graph)
deciduous board post --as <label> -s "subject" [-m "body"] [--reply-to <id>] [--branch <b>] [--json]
deciduous board read [--since <id>] [--from <label>] [--to <label>] [--unanswered <label>] [-q text] [--limit <n>] [--json]
deciduous board show <id> [--json]

# Shared graph server
deciduous remote setup [--local | --url <url> | --database-url <url> | --docker-postgres | --docker] [--port <n>]
deciduous remote login [--url <url>]      # token on stdin, stored mode 0600
deciduous remote logout
deciduous remote init <url> [--workspace <name>]
deciduous remote status                   # what differs, field by field; exit 1 if anything
deciduous remote push [--seed] [--repair] [--overwrite-server] [--overwrite] [--drop-rejected] [--retry-rejected] [--drop <op_id>]
deciduous remote pull
deciduous remote adopt --url <url> [<root>] [-n]
deciduous remote watch [--types a,b] [--branch <b>]... [--edges] [--json] [--url | --claude-code]

# Local graph file and viewer
deciduous sync [--check]                  # reconcile .deciduous/graph.json with the database; --check exits 1 if pending
deciduous backup [-o <path>]
deciduous migrate                         # add change_id columns to a pre-sync database
deciduous serve [-p <port>]
deciduous serve --api --data-dir <dir> --token <t> [--bind <addr>]   # multi-graph JSON API daemon
deciduous dot [-o <file> | --auto] [-r <ids>] [-n <spec>] [--png] [-t "title"] [--rankdir TB|LR|BT|RL]
deciduous writeup [-t "title"] [-r <ids>] [-n <spec>] [-o <file>] [--png <file> | --auto] [--no-dot] [--no-test-plan]
deciduous mcp                             # the local stdio MCP server

# Integrations
deciduous hooks install|status|uninstall  # hooks defined under [hooks] in .deciduous/config.toml
deciduous opencode install|status|uninstall
deciduous roadmap init|refresh|sync|list|link|unlink|conflicts|status|check   # ROADMAP.md <-> GitHub Issues
```

---

## Who Uses Deciduous

**You, the developer:**
- Think a decision through by having to name what it beat
- Remember why you made a choice months later
- Review a PR by its goal, decisions and rejected options, not only its diff
- Import the transcript of a session you did not watch and see what the agent did, with what evidence

**Your AI assistant:**
- Recover context after compaction or a session boundary (`/recover`, `query_nodes`, `ask_graph`)
- Build on previous reasoning instead of starting fresh
- Coordinate with the other agents on the same repository through the board, and borrow with `took_from`
- Use `/pulse`, `/narratives`, `/archaeology` and `/decision-graph` to map and correct history

**Your team:**
- One graph per repository on a shared server, written from every clone, worktree and agent
- A git-tracked `.deciduous/graph.json` for a project with no server, merged record by record
- Decision context that survives turnover

---

## Why "deciduous"?

It almost has the word "decision" in it, and they're trees.

---

**[Tutorial](https://deciduous.dev/tutorial/)** · **[Live Demo](https://deciduous.dev/demo/)** · **[GitHub](https://github.com/notactuallytreyanastasio/deciduous)**
