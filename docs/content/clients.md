# Connect your coding agents

Every client connects to the same thing: `<base>/mcp` over Streamable HTTP, with `Authorization: Bearer <token>`. What differs is where each client keeps that registration and whether it pins a workspace. The one thing to get right is that the agent's URL, token and workspace match what the CLI uses in `.deciduous/config.toml`; when they do not, the CLI and the agent write to two different graphs and neither complains.

| Value | Where it comes from | Example |
| --- | --- | --- |
| Base URL | `url` under `[remote]` in `.deciduous/config.toml` | `https://deciduous.example.com` |
| MCP URL | base URL + `/mcp` | `https://deciduous.example.com/mcp` |
| Token | the server's operator; stored by `deciduous remote login` | `<token>` |
| Workspace | `workspace` under `[remote]` | `my-app` |
| Branch | the current git branch, passed on every write | `main` |

Agents never need database credentials. One token opens every workspace on a server; see [Architecture](architecture.md).

## Claude Code

### User scope: one registration for every repository

This is what `deciduous remote setup` and `deciduous init` create when Claude Code has no `deciduous` server yet:

```sh
claude mcp add --scope user --transport http deciduous https://deciduous.example.com/mcp \
  --header "Authorization: Bearer $DECIDUOUS_MCP_TOKEN"
```

No workspace header: each tool call carries `workspace`, and the server's instructions tell the agent to take it from `[remote]` in `.deciduous/config.toml`, or else the repository root's directory name. The shell expands `$DECIDUOUS_MCP_TOKEN` when the command runs, so the token is written into `~/.claude.json`, outside every repository, in plain text.

If a `deciduous` server is already registered, setup leaves it as it is and prints `Unchanged Claude Code already has a deciduous MCP server`, whatever URL it points at. Check it every time you join a new server:

```sh
$ claude mcp get deciduous
deciduous:
  Scope: User config (available in all your projects)
  Status: ✔ Connected
  Type: http
  URL: https://deciduous.example.com/mcp
  Headers:
    Authorization: Bearer <token>
```

If `URL` is not your `[remote] url` + `/mcp`, `claude mcp remove deciduous -s user` and add it again as above. `No MCP server named "deciduous"` (exit 1) means nothing is registered; `remote setup` adds it, or run the `add` line yourself.

Because there is one user-scope registration per machine, every repository on the machine talks to the same server through it. A developer whose repositories live on two different servers needs project registrations for at least one of them.

### Project scope: `.mcp.json`, pinned to one workspace

A `.mcp.json` in the repository root is shared with everyone who clones it, holds no secret, and pins the workspace:

```json
{
  "mcpServers": {
    "deciduous": {
      "type": "http",
      "url": "https://deciduous.example.com/mcp",
      "headers": {
        "Authorization": "Bearer ${DECIDUOUS_MCP_TOKEN}",
        "X-Deciduous-Workspace": "my-app"
      }
    }
  }
}
```

Claude Code expands `${DECIDUOUS_MCP_TOKEN}` from its own environment when it starts, so the variable must be set in the shell that launches `claude`; it does not read `~/.config/deciduous/credentials`. Claude Code asks you to approve a project server the first time, and when a project server and a user server share a name, project scope takes precedence. (The expansion, the approval and the precedence are Claude Code behaviour, taken from its documentation; this page did not test them.)

`X-Deciduous-Workspace` wins over the `workspace` argument of every call. Checked against a server: with the header set to `my-app`, `query_nodes({workspace: "some-other-repo"})` returned `my-app`'s nodes. That protects against an agent guessing the wrong workspace. It is not access control: the same token, without the header, reaches every workspace.

After either change, restart Claude Code. The tools appear as `mcp__deciduous__add_node`, `mcp__deciduous__query_nodes` and so on, 21 of them ([MCP tools](mcp.md)).

## OpenCode

`deciduous init --opencode` writes `AGENTS.md`, `opencode.json` (which only adds `AGENTS.md` to the instructions), `.opencode/commands/`, `.opencode/skills/`, an update-check plugin, and a custom tool, `.opencode/tools/deciduous.ts`. That tool runs the `deciduous` CLI (`deciduous add goal ...`, `deciduous link ...`), and in a project with `[remote]` every CLI write goes to the server before the command exits. So OpenCode writes to the shared graph with no MCP registration at all, as long as `remote setup` has been run in the repository.

To give OpenCode the server's MCP tools as well, register the endpoint as a remote MCP server in its config. OpenCode's documentation (2026-11, its V2 config) shows this shape; it was not run for this page:

