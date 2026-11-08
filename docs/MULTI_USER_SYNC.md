<!-- Generated from content/local.md by scripts/docs/build.mjs. Edit the source. -->

# Without a shared server

There are two fallbacks for when nobody runs a server you can join. `deciduous remote setup --local` runs the same server on your own machine as a background service, so the CLI and your agents work exactly as they would against a team server, with one person writing. `DECIDUOUS_NO_SERVER=1 deciduous init` skips the server entirely and shares the graph through a git-tracked `.deciduous/graph.json`. Use the first if you might join a team later; use the second only when a running service is not an option.

## A server on this machine

`remote setup --local` looks for a server this machine already has, and installs one if there is none. On a machine that has one, it reuses it and changes nothing about it:

```text
$ deciduous remote setup --local
deciduous remote setup
Every project writes its decision graph to a shared server. This connects /home/you/code/my-app
to one: a server on this machine, or one someone else runs.

   Found local server at http://127.0.0.1:4000
   Wrote .deciduous/config.toml [remote] url = http://127.0.0.1:4000
   Connected http://127.0.0.1:4000 holds 0 nodes, 0 edges in workspace my-app
   Unchanged Claude Code already has a deciduous MCP server

Done: this project writes to http://127.0.0.1:4000. Restart Claude Code so it picks up the server;
it sends the logging instructions when it connects.
```

"This machine has a server" means `~/.config/deciduous/server/.env` exists. That settings file is authoritative once written: every later `--local`, in every project, points at the server it describes. The flags that choose how a new server is built do not replace an existing one. `--docker` on a machine with a native server is refused:

```text
Error: this machine's server is already set up to run as a background service (/Users/you/.config/deciduous/server/.env), and it is kept that way. To replace it, stop it and move that file away first; its database is not touched.
```

and `--database-url` against the same native server is accepted but ignored, with one line saying so: `Using the server already set up in ~/.config/deciduous/server/.env; its database and port stand`.

The `Unchanged Claude Code already has a deciduous MCP server` line deserves a second look. `remote setup` registers its server with Claude Code at user scope only when no `deciduous` server is registered there. If one is, it is left alone even when it points somewhere else, and the "Restart Claude Code" advice below it will not fix that. Check with `claude mcp get deciduous`; the [Claude Code section of Connect your agents](content/clients.md) has the fix.

### What a fresh install does

The output above was observed on a machine that already had a server. The steps below are what `src/server.rs`, `src/local_server.rs` and `deciduous_mcp/DEPLOY.md` say happens on a machine without one; they were read, not run.

| Flag | Server runs as | Database |
| --- | --- | --- |
| `--local` (nothing else) | the release's single-file `deciduous-mcp` executable, as a user service | PostgreSQL on `localhost:5432` as `$USER`, no password, database `deciduous` |
| `--database-url <url>` | the same user service | the PostgreSQL you name |
| `--docker` | a container from the release's `deciduous-mcp-docker.tar.gz` | the PostgreSQL from the first two rows; `localhost` is rewritten to `host.docker.internal` |
| `--docker-postgres` | a container (implies `--docker`) | a new PostgreSQL 17 container |
| `--port <port>` | any of the above | used only on first setup; asked otherwise, with a free port in 20000-32767 suggested |

With no flags and a terminal, `remote setup` asks the same questions as a menu. Without a terminal and without `--local` or `--url`, it stops and prints the four scripted forms.

The native install, in order:

1. Downloads `deciduous-mcp-<os>-<arch>` for the CLI's own version, checks it against the release's `checksums.txt`, and installs it as `~/.config/deciduous/server/bin/deciduous-mcp`.
2. Picks the bearer token. If the machine already has one (`DECIDUOUS_MCP_TOKEN`, else `~/.config/deciduous/credentials`) of at least 32 bytes, that becomes the server's token; otherwise 64 random hex digits.
3. Runs the executable once with `DECIDUOUS_MCP_COMMAND=prepare-database`: it connects, creates the database if missing (the role needs `CREATEDB`), migrates, and exits. A refusal comes back in PostgreSQL's words (`role "x" does not exist`, `password authentication failed`). If nothing listens on 5432 at all, setup says so and prints `brew install postgresql@17 && brew services start postgresql@17` / `sudo apt install postgresql`.
4. Writes `~/.config/deciduous/server/.env`, mode 600. This machine's file has these keys (values other than the secrets shown):

   ```text
   DECIDUOUS_SERVER_MODE=native
   DECIDUOUS_SERVER_VERSION=1.1.0
   DECIDUOUS_BIND_ADDRESS=127.0.0.1
   DECIDUOUS_PORT=4000
   DECIDUOUS_MCP_TOKEN='<token>'
   DATABASE_URL='<ecto url>'
   DB_SSL=false
   DB_SSL_VERIFY=full
   DB_SSL_CA_FILE=
   POOL_SIZE=10
   DECIDUOUS_PREPARE_DATABASE=true
   ```

