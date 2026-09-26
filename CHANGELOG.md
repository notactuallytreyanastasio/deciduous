# Changelog

## [1.0.9] - 2026-09-26

Agents running in parallel had nowhere in deciduous to talk. They wrote to a scratch markdown file, which nobody could query, which worktrees did not share, and which vanished with the session. 1.0.9 gives them a message board in the same store as the graph.

### Added
- **Message board.** `post_message` and `read_messages`, with identical schemas on the Elixir server and the stdio server; `deciduous board post|read|show [--json]` on the CLI. `@label` in a subject or body addresses the message (emails are not mentions); `reply_to` answers one. `read_messages` filters by `since_id`, `author`, `to`, `unanswered_for` (mentions of a label it has not replied to), full-text `query`, `branch` and `id`, ascending, at most 200 per call.
- **Where it lives.** With a `[remote]`, in the server's Postgres (`agent_messages`, one migration; a reply into another workspace is refused by a foreign key). Without one, in the main worktree's SQLite database, so every worktree of a repository reads the same board, also when a worktree sets `DECIDUOUS_DB_PATH`. Messages are coordination, not decisions: they are never in `graph.json`, `docs/graph-data.json`, `graph` or `dot`.
- **Server HTTP.** `POST /messages` and `GET /messages`, with the same auth and workspace pinning as `/export`. Posts are `message_posted` events on `/events`, and `deciduous remote watch` prints them.
- **`board-mentions.sh` hook** on SessionStart and UserPromptSubmit. With `DECIDUOUS_AGENT_LABEL` set, it shows that label's unanswered messages at start and only newer ones after; without it, or with nothing waiting, it prints nothing. `update` adds its entries to `.claude/settings.json` only if missing and keeps yours.

### Changed
- **Every template teaches the board.** CLAUDE.md, `/decision`, `/work`, `/recover` (which now reads what is unanswered for you), `/sync`, `/decision-graph`, the pulse, narratives and archaeology skills, `agents.toml`, the Windsurf rules, every OpenCode command, skill and agent, and `/demo-swarm` (the board is the record; direct messages are for interrupts). A test fails if any of them stops mentioning it.

### Not done
- The local `query` filter is a case-insensitive substring match; the server's is stemmed full-text search.
- Messages are never edited, deleted or pruned.
- The hook reaches separate sessions (swarm panes, `DECIDUOUS_AGENT_LABEL=w1 claude`), not Task subagents, which get the rule from their instructions. It is silent when the `deciduous` on PATH predates 1.0.9.
- With a `[remote]`, the board needs a 1.0.9 server; an older one answers 404 and the CLI says so instead of writing locally.

## [1.0.8] - 2026-09-23

1.0.7 sent a CLI write to the server as whole nodes: the ones the server lacked, plus the one the command touched. `status` put back an agent's title every time it changed a status, an edit made offline to a node the server already had was never sent by `remote push`, and deletes and unlinks were not sent at all. 1.0.8 sends each write as the write it was.

### Changed
- **CLI writes go through a log.** Every write the CLI makes to its database is appended to `.deciduous/remote-log.jsonl` (ignored by git) and replayed on the server through `POST /ops`, one field at a time, each op at most once. An edit made while the server is down waits and goes with the next write or `deciduous remote push`. An update carries the value it replaced, and the server applies it only over that value, so a queued edit never overwrites a newer agent edit to the same field; it is refused by name instead. `delete`, `unlink`, archaeology `pivot` and `supersede`, `commit`, and writes through `deciduous mcp` all reach the server. `deciduous serve` still sends its writes only with the next CLI write.
- **`remote status` compares content.** It lists nodes and edges only here, only on the server, deleted on the server, and differing field by field, with the queue and anything refused, says which command fixes each, and exits 1 when anything differs. `remote push --seed` sends history the log never had (documents with their bytes); `remote push --repair` sends this copy's fields for nodes that differ; `--drop-rejected` discards refused writes.
- **A workspace belongs to its repository.** The server records the root commits of the first repository to write to a workspace and refuses an unrelated one, on `/ops`, `/import`, `/export` and `/claim`. `remote init` records the workspace in `.deciduous/config.toml`; a worktree uses its main repository's; a project renamed since 1.0.7 finds the workspace that holds its nodes (`POST /locate`). A shallow clone sends no roots and is not checked. A 1.0.7 client is not checked either, so it is not locked out of its own workspace.

