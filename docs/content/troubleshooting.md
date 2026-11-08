# Troubleshoot the shared memory

Start with the base URL, workspace, and client version. Most split graphs come from a local writer or a different workspace name, not from Postgres losing a record.

## Separate reachability from authentication

For the local stack:

```bash
curl --fail http://127.0.0.1:4000/health
deciduous --version
deciduous remote status
```

`/health` is unauthenticated liveness. An `ok` response confirms the HTTP service answered; it does not verify your bearer token or every database operation. `remote status` checks the configured server and requests the authenticated workspace export.

| Symptom | Check |
| --- | --- |
| Connection refused | Start the local stack; confirm the configured port and container status. |
| Host cannot be reached | Verify DNS, private network access, and the address from the agent's machine. |
| HTTP 401 | Check the MCP token source and the `Authorization` header. |
| HTTP 404 | Use the deployment's correct path prefix; the CLI takes the base URL and the MCP client takes `/mcp`. |
| HTTP 405 on `GET /mcp` | Expected: the server refuses the optional SSE stream. Tool calls use Streamable HTTP POST. |

For container status and server logs, use the commands in [Local Postgres](local-postgres.md). Do not paste unredacted environment files or request headers into an issue.

If several clients get an HTTP 500 while connecting to a brand-new workspace, connect one first and wait for `check_activity` to succeed, then reconnect the others. The current server's first-workspace creation can race on the unique name constraint. Check the server log before assuming this is the cause of another HTTP 500; do not reset the database.

## CLI login works, but the agent gets 401

`deciduous remote login` stores a CLI credential outside repositories. Claude Code, Codex, and Cursor use their own MCP configuration and environment. Set `DECIDUOUS_MCP_TOKEN` for the actual client process and reconnect.

Check the interpolation syntax in [Clients](clients.md): Claude Code uses `${DECIDUOUS_MCP_TOKEN}`, Cursor uses `${env:DECIDUOUS_MCP_TOKEN}`, and Codex uses `bearer_token_env_var`. Avoid putting a literal token in a project file to test a fix.

For the CLI, the environment variable overrides the stored credential. Remove a stale override from that shell before retesting a stored login. `remote logout` removes the stored credential; it neither clears the environment nor revokes the server token.

## Two agents see different graphs

Compare the following on both clients:

1. The full MCP endpoint, including any path prefix.
2. The `X-Deciduous-Workspace` header and the `workspace` argument.
3. Whether the connection is shared HTTP or local `deciduous mcp` stdio.
4. A node's full server ID and `change_id`, not a local integer ID.

The header wins over a workspace argument for workspace-aware calls. An unpinned MCP call with no workspace falls back to `scratch`; the server cannot inspect the client's repository. The CLI instead derives a default from the lowercased Git root directory name. Different clone/worktree directory names can therefore select different workspaces. Pin an explicit shared name in both configurations.

`list_workspaces` helps an operator find an accidental split. Do not start copying records between workspaces until you have confirmed which one is authoritative.

## A CLI node never appears in another agent's MCP results

`remote init` does not redirect local commands. `deciduous add`, `link`, `doc attach`, and the stdio MCP server write SQLite. Shared HTTP MCP tools write Postgres.

Use HTTP MCP for new team work. Preserve any local-only records, then follow [Upgrading](upgrading.md) for a reviewed import. A blind `remote push` can replace matching server nodes with stale local content and does not upload document bytes.

The reverse direction is explicit too: run `remote pull` before inspecting server changes in the local viewer. `deciduous sync` does not contact the shared server.

## "Workspace is locked"

Call `check_activity` in the affected workspace. Inspect the branch, client, and lease expiry. The default ten-second lease renews while its session keeps writing; wait for the owner to finish and retry your intended operation.

Check that each agent passes its assigned Git branch. Omitted branches share the empty lock bucket. If the result says `workspace_wide_lock: true`, different branches share a workspace-wide lock by configuration.

Do not bypass a collision by using a fake branch or modifying lock rows. Locks coordinate graph writes, so also review task/file ownership if the agents are editing the same code.

## Events are silent, duplicated, or incomplete

With CLI 1.0:

```bash
deciduous remote watch --json
```

Check the workspace and remove restrictive type or branch filters. Update events are not additional nodes. The socket reconnects, but notifications missed during a disconnection are not replayed. Use `check_activity`, `query_nodes`, or `get_graph` to verify persisted state.

Run `deciduous --version` and `deciduous remote watch --help` before sharing terminal output. Older versions print a URL containing the token. In 1.0, `--url` still exposes that credential; do not paste its output into logs or bug reports.

## MCP fails after a server restart

The server may reject an unknown or stale MCP session with HTTP 404. Reconnect the MCP server in your client so it performs a fresh initialization. Do not keep replaying a captured `Mcp-Session-Id`.

Use Streamable HTTP, not a legacy SSE-only client configuration. A rejected `GET /mcp` stream is expected; an authenticated initialized POST should still work. Check reverse-proxy routing and response buffering if local calls work but remote calls hang.

## A tool failed halfway through a write

Query the relevant branch and inspect the returned nodes before retrying. Some convenience tools create several records without a single enclosing transaction; a failure can leave a partial chain. Add missing connections using known IDs instead of repeating the whole request.

`log_observation` writes its observation and optional `related_to`/`took_from` edges in one transaction. A bad source ID should fail without leaving the observation behind. Use full UUIDs or full `change_id` values for those references.

## Attachment metadata exists, but the file will not open

A `410` from `/documents/{id}` means the bytes are missing. A graph-only import or `remote push` can bring across document rows without uploading content. Preserve the original files, then use the [migration checks](upgrading.md) to locate and upload verified blobs. See [Evidence](evidence.md) for the storage model.

`remote pull` does not retrieve attachment bytes. A local viewer after pull is not a complete copy of the server.

## Counts match, but content differs

`remote status` compares counts. Two graphs can have equal counts and different records. Compare `change_id`, content, and timestamps through the shared graph, preserving any local-only work. Avoid treating a count match as a backup verification or authorization to push.

## Report a problem without leaking memory

Include the CLI/server versions, relevant tool or command, sanitized error, and whether the client uses HTTP or stdio. Share the workspace name only if it is safe to disclose. Redact bearer tokens, database URLs, event query strings, private prompts, and attachments. Use a small test workspace to reproduce the issue when possible.
