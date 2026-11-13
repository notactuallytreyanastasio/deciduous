# Join your team's graph server

You need two things from whoever runs the server: its base URL and its token. With those, five commands connect a repository: `remote login` stores the token, `remote setup --url` points the repository at the server, `init` installs the agent integration, and `claude mcp get deciduous` confirms your agent talks to the same server as your CLI. Then `deciduous remote status` says In sync.

Everything below was run against deciduous 1.1.0. The examples use `https://deciduous.example.com` for the server and a repository called `my-app`; the output is real, captured against a test server, with the URL and home directory swapped in. This page does not cover running a server. For that, see [Run a server](run-a-server.md).

## The short version

```sh
brew install notactuallytreyanastasio/tap/deciduous
pbpaste | deciduous remote login --url https://deciduous.example.com   # the token, from your clipboard
cd my-app
deciduous remote setup --url https://deciduous.example.com
deciduous init
claude mcp get deciduous | grep -E 'Status|URL'   # URL must be https://deciduous.example.com/mcp
deciduous remote status                            # In sync
```

Each line is a step below, with its output and what goes wrong at it. The two that catch people: `init` run before `remote setup` offers to install a server on your machine (step 4), and Claude Code can stay pointed at a different server without anything saying so (step 5).

## 1. Install the CLI

```sh
brew install notactuallytreyanastasio/tap/deciduous   # macOS, Linux
cargo install deciduous --locked                      # Rust 1.89 or newer
```

Or download a binary from the [v1.1.0 release](https://github.com/notactuallytreyanastasio/deciduous/releases/tag/v1.1.0): `deciduous-darwin-arm64`, `-darwin-amd64`, `-linux-arm64`, `-linux-amd64`, `deciduous-windows-amd64.exe`, and `checksums.txt` to check them against.

```sh
$ which -a deciduous
/opt/homebrew/bin/deciduous
$ deciduous --version
deciduous 1.1.0
```

If `which -a` lists two different paths, the first one runs. Remove the stale one rather than installing a third. The same path listed twice, as above, is a directory that appears twice in `PATH`, not a second install; delete nothing.

## 2. Get the token

Ask the person who runs the server. They read it from the server's settings file: the value of the `DECIDUOUS_MCP_TOKEN` line, without the single quotes `setup.sh` puts around it (a token pasted with its quotes gets a 401): `.env` in the extracted Docker bundle, or `~/.config/deciduous/server/.env` for a server the CLI installed. It should reach you through your team's secret manager, not chat.

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

Make the repository's first commit before you write anything. `remote setup` itself works in a repository with no commit, but the server ties a workspace to the repository's root commit, so that an unrelated project that happens to share the directory name cannot write into your graph. In a repository with no commit, CLI writes are kept in `.deciduous/remote-log.jsonl` and `deciduous remote push` sends them after the first commit.

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

The workspace is the repository directory's name, lowercased (a repository in `MyApp/` gets `myapp`; a linked worktree uses its main checkout's name). Every clone and worktree of the repository should use the same one, so commit this file. To choose a different name, run `deciduous remote init https://deciduous.example.com --workspace <name>` first. `remote init` only writes `[remote]` and claims the workspace; it does not register Claude Code. Then run `remote setup --url` as above: it keeps that workspace and does the Claude Code registration. `remote setup` has no `--workspace` flag.

Without a stored token it stops before writing anything: ``Error: no token: set DECIDUOUS_MCP_TOKEN or run `deciduous remote login --url https://deciduous.example.com` first``. A token given only through `DECIDUOUS_MCP_TOKEN` works for setup but is not stored by it.

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

  1 and 2 install the release's server executable in /Users/you/.config/deciduous/server
  and start it at login. --docker runs that server in Docker instead.

