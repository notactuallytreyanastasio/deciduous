# Pick up where the last session stopped

A new session starts with nothing but the repository, and the graph on the server holds everything the last one decided. Recovery is reading that graph before writing anything: the goal still open, the decision under it, the last outcome, and anything another agent left on the board for you. An agent does it with the server's MCP tools; a person, or an agent running `/recover`, does it with the CLI after refreshing the local cache.

## What the agent is told

On connect, the server sends its instructions in the MCP `initialize` reply. The part about starting:

```text
At the start of a session, read before writing: `query_nodes` for this branch, `ask_graph` for the topic.
If the work continues an existing goal, attach to it. When a line of work ends: `close_thread`, then
`find_orphans`.
```

So the first calls of a session look like this:

```javascript
query_nodes({workspace: "my-app", type: "goal", status: "active"})
query_nodes({workspace: "my-app", branch: "agent-api", limit: 30})
ask_graph({workspace: "my-app", question: "why did we change the retry deadline?"})
check_activity({workspace: "my-app"})
read_messages({workspace: "my-app", unanswered_for: "api"})
```

`query_nodes` returns summaries, newest first: id, type, status, title, branch, `change_id`, confidence. No descriptions and no rationale. Follow the ids that matter with `show_node`, which adds the description, metadata (prompt, files, commit), documents, and every edge in and out with its rationale.

`ask_graph` finds entry nodes by text and then follows the edges the question asks for: "why" follows choices and revisits, "history" or "pivot" follows revisits and superseded nodes. Each result says how it was reached. For that question it returned the superseded action and decision, the observation that killed them, the revisit and the decision that replaced them, and the goal:

```text
action      superseded  anchor     Apply a 30s deadline to the retry loop
decision    pending     anchor     Per-caller deadline passed in the request context
revisit     pending     anchor     Reconsidering: Use an overall deadline, not an attempt cap
observation pending     anchor     A fixed 30s deadline fails batch jobs that legitimately wait
option      pending     rationale  Cap the attempt count
decision    superseded  anchor     Use an overall deadline, not an attempt cap
goal        active      anchor     Bound retries for failed requests
```

`get_ancestors` from an unfinished action walks back to the decisions and the goal behind it; `get_descendants` from a goal shows everything done under it.

Two traps:

- `add_node` defaults to `pending`. A search for `status: "active"` misses goals nobody activated; query `pending` too before concluding there is no open goal.
- A `superseded` decision is history, not the plan. Look for the revisit after it.

If the work continues an existing goal, attach to it with `parent_id`. Do not log the original request as a second goal with the same title.

## From the CLI: refresh the cache, then read

The CLI reads the local cache, `.deciduous/deciduous.db`, which only knows what this checkout wrote or last pulled. Agents writing over MCP never touch it. Check first:

```console
$ deciduous remote status
...
                 local    server
  nodes              6        23
  edges              6        22

Only on the server (17)
  observation 048e98e6 "The router already parses JSON before handlers run"
  decision 17365e54 "Validate at the HTTP boundary, not after the write"
  ...
Differs: `deciduous remote pull` takes the server's side.
```

`remote status` exits 1 when anything differs. Then:

```console
$ deciduous remote pull
Pulled: my-app <- https://deciduous.example.com
  fetched 23 nodes, 22 edges, 1 deletion(s)
  imported 17 new node(s), updated 0, removed 0; imported 16 edge(s), removed 0
$ deciduous remote status | tail -2
In sync: no writes waiting, and every node, edge and document matches field by field.
Themes and tags are not sent to the server, so they are not compared.
```

`remote pull` first sends anything still waiting in `.deciduous/remote-log.jsonl`, then takes the server's side for every node and edge, including deletions. It does not download document bytes ([Evidence](evidence.md#documents-from-the-cli-reach-the-server)). A fresh clone or a new worktree has an empty cache; `remote pull` is the first thing to run there.

Then read:

```console
$ deciduous nodes --branch "$(git rev-parse --abbrev-ref HEAD)"
$ deciduous show 5ca33b18
$ deciduous narratives pivots
$ deciduous pulse --summary
$ deciduous board read --unanswered api
```

Local ids differ in every checkout. `show`, `link`, `status` and `doc attach` take a `change_id` prefix or the server UUID as well. A prefix only resolves against the cache, so before a pull it fails with advice meant for the no-server mode:

```text
Error: No node has a change_id starting with '5ca33b18'. Run 'deciduous sync' if a teammate created it.
It is not a server id in workspace my-app either.

A node is named by its local id (12, or #12 when digits could also be a change_id prefix), its change_id or a prefix of it (the CHANGE column of `deciduous nodes`, at least 4 hex characters), or, in a project with a [remote], the server's id for it (the `id` MCP tools return) or a prefix of that.
```

