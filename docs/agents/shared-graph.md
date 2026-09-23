# Shared graph

A shared server is one PostgreSQL database behind an HTTP MCP endpoint. Every
project on it is a **workspace**, and a workspace's branches are where
different agents write side by side. Use it when more than one agent, or more
than one machine, needs to see the others' reasoning while it is happening.

Two separate things have to be configured, and they do not know about each other:

1. **The MCP client**, so your tool calls reach the server. This is how nodes
   get written.
2. **The project's `[remote]` table**, so the CLI can compare, pull and push.
   This does not make the CLI write to the server.

## 1. The token

The server authenticates with one bearer token. The user should store it
themselves, so it never passes through your context:

```sh
deciduous remote login --url https://example.com/deciduous-mcp
# Token (read from stdin, not saved to shell history):
# Token: stored in ~/.config/deciduous/credentials
#   mode 0600, outside every repository
```

`remote login` checks the token against the server before storing it. The CLI
reads `DECIDUOUS_MCP_TOKEN` first and falls back to that file. Never write the
token into a file inside a repository, into `config.toml`, or into a node.

## 2. Register the MCP server with your client

The endpoint is the base URL plus `/mcp`. It speaks Streamable HTTP.

**Claude Code, once per machine (user scope).** One registration serves every
repository, and each call names its workspace:

```sh
claude mcp add --scope user --transport http deciduous \
  https://example.com/deciduous-mcp/mcp \
  --header "Authorization: Bearer $DECIDUOUS_MCP_TOKEN"
claude mcp get deciduous    # Status: ✔ Connected
```

The shell expands `$DECIDUOUS_MCP_TOKEN` when the command runs, so the token
ends up in `~/.claude.json`. That file is outside every repository, but it is
plain text. If the user would rather not store the token there, use the
project form below with `${DECIDUOUS_MCP_TOKEN}`, which Claude Code expands
from its environment at startup.

**Claude Code, per project (`.mcp.json`, safe to commit).** This pins every call
to one workspace:

```json
{
  "mcpServers": {
    "deciduous": {
      "type": "http",
      "url": "https://example.com/deciduous-mcp/mcp",
      "headers": {
        "Authorization": "Bearer ${DECIDUOUS_MCP_TOKEN}",
        "X-Deciduous-Workspace": "my-project"
      }
    }
  }
}
```

Other clients take the same URL and headers. Configure them the way that
client handles HTTP MCP servers and secrets. `${...}` expansion is specific to
each client.

After registering, the user must restart the client. The tools appear as
`mcp__deciduous__add_node` and so on. Check the list against [MCP tools](tools.md).
If you see `link_nodes`, `search_nodes` or `trace_chain`, the client is running
the old `deciduous mcp` stdio server instead. Remove it and register this one.

## 3. Which workspace a call lands in

Highest precedence first:

1. The `X-Deciduous-Workspace` header, if the client sends one. No tool
   argument can override it.
2. The `workspace` argument on the call. Pass the **basename of the git
   repository root**, lowercased, even when you are in a subdirectory or a
   worktree. A worktree at `.claude/worktrees/fix-x` belongs to the repository
   it was made from, not to a workspace called `fix-x`.
3. `scratch`, for anything outside a git repository.

Read tools also accept `workspace: "*"`, every project at once. Write tools
refuse it. An unknown workspace name is created on first write, not rejected,
so a typo makes a new, empty project. Run `list_workspaces` when unsure.

Always pass `branch` too, set to the current git branch. Writes take a short
advisory lock per workspace and branch. Two agents on different branches never
contend. A second session writing to a branch another session holds is refused,
and the refusal names the holder. Work on your own branch, and call
`check_activity` before a burst of writes to see who else is active.

## 4. Point the project at the server

```sh
deciduous remote init https://example.com/deciduous-mcp
# Remote: https://example.com/deciduous-mcp
#   workspace: my-project
#   server holds 0 nodes, 0 edges, 0 documents
#
# Written to .deciduous/config.toml. The token stays in DECIDUOUS_MCP_TOKEN.
```

It verifies the server and token, then writes:

```toml
[remote]
url = "https://example.com/deciduous-mcp"
workspace = "my-project"
```

Commit `config.toml`. It holds no secret, and every clone then resolves to the
same workspace. `--workspace NAME` overrides the name when the directory name
is wrong for the graph.

To configure many repositories at once:

```sh
deciduous remote adopt ~/code --url https://example.com/deciduous-mcp --dry-run
deciduous remote adopt ~/code --url https://example.com/deciduous-mcp
```

`adopt` only touches projects that already have a `.deciduous/`, and never
repoints one that is aimed at a different server. Show the user the dry run first.

## The write boundary

This is what `remote init` does **not** do, observed on 1.0.2:

```sh
$ deciduous add goal "does a CLI write reach the server" -c 50
Created node 1 (type: goal, title: does a CLI write reach the server) [confidence: 50%]
$ deciduous remote status
                 local    remote
  nodes              1         0
  edges              0         0

Drift: local holds more than the server. `deciduous remote push` to send it up.
```

So, in a project with a shared server:

- **Write through the HTTP MCP tools.** They land on the server immediately and
  every other agent can see them.
- Do not log with the CLI. If something was written that way, `deciduous
  remote push` sends what the server lacks. Tell the user you ran it.
- `deciduous remote status` compares counts only. `counts match` is not the
  same as identical: the two sides can hold the same number of different nodes.

## Watching other agents

```sh
deciduous remote watch                          # one line per write, as it lands
deciduous remote watch --types outcome,observation --branch main
deciduous remote watch --claude-code            # prints a ready-made Monitor call
```

When you use another agent's idea, record the borrow:
`log_observation(..., took_from: "<their node id or change_id>")` writes your
observation and a `took_from` edge from their node in one call.

## Running a server

If the user wants their own server, the deployment guide is
[deciduous_mcp/DEPLOY.md](https://github.com/notactuallytreyanastasio/deciduous/blob/main/deciduous_mcp/DEPLOY.md):
a Docker bundle with one setup script, or a native `deciduous-mcp-*` binary
against an existing PostgreSQL 17. A server listens on `127.0.0.1:4000` by
default. It needs a TLS proxy before any other machine can reach it, and one
token grants access to every workspace on it.
