# Troubleshooting

Most problems are one of four things: the CLI and the agent are talking to different servers, the token in use belongs to a different server, the workspace name collides with another repository, or the server was unreachable and writes are waiting. Each entry below starts from the text deciduous 1.1.0 prints. Every entry was captured by provoking it, except where the entry says otherwise.

Two commands answer most questions:

```bash
deciduous remote status      # this copy vs the server: waiting writes, rejected writes, rows that differ
claude mcp get deciduous     # where Claude Code's agent is actually writing
```

`remote status` exits 0 only when it prints `In sync`; anything waiting, rejected or different exits 1.

## Setting up

### `init` stops with "no terminal to ask"

```text
Error: /home/you/code/my-app has no server configured ([remote] in .deciduous/config.toml), and there is no terminal to ask where its graph should live. Choose one:

    deciduous remote setup --local                  # this machine: a background service, PostgreSQL on localhost:5432
    deciduous remote setup --database-url <url>     # this machine's server, a PostgreSQL you name
    deciduous remote setup --docker-postgres        # this machine: PostgreSQL and the server in Docker
    deciduous remote setup --url <url>              # a server someone else runs

then rerun this command. Tests and CI in a throwaway directory can set DECIDUOUS_NO_SERVER=1.
```

**Cause.** `init` was run from a script, an agent's shell, or anything else without a terminal on stdin, in a project with no `[remote]`. It exits 1, but it has already written `.claude/`, `CLAUDE.md`, `.deciduous/`, `.gitignore`, `.gitattributes` and `.github/workflows/cleanup-decision-graphs.yml` before stopping, so the directory looks initialized.