### Fixed
- **Server deletes reach the local graph.** `/export` carries deleted nodes as tombstones with no content. `remote pull` deletes them locally, even when the node was edited here after the delete (the server refuses that edit), empties the local tombstone, and drops refused writes that touch it. `/import` and `--seed` no longer write into deleted rows.
- **The server keeps a pinned client in its workspace** on every route that names one (`/mcp` tools by id and by name, `/import`, `/ops`, `/claim`, `/locate`, `/export`, `/documents`), including a pin to a workspace that does not exist yet, and answers a deleted node in another workspace like a live one.
- **Deleted nodes are gone from every tool**: reads, writes, traversal, `find_orphans` (which also finds a cycle stranded by a delete), `list_workspaces` counts and document downloads.
- **Bad input gets a sentence, not a 500.** Every tool's advertised schema is enforced with bounded strings; `/import` checks types, NUL and metadata before writing anything; malformed JSON-RPC gets the answer the protocol prescribes under the request's own id; a notification with bad params is dropped. `*` cannot be created as a workspace, reads never create one, and neither does a write that fails. `update_node` merges metadata. `close_thread` is all or nothing and stays in its workspace. Search terms are literal.
- **graph.json keeps every edit.** Fields a newer version wrote survive every write; an edit to a future-dated node or under a skewed clock is stamped after the version it replaces; a relinked edge's rationale reaches clones; a failed merge driver is noticed via `MERGE_HEAD` and finished by `sync`; a merge whose ancestor will not parse is refused rather than decided by timestamp; `remote pull` merges into graph.json instead of overwriting it. An edit is refused while graph.json cannot be read, instead of being reverted by the next sync. `sync --check` fails while edges wait or records are unreadable.
- **A digit-only change_id prefix is a change_id**, also over MCP, and an id that does not fit in 32 bits is refused instead of wrapped onto another node.
- **SQLite and graph.json hold under load.** SQLite waits for its write lock in WAL mode; every graph.json write happens under a file lock against the file as it is now; a long-lived MCP or API process starts writing graph.json once `sync` creates it.
- **The local MCP server and API daemon**: `attach_document` reads only regular files inside the project, up to 25 MiB, and checks the file it opened; documents and the session file live next to the database; every request gets a reply to its own id, and invalid UTF-8 no longer kills the server; `resume_session` reopens what it says it resumed; `/query` stops after 5 s and 8 MiB and cannot ATTACH; `serve --api` needs a data dir and a token a client can send; export node specs refuse what they cannot parse; a writeup's descriptions cannot swallow the rest of it.
- **`/demo-swarm` asks for trust once**, in the lead pane, and the workers wait for the answer.
- **The log survives a crash.** A logged write and its op commit in one SQLite transaction (a `remote_outbox` row, moved into the log after the commit), so a kill between the two loses neither; a torn last line of the log is set aside, not glued to the next write; the log's lock is released when its holder dies; compaction syncs the new file before and after the rename. `DECIDUOUS_DB_PATH=deciduous.db` queues its writes like any other path, and the log is replayed as its database's project, not the working directory's.
- **One bad op no longer blocks the rest.** The server rejects an op it cannot store (a NUL, an over-long id, a weight past the float range) alone and applies the ones after it. The CLI retries a single 500 once and sets an op aside only when the server fails on it and not on others; three failures in a row are an outage, and nothing is set aside. A write holding a NUL is refused before it is made, and `remote push --seed` withholds such rows by name and sends the rest. The replay after a write gets 10 s in all, and a stdio MCP server exits within 3 s of stdin closing; a tool result says when its write was not queued or was refused.
- **Queued edits respect newer ones.** A delete says what the node held and is refused if the server's copy changed since; an unlink leaves a tombstone on the server and in `/export`, so an agent's unlink reaches clones and a stale link replayed later does not put the edge back; an edit made after a delete brings the node back, whichever reached the server first. Edits that came through git are queued for the server, documents go through the log, `remote pull` takes a field the server refused, `--repair` never writes over a field the server changed, and `remote push` exits 1 on a refusal still standing and drops one that has settled.
- **The server trusts no client.** Every MCP tool refuses an argument or nested key it does not declare, naming the one it meant; `/ops` and `/import` hold creates, updates and edges to the bounds MCP holds them to, counted in codepoints; no path writes a 2-cycle; racing creates answer `exists`; a change_id is one node on every path; a batch or import that writes nothing creates no workspace and claims none. `serverInfo.version` comes from mix.exs.
- **The CLI says what it did.** `unlink` removes one edge and refuses to guess between two; a duplicate `link` is refused by name; a node can be named by the server id agents quote; `add --commit` stores a full hash or refuses; `add`, `sync` and `remote pull` on a detached commit leave its graph.json alone (G7); `sync --check` in a clone with no database no longer creates one; `/query` runs in a child process killed at 5 s with a 64 MB heap.

