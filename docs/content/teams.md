# Many agents, one workspace

Every agent and every person on a project writes to the same workspace on the same server, each on its own git branch. Nothing locks: two sessions writing one branch both succeed and their nodes interleave. What keeps them from colliding is that each can see the others, through `check_activity`, [the message board](board.md), and `took_from` edges that say whose idea a node built on.

## One workspace per repository

The workspace is `workspace` under `[remote]` in `.deciduous/config.toml`. Commit that file, and every clone and worktree writes to the same graph:

```toml
[remote]
url = "https://deciduous.example.com"
workspace = "my-app"
```

`deciduous init` writes a `.gitignore` that ignores `.deciduous/*` except `config.toml` and `graph.json`. `remote init` on its own writes none: after it, `git status` shows all of `.deciduous/` untracked, cache and write log included. If you joined with `remote init` only, stage `.deciduous/config.toml` by name and ignore the rest of `.deciduous/` yourself.

The server ties a workspace to the root commit of the first repository that claims it. `remote init` claims at once, before anything is written. `remote setup` does not: the claim is made by the next `remote status`, `remote pull` or write. A clone shares that root, so it is let in:

```console
$ deciduous remote init https://deciduous.example.com --workspace my-app
Remote: https://deciduous.example.com
  workspace: my-app
  claim: verified (by this repository's root commit)
```

An unrelated repository that merely has the same directory name is refused, which is what stops two projects called `api` on one server from writing one graph by accident:

```console
$ deciduous remote init https://deciduous.example.com
Error: workspace "my-app" on https://deciduous.example.com belongs to another repository: an unrelated
project whose directory is also called my-app (the name is the directory name, lowercased) claimed it
first, and this repository shares none of its root commits.
...
Nothing was written; the project is unchanged.
```

Naming the workspace explicitly with `--workspace` is how you say two repositories really should share one graph. The server then answers `claim: adopted` and accepts writes from both. A repository with no commit yet cannot be checked, so its writes wait in the log until the first commit.

## Worktrees

A worktree whose checkout includes `.deciduous/config.toml` uses it. One made from a commit before that file existed inherits the main checkout's:

```console
$ git worktree add -b old-wt ../my-app-old HEAD~1 && cd ../my-app-old
$ deciduous remote status
Remote: https://deciduous.example.com
Workspace: my-app
From: this worktree's config names no server; its writes go to the main checkout's (.../my-app/.deciduous/config.toml)
```

Each worktree keeps its own local cache (`.deciduous/deciduous.db`), so a new worktree starts empty: run `deciduous remote pull` there before `deciduous nodes`. The server holds one copy; the caches are per directory.

Agents need the same name. The server's instructions tell an agent to read `workspace` from `.deciduous/config.toml`, and to fall back to the repository root's directory name, not the worktree's. If an agent client could still guess wrong, pin it: an `X-Deciduous-Workspace` header on the MCP connection overrides the `workspace` argument on every call ([Clients](clients.md)).

## Branches

Pass the real git branch on every MCP write. The server cannot see your checkout. The CLI fills it in from git (`[branch: agent-tests]`); `-b` overrides it.

Two worktrees for two agents:

```bash
git worktree add -b agent-api   ../my-app-api   HEAD
git worktree add -b agent-tests ../my-app-tests HEAD
```

Worktrees start from the commit you name and do not carry uncommitted changes.

## check_activity: who is writing where

Call it before a burst of writes and after each milestone.

```javascript
check_activity({workspace: "my-app"})
```

Trimmed, from a workspace with two MCP sessions and the CLI writing:

```json
{
  "sessions": [
    { "session": "GOYltCLK", "client": "docs-work", "branch": "agent-api",   "is_you": true,  "seconds_ago": 0 },
    { "session": "GOYljadZ", "client": "docs-work", "branch": "agent-api",   "is_you": false, "seconds_ago": 0 },
    { "session": "GOYljadZ", "client": "docs-work", "branch": "agent-tests", "is_you": false, "seconds_ago": 12 },
    { "session": "cli:4c3b", "client": "deciduous CLI", "branch": "agent-tests", "is_you": false, "seconds_ago": 29 },
    { "session": "cli:4c3b", "client": "deciduous CLI", "branch": null,          "is_you": false, "seconds_ago": 18 }
  ],
  "active_sessions": 3,
  "window_seconds": 300,
  "branches": [
    { "branch": "agent-tests",
      "last_node": { "node_type": "observation", "title": "Reuse the RFC 7807 shape in fixtures", "change_id": "81c332df-..." },
      "writers": [ ... ] }
  ],
  "branches_total": 3
}
```

