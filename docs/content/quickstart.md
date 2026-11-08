# Two agents. One shared memory.

Connect your agents to the same Deciduous server and project workspace. Their decisions go into Postgres, where the next agent can read them before repeating the work.

The examples use workspace `example-app`, a server at `http://127.0.0.1:4000`, and Deciduous 1.0. A local server is enough for agents running on your machine. Agents on other machines need a reachable server address; see [Remote Postgres](remote-postgres.md).

## Start the shared server

Follow [Local Postgres](local-postgres.md) to install the prerequisites and build the CLI. From the Deciduous source checkout, the setup scripts are:

```bash
python3 scripts/team-memory/local-stack.py configure --apply
python3 scripts/team-memory/local-stack.py up --apply
curl --fail http://127.0.0.1:4000/health
```

Expect `ok` from the health endpoint. The local stack keeps Postgres on a private container network and publishes the MCP server on loopback. Agents speak HTTP to Deciduous; they do not need a Postgres password or a SQL connection.

The generated credentials live outside the repository in an owner-only file. Set `DECIDUOUS_SOURCE` to the absolute path of that checkout. Store the CLI token through a pipe, without printing it:

```bash
DECIDUOUS_SOURCE="/absolute/path/to/deciduous"
python3 "$DECIDUOUS_SOURCE/scripts/team-memory/local-stack.py" token \
  | deciduous remote login --url http://127.0.0.1:4000
```

Do not run the `token` command by itself in a recorded terminal. It emits a bearer credential. For a hosted server, obtain the token from its operator through your team's secret manager.

## Connect both agents

Use the [client configuration](clients.md) for your coding agent. Set these values in both clients:

| Setting | Value |
| --- | --- |
| Transport | Streamable HTTP |
| MCP endpoint | `http://127.0.0.1:4000/mcp` |
| Authentication | Bearer token from `DECIDUOUS_MCP_TOKEN` |
| `X-Deciduous-Workspace` header | `example-app` |

For a new workspace, connect one client first and call `check_activity({workspace: "example-app"})`. Wait for that call to succeed before connecting the second client. Concurrent first connections can race workspace creation in the current server; once the workspace exists, agents can work in parallel.

The CLI's stored credential does not configure your MCP client. Supply the same token through the client process's environment or secret mechanism. Keep literal credentials out of project configuration.

Give the agents separate Git worktrees and branches, such as `agent-api` and `agent-tests`. Both use workspace `example-app`. A workspace groups the project's memory; a branch identifies each agent's line of work inside it.

## Prove that they share a graph

These are MCP tool calls, shown as `tool_name(arguments)`, not shell commands. Your client may prefix the tool names with the server name.

Ask agent A to run:

```javascript
check_activity({workspace: "example-app"})
add_node({
  workspace: "example-app",
  branch: "agent-api",
  node_type: "goal",
  title: "Share an API change with the test agent",
  prompt: "Add request validation and tests for the create-item endpoint.",
  status: "active"
})
```

Record the returned `id` and `change_id`. Ask agent B to read it:

```javascript
query_nodes({
  workspace: "example-app",
  branch: "agent-api",
  search: "Share an API change",
  limit: 10
})
```

Both clients should return the same server node UUID and full `change_id`. If agent B sees nothing, check its endpoint and workspace header before creating another goal. Continue with the [two-agent workflow](teams.md) to record a decision, reuse it, and connect the result.

## Connect the optional CLI cache

The MCP connection above can work without a local Deciduous database. The CLI adds a local viewer, exports, and an event watcher.

Inside the code repository, run `deciduous init` once if it has no `.deciduous` directory. This also installs assistant integration files; review its changes and keep your instructions aligned with the server-first workflow. Then configure the base URL, without `/mcp`:

```bash
deciduous remote init http://127.0.0.1:4000 --workspace example-app
deciduous remote status
deciduous remote pull
deciduous serve
```

Use the viewer address printed by `serve`. `remote pull` refreshes local node and edge records; it does not download document attachments or turn the viewer into a live Postgres client.

With Deciduous 1.0, watch new server writes in another terminal:

```bash
deciduous remote watch --types decision,observation,outcome
```

Check `deciduous --version` first. Earlier `remote watch` implementations print a credential-bearing WebSocket URL instead of starting this stream.

## Keep one normal write path

Have agents write through the shared HTTP MCP connection. `deciduous add`, `deciduous link`, and the local `deciduous mcp` stdio server still write SQLite, even after `remote init`.

Use [Upgrading](upgrading.md) to bring an existing local graph into Postgres. Do not use a stale `remote push` as a routine sync: importing matching records can replace newer server content. [Solo and offline](solo.md) covers the separate SQLite workflow.