### Not fixed
- A node added on one branch still shows on every branch (G6): the one local database has no branch semantics yet. Whether a resurrected node's edges come back is undecided (G9).
- A field set back to its ancestor's value loses git's merge to a stale pulled copy (MODEL-7833): fixing it needs a timestamp per field in graph.json, which changes the file's format.
- An `/import` whose every row is already a tombstone still claims the workspace, and a create that passes the server's checks and then fails at the insert still leaves the workspace it created.
- A short server-id prefix is resolved as a local change_id prefix first when both match; quote 8 or more characters.
- Two MCP servers resuming the same session from the session file share it.

## [1.0.7] - 2026-09-23

A local write that the server never saw was invisible to the agents, and nothing said so. One workspace held 502 nodes and 462 edges in its local database against 0 and 0 on the server it was configured to use.

### Fixed
- **Writes from the CLI reach the server.** `add`, `link`, `status` and `prompt` write locally, then send what the server lacks, plus the node just touched. A server that cannot be reached leaves a warning that names `deciduous remote push` and says plainly that until then the local graph and the graph the agents read are different graphs; the write itself still succeeded, so the CLI stays usable offline. `remote push` is unchanged and still the way to send a backlog.
- **`delete` and `unlink` say they are local only.** The server's `/import` applies additions and edits, not removals, so a node deleted locally stays on the server. They now print that instead of appearing to have converged.

### Changed
- **The local server's port is chosen, not assumed.** `remote setup --local` suggests a free port in 20000–32767 and takes any port you name; `--port` and `DECIDUOUS_PORT` answer for scripts. Before this, setup put every install on 4000, which is Phoenix's default: on a Phoenix project the graph server squatted the port of the dev server of the very project being logged. An install that already has a port keeps it. Port 0 is refused, because the URL written into the project's config would not survive a restart.
- **`/demo-swarm` explains the error it cannot avoid.** The command file travels through a repository, so it can reach a machine whose deciduous is older than 1.0.4, the first release with the subcommand, and clap answers `unrecognized subcommand 'demo-swarm'`. The template now names the cause, including a second deciduous earlier on PATH than `~/.cargo/bin`, and the checks that tell which.

## [1.0.6] - 2026-09-23

### Changed
- **`init` and `update` ask where the graph lives instead of choosing for you.** In 1.0.5 a project with no `[remote]` got a local Docker server without being asked. Now, in a terminal, they run the `deciduous remote setup` questions: this machine (PostgreSQL and the server in Docker), or a server someone else runs (URL and token). They continue once the project points at a server that answers. With no terminal they stop with exit 1 and name `deciduous remote setup --local` and `--url <url>`, so a script says which it wants. A project that already has a `[remote]` is only checked, as before.

## [1.0.5] - 2026-09-23

A project is not set up until it points at a server that answers. Since 1.0.3 agents write through the HTTP MCP server, so a project with no server gives its agents nowhere to write, and `init` used to finish without one.

