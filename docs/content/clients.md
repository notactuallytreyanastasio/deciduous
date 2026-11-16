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

This is the `claude mcp add` that `deciduous remote setup --url https://deciduous.example.com` runs (so do the interactive `remote setup` and a `deciduous init` that falls into it in a repository with no server yet), when Claude Code has no `deciduous` server:

```sh
claude mcp add --scope user --transport http deciduous https://deciduous.example.com/mcp \
  --header "Authorization: Bearer $DECIDUOUS_MCP_TOKEN"
```

`deciduous init` in a repository that already has `[remote]` does not run it: it checks the server, prints `Connected ... holds N nodes`, and registers nothing. So a teammate who clones a repository with `[remote]` in it runs `deciduous remote setup --url https://deciduous.example.com`, or the `claude mcp add` line above, themselves. `remote setup` with no flags and no terminal registers nothing either; it exits with an error listing the flags.

No workspace header: each tool call carries `workspace`, and the server's instructions tell the agent to take it from `[remote]` in `.deciduous/config.toml`, or else the repository root's directory name. The shell expands `$DECIDUOUS_MCP_TOKEN` when the command runs, so the token is written into `~/.claude.json`, outside every repository, in plain text.

If a `deciduous` server is already registered, setup leaves it as it is and prints `Unchanged Claude Code already has a deciduous MCP server`. It decides that from the exit status of `claude mcp get deciduous` alone, which is 0 for a registration in any scope, whatever URL it points at, including a project `.mcp.json` still waiting for approval and a server that rejects its token. Read the `Status:` line every time you join a new server:

```sh
$ claude mcp get deciduous
deciduous:
  Scope: User config (available in all your projects)
  Status: ✔ Connected
  Type: http
  URL: https://deciduous.example.com/mcp
  Headers:
    Authorization: Bearer <token>

To remove this server, run: claude mcp remove deciduous -s user
```

A wrong or stale token looks like this (Claude Code 2.1.319, trimmed), and `get` still exits 0:

```
  Status: ✘ Failed to connect
  Issue: Server rejected the configured Authorization header (HTTP 401). ... Error detail: {"error":"unauthorized"}
```

`get` prints the token in clear text; do not paste its output anywhere. If `URL` is not your `[remote] url` + `/mcp`, or the status is `✘ Failed to connect`, `claude mcp remove deciduous -s user` and add it again as above. `No MCP server named "deciduous"` (exit 1) means nothing is registered; `remote setup --url` adds it, or run the `add` line yourself.

### Without the token in `~/.claude.json`

Claude Code's `headersHelper` runs a command when it connects and sends the headers it prints. This one reads the file `deciduous remote login` writes, so the token stays out of `~/.claude.json` and a new `remote login` reaches the agent too:

```sh
claude mcp add-json --scope user deciduous '{"type":"http","url":"https://deciduous.example.com/mcp","headersHelper":"printf '"'"'{\"Authorization\":\"Bearer %s\"}'"'"' \"$(cat ~/.config/deciduous/credentials)\""}'
```

Tested against a server with Claude Code 2.1.319: `claude mcp get deciduous` shows `Status: ✔ Connected` and no `Headers` lines, and `~/.claude.json` holds the helper command, not the token. If `XDG_CONFIG_HOME` is set, the credentials file is under it instead of `~/.config`.

### Local scope: a second server, with no file in the repository

Because there is one user-scope registration per machine, every repository on the machine talks to the same server through it. For a repository on a different server, register it in local scope, which is private to you, applies to that one project, and is also stored in `~/.claude.json`. Run this inside that repository:

```sh
claude mcp add --scope local --transport http deciduous https://other.example.com/mcp \
  --header "Authorization: Bearer $DECIDUOUS_MCP_TOKEN"
```

`claude mcp get deciduous` there then shows `Scope: Local config (private to you in this project)`. `remote setup` in that repository sees it and says `Unchanged`.

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

Claude Code expands `${DECIDUOUS_MCP_TOKEN}` from its own environment, so the variable must be set in the shell that launches `claude`; it does not read `~/.config/deciduous/credentials`. If the variable is unset, the server still loads with the literal text `${DECIDUOUS_MCP_TOKEN}`, and `claude mcp list` warns `Missing environment variables: DECIDUOUS_MCP_TOKEN`.

