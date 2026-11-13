<!-- Generated from content/local.md by scripts/docs/build.mjs. Edit the source. -->

# Without a shared server

There are two fallbacks for when nobody runs a server you can join. `deciduous remote setup --local` runs the same server on your own machine as a background service, so the CLI and your agents work exactly as they would against a team server, with one person writing. `DECIDUOUS_NO_SERVER=1 deciduous init` skips the server entirely and shares the graph through a git-tracked `.deciduous/graph.json`. Use the first if you might join a team later; use the second only when a running service is not an option.

## A server on this machine

`remote setup --local` looks for a server this machine already has, and installs one if there is none.

> **On Linux, 1.1.0's `--local` does not finish on its own.** With apt's PostgreSQL it needs a role with a password and `--database-url`, and the systemd unit it writes is refused until one line is edited; both are under [What a fresh install does](#what-a-fresh-install-does). The fix for the unit is PR #330, not yet released. `remote setup --docker-postgres` avoids both.

 On a machine that has one, it reuses it and changes nothing about it:

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

The `Unchanged Claude Code already has a deciduous MCP server` line deserves a second look. `remote setup` registers its server with Claude Code at user scope only when `claude mcp get deciduous`, run in the project, finds no `deciduous` server in any scope: user, the project's `.mcp.json` (even one still waiting for approval), or local. If one is found, it is left alone even when it points somewhere else, and the "Restart Claude Code" advice below it will not fix that. Check with `claude mcp get deciduous`; the [Claude Code section of Connect your agents](content/clients.md) has the fix.

### What a fresh install does

The output above was observed on a machine that already had a server. The steps below were run on Linux (debian:12 with systemd 252 and PostgreSQL 15 from apt, deciduous 1.1.0) and checked against `src/server.rs` and `src/local_server.rs`; the macOS service step was read, not run.

| Flag | Server runs as | Database |
| --- | --- | --- |
| `--local` (nothing else) | the release's single-file `deciduous-mcp` executable, as a user service | PostgreSQL on `localhost:5432` as `$USER`, no password, database `deciduous`. Works with Homebrew's PostgreSQL, which trusts localhost. Not with apt's: see below |
| `--database-url <url>` | the same user service | the PostgreSQL you name |
| `--docker` | a container from the release's `deciduous-mcp-docker.tar.gz` | the PostgreSQL from the first two rows; `localhost` is rewritten to `host.docker.internal`. On Linux that is the Docker gateway, where PostgreSQL does not listen out of the box: set `listen_addresses` and add a `pg_hba.conf` line for `172.16.0.0/12`, or use `--docker-postgres` |
| `--docker-postgres` | a container (implies `--docker`) | a new PostgreSQL 17 container |
| `--port <port>` | any of the above | used only on first setup. Without it, `DECIDUOUS_PORT` from the environment is used; failing that, a terminal asks (suggesting a free port in 20000-32767), and without a terminal a free port is chosen and printed (`Chose port N`) |

With no flags and a terminal, `remote setup` asks the same questions as a menu. Without a terminal and without `--local` or `--url`, it stops and prints the four scripted forms.

**Debian and Ubuntu: `--local` alone fails.** The CLI's hint for a machine without PostgreSQL is `sudo apt install postgresql`, but that package's `pg_hba.conf` requires a password for TCP to `127.0.0.1` (`host all all 127.0.0.1/32 scram-sha-256`), the CLI connects over TCP with no password, and nothing creates a `$USER` role. Even with the role, the failure is the client's, not PostgreSQL's:

```text
Preparing postgres://you@localhost:5432/deciduous ...
Error: cannot connect to you@localhost:5432/deciduous: key :password not found in: [ parameters: [], ... username: "you", database: "deciduous", port: 5432, sync_connect: true ]
```

Give the role a password and name it in the URL:

```bash
sudo -u postgres createuser --createdb --pwprompt $USER
deciduous remote setup --database-url postgres://$USER:<password>@localhost:5432/deciduous
```

On debian:12 that created the database and got as far as the service step, which fails in 1.1.0 for the reason below.

Before any of the steps, setup checks for a service manager. On Linux without a `systemctl --user` session (a container, WSL without systemd, a login without a user bus) it prints `Note systemd user services are not available here ...`, offers Docker if there is a terminal, and otherwise still installs the executable, prepares the database and writes `.env`, then stops with the command to run the server in the foreground and rerun setup.

The native install, in order:

1. Downloads `deciduous-mcp-<os>-<arch>` for the CLI's own version, checks it against the release's `checksums.txt`, and installs it as `~/.config/deciduous/server/bin/deciduous-mcp`.
2. Picks the bearer token. If the machine already has one (`DECIDUOUS_MCP_TOKEN`, else `~/.config/deciduous/credentials`) of at least 32 bytes, that becomes the server's token; otherwise 64 random hex digits.
3. Runs the executable once with `DECIDUOUS_MCP_COMMAND=prepare-database`: it connects, creates the database if missing (the role needs `CREATEDB`), migrates, and exits. A refusal usually comes back in PostgreSQL's words (`role "x" does not exist`, `password authentication failed`). Two exceptions were seen on Linux:
   - No password in the URL against a server that wants one gives the `key :password not found` keyword list above.
   - On a machine where another user ran the server executable first, it prints only `Error: error: AccessDenied`, because the executable unpacks `/tmp/libc-musl-<hash>.so` owned by that user and a second user cannot replace it (moving the file away fixed it).

   If nothing listens on 5432 at all, setup finds that out before step 1, before downloading anything, and says so, printing `brew install postgresql@17 && brew services start postgresql@17` / `sudo apt install postgresql`.
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

5. Installs a user service that starts at login and restarts on exit (**on Linux this fails in 1.1.0**; see below): a LaunchAgent `dev.deciduous.server` at `~/Library/LaunchAgents/dev.deciduous.server.plist` on macOS, a systemd user unit `deciduous-server.service` on Linux. The service definition carries only paths; the token and database URL stay in `.env`, which the server reads itself through `DECIDUOUS_ENV_FILE`. Output goes to `~/.config/deciduous/server/logs/server.log`. On Linux, user services stop at logout unless you run `loginctl enable-linger $USER`.
6. Waits for `/ready`, then stores the token in `~/.config/deciduous/credentials` if it differs from the one there, writes `[remote]`, and registers with Claude Code as above. Without Claude Code on `PATH` it prints `Note Claude Code not found; register the server in your client: http://127.0.0.1:<port>/mcp` instead.

**Linux: the service does not start in 1.1.0, or on main.** The unit file the CLI writes puts `WorkingDirectory` in double quotes, and systemd refuses the whole unit. Fixed by [PR #330](https://github.com/notactuallytreyanastasio/deciduous/pull/330), not yet merged or released as of 2026-11-08. Setup stops after writing `.env`:

```text
Error: `systemctl --user restart deciduous-server.service` failed: Failed to restart deciduous-server.service: Unit deciduous-server.service has a bad unit file setting.
```

`systemd-analyze verify` (systemd 252) says why:

```text
/home/you/.config/systemd/user/deciduous-server.service:8: WorkingDirectory= path is not absolute: "/home/you/.config/deciduous/server"
deciduous-server.service: Unit configuration has fatal error, unit will not be started.
Unit deciduous-server.service has a bad unit file setting.
```

Until that is fixed, remove the quotes from that one line, start the service, and run setup again to finish connecting the project:

```bash
sed -i 's/^WorkingDirectory="\(.*\)"$/WorkingDirectory=\1/' ~/.config/systemd/user/deciduous-server.service
systemctl --user daemon-reload && systemctl --user restart deciduous-server.service
deciduous remote setup --local
```

Setup stopped before step 6, so without the last line the project has no `[remote]`, no stored token and no Claude Code entry. Run again, it finds the running server (`Found local server at http://127.0.0.1:<port>`), stores the token and writes `[remote]`. Checked on debian:12: before the edit `systemd-analyze verify` refuses the unit, after it the unit verifies clean. The quoted `ExecStart=` and `Environment=` lines are fine. macOS (launchd) is not affected.

The edit does not last. Whenever the server is not answering (after a logout without linger, or `systemctl --user stop`), the next `remote setup --local` in any project prints `Starting the server at ..., which is not answering`, rewrites the unit with the quotes, and fails the same way. Repeat the `sed` and the restart.

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
rm ~/.config/systemd/user/deciduous-server.service && systemctl --user daemon-reload
rm -r ~/.config/deciduous/server
```

`DEPLOY.md` leaves out the unit file; `disable` only removes its `WantedBy` link, and `systemctl --user` keeps listing the unit until the file is gone. The stored token in `~/.config/deciduous/credentials` and each project's `[remote]` also stay behind.

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

Claude Code has the same shape of problem: one user-scope `deciduous` entry. For a second server, register it at local scope in the repositories that use it, or in a project `.mcp.json`, which needs `DECIDUOUS_MCP_TOKEN` exported where `claude` starts; see [Connect your agents](content/clients.md).

## No server at all

`deciduous init` does not complete without a server. In a terminal it asks; without one it writes its files (`.claude/`, `CLAUDE.md`, `.gitignore`, `.github/` and the rest), then prints the four `remote setup` forms and exits 1. To opt out, say so:

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

`sync --help` says `--check` exits 1 when anything is pending, and the `CLAUDE.md` that init writes tells agents the same (`deciduous sync --check    # what is pending? (exit 1 if anything)`). It exits 0 with changes pending, on 1.1.0 and on main. With the database edited behind the file it printed `Changes would: 1 nodes updated from records` or `Changes would: 1 nodes set back to graph.json's version`, depending on how the row was edited, and exited 0 both times. Read its output rather than relying on the exit status in a hook. Ordinary writes are mirrored into the file as they happen, so in day-to-day use there is rarely anything pending.

Numeric ids are per clone (`b` numbered "Option from B" 2, `a` numbered "Option from A" 2). Refer to nodes across machines by change_id prefix: `deciduous link 94e4b9f7 01ed84dd`.

What this mode does not have: an MCP server over HTTP, `check_activity`, live `remote watch`, and anything faster than a git push. Agents can write through `deciduous mcp`, the stdio server, against the local database, but init registers it with no client (it writes no `.mcp.json`). For Claude Code, in the project:

```text
$ claude mcp add --transport stdio --scope project deciduous -- deciduous mcp
Added stdio MCP server deciduous with command: deciduous mcp to project config
$ cat .mcp.json
{
  "mcpServers": {
    "deciduous": {
      "type": "stdio",
      "command": "deciduous",
      "args": [
        "mcp"
      ],
      "env": {}
    }
  }
}
```

(Claude Code 2.1.319; the format is on [Claude Code's MCP page](https://code.claude.com/docs/en/mcp).) The stdio tools are not the server's. They take local ids, link with `link_nodes`, and `add_node` has no `parent_id`, although the generated `CLAUDE.md` says "create and link in one call: `add_node` with `parent_id`". The stdio server answers that with `add_node has no argument "parent_id". Its arguments are: branch, commit, confidence, description, files, node_type, prompt, title`. See [MCP tools](content/mcp.md). `graph.json` also carries every prompt and description you write, so treat it like any other committed file.

### Moving to a server later

Point the project at a server with `deciduous remote init https://deciduous.example.com --workspace my-app` (or `remote setup --url`, or `--local`). If you registered the stdio server above, remove it first (`claude mcp remove deciduous -s project`, and commit the `.mcp.json` change): while any server called `deciduous` is registered, `remote setup` prints `Unchanged` and leaves it, and `remote init` registers nothing with Claude Code at all. The history written before the project had a remote goes up with it. From a clone of the two-clone repository above, pointed at an empty workspace:

```text
$ deciduous remote init http://127.0.0.1:4000 --workspace docsref-seed2
Remote: http://127.0.0.1:4000
  workspace: docsref-seed2
  claim: claimed (by this repository's root commit)
  server holds 0 nodes, 0 edges, 0 documents
  sent 2 local node(s), 1 edge(s) the server lacked
```

`deciduous remote status` then says In sync. If it ever lists rows as "only here" that no op covers, `deciduous remote push --seed` sends them; it adds what the server lacks and never changes a row the server already has. [Operating a server](content/operating.md) covers the other push flags. Once a project has a `[remote]`, `deciduous sync` is no longer the step that shares anything.

`graph.json` does not go away. Every write is still mirrored into it, the ignore rules still track it, and `deciduous sync` still syncs it. Commit `.deciduous/config.toml` so teammates' clones pick up the `[remote]`. A teammate who wrote nodes before pulling it has rows the server never got and no op to send them; `remote push` says `Nothing to push`, and `remote status` names the fix:

```text
Only here (1)
  action b4b791a1 "A wrote before pulling"  (no op covers it: `deciduous remote push --seed`)
$ deciduous remote push --seed
Seeded: my-app -> http://127.0.0.1:4000
  nodes 1 of 1
  edges 0 of 0
```

Once every clone has the `[remote]`, the server is the shared copy and committing `graph.json` only duplicates it. To stop, drop `!.deciduous/graph.json` from `.gitignore` and `git rm --cached .deciduous/graph.json`.
