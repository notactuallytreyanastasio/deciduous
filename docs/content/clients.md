# Connect your coding agents

Configure each agent to use Deciduous's shared HTTP MCP endpoint. The agents may use different clients; the server endpoint and workspace must agree.

## Connection values

| Value | Local team | Remote team |
| --- | --- | --- |
| CLI base URL | `http://127.0.0.1:4000` | `https://memory.example.com` |
| MCP URL | `http://127.0.0.1:4000/mcp` | `https://memory.example.com/mcp` |
| Workspace | `example-app` | `example-app` |
| Auth | `Authorization: Bearer ...` | `Authorization: Bearer ...` |

`memory.example.com` is a placeholder. Replace it with your deployment's hostname and preserve any configured path prefix. Only the server needs `DATABASE_URL`. Agents need the MCP token, not database credentials.

An agent in a container or on another machine cannot reach your laptop through its own `127.0.0.1`. Use a reachable private address or the [remote deployment](remote-postgres.md) pattern.

## Supply the token outside Git

The local setup generates a token in `~/.config/deciduous/team-memory/local.env` by default, mode `0600`. From a shell that will launch your client, load only the MCP token:

```bash
DECIDUOUS_SOURCE="/absolute/path/to/deciduous"
export DECIDUOUS_MCP_TOKEN="$(python3 "$DECIDUOUS_SOURCE/scripts/team-memory/local-stack.py" token)"
```

Keep shell tracing off when handling credentials. For a remote server, inject `DECIDUOUS_MCP_TOKEN` through your secret manager instead. A desktop app already running may not inherit a new shell variable; use its supported environment/secret configuration and reconnect the MCP server.

`deciduous remote login` stores a credential for the Deciduous CLI in `$XDG_CONFIG_HOME/deciduous/credentials`, or `~/.config/deciduous/credentials` by default. MCP clients do not read that file. The CLI checks `DECIDUOUS_MCP_TOKEN` before its stored credential, so a stale environment value can override a valid login.

## Claude Code

Merge this entry into the project's `.mcp.json`; preserve other servers. The environment reference stays literal in the file:

```json
{
  "mcpServers": {
    "deciduous-team": {
      "type": "http",
      "url": "http://127.0.0.1:4000/mcp",
      "headers": {
        "Authorization": "Bearer ${DECIDUOUS_MCP_TOKEN}",
        "X-Deciduous-Workspace": "example-app"
      }
    }
  }
}
```

Start Claude Code with the variable available, approve the project server, and inspect `/mcp`. Claude Code supports `${VAR}` expansion in HTTP headers. Do not replace the reference with a literal token. [Claude Code MCP configuration](https://code.claude.com/docs/en/mcp#environment-variable-expansion-in-mcpjson).

## Codex

Merge this table into your trusted project's `.codex/config.toml`, or use your user configuration for a personal connection:

```toml
[mcp_servers.deciduous_team]
url = "http://127.0.0.1:4000/mcp"
bearer_token_env_var = "DECIDUOUS_MCP_TOKEN"
http_headers = { "X-Deciduous-Workspace" = "example-app" }
```

Codex reads the token from the named environment variable and sends it as a bearer token. The workspace header is a static value. Reconnect after editing configuration and use `/mcp` in the CLI to check the connection. [Official OpenAI MCP configuration](https://developers.openai.com/codex/mcp/).

Deciduous uses a configured bearer token; it does not provide an OAuth login flow. `codex mcp login` is not the Deciduous credential setup step.

## Cursor

Merge this entry into `.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "deciduous-team": {
      "url": "http://127.0.0.1:4000/mcp",
      "headers": {
        "Authorization": "Bearer ${env:DECIDUOUS_MCP_TOKEN}",
        "X-Deciduous-Workspace": "example-app"
      }
    }
  }
}
```

Cursor's environment syntax is `${env:NAME}`, which differs from Claude Code's. Make the variable available to Cursor, reload the server, and inspect its MCP tools. Remote server entries use environment interpolation; `envFile` is a stdio setting. [Cursor MCP configuration](https://cursor.com/docs/mcp).

## Give the agent a working agreement

Add a project instruction in the file your client reads, such as `AGENTS.md` or `CLAUDE.md`. Merge it with existing instructions:

```text
Use the deciduous-team HTTP MCP server for this project's shared memory.
Workspace: example-app. Pass the current Git branch on every write.
At session start, check_activity and read the active goal and its decisions.
Before implementation, record the chosen approach and a connected action.
After a milestone, record the result and test evidence, then check teammates' work.
When reusing a finding, read its node and record took_from with the source ID.
Keep secrets and private material that is outside this project's scope out of the graph.
Do not substitute the local SQLite CLI for server writes or push a stale cache.
```

Supply the assigned branch and file scope in each agent's task. Let the [team workflow](teams.md) show the exact tool calls.

## Verify before delegating

For a new workspace, connect one client and run `check_activity` for `example-app` before connecting the others. This avoids a current race during concurrent first-time workspace creation. Then have one agent write a small goal and another find it with `query_nodes`; compare full node IDs. Confirm both clients expose the HTTP server's tools, rather than an older local stdio registration with a similar name.

The `X-Deciduous-Workspace` header pins workspace-aware calls ahead of a tool's `workspace` argument. It helps prevent accidental routing to the wrong project. It is not an authorization boundary: the current server uses one shared bearer token, has no per-project token permissions, and includes ID-based reads that do not apply this pin. Only give the token to people and agents trusted with that server's data. Use separate deployments for separate trust boundaries.

The alternative `{"command":"deciduous","args":["mcp"]}` starts a local SQLite MCP server. Use it only for the [solo/offline](solo.md) workflow. Adding a remote URL to the CLI configuration does not make that stdio server forward writes to Postgres.