Until you approve it, the project server is not used. With no other `deciduous` registration, `claude mcp get deciduous` shows ``Status: ⏸ Pending approval (run `claude` to approve)``; with a user-scope one, `get` shows the user registration, and that is the one in use. When the same name is registered in several scopes, Claude Code uses one, in the order local, project, user ([its documentation](https://code.claude.com/docs/en/mcp)); checked here, a local registration was the one `get` showed in a repository that also had `.mcp.json` and a user server.

`X-Deciduous-Workspace` wins over the `workspace` argument of every call. Checked against a server: with the header set to `my-app`, `query_nodes({workspace: "some-other-repo"})` returned `my-app`'s nodes. That protects against an agent guessing the wrong workspace. It is not access control: the same token, without the header, reaches every workspace.

After any of these changes, restart Claude Code. The tools appear as `mcp__deciduous__add_node`, `mcp__deciduous__query_nodes` and so on, 21 of them ([MCP tools](mcp.md)).

## Clients deciduous writes no MCP config for

`deciduous init` takes `--claude`, `--opencode`, `--windsurf` and `--both`, and only the Claude Code path registers an MCP server. For every client below, nothing is connected until you add the entry yourself. `init` ends with `Restart your assistant so it connects to the server; it sends the logging instructions` for OpenCode and Windsurf too; for them that line does not apply.

The snippets below follow each client's current documentation (fetched 2026-11-08, cited per section). None of them was run against a server for this page.

### OpenCode

`deciduous init --opencode` writes `AGENTS.md`, `opencode.json` (which only adds `AGENTS.md` to the instructions), `.opencode/agents/deciduous.md`, `.opencode/commands/`, `.opencode/skills/`, an update-check plugin (`.opencode/plugins/version-check.ts`) and a custom tool, `.opencode/tools/deciduous.ts`.

None of these files knows about a server. In a project with `[remote]`, `.opencode/commands/sync.md` and `recover.md` still teach `deciduous sync` and `git add .deciduous/graph.json`, and with `--opencode` alone `AGENTS.md` says `deciduous sync    # Reconcile .deciduous/graph.json with teammates`. Ignore `/sync`'s git steps there; `deciduous remote status` and `deciduous remote pull` are the steps. (Under `--both`, `AGENTS.md` is converted from the server-aware `CLAUDE.md`; the `.opencode/` files are the same either way.)

The plugin and the custom tool were written for OpenCode 1.x. [OpenCode 2's migration guide](https://opencode.ai/v2/migrate-v1.md) says "V1 plugin implementations do not run in V2", so the update check does nothing there, and [its plugin guide](https://opencode.ai/v2/build/plugins/migrate-v1.md) does not say whether a V1 `tool()` file still loads. In OpenCode 1.x the tool runs the `deciduous` CLI (`deciduous add goal ...`, `deciduous link ...`), and in a project with `[remote]` every CLI write goes to the server before the command exits. Without the tool, the agent reaches the server the same way by running `deciduous` in its shell. Either way that needs `remote setup` run in the repository and at least one commit in it: in a repository with no commit, a CLI write is kept in `.deciduous/remote-log.jsonl` with `Nothing is sent until the first commit`, and sent by the next write after that commit or by `deciduous remote push` (a read such as `deciduous nodes` sends nothing, whatever the message says).

To give OpenCode the server's MCP tools as well, register the endpoint as a remote MCP server. This is the [V1 shape](https://docs.opencode.ai/docs/mcp-servers/), servers directly under `mcp`; [OpenCode 2 still reads it](https://opencode.ai/v2/migrate-v1.md) ("The V1 config format remains supported"), so it works in both:

```jsonc
{
  "$schema": "https://opencode.ai/config.json",
  "instructions": ["AGENTS.md"],
  "mcp": {
    "deciduous": {
      "type": "remote",
      "url": "https://deciduous.example.com/mcp",
      "enabled": true,
      "oauth": false,
      "headers": {
        "Authorization": "Bearer {env:DECIDUOUS_MCP_TOKEN}",
        "X-Deciduous-Workspace": "my-app"
      }
    }
  }
}
```

The [native V2 form](https://opencode.ai/docs/mcp-servers/) puts the server under `mcp.servers` (`"mcp": {"servers": {"deciduous": {...}}}`, without `enabled`). OpenCode's docs warn not to point V1 at a file converted to V2-only shapes, so use it only if everyone on the repository runs OpenCode 2.

OpenCode's syntax for an environment variable is `{env:NAME}`, not Claude Code's `${NAME}`. [An unset variable becomes an empty string](https://opencode.ai/docs/config/), so a missing `DECIDUOUS_MCP_TOKEN` shows up as a 401 from the server, not as a config error. `"oauth": false` stops OpenCode trying an OAuth login; deciduous only takes the bearer token. OpenCode also has `{file:~/...}` substitution, but the credentials file ends in a newline and OpenCode's docs do not say whether `{file:}` trims it, so it is not suggested here.

### Windsurf (Devin Local)

`deciduous init --windsurf` writes `.windsurf/rules/deciduous.md` (the workflow rules), `.windsurf/hooks.json` and `.windsurf/hooks/version-check.sh` (a version check on `pre_write_code`), and the Claude Code files (`CLAUDE.md`, `.claude/`): `--windsurf` without `--opencode` means Claude Code plus Windsurf. `deciduous init --opencode --windsurf` writes the Windsurf and OpenCode files and no Claude Code files.

The rules file does not know about a server: in a project with `[remote]` it still says `deciduous sync    # Reconcile .deciduous/graph.json with teammates`. Use `deciduous remote status` and `deciduous remote pull` instead.

Windsurf's documentation now redirects to [Devin Local's](https://docs.devin.ai/desktop/cascade/mcp). It still loads `.windsurf/rules/*.md` ("The legacy `.windsurf/rules/*.md` ... files are imported by default"), but it documents hooks only in `.devin/hooks.v1.json`, so whether `.windsurf/hooks.json` still runs is unconfirmed.

Devin Local reads MCP servers from `~/.config/devin/mcp_config.json` (user), `.devin/mcp_config.json` (project, committed) or `.devin/mcp_config.local.json` (project, gitignored). Its pages give no remote-server example; [the Devin CLI's configuration reference](https://docs.devin.ai/cli/extensibility/mcp/configuration.md) names the keys `url`, `transport` and `headers` under `mcpServers`, which gives this shape:

```json
{
  "mcpServers": {
    "deciduous": {
      "url": "https://deciduous.example.com/mcp",
      "transport": "http",
      "headers": {
        "Authorization": "Bearer <token>",
        "X-Deciduous-Workspace": "my-app"
      }
    }
  }
}
```

It holds the token itself, so put it in the user file or `.devin/mcp_config.local.json`, never in `.devin/mcp_config.json`. Devin documents `${env:VAR}` only for OAuth fields. Older Windsurf builds read `~/.codeium/windsurf/mcp_config.json` with `serverUrl` instead of `url`, according to third-party guides, not Windsurf's own pages. Without MCP, the agent can still write through the CLI from its terminal, which reaches the server.

### Cursor

[Cursor](https://cursor.com/docs/context/mcp) reads `.cursor/mcp.json` in the project or `~/.cursor/mcp.json`, and resolves `${env:NAME}` in `url` and `headers`:

```json
{
  "mcpServers": {
    "deciduous": {
      "url": "https://deciduous.example.com/mcp",
      "headers": {
        "Authorization": "Bearer ${env:DECIDUOUS_MCP_TOKEN}",
        "X-Deciduous-Workspace": "my-app"
      }
    }
  }
}
```

### Codex CLI

[Codex](https://learn.chatgpt.com/docs/extend/mcp?surface=cli) reads `~/.codex/config.toml`, or `.codex/config.toml` in a project it trusts. `bearer_token_env_var` names the variable it sends as `Authorization: Bearer`:

```toml
[mcp_servers.deciduous]
url = "https://deciduous.example.com/mcp"
bearer_token_env_var = "DECIDUOUS_MCP_TOKEN"
http_headers = { "X-Deciduous-Workspace" = "my-app" }
```

### VS Code

[VS Code](https://code.visualstudio.com/docs/agents/reference/mcp-configuration) reads `.vscode/mcp.json`, with servers under `servers` and a token it prompts for once through an input:

```json
{
  "inputs": [
    { "type": "promptString", "id": "deciduous-token", "description": "deciduous token", "password": true }
  ],
  "servers": {
    "deciduous": {
      "type": "http",
      "url": "https://deciduous.example.com/mcp",
      "headers": {
        "Authorization": "Bearer ${input:deciduous-token}",
        "X-Deciduous-Workspace": "my-app"
      }
    }
  }
}
```

The same page describes a portable format, `mcpServers` in a workspace-root `.mcp.json`, read by Copilot harness sessions, so a repository with the Claude Code `.mcp.json` above may already show up there.

### Claude Desktop and claude.ai

These take the server as a [custom connector](https://claude.com/docs/connectors/custom/add-unlisted) added by URL under Customize > Connectors, not as a local file. The URL must be an HTTPS address reachable from the internet; a server inside a private network needs Anthropic's MCP tunnels. Choose Authentication **No sign-in** and add `Authorization` with the value `Bearer <token>` under **Request headers**, which that page says is "in beta and available to a limited set of organizations". `authorization` is on the standard list, but a custom name such as `X-Deciduous-Workspace` needs Anthropic's approval, so name the workspace in the conversation instead.

## Any other MCP client

The server speaks Streamable HTTP. A client must:

- `POST` JSON-RPC to `<base>/mcp` with `Authorization: Bearer <token>`.
- Send `Accept: application/json, text/event-stream`.
- Keep the `mcp-session-id` header from the `initialize` response and send it on every later request.
- Send `notifications/initialized` after `initialize`, before any other call. Skip it and the next request fails with `Session not found` and a message saying the session expired or the server restarted, which it did not.

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

Any other client takes the same URL and headers in its own config format. Keep the token out of any file you commit.

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