**Fix.** Run the `remote setup` form that fits, then `deciduous init` again. To join a team server: `DECIDUOUS_MCP_TOKEN=<token> deciduous remote setup --url https://deciduous.example.com`. `DECIDUOUS_NO_SERVER=1` is for throwaway directories; it gives a project with no server at all (see [Without a shared server](local.md#no-server-at-all)).

### "the server rejected the token (401)"

```text
Error: reached https://deciduous.example.com but the server rejected the token (401). Check DECIDUOUS_MCP_TOKEN.

Nothing was stored or written.
```

`remote init` words it slightly differently: `Error: the server rejected the token (401). Check DECIDUOUS_MCP_TOKEN.` followed by `Nothing was written; the project is unchanged.` Both exit 1 and change nothing.

**Causes**, in the order worth checking:

1. **The URL ends in `/mcp`.** The CLI wants the base URL; the agent wants `<base>/mcp`. Given `.../mcp`, the CLI asks for `.../mcp/health`, which is behind authentication, and reports a token problem that is really a path problem:

   ```text
   Error: cannot reach http://127.0.0.1:4000/mcp: the server rejected the token (401). Check DECIDUOUS_MCP_TOKEN.
   ```

   Drop the `/mcp`.
2. **A stale `DECIDUOUS_MCP_TOKEN` in this shell.** The environment wins over the stored token. `env | grep DECIDUOUS_MCP_TOKEN`, then `unset` it or correct it.
3. **The stored token belongs to another server.** There is one stored token per machine, not one per URL. The message says "Check DECIDUOUS_MCP_TOKEN" even when the token came from `~/.config/deciduous/credentials`. See [One token per machine](local.md#one-token-per-machine).
4. **The token is wrong.** Ask whoever runs the server. (`remote login --url` also says "Check DECIDUOUS_MCP_TOKEN" when the bad token came from stdin.) `deciduous remote login --url https://deciduous.example.com` reads a token from stdin, checks it against that server, and only then stores it (mode 0600).

### "cannot reach ... Connection refused"

```text
Error: cannot reach http://127.0.0.1:9: transport error: http://127.0.0.1:9/health: Connection Failed: Connect error: Connection refused (os error 61)
```

Nothing listens at that address and port. For a team server, check the URL with whoever runs it and `curl https://deciduous.example.com/health` (it answers `ok` without a token). For this machine's server, the port is `DECIDUOUS_PORT` in `~/.config/deciduous/server/.env` and the log is `~/.config/deciduous/server/logs/server.log`.

A name that does not resolve fails the same way, with `Dns Failed: resolve dns name 'deciduous.invalid:443': failed to lookup address information`.

### "no such endpoint (404)"

```text
Error: cannot reach http://127.0.0.1:4000/deciduous-mcp: no such endpoint (404). Is the URL right, including any path prefix?
```

The host answered but not at that path. A server behind a reverse proxy usually lives under a prefix (the reference deployment is `https://bobbby.online/deciduous-mcp`); a server reached directly has none. The base URL is the one where `curl <base>/health` prints the body `ok`. An empty 200 does not count.

### "the server's response was not a graph" / "was not a claim"

```text
Error: reached https://example.com but the server's response was not a graph: Failed to read JSON: EOF while parsing a value at line 1 column 0
```

`remote init` prints `the server's response was not a claim: ...` with the same JSON error. This is the same mistake as the 404 above, a missing path prefix, behind a proxy that answers unknown paths with an empty 200 instead of a 404. Caddy does this with only a `handle_path` block. Fix the URL, and give the proxy a catch-all 404 ([Run a server](run-a-server.md#3-put-https-in-front-of-it)).

### "timed out reading response"

The CLI gives `/health` 15 seconds. A server that is up but slow to answer (one production server took 2-7 s per `/health` on 2026-11-08) can push `remote setup --url` over that, and it fails with `timed out reading response` before anything is stored. Run it again. If it keeps happening, the proxy or the server is struggling; `curl -w '%{time_total}\n' https://deciduous.example.com/health` shows by how much.

### "no token for the shared graph"

```text
Error: no token for the shared graph.

Store one (kept outside every repository, mode 0600):

    deciduous remote login

or set DECIDUOUS_MCP_TOKEN for a one-off.
```

Neither `DECIDUOUS_MCP_TOKEN` nor `~/.config/deciduous/credentials` holds a token. Note that `remote setup --url` with a token only in the environment does not store it: the next shell without the variable gets this message. `deciduous remote login --url <base>` stores it.

### "invalid workspace name"

```text
Error: the server returned 422: invalid workspace name "a/b": it looks like a path; pass the repo root's basename
```

Workspace names are one path segment. The server trims and lowercases them (`--workspace "My App"` is stored as `my app`), so two spellings that differ only in case are one workspace.

## Writing

### "the local write succeeded but the server did not get it"

```text
Created node 1 (type: goal, title: Written while the server is down) [confidence: 70%] [branch: main]
Warning: the local write succeeded but the server did not get it: transport error: http://127.0.0.1:9/ops: Connection Failed: Connect error: Connection refused (os error 61)
1 write(s) queued in /home/you/code/my-app/.deciduous/remote-log.jsonl. They are sent on the next write, or now with `deciduous remote push` once the server is reachable.
```

**Cause.** The server was unreachable (or took longer than the 10 seconds a write waits for it). Nothing is lost: the write is in the local database and the op is in `.deciduous/remote-log.jsonl`. The command exits 0.

**What to do.** Usually nothing. The next CLI write that reaches the server sends everything queued before it. To see what is waiting:

```text
$ deciduous remote status
Log: 3 write(s) waiting, 0 rejected  (/home/you/code/my-app/.deciduous/remote-log.jsonl)
  waiting   f7b164cd  create goal f50e03ed "Written while the server is down"
  waiting   87b844eb  create action 316bf956 "Second"
  waiting   d8aa0105  link f50e03ed -> 316bf956 (leads_to)
Error: cannot reach http://127.0.0.1:9: ...
```

`remote push` while the server is still down exits 1 and keeps everything (`3 write(s) still waiting in ...`). Once it is back:

```text
$ deciduous remote push
Pushed 3 op(s) to my-app: 3 applied, 0 already there, 0 rejected
```

Pushing twice is safe; the server applies each op at most once, by its id, and reports repeats as "already there". For a server that is up but slow, `DECIDUOUS_OPS_TIMEOUT=<seconds>` raises the 120-second limit on each `remote push` request.

### "the server refused it: the server rejected the token (401)"

```text
Warning: the local write succeeded but the server refused it: the server rejected the token (401). Check DECIDUOUS_MCP_TOKEN.
1 write(s) wait in /home/you/code/my-app/.deciduous/remote-log.jsonl, and every later write will be refused the same way until that is fixed. `deciduous remote status` lists them.
```

The project's server is up but the token in use is not its token: almost always the one-token-per-machine problem after `remote login` or `remote setup --local` for a different server. The write exits 0, so a script will not notice. Put the right token in place ([One token per machine](local.md#one-token-per-machine)), then `deciduous remote push`.

### "belongs to another repository"

```text
Warning: the local write succeeded but the server refused it: workspace "p5" on http://127.0.0.1:4000 belongs to another repository: an unrelated project whose directory is also called p5 (the name is the directory name, lowercased) claimed it first, and this repository shares none of its root commits.

Give this project its own workspace:

    deciduous remote init http://127.0.0.1:4000 --workspace <another-name>

or, if the two repositories really should write one graph, say so by naming it:

    deciduous remote init http://127.0.0.1:4000 --workspace p5
```

**Cause.** The default workspace is the repository's directory name. The server ties a workspace to the root commits of the first repository that writes to it, and refuses one that shares none. Two unrelated projects both called `api` collide here; two clones or worktrees of one repository never do.

**Fix.** Pick a distinct name with `--workspace`, then `deciduous remote push` to send what waited. Naming an existing workspace explicitly is how you say two repositories should share it: `remote init` then reports `claim: adopted`.

A repository with no commit yet cannot be tied to anything. `remote init` warns, and writes wait in the log until the first commit:

```text
  note: this repository has no commit yet, so the server cannot tie workspace "my-app" to it, and an unrelated repository also called my-app could later claim the workspace with these writes in it. Nothing is sent until the first commit; the writes wait in the log, and the first command after that commit sends them.
```

### Rejected writes

`remote status` lists ops the server refused (as opposed to ops it never received) with the server's reason. They stay in the log until you decide: `deciduous remote push --retry-rejected` after fixing the cause, `--drop-rejected` to discard them all, or `--drop <op-id prefix>` for one. None was provoked for this page; the flags are from `remote push --help`.

### "Local only: no remote configured"

```text
$ deciduous remote status
Local only: no remote configured for this project.

    deciduous remote init <url>
```

`status` exits 0 here; `push`, `pull` and `watch` print `Error: no remote configured for this project.` and exit 1. The project was initialized with `DECIDUOUS_NO_SERVER=1`, or `.deciduous/config.toml` lost its `[remote]` table. A linked git worktree with no `[remote]` of its own uses its main checkout's.

## CLI and agent disagree

### The agent's nodes are not in `deciduous nodes`, or yours are not in the agent's

First, `claude mcp get deciduous`, and compare its URL with `[remote] url` in `.deciduous/config.toml`:

```text
$ claude mcp get deciduous
deciduous:
  Scope: User config (available in all your projects)
  Status: ✔ Connected
  Type: http
  URL: https://bobbby.online/deciduous-mcp/mcp
  Headers:
    Authorization: Bearer <token>
```

That is from a machine whose projects point at `http://127.0.0.1:4000`: CLI writes go to one server, the agent's to another, and both report success. `remote setup` does not fix this. It registers Claude Code only when no `deciduous` server is registered, and otherwise prints `Unchanged Claude Code already has a deciduous MCP server` whatever URL that one has. Re-register it yourself, or give the repository its own `.mcp.json`; both are on [Connect your agents](clients.md).

Second, if the URLs agree: CLI reads (`nodes`, `show`, `serve`) come from the local database, which is a cache. It does not refresh itself. After an agent writes over MCP:

```text
$ deciduous remote status
                 local    server
  nodes              1         2

Only on the server (1)
  observation 96096527 "MCP probe obs"

Differs: `deciduous remote pull` takes the server's side.
$ deciduous remote pull
Pulled: my-app <- http://127.0.0.1:4000
  fetched 2 nodes, 0 edges, 0 deletion(s)
  imported 1 new node(s), updated 0, removed 0; imported 0 edge(s), removed 0
```

Third, the workspace. An agent passes `workspace` on each call; a `.mcp.json` may pin one with `X-Deciduous-Workspace`, and a pinned header overrides the argument. `list_workspaces` over MCP shows every name on the server; two near-identical names are usually the split.

### "No node has a change_id starting with ... Run 'deciduous sync'"

```text
Error: No node has a change_id starting with '3bcdac84'. Run 'deciduous sync' if a teammate created it.
It is the server id 3bcdac84-4e0 = change_id 11111111-222 (idempotent), which this clone does not have yet. Run `deciduous remote pull` first.
```

The first line is wrong for a project with a server: `sync` is the no-server command and does nothing for this. The second line is right. An agent created the node over MCP and the local cache has not seen it; `deciduous remote pull`, then the same `link` works (`Note: '3bcdac84' is the server id ... local #7`).

### Themes and tags are missing on the server

They are never sent. `remote status` says so on every run: `Themes and tags are not sent to the server, so they are not compared.`

## MCP over HTTP

These are what an MCP client (or `curl`) sees from `<base>/mcp`.

| Response | Meaning |
| --- | --- |
| `401 {"error":"unauthorized"}` | No `Authorization: Bearer <token>` header, or the wrong token. |
| `404` with `"message":"Session not found"` and `"detail":"This session expired or the server restarted. Send a new initialize request without a session id to start another."` | The session id is unknown: the server restarted, the session idled out, or the client never sent `notifications/initialized` after `initialize` (a session that skips it is dropped, and the next call gets this). Reconnect the MCP server in the client. |
| `405` on `GET /mcp` | Expected. The server offers no server-to-client stream; everything is POST. |
| `query_nodes has no argument "bogus". Its arguments are: branch, file, limit, search, status, type, workspace` | Every tool refuses an argument it does not have and names the ones it does. Usually an agent using a stdio tool name or argument against the HTTP server; see [MCP tools](mcp.md). |
| `node_id is not a node id: "11111111"` | Server tools take the full server UUID (`id` in every result). The CLI's 8-character change_id prefixes do not work over MCP. |

## Reporting a problem

Include `deciduous --version`, the server version (`serverInfo.version` in the MCP `initialize` reply), the command, and the error text as printed. Replace tokens with `<token>`. `deciduous remote watch --url` prints a URL with the token in its query string; never paste that.
