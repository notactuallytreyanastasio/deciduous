# Quick Reference

Every command and flag here exists in deciduous 1.1.0 (`deciduous --help` and
each subcommand's `--help`, checked 2026-10-24). If it is not here, check
`deciduous <command> --help` before writing it down.

---

## Install

```bash
brew tap notactuallytreyanastasio/tap && brew install deciduous
cargo install deciduous --locked
# or a binary from https://github.com/notactuallytreyanastasio/deciduous/releases
deciduous --version          # deciduous 1.1.0
deciduous completion zsh     # bash, zsh, fish, powershell, elvish
```

## Set up a project

```bash
deciduous init                 # Claude Code (default): .claude/, CLAUDE.md section, .deciduous/
deciduous init --opencode      # OpenCode: .opencode/, AGENTS.md
deciduous init --windsurf      # Windsurf: .windsurf/
deciduous init --both          # Claude Code + OpenCode
deciduous init --no-auto-update
```

`init` asks where the graph lives: this machine's server against PostgreSQL on
localhost:5432 (default), a PostgreSQL you name, PostgreSQL and the server in
Docker, or a server someone else runs. It writes `[remote]` to
`.deciduous/config.toml` and registers the server with Claude Code. With no
terminal it stops and names the flags:

```bash
deciduous remote setup --local                 # background service, PostgreSQL on localhost:5432
deciduous remote setup --database-url <url>    # ... against a PostgreSQL you name
deciduous remote setup --docker-postgres       # PostgreSQL and the server in Docker
deciduous remote setup --docker                # the server in Docker, PostgreSQL from --local/--database-url
deciduous remote setup --url <url>             # a server someone else runs (token from DECIDUOUS_MCP_TOKEN or stored)
deciduous remote setup --port <n>              # the server's port, first setup only
DECIDUOUS_NO_SERVER=1 deciduous init           # skip the server step (tests, CI)
```

```bash
deciduous update               # refresh commands, skills, hooks, the CLAUDE.md section; keeps what you edited
deciduous update --all ~/code  # every project under a directory
deciduous check-update         # exit 1 if .deciduous/.version is behind the binary
deciduous auto-update on|off
```

## Node types, edge types, statuses

| Node types | `goal` `option` `decision` `action` `outcome` `observation` `revisit` |
|---|---|
| Edge types | `leads_to` (default) `requires` `chosen` `rejected` `blocks` `enables` `took_from` |
| Statuses | `pending` (default) `active` `completed` `rejected` `superseded` `abandoned` |

`add --help` lists six types and `status --help` four statuses; all seven and
six are accepted. The flow is `goal -> options -> decision -> actions -> outcomes`.

## Add nodes

```bash
deciduous add goal "Rate limit the public API" -c 90 -p "the user's message, verbatim"
deciduous add goal "Add auth" --prompt-stdin << 'EOF'
The full multi-line request...
EOF
deciduous add option "Token bucket per API key" -c 80
deciduous add decision "Token bucket per key over fixed window per IP" -c 85
deciduous add action "Add token bucket middleware" -c 85 -f "src/middleware/rate_limit.rs"
deciduous add outcome "Passes load test at 10k rps" -c 90 --commit HEAD
deciduous add observation "Shared IPs behind carrier NAT" -d "the detail goes in the description"
deciduous add revisit "Reconsidering per-IP fallback"

# flags
-c, --confidence 0-100     -d, --description     -f, --files "a.rs,b.rs"
-p, --prompt "..."         --prompt-stdin        --commit HEAD|<branch>|<tag>|<hash>  (stored as the full hash)
-b, --branch <name>        --no-branch           --date "YYYY-MM-DD"  (backdate, for archaeology)
```

Node arguments everywhere take a local id, a `change_id` prefix (4+ hex
characters, the CHANGE column of `nodes`), or a server id.

## Link

```bash
deciduous link 1 2 -r "possible approach"             # leads_to
deciduous link 2 4 -t chosen -r "survives NAT"
deciduous link 3 4 -t rejected -r "carrier NAT"
deciduous link 7 3 -t blocks
deciduous link <their_change_id> 42 -t took_from -r "borrowed the lock-delay reset"
deciduous unlink 1 2                                   # -t <type> when two edges join the pair
```

## Read

```bash
deciduous nodes                        # ID  CHANGE  TYPE  STATUS  TITLE
deciduous nodes -b main                # by branch
deciduous nodes -t revisit             # by type
deciduous nodes --theme rate-limiting  # by theme
deciduous edges
deciduous show 4                       # metadata, evidence, consulted files, hashes, connections
deciduous show 918ffbe9 --json         # raw record with metadata_json
deciduous graph                        # {"nodes","edges","themes","node_themes"[,"documents"]} as JSON
```

There is no `nodes --status`.

## Change

```bash
deciduous status 4 superseded
deciduous prompt 1 "the verbatim prompt"      # or: deciduous prompt 1 < prompt.txt
deciduous delete 9 --dry-run                  # then without, to delete the node and its edges
```

## Transcripts (1.1.0)

```bash
deciduous transcript list                     # this project's sessions under ~/.claude/projects/<slug>/
deciduous transcript list --dir <dir> --json
deciduous transcript import 328f8485          # a session id prefix, or a path to the .jsonl
deciduous transcript import --dry-run <session>
deciduous transcript import --no-hash <session>   # skip hashing the files actions touched
deciduous transcript import --json <session>...
```

Goals from prompts (verbatim), actions from edits and commits, outcomes from
test runs, each with `metadata.evidence.kind` of `proven` or `inferred`.
Importing twice adds nothing. See [transcripts.md](transcripts.md).

## Shared server (`[remote]`)

```bash
deciduous remote status            # local vs server, field by field; exit 1 when anything differs
deciduous remote pull              # refresh the local database from the server
deciduous remote push              # send the writes waiting in .deciduous/remote-log.jsonl
deciduous remote push --seed       # also send history from before the project had a remote
deciduous remote push --repair     # also send fields `status` lists as Different; --overwrite-server to win
deciduous remote push --drop-rejected | --retry-rejected | --drop <op_id>
deciduous remote watch             # one line per write, live; --types, --branch, --edges, --json, --url, --claude-code
deciduous remote login --url <url> # store a token read from stdin, mode 0600
deciduous remote logout
deciduous remote init <url> --workspace <name>   # point one repo at a server (setup does this for you)
deciduous remote adopt ~/code --url <url> -n     # point every project under a directory; -n = dry run
```

## Git model (no `[remote]`)

```bash
git pull
deciduous sync             # reconcile .deciduous/graph.json with the local database, both ways
deciduous sync --check     # exit 1 if anything is pending (pre-push check)
git add .deciduous/graph.json && git commit -m "graph: ..." && git push
```

`sync` does not write `docs/graph-data.json`; that export was removed in 1.0.5.

## Message board

```bash
deciduous board post --as lead -s "@w1 middleware signature" -m "rate_limit(req, key) -> Result<(), Retry>"
deciduous board post --as w1 -s "re: signature" -m "ok" --reply-to 1
deciduous board read --unanswered w1          # what is addressed to w1 and unanswered
deciduous board read --since 3 --from lead --to w1 -q "signature" --limit 20 --json
deciduous board show 1
```

`--as` defaults to `$DECIDUOUS_AGENT_LABEL`. Messages are not graph: never in
`graph.json`, never exported.

## Documents

```bash
deciduous doc attach 1 spec.pdf -d "Rate limit spec from kickoff"
deciduous doc attach 5 diagram.png --ai-describe        # description written by the claude CLI
deciduous doc list [node] [--include-detached] [--json]
deciduous doc show 1 [--json]
deciduous doc describe 1 "better description"            # or --ai, or text on stdin
deciduous doc open 1
deciduous doc detach 1                                   # soft delete
deciduous doc gc --dry-run && deciduous doc gc           # remove files no record references
```

## Themes and tags

```bash
deciduous themes create rate-limiting -c "#ef4444" -d "Everything about throttling"
deciduous themes list
deciduous themes delete rate-limiting
deciduous tag add 1 rate-limiting
deciduous tag remove 1 rate-limiting
deciduous tag list 1
deciduous tag suggest [node] [--apply]       # keyword and AI suggestions; omit node for all untagged
deciduous tag confirm 1 rate-limiting        # suggested -> manual
```

## Health, history, archaeology

```bash
deciduous pulse [--summary] [--json] [-b branch] [-r 10]
deciduous audit --associate-commits [--min-score 50] [--dry-run] [--yes]   # the only audit action
deciduous narratives init [--force] [-o path]      # .deciduous/narratives.md with a section per active goal
deciduous narratives show
deciduous narratives pivots [--json] [-b branch]   # every revisit with its chain
deciduous archaeology pivot <old_id> "<what was learned>" "<new approach>" -c 80 -r "why it failed" [--dry-run]
deciduous archaeology supersede <id> [--cascade] [--dry-run]
deciduous archaeology timeline [-l 20] [-t type] [-b branch] [--json]
```

## Visualize and write up

```bash
deciduous serve                     # viewer on http://localhost:3000, reads the local database
deciduous serve -p 8080
deciduous dot -o graph.dot          # Graphviz DOT; --png needs graphviz; --rankdir TB|LR|BT|RL
deciduous dot --auto --nodes 1-15 --png        # docs/decision-graph-<branch>.{dot,png}
deciduous writeup -t "Rate limiting" --nodes 1-15 -o PR.md
deciduous writeup --auto -t "Rate limiting" --roots 1 [--no-dot] [--no-test-plan]
gh pr edit N --body "$(deciduous writeup --auto -t 'Rate limiting' --nodes 1-15)"
```

## MCP

```bash
deciduous mcp        # stdio server, 34 tools: {"command":"deciduous","args":["mcp"]}
```

The shared server's MCP endpoint is `<url>/mcp` (21 tools: `add_node`,
`capture_conversation_turn`, `ask_graph`, `query_nodes`, `check_activity`,
`post_message`, ...). `init` registers it with Claude Code at user scope.