### Changed
- **`deciduous init` and `deciduous update` set up or check the shared graph server, and fail if they cannot.** They write their files first, then:
  - A project with `[remote] url` is checked. On `init` the server must answer and accept the stored token; on `update` it must answer and a token must be stored. `update --all` checks every project.
  - A project without a remote gets this machine's local server. If one is running (`~/.config/deciduous/server/.env`, `/health` answering), the project is pointed at it. If not, `init` downloads the `deciduous-mcp-docker.tar.gz` for its own version and checks it against the release's `checksums.txt`. It then runs the bundle's `scripts/setup.sh`, which starts PostgreSQL 17 and the server on `127.0.0.1:4000` with credentials in that `.env` (mode 0600). It stores the token, writes `[remote]`, and registers the server with Claude Code at user scope unless a `deciduous` server is already registered there. A token the machine already has becomes the local server's token, so a machine never holds two.
  - No Docker, Docker not running, a checksum mismatch, or a server that does not answer is an error with the fix in it, exit 1. The project files are already written by then. Measured: the first `init` on a machine takes 1 min 35 s (image build); the next project finds the running server in 1.2 s.
  - `DECIDUOUS_NO_SERVER=1` skips the step, for tests and CI. `DECIDUOUS_SERVER_BUNDLE` points at a local bundle, for builds that are not releases.

### Added
- **`deciduous remote setup`, a wizard.** It asks whether the graph lives on this machine (PostgreSQL and the server in Docker, set up if needed) or on a server someone else runs (URL and token, the token read with echo off). A project that already has a remote is asked "Keep it?" first. It ends where `init`'s server step ends. A token is checked against the server before anything is stored or written; a wrong one leaves the credentials and config untouched. `--local` and `--url <url>` answer the question for scripts; with neither and no terminal it stops and names them.

### Removed
- **GitHub Pages.** The graph lives on the shared server, so `init` no longer writes the Pages viewer (`docs/index.html`), `docs/graph-data.json`, `docs/.nojekyll`, `.github/workflows/deploy-pages.yml` or the `/sync-graph` command (Claude Code and OpenCode). `deciduous sync` only reconciles `.deciduous/graph.json` with the local database; it no longer exports `docs/graph-data.json` and `docs/git-history.json`. `--no-pages` and `--output` are accepted and ignored, so existing scripts keep working. `update` removes `/sync-graph` and `deploy-pages.yml` when deciduous wrote them, keeps them when someone edited them, and leaves `docs/` alone. The PR image cleanup workflow stays; `dot --auto` and `writeup --png` still use it. Browse a graph with `deciduous serve`.

### Fixed
- **The README's Claude Code MCP setup never connected anything.** It put `mcpServers` in `.claude/settings.local.json`, which Claude Code does not read for MCP. It now uses `claude mcp add`. The README also gains a first-time setup section.

## [1.0.4] - 2026-09-23

Agents are told how to log by the server itself, on every connection, in place of the hook 1.0.3 removed. The release also adds a multi-agent demonstration and fixes `update --help`.

### Added
- **The MCP server sends logging instructions on `initialize`.** MCP's `InitializeResult` has an optional `instructions` string, which Claude Code and other clients put in the model's context for the whole session. Upstream Hermes never sends it; a fifth vendored patch does. The text (`DeciduousMcp.MCP.Instructions`) says when to write (a request, a fork, a choice, a change, a result, a fact that changes the plan), names `capture_conversation_turn` as the one-call step and `add_node` with `parent_id` for single nodes, and asks for `workspace` and `branch` on every write. It reaches every client with no project configuration, which is where the hook was weakest, and it cannot block anything.
- **`/demo-swarm`, a multi-agent demonstration for iTerm2 and Ghostty.** `deciduous demo-swarm` (hidden from `--help`) creates a fresh repository and opens one window. An Opus lead walks through the setup, then four Sonnet workers start, each in its own worktree: the functional core, the imperative shell, the view and QA. They build one Tetris and share one deciduous workspace; each worker's goal links to the lead's assignment node, so the edges cross branches. The team works to five rules: functional core and imperative shell, tests first under `node --test`, Playwright tests that reproduce user errors, JSDoc types checked by `tsc --checkJs` (the page runs from disk with no build step), and simple over easy. The lead merges only when all three checks pass. Splits are native AppleScript in both terminals. Every pane runs under `script -r` with a pinned session id, so a run can be replayed from `.swarm/rec/`. Any other terminal gets a one-line refusal and exit 1. `deciduous demo-swarm --preview` plays the walkthrough alone in the current terminal, creating and starting nothing.