```jsonc
{
  "$schema": "https://opencode.ai/config.json",
  "instructions": ["AGENTS.md"],
  "mcp": {
    "servers": {
      "deciduous": {
        "type": "remote",
        "url": "https://deciduous.example.com/mcp",
        "oauth": false,
        "headers": {
          "Authorization": "Bearer {env:DECIDUOUS_MCP_TOKEN}",
          "X-Deciduous-Workspace": "my-app"
        }
      }
    }
  }
}
```

OpenCode's syntax for an environment variable is `{env:NAME}`, not Claude Code's `${NAME}`. `"oauth": false` stops it trying an OAuth login; deciduous only takes the bearer token.

## Windsurf

`deciduous init --windsurf` writes `.windsurf/rules/deciduous.md` (the workflow rules), `.windsurf/hooks.json` with a version-check hook, and, in 1.1.0, the Claude Code files too (`CLAUDE.md`, `.claude/`). It does not register an MCP server. Add `https://deciduous.example.com/mcp` in Windsurf's MCP settings as an HTTP server with the `Authorization: Bearer <token>` header, in whatever form your Windsurf version takes; Windsurf's MCP documentation now redirects to Devin's, and this page has not tested a Windsurf configuration. Without MCP, the agent can still write through the CLI from its terminal, which reaches the server.

## Any other MCP client

The server speaks Streamable HTTP. A client must:

- `POST` JSON-RPC to `<base>/mcp` with `Authorization: Bearer <token>`.
- Send `Accept: application/json, text/event-stream`.
- Keep the `mcp-session-id` header from the `initialize` response and send it on every later request.

This is the handshake, done by hand:

```sh
$ curl -s -D - https://deciduous.example.com/mcp \
    -H "Authorization: Bearer $DECIDUOUS_MCP_TOKEN" \
    -H 'Content-Type: application/json' \
    -H 'Accept: application/json, text/event-stream' \
    -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"probe","version":"0"}}}'
...
mcp-session-id: session_GOYlpWIM6imVxCMqh0M=
...
```

The result's `serverInfo` is `{"name": "deciduous-mcp", "version": "1.1.0"}`, and its `instructions` field carries the logging rules the agent should follow (quoted on [For agents](for-agents.md)). A client that shows the instructions to its model gets the workflow without any project file.

Codex, Cursor and others take the same URL and headers in their own config format. Their syntax for referencing an environment variable differs per client; keep the token out of any file you commit.

### Not the same thing: `deciduous mcp`

`deciduous mcp` is a different MCP server: it runs on stdin/stdout, on the machine, against the local database, with 34 tools under different names (`link_nodes`, `search_nodes`, `trace_chain`, `list_nodes`, ...). If your agent's tool list has those names, it is talking to this one, not to `<base>/mcp`.

In a project with `[remote]` its writes do reach the server, the same way CLI writes do (an `add_node` through it showed up as In sync in `remote status`). But it sends no `instructions` on connect, and its tools take local integer ids. Prefer the HTTP endpoint; use `deciduous mcp` only for a client that cannot speak HTTP, or for the no-server mode on [Local](local.md).

## Making the CLI and the agent agree

Three settings have to match, and nothing checks that they do:

1. **URL.** `[remote] url` + `/mcp` is the URL in the client's registration (`claude mcp get deciduous`).
2. **Token.** The CLI uses `$DECIDUOUS_MCP_TOKEN` if it is set, otherwise `~/.config/deciduous/credentials` (under `$XDG_CONFIG_HOME` if that is set). The client uses whatever its registration holds. A stale `DECIDUOUS_MCP_TOKEN` in your shell overrides a fresh `remote login`.
3. **Workspace.** `[remote] workspace` is what the agent passes as `workspace`, or what the `X-Deciduous-Workspace` header says.

Then prove it in both directions. Write a goal with the CLI and ask the agent to find it with `query_nodes`; the `change_id` it reports starts with the CHANGE column `deciduous nodes` prints. Have the agent write a node and run `deciduous remote status`: the node is listed under `Only on the server`, and `deciduous remote pull` brings it down. If the agent's node is not there, it went to another server or another workspace. `list_workspaces` on the agent's side shows where.

One trap in that check: the two sides use different ids. The CLI shows small local integers and an 8-character change_id prefix; the server's tools use UUIDs. `add_node`'s `parent_id` must be the server UUID from `query_nodes`. A change_id is refused:

```
parent_id is not a node id: "b1710c03"
```
