# Join your team's graph server

You need two things from whoever runs the server: its base URL and its token. With those, five commands connect a repository: `remote login` stores the token, `remote setup --url` points the repository at the server, `init` installs the agent integration, and `claude mcp get deciduous` confirms your agent talks to the same server as your CLI. Then `deciduous remote status` says In sync.

Everything below was run against deciduous 1.1.0. The examples use `https://deciduous.example.com` for the server and a repository called `my-app`; the output is real, captured against a test server, with the URL and home directory swapped in. This page does not cover running a server. For that, see [Run a server](run-a-server.md).

## 1. Install the CLI

```sh
brew install notactuallytreyanastasio/tap/deciduous   # macOS, Linux
cargo install deciduous --locked                      # anywhere with Rust
```

Or download a binary from the [v1.1.0 release](https://github.com/notactuallytreyanastasio/deciduous/releases/tag/v1.1.0): `deciduous-darwin-arm64`, `-darwin-amd64`, `-linux-arm64`, `-linux-amd64`, `deciduous-windows-amd64.exe`, and `checksums.txt` to check them against.

```sh
$ which -a deciduous
/opt/homebrew/bin/deciduous
$ deciduous --version
deciduous 1.1.0
```

If `which -a` lists two paths, the first one runs. Remove the stale one rather than installing a third.

## 2. Get the token

Ask the person who runs the server. They read it from the server's settings file, the `DECIDUOUS_MCP_TOKEN='...'` line in it: `.env` in the extracted Docker bundle, or `~/.config/deciduous/server/.env` for a server the CLI installed. It should reach you through your team's secret manager, not chat.

Store it with `remote login`. It reads one line from stdin, so the token stays out of shell history and the process list, and checks it against the server before writing anything. Pipe it in from wherever you keep it (`pbpaste`, your secret manager's CLI); if you paste it at the prompt instead, the terminal shows it as you paste.

```sh
$ pbpaste | deciduous remote login --url https://deciduous.example.com
Token (read from stdin, not saved to shell history): Token: stored in /Users/you/.config/deciduous/credentials
  mode 0600, outside every repository
```

A wrong token stores nothing:

```
Error: reached https://deciduous.example.com but the server rejected the token (401). Check DECIDUOUS_MCP_TOKEN.

Nothing was stored.
```

(The message names `DECIDUOUS_MCP_TOKEN` even when the token came from stdin.)

**One token per machine.** The credentials file holds a single token, not one per URL. Logging in to a second server replaces the first server's token, and every repository on this machine then sends the new one. If you work against two servers, set `DECIDUOUS_MCP_TOKEN` in the environment for one of them; the CLI reads that variable before the file.

## 3. Point the repository at the server

The repository needs at least one commit first. The server ties a workspace to the repository's root commit, so that an unrelated project that happens to share the directory name cannot write into your graph. In a repository with no commit, CLI writes are kept in `.deciduous/remote-log.jsonl` and `deciduous remote push` sends them after the first commit.

```sh
$ cd my-app
$ deciduous remote setup --url https://deciduous.example.com
deciduous remote setup
Every project writes its decision graph to a shared server. This connects /Users/you/code/my-app
to one: a server on this machine, or one someone else runs.

   Wrote .deciduous/config.toml [remote] url = https://deciduous.example.com
   Connected https://deciduous.example.com holds 0 nodes, 0 edges in workspace my-app
   Unchanged Claude Code already has a deciduous MCP server

Done: this project writes to https://deciduous.example.com. Restart Claude Code so it picks up the server;
it sends the logging instructions when it connects.
```

It writes this to `.deciduous/config.toml`, and nothing secret:

```toml
[remote]
url = "https://deciduous.example.com"
workspace = "my-app"
```

The workspace is the repository directory's name. Every clone and worktree of the repository should use the same one, so commit this file. To choose a different name, `deciduous remote init https://deciduous.example.com --workspace <name>`. `remote setup` has no `--workspace` flag.

Without a stored token it stops before writing anything: `Error: no token: set DECIDUOUS_MCP_TOKEN or run 'deciduous remote login --url ...' first`. A token given only through `DECIDUOUS_MCP_TOKEN` works for setup but is not stored by it.

If setup fails with `cannot reach <url>: ... timed out reading response`, the server's `/health` took longer than the CLI's 15 seconds. Run it again; if it keeps happening, tell whoever runs the server.

## 4. Install the agent integration

```sh
$ deciduous init
...
   Creating .claude/commands/decision.md
   ...
   Creating CLAUDE.md
...
Shared graph server
   Connected https://deciduous.example.com holds 0 nodes, 0 edges in workspace my-app
```

`init` writes the Claude Code integration (`CLAUDE.md`, `.claude/commands/`, `.claude/skills/`, `.claude/settings.json` with an update check and a message-board hook) and then checks the server named in `[remote]`. Use `--opencode`, `--windsurf` or `--both` for other assistants; see [Clients](clients.md).

**If you ran `init` first,** before `remote setup`, it has no server to check and asks where the graph lives. In a terminal the menu is:

```
Where should this project's graph live?

  1) This machine: the deciduous server as a background service, using PostgreSQL on
     localhost:5432 as you (answering now)
  2) This machine: the server as a background service, using a PostgreSQL you name
     (any host, port, user, password)
  3) This machine, in Docker: a new PostgreSQL and the server in containers
  4) A server someone else runs: you need its URL and token
```

Choose 4 to join your team's server; it asks for the URL and then the token. The choices are the same as these flags: 1 is `remote setup --local`, 2 is `--database-url <url>`, 3 is `--docker-postgres`, 4 is `--url <url>`. If this machine already has a server installed, the menu shows two choices instead: 1) that server, 2) a server someone else runs.

Without a terminal (a script, CI, an agent's shell), `init` writes its files and then stops:

```
Error: /Users/you/code/my-app has no server configured ([remote] in .deciduous/config.toml), and there is no terminal to ask where its graph should live. Choose one:

    deciduous remote setup --local                  # this machine: a background service, PostgreSQL on localhost:5432
    deciduous remote setup --database-url <url>     # this machine's server, a PostgreSQL you name
    deciduous remote setup --docker-postgres        # this machine: PostgreSQL and the server in Docker
    deciduous remote setup --url <url>              # a server someone else runs

then rerun this command. Tests and CI in a throwaway directory can set DECIDUOUS_NO_SERVER=1.
```

It exits 1. Run step 3, then `init` again; files that already exist are skipped.

`init` 1.1.0 also sets up the no-server git mode even when `[remote]` is there: `.deciduous/graph.json`, a `!.deciduous/graph.json` line in `.gitignore` that tracks it, a `.gitattributes` line and a git merge driver. With a server, the graph is on the server and `graph.json` is only a local copy that the CLI rewrites on every write; committed, it would conflict between every two developers for nothing. Delete the `!.deciduous/graph.json` line from `.gitignore` and leave `.gitattributes` out of your commits. (`deciduous update` knows this and skips those files in a project with `[remote]`; `init` does not, and running `init` again puts the line back.)

## 5. Make sure Claude Code talks to the same server

`remote setup` registers the server with Claude Code (user scope, every repository) only when Claude Code has no server called `deciduous` yet. If it already has one, setup prints `Unchanged Claude Code already has a deciduous MCP server` and leaves it alone, **even when that one points at a different server.** Your CLI writes then go to one graph and your agent's to another, and nothing warns you.

Check it:

```sh
$ claude mcp get deciduous
deciduous:
  Scope: User config (available in all your projects)
  Status: ✔ Connected
  Type: http
  URL: https://old-server.example.com/deciduous-mcp/mcp
  Headers:
    Authorization: Bearer <token>
```

This is the mismatch the step exists for, and it happened while this page was being written: the project pointed at one server and Claude Code at another. The `URL` must be your `[remote] url` followed by `/mcp`. If it is not, replace the registration:

```sh
claude mcp remove deciduous -s user
claude mcp add --scope user --transport http deciduous https://deciduous.example.com/mcp \
  --header "Authorization: Bearer $DECIDUOUS_MCP_TOKEN"
```

The shell expands `$DECIDUOUS_MCP_TOKEN` when the command runs, so the token is stored in `~/.claude.json`, outside every repository. A user-scope registration serves every repository on the machine; each tool call names its workspace. To pin one repository to its workspace instead, use a project `.mcp.json` ([Clients](clients.md#claude-code)).

Restart Claude Code after any change. The tools appear as `mcp__deciduous__add_node` and so on.

## 6. Write the first node

```sh
$ deciduous add goal "Add rate limiting to the public API" -c 90 \
    -p "Add rate limiting to the public API: 100 requests a minute per token"
Created node 1 (type: goal, title: Add rate limiting to the public API) [confidence: 90%] [prompt: 68 chars] [branch: main]

$ deciduous remote status
Remote: https://deciduous.example.com
Workspace: my-app
Log: 0 write(s) waiting, 0 rejected  (/Users/you/code/my-app/.deciduous/remote-log.jsonl)

                 local    server
  nodes              1         1
  edges              0         0

In sync: no writes waiting, and every node, edge and document matches field by field.
Themes and tags are not sent to the server, so they are not compared.
```

The goal reached the server before `add` returned. `remote status` exits 0 when in sync and 1 when anything differs. (The CLI also printed a warning that a 68-character prompt looks like a summary. Goals carry the user's message verbatim.)

## 7. Your agent writes to the same workspace

Open Claude Code in the repository. On connect the server sends the agent its logging instructions: a goal for each request, options when there is more than one way, a decision naming what it beat, actions and outcomes. Ask it to look at the rate-limiting goal and suggest approaches. It finds the goal with `query_nodes` and writes options under it with `add_node`, passing `workspace: "my-app"` and the current branch.

Your local copy is now behind, and `remote status` says exactly how:

```
                 local    server
  nodes              1         2
  edges              0         1

Only on the server (1)
  option cad0b58d "Token bucket per API token in Redis"

Edges only on the server (1)
  b1710c03 -> cad0b58d (leads_to)

Differs: `deciduous remote pull` takes the server's side.
```

```sh
$ deciduous remote pull
Pulled: my-app <- https://deciduous.example.com
  fetched 2 nodes, 1 edges, 0 deletion(s)
  imported 1 new node(s), updated 0, removed 0; imported 1 edge(s), removed 0

$ deciduous nodes
ID    CHANGE    TYPE         STATUS     TITLE
------------------------------------------------------------------------------
1     b1710c03  goal         pending    Add rate limiting to the public API
2     cad0b58d  option       pending    Token bucket per API token in Redis
```

To see writes as they land, from your agent or anyone else's, keep `remote watch` open in another terminal:

```sh
$ deciduous remote watch
Watching: my-app
13:37:13  main      option      "Fixed window counter in Postgres"
```

If the agent's nodes never show up on the server, it is writing somewhere else: go back to step 5.

## 8. Commit

```sh
git add .deciduous/config.toml .gitignore CLAUDE.md .claude/
git commit -m "chore: connect to the team's deciduous server"
```

With the `graph.json` line gone from `.gitignore` (step 4), `git status` stays clean after CLI writes; the SQLite cache and `graph.json` are both ignored.

`init` also wrote `.github/workflows/cleanup-decision-graphs.yml`, which deletes the graph images `deciduous dot --auto` puts in PRs; commit it only if you use that. Stage files by name, never `git add -A`.

A teammate who clones the repository runs steps 1, 2 and 5, then `deciduous remote pull` to fill their local copy. The committed `[remote]` already names the server and workspace, and the committed integration files mean they do not need `init`.

Next: [the workflow](workflow.md) for how a piece of work becomes goal, options, decision, actions and outcomes, and [teams](teams.md) for several agents on one workspace. When something here does not match what you see, [troubleshooting](troubleshooting.md).