Choose 1-4 [1]:
```

Type 4 to join your team's server; it asks for the URL, then whether to use the token already stored on this machine (step 2), or for the token if none is stored.

> **Do not just press Enter.** The default is 1: it downloads the server executable, creates and migrates a `deciduous` database on localhost:5432, and installs a background service on your machine, which is not what someone joining a team wants. Pressing Ctrl-D at the menu picks 1 as well.

The choices are the same as these flags: 1 is `remote setup --local`, 2 is `--database-url <url>`, 3 is `--docker-postgres`, 4 is `--url <url>`. If this machine already has a server installed, the menu shows two choices instead: 1) that server, 2) a server someone else runs. Piping input into `init` does not answer the menu: without a terminal it stops and lists the flags, below.

On Linux, 1.1.0's systemd unit for choices 1 and 2 does not even start: systemd rejects its quoted `WorkingDirectory=`. The fix is [#330](https://github.com/notactuallytreyanastasio/deciduous/pull/330), not yet merged or released; until then use choice 3 or run the server by hand.

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

`remote setup` registers the server with Claude Code (user scope, every repository) only when Claude Code has no server called `deciduous` yet. If it already has one, setup prints `Unchanged Claude Code already has a deciduous MCP server` and leaves it alone.

> **It leaves it alone even when that one points at a different server.** Your CLI writes then go to one graph and your agent's to another, and nothing warns you. Setup's last line still says to restart Claude Code so it picks up the server; restarting picks up the old registration.

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

To remove this server, run: claude mcp remove deciduous -s user
```

Where this page shows `<token>`, the real output prints the whole token. To check without putting it on screen, `claude mcp get deciduous | grep -E 'Status|URL'`.

This is the mismatch the step exists for, and it happened while this page was being written: the project pointed at one server and Claude Code at another. The `URL` must be your `[remote] url` followed by `/mcp`. If it is not, replace the registration:

```sh
claude mcp remove deciduous -s user
claude mcp add --scope user --transport http deciduous https://deciduous.example.com/mcp \
  --header "Authorization: Bearer $(cat ~/.config/deciduous/credentials)"
```

`$(cat ...)` reads the token `remote login` stored in step 2; the shell substitutes it when the command runs, and Claude Code stores it in `~/.claude.json`, outside every repository. Do not write `$DECIDUOUS_MCP_TOKEN` here unless you exported that variable: `remote login` does not set it, and with it empty `claude mcp add` still succeeds, registers `Authorization: Bearer` with no token, and `claude mcp get` then reports `✘ Failed to connect ... (HTTP 401)`. A user-scope registration serves every repository on the machine; each tool call names its workspace. To pin one repository to its workspace instead, use a project `.mcp.json` ([Clients](clients.md#claude-code)).

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
2 nodes:
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

With the `graph.json` line gone from `.gitignore` (step 4), CLI writes no longer change `git status`: the SQLite cache and `graph.json` are both ignored. What stays in `git status` is what `init` wrote and you chose not to commit: `?? .gitattributes` and, unless you committed it, `?? .github/`. Delete them if you do not want them listed.

`init` also wrote `.github/workflows/cleanup-decision-graphs.yml`, which deletes the graph images `deciduous dot --auto` puts in PRs; commit it only if you use that. Stage files by name, never `git add -A`.

A teammate who clones the repository runs steps 1 and 2, then `deciduous remote setup --url https://deciduous.example.com`: it keeps the workspace in the committed `[remote]` and registers Claude Code if nothing called `deciduous` is registered yet. (Skipping it leaves Claude Code with no registration at all, and `claude mcp get deciduous` prints `No MCP server named "deciduous"`.) Then step 5, then `deciduous remote pull` to fill their local copy. The committed integration files mean they do not need `init`.

Next: [the workflow](workflow.md) for how a piece of work becomes goal, options, decision, actions and outcomes, and [teams](teams.md) for several agents on one workspace, who talk to each other on [the message board](board.md). When something here does not match what you see, [troubleshooting](troubleshooting.md).
