# CLI reference

The `deciduous` commands people actually use, grouped by what you are trying to do, with flags as `deciduous <command> --help` prints them in 1.1.0. In a project with a `[remote]`, every write command writes the local database, queues an op in `.deciduous/remote-log.jsonl`, and sends it to the server before it exits; every read command reads the local database, which is a cache refreshed by `remote pull`. Commands marked **server** need a `[remote]`; commands marked **no server** only matter without one.

Anywhere a command takes a node, it accepts a local number or a change_id prefix. In a project with a `[remote]` it also accepts a server UUID, once the node is in the local cache.

## Connect a project to a server

### `remote setup` (server)

Connect this project to a shared graph server, step by step. In a terminal it asks; flags answer for scripts. Ends by writing `[remote]`, storing the token if needed, and registering Claude Code (only if Claude Code has no `deciduous` server yet).

| Flag | Meaning |
| --- | --- |
| `--url <URL>` | Use the server at this URL (token from `DECIDUOUS_MCP_TOKEN` or the stored one). The usual choice. |
| `--local` | Use this machine's server; set it up if there is none. See [Without a shared server](local.md). |
| `--port <PORT>` | Port for this machine's server, first setup only. |
| `--database-url <URL>` | PostgreSQL for this machine's server; `?sslmode=require\|verify-ca\|verify-full` for TLS. The environment's `DATABASE_URL` is never used unless passed here. |
| `--docker` | Run this machine's server in Docker. |
| `--docker-postgres` | Also run a new PostgreSQL in Docker (implies `--docker`). |

```text
$ DECIDUOUS_MCP_TOKEN=<token> deciduous remote setup --url https://deciduous.example.com
```

A token given only through the environment is checked but not stored.

### `remote login` / `remote logout`