A prefix needs only 4 hex characters. With a `[remote]` the fix is `deciduous remote pull`, not `sync`. A server UUID does not get you around the pull. Before it, the CLI recognises the UUID and tells you to pull:

```console
$ deciduous show 14a9f4d8-b37a-...
Error: No node has a change_id starting with '14a9f4d8-b37a-...'. Run 'deciduous sync' if a teammate created it.
It is the server id 14a9f4d8-b37 = change_id 3e460153-12b (Validate create-item requests), which this clone does not have yet. Run `deciduous remote pull` first.
```

After the pull it resolves to the local node:

```console
$ deciduous show 32d05bcb-1b2b-4b7b-b3b8-170a8aa8b7fb
Note: '32d05bcb-1b2b-4b7b-b3b8-170a8aa8b7fb' is the server id 32d05bcb-1b2 = change_id 5ca33b18-c27 (Validate create-item requests): local #11
```

## The /recover command

`deciduous init` writes `.claude/commands/recover.md` to match the project. With a `[remote]` already in `.deciduous/config.toml` (after `remote init` or `remote setup`), the first step is `deciduous remote pull` and `deciduous remote status`, then `nodes`, `nodes --branch`, `edges`, `commands`, `doc list`, the board, and `git status` / `git log`. Without one, you get the git-mode version, which ends with `deciduous sync` instead. If you add `[remote]` after `init`, run `deciduous update` to switch.

Parts of the 1.1.0 template do not work as written:

- `deciduous commands` prints `No commands logged.` in 1.1.0, however many writes the checkout made. Fixed on main (commit ec3ce7a), not in a release yet; the next release logs commands.
- The orphan audit (`deciduous edges | cut -d'>' -f2 ...`) looks for a `>` the tabular `edges` output does not contain, so it flags every node as an orphan: one `CHECK:` line per node, plus one for the table's separator line (`CHECK: ------...`). It also pipes through `awk`, `grep`, `sort` and `cut`, which the template's `allowed-tools: Bash(deciduous:*, git:*, cat:*, tail:*)` does not cover, so Claude Code asks permission to run it. Use `find_orphans` over MCP.
- Step 3, `cat git.log | tail -30`, reads a file nothing creates.
- Its closing section links to `https://notactuallytreyanastasio.github.io/deciduous/` as the "Live graph" and says `PUSH -> Live graph updates`. That is deciduous's own site, not your project's graph.

The template on main adds Step 1.6, which runs `deciduous transcript list` and `deciduous transcript import <session-id>` for sessions the graph does not have yet; 1.1.0 does not ship it, and you can run the import yourself ([Evidence](evidence.md#deriving-the-graph-from-a-transcript)). `deciduous commands` answers on main because of the binary, not the template. Everything else in the list above is unchanged on main: the orphan audit, the `git.log` step and the "Live graph" link are all still there. Delete those lines from your copy of `recover.md`. In a project with a `[remote]`, `deciduous update` then treats the file as yours and leaves it untouched, writing the template it would have used to `.deciduous/update-templates/.claude/commands/recover.md` so you can diff it; you will not get later template fixes without merging them by hand.

## Check the graph against the checkout

The graph says what someone recorded, not what the code is now:

```bash
git status --short
git branch --show-current
git log -5 --oneline
```

Match the branch and commit in the node metadata to the checkout. If an outcome says tests passed and the code has changed since, run them again and record the new result under the same action.

## Leaving a session so the next can recover

The next session reads what you wrote, so end with:

- `close_thread` on each line of work, with the command run and its result, or `success: false` and the error.
- `find_orphans` empty, or each remaining orphan linked to its real cause.
- A reply to every board message addressed to you.
- From the CLI: `deciduous remote status` saying `In sync`. Anything waiting means the server never saw it: `deciduous remote push` sends it.

## When the server is down

CLI writes still succeed locally, exit 0, and say so:

```text
Created node 2 (type: observation, title: written while server down) [branch: main]
Warning: the local write succeeded but the server did not get it: transport error: https://deciduous.example.com/ops: Connection Failed: Connect error: Connection refused (os error 61)
1 write(s) queued in .../.deciduous/remote-log.jsonl. They are sent on the next write, or now with `deciduous remote push` once the server is reachable.
```

`remote status` lists them as `waiting` before it fails on the health check. Once the server answered again, `remote pull` sent the queued write and `remote status` said `In sync`. MCP writes fail outright, since the server is the only place they go. Nothing a teammate cannot see should be described to them as shared. [Operating](operating.md) covers backups and restoring the server; a local `deciduous backup` copies only this machine's cache.