## Roadmap

```bash
deciduous roadmap init [-p ROADMAP.md]
deciduous roadmap refresh
deciduous roadmap sync [--execute] [--create-issues] [-r owner/repo]   # dry run by default
deciduous roadmap list [-s section] [--with-issues|--without-issues]
deciduous roadmap link <item> <outcome_id> / unlink <item>
deciduous roadmap conflicts [--resolve] / status / check [--incomplete|--complete]
```

## Hooks and integration status

```bash
deciduous hooks install      # the hooks listed in .deciduous/config.toml (none by default)
deciduous hooks status / uninstall
deciduous integration        # Claude Code hooks, commands, skills
deciduous opencode install / status / uninstall
```

`init` installs two non-blocking hooks: `version-check.sh` and
`board-mentions.sh`. No hook blocks a tool call (removed in 1.0.3).

## Database

```bash
deciduous backup [-o file.db]
deciduous migrate            # add change_id columns to a pre-0.17 database
deciduous commands [-l 20]   # the command log; empty in 1.1.0 projects, nothing writes it
```

## Session start

```bash
deciduous board read --unanswered "$DECIDUOUS_AGENT_LABEL"
deciduous nodes -b "$(git branch --show-current)"
deciduous pulse --summary
deciduous remote status      # with a [remote]
deciduous sync --check       # without one
git log --oneline -10 && git status
```

