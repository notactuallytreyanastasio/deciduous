# One decision graph for every agent on the team

Deciduous records why the code is the way it is: what was asked, the approaches considered, the one chosen and what it beat, what was changed, and what happened. The graph lives on a server your team runs. Every developer's CLI and every coding agent writes to it, so the next session, yours or someone else's agent, reads the reasoning instead of guessing it from the diff.

## What runs where

```
 Claude Code, OpenCode, any MCP client          deciduous CLI
   (agents, one per worktree/branch)        (add, link, remote status,
                │                              pull, watch, serve)
                │ HTTPS, bearer token                 │ HTTPS, same token
                │ <base>/mcp                          │ <base>/ops, /export, ...
                ▼                                     ▼
        ┌──────────────────────────────────────────────────┐
        │  deciduous server (deciduous-mcp, Elixir)        │
        │  one workspace per repository                    │
        └──────────────────────────────────────────────────┘
                                │
                                ▼
                     PostgreSQL (the graph)
```

- **The server** is PostgreSQL plus the deciduous server, normally on a VM behind a TLS proxy. It exposes authenticated MCP over HTTP at `<base>/mcp`, and `/health` and `/ready` for monitoring. One server holds many repositories, each in its own workspace. A running example is the reference install at `https://bobbby.online/deciduous-mcp`, on a Hetzner VM behind Caddy.
- **Agents** connect to `<base>/mcp` with a bearer token. On connect the server sends them its logging instructions, so an agent knows when to write a goal, an option, a decision, an action and an outcome without a prompt telling it.
- **The CLI** (`deciduous`) writes to the same server. A project whose `.deciduous/config.toml` has a `[remote]` table sends every `deciduous add` and `deciduous link` to the server before the command exits. The SQLite file in `.deciduous/` is this machine's cache of the server, not a second graph.

One token per server, not per person or per workspace. Anyone holding it can read and write every workspace on that server. Run separate servers for groups that must not see each other's graphs. [Architecture](architecture.md) covers the trust boundary in full.

## Where to go next

- **Your team already has a server:** [Quickstart](quickstart.md). From install to your first node, with your agent writing to the same workspace.
- **You are the one who runs it:** [Run a server](run-a-server.md), a VM with the Docker bundle and a TLS proxy, or the server binary against a managed PostgreSQL.
- **You are an AI agent told to set up or use deciduous:** [For agents](for-agents.md).
- **A client other than Claude Code, or a per-project registration:** [Clients](clients.md).
- **No server, one machine, or offline:** [Local](local.md) covers a server on your own laptop and the git-only mode where the graph is a file in the repository. Both work; neither is how a team shares a graph.

Once connected: [the workflow](workflow.md) for how a piece of work becomes nodes, [teams](teams.md) for many agents on one workspace, and [recovery](recovery.md) for picking up where the last session stopped.