5. Installs a user service that starts at login and restarts on exit: a LaunchAgent `dev.deciduous.server` at `~/Library/LaunchAgents/dev.deciduous.server.plist` on macOS, a systemd user unit `deciduous-server.service` on Linux. The service definition carries only paths; the token and database URL stay in `.env`, which the server reads itself through `DECIDUOUS_ENV_FILE`. Output goes to `~/.config/deciduous/server/logs/server.log`. On Linux, user services stop at logout unless you run `loginctl enable-linger $USER`.
6. Waits for `/ready`, then stores the token in `~/.config/deciduous/credentials` if it differs from the one there, writes `[remote]`, and registers with Claude Code as above.

The server binds `127.0.0.1` because `DECIDUOUS_BIND_ADDRESS` says so; a server started by hand without that setting listens on every interface. Every start re-runs the migrations (`DECIDUOUS_PREPARE_DATABASE=true`), and `deciduous update` in a project that writes to this server replaces an older executable and restarts the service. Docker installs are not upgraded by `update`.

A database URL may carry `?sslmode=`. The CLI maps it onto the server's TLS settings: `require` → `DB_SSL=true DB_SSL_VERIFY=none`, `verify-ca` → `ca`, `verify-full` → `full`. `disable`, `allow` and `prefer` all turn TLS off; `prefer` does not mean "try TLS" here. Any other query parameter is refused rather than dropped. The meaning of each mode is on [Architecture](content/architecture.md#tls-to-postgresql).

The first log lines after a start say what the server is doing. From this machine's `server.log`:

```text
[info] Database TLS: off (DB_SSL unset or false)
[info] Events.Listener: listening on graph_events
[info] Running DeciduousMcp.Web.Router with Bandit 1.13.1 at 127.0.0.1:4000 (http)
```

To remove a native install (from `DEPLOY.md`; the database is left in PostgreSQL):

```bash
launchctl bootout gui/$(id -u)/dev.deciduous.server      # macOS
rm ~/Library/LaunchAgents/dev.deciduous.server.plist
systemctl --user disable --now deciduous-server.service  # Linux
rm -r ~/.config/deciduous/server
```

### One token per machine

The CLI keeps exactly one stored token, in `~/.config/deciduous/credentials`, not one per server URL. If you write to a local server for some projects and to a team server for others, only one of them can use the stored token. Two things replace it without asking:

- `deciduous remote login --url https://deciduous.example.com` stores the team token over the local one.
- `deciduous remote setup --local` stores the local server's token over whatever is there, whenever they differ.

The other server's projects then fail on every write. The write itself still succeeds locally and the command exits 0; the op waits in the log:

```text
Created node 6 (type: observation, title: written with the other server's token) [branch: main]
Warning: the local write succeeded but the server refused it: the server rejected the token (401). Check DECIDUOUS_MCP_TOKEN.
1 write(s) wait in /home/you/code/my-app/.deciduous/remote-log.jsonl, and every later write will be refused the same way until that is fixed. `deciduous remote status` lists them.
```

`DECIDUOUS_MCP_TOKEN` is read before the stored file, so the working arrangement is: store the token you use most, and export the other one in the environment of the projects that need it (a per-directory env tool such as direnv, or a wrapper script). Once the right token is in place, `deciduous remote push` sends what waited.

The ordering also matters the first time. A brand-new local server adopts whatever token the machine already holds (step 2 above), so installing a local server after `remote login` to a team server gives the local server the team's token. Both then accept the same secret, and rotating the team token later breaks the local server's projects too. Run `remote setup --local` with `DECIDUOUS_MCP_TOKEN` unset and the credentials file empty if you want the two kept apart. (This ordering was read from `machine_token()` in `src/server.rs`, not run.)

Claude Code has the same shape of problem: one user-scope `deciduous` entry. For a second server, add a project-scoped `.mcp.json` in the repositories that use it; see [Connect your agents](content/clients.md).

## No server at all

`deciduous init` will not finish without a server. In a terminal it asks; without one it prints the four `remote setup` forms and exits 1. To opt out, say so:

```text
$ DECIDUOUS_NO_SERVER=1 deciduous init
   Creating .gitignore (track .deciduous/graph.json, ignore the database)
   Creating .gitattributes (merge the graph file record by record, fold it in PR diffs)
   Configured git merge driver for graph records
   Creating .deciduous/graph.json (run `deciduous sync` to fill it, then commit it)
   ...
   Skipped shared graph server (DECIDUOUS_NO_SERVER is set)
```

The ignore rules keep the database private and track two files:

```text
# .gitignore
.deciduous/*
!.deciduous/config.toml
!.deciduous/graph.json

# .gitattributes
.deciduous/graph.json merge=deciduous linguist-generated=true
```

and the merge driver goes into `.git/config`, which git never shares:

```text
merge.deciduous.name deciduous decision graph record
merge.deciduous.driver deciduous merge-record %O %A %B
```

Every write goes to the local database and is mirrored into `graph.json` at once. Each write also says where it went, so nobody mistakes this mode for a shared one:

```text
Note: written to /home/you/code/my-app/.deciduous/deciduous.db only; this project has no shared server. `deciduous remote setup` connects one.
```

### Two clones, one graph

This was run with a bare origin and two clones, `a` and `b`. In `a`: add a goal, `deciduous sync`, commit `.gitignore .gitattributes .deciduous/config.toml .deciduous/graph.json` by name, push. A fresh clone `b` sees the goal immediately (the database follows `graph.json` on the next command), but its `.git/config` has no merge driver until something registers it. `deciduous sync` does:

```text
$ deciduous sync
Configured git merge driver for the graph file (this clone)
Synced /home/you/code/b/.deciduous/graph.json (1 nodes, 0 edges, 0 themes, 0 tags on disk)
  Database and records already agree
```

Then both clones changed the graph at once. `a` added an option, linked it, and set the goal `active`; `b` added a different option, linked it, and replaced the goal's prompt. Both committed; `a` pushed; `b` pulled:

```text
$ git pull --no-edit origin HEAD
Merge made by the 'ort' strategy.
 .deciduous/graph.json | 24 +++++++++++++++++++++++-
$ deciduous nodes
ID    CHANGE    TYPE         STATUS     TITLE
1     94e4b9f7  goal         active     Shared goal
3     d04a258b  option       pending    Option from A
2     01ed84dd  option       pending    Option from B
```

No conflict markers, both new options and both edges, and node 1 carries `a`'s status and `b`'s prompt. The driver merges record by record, and a record both sides changed field by field, breaking ties on `updated_at`. Deletes are tombstones, so a delete reaches clones that already had the node. A file that does end up with conflict markers (merged in a clone with no driver registered) is repaired by the next `deciduous sync`.

The routine:

```bash
git pull
deciduous sync          # takes in what the pull brought, registers the driver if missing
deciduous sync --check  # before pushing: reports what sync would change, writes nothing
git add .deciduous/graph.json
```

`sync --help` says `--check` exits 1 when anything is pending. In the one pending case provoked here (the database edited behind the file), it printed `Changes would: 1 nodes set back to graph.json's version` and exited 0, so read its output rather than relying on the exit status in a hook. Ordinary writes are mirrored into the file as they happen, so in day-to-day use there is rarely anything pending.

Numeric ids are per clone (`b` numbered "Option from B" 2, `a` numbered "Option from A" 2). Refer to nodes across machines by change_id prefix: `deciduous link 94e4b9f7 01ed84dd`.

What this mode does not have: agents writing over MCP (there is no HTTP server; `deciduous mcp` over stdio works against the local database, see [MCP tools](content/mcp.md)), `check_activity`, live `remote watch`, and anything faster than a git push. `graph.json` also carries every prompt and description you write, so treat it like any other committed file.

### Moving to a server later

Point the project at a server with `deciduous remote init https://deciduous.example.com --workspace my-app` (or `remote setup --url`, or `--local`). The history written before the project had a remote goes up with it. From a clone of the two-clone repository above, pointed at an empty workspace:

```text
$ deciduous remote init http://127.0.0.1:4000 --workspace docsref-seed2
Remote: http://127.0.0.1:4000
  workspace: docsref-seed2
  claim: claimed (by this repository's root commit)
  server holds 0 nodes, 0 edges, 0 documents
  sent 2 local node(s), 1 edge(s) the server lacked
```

`deciduous remote status` then says In sync. If it ever lists rows as "only here" that no op covers, `deciduous remote push --seed` sends them; it adds what the server lacks and never changes a row the server already has. [Operating a server](content/operating.md) covers the other push flags. Once a project has a `[remote]`, `deciduous sync` is no longer the step that shares anything.
