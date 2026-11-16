<!-- Generated from content/for-agents.md by scripts/docs/build.mjs. Edit the source. -->

# For agents: setting up and using deciduous

This page is written to you, the coding agent that was told "set up deciduous here" or "use deciduous". The graph lives on a server the user's team runs; your job is to connect this repository to that server, prove the connection, and then write to the graph while you work. The one thing you must not do is guess which server. Ask.

Everything here is true of deciduous 1.1.0. The output shown was captured from it.

## First, find out what is already there

Run these from the repository root and read the results before doing anything else:

```sh
deciduous --version                      # deciduous 1.1.0
git rev-parse --show-toplevel
cat .deciduous/config.toml 2>/dev/null   # look for a [remote] table
claude mcp get deciduous                 # Claude Code only; read-only
```

- **`[remote]` with `url` and `workspace` is present:** the repository is connected. Skip to [Verify](#verify-the-connection).
- **No `.deciduous/`, or no `[remote]`:** it needs setting up. Ask the user first (next section).
- **Your tool list has `mcp__deciduous__add_node`, `query_nodes`, `check_activity`:** you are connected to a deciduous server over HTTP. **It has `link_nodes`, `search_nodes`, `trace_chain`:** you are on the local `deciduous mcp` stdio server instead, which has different tools and sends no logging instructions; tell the user.

## Ask the user, never invent

Before any setup command, ask:

1. **Which server?** Its base URL, for example `https://deciduous.example.com`. Do not make one up, do not assume `http://127.0.0.1:4000`, and do not pick `remote setup --local` because it needs no answer. `--local` installs a server and a background service on this machine; that is the user's decision, covered on [Local](content/local.md).
2. **Is the token already on this machine?** It is if `~/.config/deciduous/credentials` exists or `DECIDUOUS_MCP_TOKEN` is set. If not, the user gets it from whoever runs the server and stores it themselves:
   ```sh
   deciduous remote login --url https://deciduous.example.com
   ```
   It reads the token from stdin and checks it against the server before storing it. Never ask the user to paste the token into the conversation, never put it in a file in the repository, a node, or a commit. Do not run `remote login` or `remote logout` yourself: there is one stored token per machine, and login replaces it for every repository on the machine.
3. **Should the integration files be committed** (`CLAUDE.md`, `.claude/`), or kept to this checkout?

If the user says they have no server and want one, point them at [Run a server](content/run-a-server.md); do not stand one up unasked.

## Set up

With the URL from the user and the token stored:

```sh
deciduous remote setup --url https://deciduous.example.com
deciduous init
```

Order matters. Your shell is not a terminal, and `init` in a repository with no `[remote]` cannot ask where the graph lives. It writes its files, then fails:

```
Error: /path/to/my-app has no server configured ([remote] in .deciduous/config.toml), and there is no terminal to ask where its graph should live. Choose one:

    deciduous remote setup --local                  # this machine: a background service, PostgreSQL on localhost:5432
    deciduous remote setup --database-url <url>     # this machine's server, a PostgreSQL you name
    deciduous remote setup --docker-postgres        # this machine: PostgreSQL and the server in Docker
    deciduous remote setup --url <url>              # a server someone else runs
```

That is a question for the user, not a menu for you. Run `remote setup --url` with the URL they gave you, then `init` again; it skips files that exist. Do not set `DECIDUOUS_NO_SERVER=1` to get past it; that exists for throwaway test directories.

Use `init --opencode`, `--windsurf` or `--both` if the user runs those assistants ([Clients](content/clients.md)).

What can go wrong at this step, by message:

| Message | Cause | What to do |
| --- | --- | --- |
| `no token: set DECIDUOUS_MCP_TOKEN or run 'deciduous remote login --url ...' first` | No token on this machine | Ask the user to run `remote login` |
| `server rejected the token (401) ... Nothing was stored or written.` | Wrong token for this server | Tell the user; nothing changed |
| `cannot reach <url>: ... timed out reading response` | `/health` took over 15 s | Run it once more, then report it |
| `Warning: the local write succeeded but could not be sent: ... this repository has no commit yet` | The server ties a workspace to the repository's root commit | After the user's first commit, `deciduous remote push` |

### Claude Code's registration may point elsewhere

`remote setup` registers the server with Claude Code only if no server called `deciduous` is registered. If one is, it prints `Unchanged Claude Code already has a deciduous MCP server` and leaves it alone, even when it points at a different server. Compare the `URL` from `claude mcp get deciduous` with `[remote] url` + `/mcp`. If they differ, your MCP writes and the CLI's go to two different graphs.

Do not change the registration yourself; it is the user's machine-wide configuration. Tell them, and give them the fix:

```sh
claude mcp remove deciduous -s user
claude mcp add --scope user --transport http deciduous https://deciduous.example.com/mcp \
  --header "Authorization: Bearer $DECIDUOUS_MCP_TOKEN"
```

Claude Code loads MCP servers at start. A new or changed registration reaches you only after the user restarts it, so until then you cannot see the tools; say so instead of looking for them.

## Verify the connection

Report each check to the user as pass or fail, with its output. A setup that writes to the wrong place is worse than one that fails.

1. **CLI and server agree.**
   ```
   $ deciduous remote status
   Remote: https://deciduous.example.com
   Workspace: my-app
   Log: 0 write(s) waiting, 0 rejected  (...)

                    local    server
     nodes              1         1
     edges              0         0

   In sync: no writes waiting, and every node, edge and document matches field by field.
   ```
   Exit 0 is In sync, exit 1 is anything else.
2. **Your MCP connection reaches the same server.** `claude mcp get deciduous` shows `[remote] url` + `/mcp`, `Status: ✔ Connected`.
3. **Your writes land in this workspace.** Write a node with `add_node` (the goal you are working under as `parent_id`, if any), then run `deciduous remote status`. Your node appears under `Only on the server`:
   ```
   Only on the server (1)
     option cad0b58d "Token bucket per API token in Redis"

   Differs: `deciduous remote pull` takes the server's side.
   ```
   `deciduous remote pull` brings it into the local copy, and status is In sync again. If your node is not listed, it went to another workspace or another server: `list_workspaces` shows where.

Leave the node; it records that the connection was checked.

## While you work

The server sends you these rules when you connect. They are the rules; the rest of this section only adds detail.

> - The user asks for something: a goal, with their message verbatim in `prompt`.
> - You see more than one way to do it: options. You pick one: a decision whose title names what it beat.
> - You change code or config: an action. It works or fails: an outcome with the number or the error.
> - You learn a fact that changes the plan: an observation. An earlier choice turns out wrong: a revisit.

**Session start: read before writing.** `query_nodes` with `workspace` and `branch`, then `ask_graph` with the topic of the request. `check_activity` shows who else has been writing where in the last five minutes. If the work continues an existing goal, put new nodes under it rather than starting a second one. If other agents are working in parallel, `read_messages` with `unanswered_for` set to your label.

**Every write carries `workspace` and `branch`.** The workspace is `workspace` under `[remote]` in `.deciduous/config.toml`; without one, the repository root's directory name, also from a worktree (a worktree under `.claude/worktrees/fix-x` belongs to the repository, not to a workspace called `fix-x`). The branch is the current git branch. If the client sends an `X-Deciduous-Workspace` header, that wins over your argument.

**Create and link in one call.** `add_node` with `parent_id` makes the node and its edge together. `parent_id` is the server's UUID for the parent, as `query_nodes` returns it in `id`; a CLI change_id is refused (`parent_id is not a node id: "b1710c03"`). Never send `add_edge` in the same batch as the `add_node` whose id it needs: the id does not exist yet. To make a retried `add_node` safe, give it a `change_id` of your choosing; sending it again returns the node already made.

**One step, one call.** `capture_conversation_turn` writes a step's goal, observations, decision, action and outcome together, all or nothing; `parent_node_id` puts it under the goal you are working on.

**Commits.** After a commit, log the action or outcome with `commit` set to the hash.

**Do not log your own process:** reading files, searching, planning, "starting work". The test: would the user put it in a PR description?

**When a line of work ends:** `close_thread` writes the closing outcome and can mark the goal completed; then `find_orphans` and link anything it lists except root goals. Tell the user which nodes you added.

**Another session writing your branch does not block you.** Agents in parallel coordinate on the message board (`post_message`, `read_messages`, `reply_to`), never in a scratch file. [Teams](content/teams.md) has the details.

### Writing from the CLI instead

In a project with `[remote]`, `deciduous add` and `deciduous link` write to the server before the command exits, so a shell is a fallback when MCP is not available. CLI node references are local ids or change_id prefixes, not server UUIDs:

```sh
deciduous add goal "Add rate limiting to the public API" -c 90 -p "<the user's message, verbatim>"
deciduous link b1710c03 2 -r "possible approach"
```

The local copy does not see MCP writes until it pulls. Linking the CLI to a node you just made through MCP fails until `deciduous remote pull`:

```
$ deciduous link 80f8630d 6 -r "implements the decision"
Error: No node has a change_id starting with '80f8630d'. Run 'deciduous sync' if a teammate created it.
It is not a server id in workspace my-app either.
$ deciduous remote pull
...
$ deciduous link 80f8630d 6 -r "implements the decision"
Created edge 5 (7 -> 6 via leads_to)
```

(With a server, the fix is `remote pull`, not the `sync` the message names.)

More: [the workflow](content/workflow.md), [recovering context](content/recovery.md), [MCP tools](content/mcp.md), [troubleshooting](content/troubleshooting.md).