## Environment variables

| Variable | Read by | Effect |
|---|---|---|
| `DECIDUOUS_DB_PATH` | every command | Use this database instead of walking up to `.deciduous/deciduous.db`. Worktrees that want their own graph set it. |
| `DECIDUOUS_MCP_TOKEN` | `remote`, MCP registration | The server token; beats `~/.config/deciduous/credentials`. Use `${DECIDUOUS_MCP_TOKEN}` in `.mcp.json`, never a literal. |
| `DECIDUOUS_PORT` | `remote setup`, the server's `.env` | Port for this machine's server, first setup only. |
| `DECIDUOUS_AGENT_LABEL` | `board post`, `board-mentions.sh` | This session's label: the default `--as`, and what the hook shows unanswered messages for. |
| `DECIDUOUS_NO_SERVER` | `init`, `update` | Skip the shared-server step (tests, CI). |
| `DECIDUOUS_QUIET_LOCAL_ONLY` | every write | Silence the stderr line that says a write stayed on this machine because the project has no server. |
| `DECIDUOUS_OPS_TIMEOUT` | `remote push` | Seconds one `POST /ops` request may take (default 120). |
| `DECIDUOUS_API_TOKEN`, `DECIDUOUS_API_DATA_DIR` | `serve --api` | Bearer token and data directory for the multi-graph API daemon. |
| `DECIDUOUS_ENV_FILE`, `DECIDUOUS_MCP_INSTALL_DIR`, `DECIDUOUS_SERVER_MODE`, `DECIDUOUS_BIND_ADDRESS` | the server service | Written into `~/.config/deciduous/server/.env` and the launchd/systemd unit by `remote setup`; not for you to set. |

## Slash commands and skills `init` writes

| Command | Purpose |
|---|---|
| `/decision` | Add nodes, link edges, sync |
| `/recover` | Rebuild context at session start, including unanswered board messages |
| `/work` | Create the goal node before implementing |
| `/document` | Document a file or directory |
| `/build-test` | Build and run the tests |
| `/serve-ui` | Start the viewer |
| `/decision-graph` | Build a graph from commit history |
| `/sync` | Git model: reconcile `graph.json`; remote model: check against the server and refresh |
| `/demo-swarm` | The multi-agent demonstration (iTerm2 and Ghostty) |
| `/pulse`, `/narratives`, `/archaeology` | The three skills; see [narratives.md](narratives.md) |

There is no `/sync-graph`; it went with the GitHub Pages export in 1.0.5.

## Files

| Path | Holds | In git |
|---|---|---|
| `.deciduous/deciduous.db` | the local database (cache) | no |
| `.deciduous/graph.json` | the shared graph, git model | yes |
| `.deciduous/config.toml` | `[branch]`, `[github]`, `[hooks]`, `[remote]` | yes |
| `.deciduous/remote-log.jsonl` | ops waiting for the server | no |
| `.deciduous/documents/` | attached files, `<name>.<hash8>` | no |
| `.deciduous/narratives.md` | the narratives file | your call |
| `.deciduous/.version`, `.latest_version`, `.last_version_check` | update checks | no |
| `.deciduous/harness.json`, `update-backups/`, `update-templates/` | what `update` wrote, backed up, and would have written | no |
| `~/.config/deciduous/credentials` | the server token, mode 0600 | outside the repo |
| `~/.config/deciduous/server/` | this machine's server, its `.env` and logs | outside the repo |
| `.claude/commands/`, `.claude/skills/`, `.claude/hooks/`, `.claude/agents.toml` | the Claude Code harness | yes |