`login [--url <URL>]` reads a token from stdin and stores it in `~/.config/deciduous/credentials` (mode 0600; `$XDG_CONFIG_HOME/deciduous/credentials` if that is set). With `--url` it checks the token against that server first and stores nothing if the server refuses it. `logout` deletes the stored token. There is one stored token per machine; see [One token per machine](local.md#one-token-per-machine).

### `remote init <URL>` (server)

Point this project at a server without the wizard: checks `/health` and the token, claims the workspace for this repository, sends any local history the server lacks, writes `[remote]`. Does not touch Claude Code.

| Flag | Meaning |
| --- | --- |
| `--workspace <NAME>` | Workspace name. Defaults to the git repository's root directory name. Naming an existing workspace explicitly lets this repository share it. |

```text
$ deciduous remote init http://127.0.0.1:4000 --workspace my-app
Remote: http://127.0.0.1:4000
  workspace: my-app
  claim: claimed (by this repository's root commit)
  server holds 0 nodes, 0 edges, 0 documents

Written to .deciduous/config.toml. The token stays in ~/.config/deciduous/credentials (or DECIDUOUS_MCP_TOKEN), never here.
```

The message names `~/.config` even when `XDG_CONFIG_HOME` has moved the file.

### `remote adopt [ROOT] --url <URL>` (server)

Walk a directory for projects that already have a `.deciduous` and point each at the server. Existing configuration is left alone. `-n, --dry-run` shows what would change, but in 1.1.0 it is not free of side effects: it leaves a `.deciduous/deciduous.db` in the directory it walks. That stray `.deciduous` then captures any repository below it that has no `.deciduous` of its own, including a worktree, which writes to that file instead of its main checkout's server. Delete it after a dry run.

## Check and repair the connection

### `remote status` (server)

How far the local copy has drifted from the server: ops waiting and rejected, node and edge counts, and every row that differs, field by field. Exits 0 only on `In sync`.

```text
Remote: http://127.0.0.1:4000
Workspace: my-app
Log: 0 write(s) waiting, 0 rejected  (/home/you/code/my-app/.deciduous/remote-log.jsonl)

                 local    server
  nodes              2         2
  edges              1         1

In sync: no writes waiting, and every node, edge and document matches field by field.
Themes and tags are not sent to the server, so they are not compared.
```

### `remote push` (server)

Send the ops waiting in `.deciduous/remote-log.jsonl`, in order. The server applies each at most once, so pushing twice is safe. Normally unnecessary: every write sends what is waiting.

| Flag | Meaning |
| --- | --- |
| `--retry-rejected` | Send rejected ops again, after fixing the cause. |
| `--drop-rejected` | Discard the ops the server rejected (read why with `remote status` first). |
| `--drop <OP_ID>` | Discard one op, waiting or rejected, by the start of its id. |
| `--seed` | Also send nodes, edges and documents the server lacks that no op covers. Never changes a row the server has. |
| `--repair` | Also send what this copy changed and never logged, for nodes `status` lists as Different, field by field where the server has not changed the field since the last pull. |
| `--overwrite-server` | With `--repair`: also send fields the server did change, over the server's value. |
| `--overwrite` | Replace the server's copy of every row with the local one (the pre-1.0.3 behaviour). Last resort. |

`DECIDUOUS_OPS_TIMEOUT=<seconds>` changes the 120-second limit per request.

### `remote pull` (server)

Refresh the local database from the server: takes the server's side of every difference. Run it before reading with `nodes`, `show` or `serve` if agents have been writing over MCP. No flags.

### `remote watch` (server)

Stream this workspace's writes, one line per node event, reconnecting with backoff.

```text
$ deciduous remote watch
Watching: my-app
13:38:02  main      outcome     "watched outcome"
```

| Flag | Meaning |
| --- | --- |
| `--types <TYPES>` | Only these node types, comma-separated. |
| `--branch <BRANCH>` | Only this branch; repeat for several. |
| `--edges` | Also show edge events. |
| `--json` | Raw JSON frame per event. |
| `--url` | Print the socket URL and exit. The URL carries the token in its query string. |
| `--claude-code` | Print the URL plus a ready-to-paste Monitor tool call for Claude Code, and exit. Also carries the token. |

## Record decisions

### `add <TYPE> <TITLE>`

Types: `goal`, `option`, `decision`, `action`, `outcome`, `observation`, `revisit` (`--help` omits `revisit`; it works).

| Flag | Meaning |
| --- | --- |
| `-d, --description <TEXT>` | Description. Observations warn without one. |
| `-c, --confidence <0-100>` | Confidence. |
| `-p, --prompt <TEXT>` | The user's request, verbatim. Warns when it looks like a summary. |
| `--prompt-stdin` | Read the prompt from stdin, for multi-line requests. |
| `-f, --files <a,b>` | Associated files, comma-separated. |
| `-b, --branch <BRANCH>` / `--no-branch` | Override or skip branch detection. |
| `--commit <REV>` | HEAD, a branch, a tag, a hash or prefix. Stored as the full hash; a rev git cannot resolve is refused. |
| `--date <DATE>` | Backdate: RFC 3339, `YYYY-MM-DD` or `YYYY-MM-DD HH:MM:SS`. |

```bash
deciduous add goal "Bound the retry delay" -c 90 --prompt-stdin <<'EOF'
Retries should stop after a bounded delay; right now a 429 without Retry-After loops forever.
EOF
```

### `link <FROM> <TO>` / `unlink <FROM> <TO>`

`link -r, --rationale <TEXT>`, `-t, --edge-type <TYPE>` (default `leads_to`; also `requires`, `chosen`, `rejected`, `blocks`, `enables`, `took_from`). `unlink -t, --type <TYPE>` removes only the edge of that type, required when two nodes are joined by more than one.

### `status <ID> <STATUS>`

`pending`, `active`, `completed`, `rejected`, `superseded`, `abandoned`. (`--help` lists the first four.)

### `prompt <ID> [PROMPT]`

Set or replace a node's prompt; reads stdin when the text is omitted.

### `delete <ID>`

Delete a node and its edges. `--dry-run` shows what would go.

## Read the graph

| Command | Flags |
| --- | --- |
| `nodes` | `-b, --branch`, `-t, --node-type`, `--theme` |
| `edges` | none |
| `show <ID>` | `--json` |
| `graph` | none; the whole graph as JSON on stdout |
| `pulse` | `-b, --branch`, `-r, --recent <N>` (default 10), `--json`, `--summary`. Active state, gaps, health. |
| `serve` | `-p, --port` (default 3000). The web viewer, on the local database. |
| `commands` | `-l, --limit` (default 20). Recent CLI writes with exit status. |

All of these read the local database. Over MCP, agents read the server directly; see [MCP tools](mcp.md).

## Design history

| Command | Purpose and flags |
| --- | --- |
| `archaeology pivot <FROM_ID> <OBSERVATION> <NEW_APPROACH>` | Creates observation -> revisit -> new decision and marks the old one superseded, in one step. `-c, --confidence`, `-r, --reason`, `--dry-run`. |
| `archaeology supersede <ID>` | Mark superseded. `--cascade` for every descendant, `--dry-run`. |
| `archaeology timeline` | Nodes in time order. `-l, --limit` (0 = all), `-t, --node-type`, `-b, --branch`, `--json`. |
| `narratives init` / `show` / `pivots` | `.deciduous/narratives.md`: seed it from active goals (`-o`, `--force`), print it (`-p`), list revisit chains (`-b`, `--json`). |

See [Design history](history.md).

## Evidence

| Command | Purpose and flags |
| --- | --- |
| `doc attach <NODE_ID> <FILE>` | Attach a file. `-d, --description`, `--ai-describe` (uses the `claude` CLI). |
| `doc list [NODE_ID]` | `--include-detached`, `--json`. |
| `doc show <DOC_ID>` / `doc open <DOC_ID>` | Details (`--json`) / open in the default application. |
| `doc describe <DOC_ID> [TEXT]` | Set the description; stdin if omitted; `--ai`. |
| `doc detach <DOC_ID>` | Soft-delete. `doc gc [--dry-run]` removes files nothing references. |
| `transcript list` | This project's Claude Code sessions and whether each is imported. `--dir`, `--json`. |
| `transcript import <PATH or SESSION-ID>...` | Derive goals, actions and outcomes from a transcript, each with evidence. Re-importing adds nothing. `--dry-run`, `--no-hash`, `--json`. |

See [Documents and transcripts](evidence.md).

## Coordinate agents

`board post -s, --subject <S> [-m <BODY>]` (body from stdin if omitted), `--as <LABEL>` (default `$DECIDUOUS_AGENT_LABEL`), `--reply-to <ID>`, `--branch`, `--json`. `@label` in the subject or body addresses someone.

`board read`: `--unanswered <LABEL>` (what is waiting on you), `--to`, `--from`, `--since <ID>`, `-q, --query`, `--limit` (default 50, max 200), `--json`. `board show <ID> [--json]`.

With a `[remote]` the board lives on the server, per workspace; without one, in the main worktree's database. Messages are not graph nodes and are never exported. See [Many agents, one graph](teams.md).

## Export

| Command | Flags |
| --- | --- |
| `dot` | `-o`, `-r, --roots`, `-n, --nodes "1-11"`, `--png` (needs graphviz), `--auto` (branch-specific name in `docs/`), `-t, --title`, `--rankdir TB\|LR\|BT\|RL` |
| `writeup` | `-t, --title`, `-r, --roots`, `-n, --nodes`, `-o`, `--png <FILE>`, `--auto`, `--no-dot`, `--no-test-plan` |
| `backup` | `-o, --output` (default `deciduous_backup_<timestamp>.db`). Copies the local database only; back up the server's PostgreSQL separately ([Operating](operating.md)). |

## Themes and tags (local only)

`themes create <NAME> [-c '#hex'] [-d TEXT]`, `themes list`, `themes delete <NAME>`; `tag add|remove|confirm <NODE_ID> <THEME>`, `tag list <NODE_ID>`, `tag suggest [NODE_ID] [--apply]`. These stay in the local database: they are not sent to the server, and `remote status` does not compare them.

## Without a server (no server)

| Command | Purpose |
| --- | --- |
| `sync` | Reconcile `.deciduous/graph.json` with the local database, both ways, and register the git merge driver in this clone. `--check` reports without writing. |
| `migrate` | Add change_id columns to a pre-sync database. |

See [Without a shared server](local.md#no-server-at-all).

## Project setup

| Command | Purpose and flags |
| --- | --- |
| `init` | Set up the project: `.deciduous/`, assistant files, and the server step. `--claude` (default), `--opencode`, `--windsurf`, `--both`, `--no-auto-update`. Without a terminal and without a `[remote]`, exits 1 after writing its files; `DECIDUOUS_NO_SERVER=1` skips the server step. |
| `update` | Refresh the assistant integration files deciduous wrote, keeping yours (backups in `.deciduous/update-backups/<time>/`). Leaves `.deciduous/config.toml` alone. In a project on this machine's native server, upgrades that server too. `--all <DIR>` for every project under a directory. |
| `check-update` | Exit 1 if this project's files are older than the binary. |
| `auto-update on\|off` | The once-a-day crates.io version check. |
| `integration` | Show Claude Code hooks, commands and skills installed. |
| `hooks install\|status\|uninstall` | Claude Code hooks from `.deciduous/config.toml`. |
| `opencode install\|status\|uninstall` | OpenCode plugins, commands and `AGENTS.md`. |
| `completion <SHELL>` | `bash`, `zsh`, `fish`, `powershell`, `elvish`. |

## Other commands

`mcp` runs the stdio MCP server on the local database ([MCP tools](mcp.md)). `serve --api` runs a separate multi-graph JSON daemon (`--data-dir`, `--token`, `--bind`, default `127.0.0.1`); it is not the graph server. `audit --associate-commits` matches commits to nodes by title (`--min-score`, `--dry-run`, `--yes`). `roadmap init|refresh|sync|list|link|unlink|conflicts|status|check` keeps `ROADMAP.md` in step with GitHub issues; `sync` is a dry run unless `--execute`.

## Environment

| Variable | Effect |
| --- | --- |
| `DECIDUOUS_MCP_TOKEN` | Token for the server. Read before the stored token. |
| `XDG_CONFIG_HOME` | Moves `~/.config/deciduous/` (credentials, and this machine's server settings). |
| `DECIDUOUS_NO_SERVER` | Any value other than empty or `0` makes `init` and `update` skip the server step. |
| `DECIDUOUS_PORT` | Port for this machine's server the first time `remote setup --local` installs it. |
| `DECIDUOUS_DB_PATH` | Use this SQLite file instead of finding `.deciduous/deciduous.db` by walking up (Claude Desktop, unusual layouts). |
| `DECIDUOUS_API_DATA_DIR`, `DECIDUOUS_API_TOKEN` | Fallbacks for `serve --api`'s `--data-dir` and `--token`. |
| `DECIDUOUS_OPS_TIMEOUT` | Seconds per `remote push` request (default 120). |
| `DECIDUOUS_AGENT_LABEL` | Default `--as` for `board post`. |
| `DECIDUOUS_QUIET_LOCAL_ONLY` | Silences the "written to ... only; this project has no shared server" note. |
| `DECIDUOUS_SERVER_BINARY`, `DECIDUOUS_SERVER_BUNDLE` | A local server executable or Docker bundle for `remote setup --local` to install instead of downloading one (unreleased builds). |