### Fixed
- **`deciduous update --help` said it did not touch settings files.** It has edited `.claude/settings.json` since 1.0.2; it now says what it does there (removes the logging hooks earlier versions installed, keeping the user's entries and key order), what it backs up, and what it leaves alone with and without a `[remote]`.

## [1.0.3] - 2026-09-23

`deciduous update` no longer destroys what it did not write. Upgrading the 86 projects on one machine to 1.0.2 took a script that backed everything up, restored what `update` wrote outside its own files, and put back 59 files it had replaced: a project's own `build-test` command, hand-edited hook scripts, a staged edit. That script is now the update.

### Removed
- **The log-loop hook is gone.** 1.0.2's `deciduous log-loop` counted an agent's actions between graph writes and denied its next tool call after ten, or after an unlogged commit. In practice it stopped a session cold when the reset stopped reaching it: a `PreToolUse` for one call runs before the `PostToolUse` of the graph write sent beside it, a session whose settings changed underneath it kept counting with no way to reset, and `DECIDUOUS_LOG_LOOP=off` typed into the session never reaches hook processes, which inherit the environment Claude Code started with. A rule that blocks work has to be one the agent can satisfy every time, and this one could not. `init` installs no logging hooks now, for Claude Code, Windsurf or OpenCode; `update` deletes the hook scripts and plugins deciduous wrote (a user's own are kept, with their `settings.json` entries), and takes every `deciduous log-loop` command out of `.claude/settings.json`, dropping entries and events it leaves empty. `deciduous log-loop` stays as a hidden command that exits 0, so a 1.0.2 project is quiet until its next update. Logging is encouraged by the `CLAUDE.md` section and the MCP tools' own descriptions and replies instead.

### Changed
- **`update` keeps what it did not write.** A harness file (command, skill, hook script, `agents.toml`, OpenCode and Windsurf files) is replaced only when deciduous wrote it: its hash is recorded in `.deciduous/harness.json` when written, and for older installs it is recognised against every template text deciduous ever shipped (234, from the repository's whole history, embedded in the binary). A Markdown file someone else wrote is kept, with the new text appended in a `<!-- deciduous:start -->` block that later updates replace on their own; a script, TOML or other file someone else wrote is left untouched.
- **Everything `update` changes is backed up first**, to `.deciduous/update-backups/<unix-time>/`, and the run prints where.
- **On a project with `[remote]` configured, `update` leaves `.gitignore`, `.gitattributes`, `.git/config` and `graph.json` alone.** The graph lives on the server; the `graph.json` sync only wrote into files that belong to the project.
- **`.claude/settings.json` keeps its key order** when `update` edits it; before, it came back alphabetised.
- **`remote push` sends only what the server does not have.** The server's import replaces a row it already holds, so pushing a whole local graph overwrote anything changed on the server since with the stale local copy. Edges are matched through the local node map, because an older database's stored edge change ids can be stale or missing: matched by those alone, one 7,805-node workspace looked like 51,092 missing edges, all of which the server had. `--overwrite` restores the old behaviour.

### Added
- **`deciduous update --all <dir>`** updates every deciduous project directly under `<dir>` (and `<dir>` itself), one after another, reporting each and continuing past failures.

### Not in this release
- OpenCode command files are generated from the Claude templates at run time, so older generated copies are not in the embedded list: an OpenCode project's first 1.0.3 update appends to them instead of replacing them. Later updates recognise them by the hash recorded on write.

## [1.0.2] - 2026-09-23

The shared graph server can be installed without Erlang, Elixir or a hand-built database, and no MCP call can hang long enough for Claude Code to background it. Both packages move to 1.0.2 together; the release workflow refuses to publish a tag that Cargo.toml and deciduous_mcp/mix.exs do not both declare.

### Added
- **The server ships as a single executable.** Burrito wraps the release with ERTS and Elixir for macOS (arm64, x86_64), Linux (arm64, x86_64) and Windows (x86_64). A native install bootstraps an empty PostgreSQL from the release's `STRUCTURE.sql`.
- **One-command Docker setup.** `deciduous_mcp/scripts/setup.sh` generates private credentials, starts PostgreSQL 17 on a persistent volume, applies migrations and waits for readiness. It can use an existing database instead, and a repeat run keeps the saved token and password byte for byte.
- **`GET /ready`** answers 200 only when the database responds and every required migration is applied, and 503 until then. `/health` stays unauthenticated liveness.
- **`deciduous log-loop`: agents log as they work.** The Claude Code hook that `init` installs counts what an agent does between graph writes and denies its next tool call after ten unlogged actions, or straight after an unlogged `git commit`, naming what went unlogged and the exact `add_node` call; a turn cannot end with three or more unlogged actions. A write through the MCP tools or `deciduous add|link|...` resets it; read-only shell commands (`git diff`, `cat`, `grep`, ...) and the CLI graph writes are never counted or denied, only a real `git commit`/`git merge` counts as a commit, the Stop check counts the current turn only, and parallel tool calls cannot lose a reset. `deciduous update` brings existing projects onto it: the two hook scripts become wrappers around the binary, and `.claude/settings.json` is merged, keeping every hook you added. `DECIDUOUS_LOG_LOOP=off` turns it off. The hook it replaces checked the local database, which never sees MCP writes, and looked for output `deciduous nodes` has never printed, so it never blocked anything.
- **`add_node` creates and links in one call.** `parent_id` (with optional `edge_type` and `rationale`) adds the incoming edge in the same transaction; if the parent is not a node in the workspace, nothing is created. The two-step form made an agent write the second call before it had the first answer, and it did: `add_edge` sent alongside `add_node` with `"PLACEHOLDER"` where the new id belonged, five times in one night. Such an id is now refused with a pointer to `parent_id`, and the installed instructions and the log-loop hook teach the one-call form.
- **Trigram indexes** (`pg_trgm` GIN) on node title, description and `metadata::text`. `query_nodes(search:)` and `ask_graph` were `ILIKE '%term%'` over every row: 84.5 ms on production across 30,249 nodes, now 3.0 ms, and 1.6 ms within one workspace.

### Fixed
- **A tool call that ran long was left to the client's timeout.** The transport waited up to four minutes, longer than Cloudflare's 100-second origin cut and Claude Code's 120-second background move. Hermes.Server.Base now stops any handler still running at 60 seconds, which rolls back its open transaction, and answers the request under its own id: `list_workspaces did not finish within 60s and was stopped; nothing it had not committed was kept`. The transport's own timeout is 90 seconds, as a backstop. The heaviest call measured on production is 7.2 seconds.
- **The server never logged its own failures.** `Hermes.Logging.should_log?/1` compared log levels backwards, so at `:info` every warning and error Hermes emitted (`request_handler_crashed`, `server_call_failed`) was dropped before it reached Logger.
- **`ask_graph` searched every workspace on the server.** Its search terms were added with `or_where`, which Ecto renders as `(workspace AND not deleted AND scope) OR term...`, so a question in one workspace returned nodes from others, deleted ones included.
- **A 16-character node id crashed the tool call.** `Ecto.UUID.cast/1` accepts any 16-byte binary as a raw UUID, so `"PLACEHOLDER_SKIP"` passed the id check and failed in the query. Only the 36-character form is accepted now.
- **`query_nodes` with `limit` below 1** reached Postgres as `LIMIT -5` and came back as a raw Postgrex error. It is now `limit must be at least 1, got -5`.
- **The events listener recovers after a database outage**, and `DB_SSL=true` now verifies the certificate and hostname by default.

### Changed
- **`DB_SSL=true` now verifies the database server, in libpq's three levels.** 1.0.1 encrypted without checking who answered. `DB_SSL_VERIFY=full` (the default; `peer` still accepted) checks the chain and that the certificate names the host, including an IP address through an IP subjectAltName; `ca` checks the chain only, for a certificate that does not name the address you connect to; `none` encrypts without authenticating and says so in a warning at every boot. `DB_SSL_CA_FILE` trusts a private CA and is checked at boot: a missing file or one without a certificate stops the server with a sentence, where before it ran with every connection failing (a missing file logged nothing at all). Each boot logs the mode in effect. Verified against a TLS PostgreSQL 17 with a private CA: full over DNS and over IP connect, ca connects to a certificate naming another host, and a wrong CA is refused under both full and ca. Upgrading from 1.0.1 with `DB_SSL=true` and a private CA: set `DB_SSL_CA_FILE`, plus `DB_SSL_VERIFY=ca` if the certificate does not name the address.
- **`/ready` waits for a pooled connection** (up to one second) instead of answering 503 whenever every connection was busy, which made a loaded server look down to healthchecks.
- **`STRUCTURE.sql` bootstraps PostgreSQL 16 as well as 17.** The dump no longer carries `SET transaction_timeout`, which exists only in 17; checked by applying it to empty 16 and 17 databases.

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