What the fields mean, as observed:

- `sessions` is every (session, branch) pair that wrote in the last five minutes, not every running agent. An agent that has only read does not appear.
- `client` is the name the MCP client sent in `initialize` (`docs-work` was the probe client that produced this output); the CLI shows as `deciduous CLI`.
- The CLI's session is named after the repository's root commit (`cli:` plus its first characters), so every clone and worktree of one repository shows as one CLI session. `link` and `status` writes record `branch: null`.
- `branches` lists the latest node on each branch, newest first, writers included, whether or not anyone is writing now. `branches` (the argument) sets how many, default 20; `branches_total` says how many exist.

Nothing in it is a lock. Until 1.0.8 the server took a ten-second lease per (workspace, branch) and refused a second writer; it stopped one-shot clients from writing after themselves and could not stop the CLI, so it was removed. Two sessions writing `agent-api` at the same moment both got `"Node created and linked"`. Every write is atomic on its own; `log_decision`, `capture_conversation_turn` and `close_thread` are each one transaction.

Locks never protected source files either. File ownership is the coordinator's job; an action's `files` list is how one agent shows another what it is about to touch, and `query_nodes` with `file` finds what was decided about a path.

## The message board

Agents ask each other questions and announce interface changes on the board, never in a scratch file. `@label` addresses a message, a reply takes it off the addressee's list, and each agent asks what is waiting for it:

```console
$ deciduous board post --as tests -s "@api is the 422 body stable?" -m "I am asserting code and field, not message. @api confirm the shape."
Posted #222, to api
$ deciduous board read --unanswered api
```

Over MCP the same two operations are `post_message` and `read_messages`, on the same board. Start each agent with its label, `DECIDUOUS_AGENT_LABEL=api claude`, and the hook that `deciduous init` installs shows it what is waiting at session start and on each prompt.

[The message board](board.md) has a full exchange between a CLI and an MCP agent, the rules for what counts as addressed and answered, and the limits.

## Borrowing across branches: took_from

When one agent's node changes another's plan, record the borrow as an edge, not just a sentence. A `took_from` edge runs from the node you took the idea from (any branch) to your node that used it.

MCP, in one call:

```javascript
log_observation({
  workspace: "my-app", branch: "agent-tests",
  title: "Reuse the RFC 7807 shape in fixtures",
  related_to: "32d05bcb-1b2b-4b7b-b3b8-170a8aa8b7fb",
  took_from: "183e9e9a-153a-4141-9fb2-8312fe486012",
  why: "The API side already emits problem+json; fixtures that match it keep the tests honest."
})
```

```json
{ "id": "3c2a5b0b-...", "message": "Observation logged",
  "took_from": { "from_node_id": "183e9e9a-...", "from_change_id": "9ad1b0b7-2c85-4383-b9d9-902411abb3ac", "edge_id": "236548bb-..." } }
```

`took_from` and `related_to` take the server UUID or the full `change_id`. A short prefix is refused: `took_from is not a node id: "9ad1b0b7"`. `add_edge` and `add_node` also take `edge_type: "took_from"`.

CLI, where a `change_id` prefix is enough:

```console
$ deciduous link 17365e54 2b787037 -t took_from -r "Tests use the API agent's 422 contract instead of inventing one"
Created edge 24 (1 -> 24 via took_from)
```

Taking a decision does not make it right in your context. Say in `why` what you checked.

## Watching the workspace live

```console
$ deciduous remote watch --types observation,outcome
Watching: my-app
13:37:11  agent-api  observation "Watch probe observation"
13:37:11  main      outcome     "Watch probe outcome"
```

It prints one line per node write from every writer, MCP and CLI alike; the action written in between was filtered out by `--types`. `--branch` (repeatable) narrows it, `--edges` adds edge events, `--json` prints raw frames. When the connection drops it reconnects with a backoff that starts at one second and doubles to thirty, and asks the server for every event after the last one it saw. Up to 5,000 missed events are replayed; past that the server reports a gap, and you should query the graph. A newly started `remote watch` shows only new writes. `--url` prints a WebSocket URL with the token in its query string, and `--claude-code` prints that URL inside a ready-to-paste Monitor tool call for Claude Code, so keep the output of both out of screenshots and logs.

## What a coordinator still does

Deciduous does not assign tasks, merge branches or run tests. Mark the shared goal `completed` (`close_thread` with `goal_node_id`) only after the branches are integrated and the combined result checked, not when one agent finishes its part.
