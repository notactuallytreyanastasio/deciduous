# Troubleshooting

Each entry is a failure that has actually happened, with the text you will see.
Match the message, apply the fix, and tell the user what you changed.

## "DECIDUOUS: 10 actions since your last graph write"

A project set up by 1.0.2 still has its logging hooks. Upgrade the binary to
1.0.5 and run `deciduous update` in the project. It removes the hook scripts it
wrote and their `settings.json` entries, and keeps the rest.

## "Docker is required" from `init` or `update`

```
Error: Docker is required: deciduous keeps every project's graph in a PostgreSQL server, and without a remote configured `init` sets one up on this machine with Docker.
```

The project has no `[remote]`, so `init` tried to set up a local server. Ask
the user whether to install and start Docker, or to point the project at a
team's server first (`deciduous remote login --url <url>`, then
`deciduous remote init <url>`, then `init` again). The project files are
already written; only the server step is left. Do not set
`DECIDUOUS_NO_SERVER` to get past it: that leaves a project with nowhere to write.

## "cannot reach ..." from `init` or `update`

The project's `[remote]` server does not answer, or the stored token is wrong.
The message names the URL. Tell the user. Starting the server or fixing the
token (`deciduous remote login --url <url>`) is theirs to do.

## A write is refused with a placeholder id

```
to_node_id is not a node id: "PLACEHOLDER". To link a node you are creating now, pass parent_id to add_node instead; an id must come from a previous answer, not be written ahead of it
```

You sent `add_edge` in the same parallel batch as the `add_node` whose id it
needed, and put a placeholder where the id would be. Use `add_node` with
`parent_id` instead. It creates the node and its edge together.

## `parent_id ... is not a node in this workspace; nothing was created.`

The parent is missing, or it lives in a different workspace from the one this
call resolved to. Look it up with `show_node`. If it is in another workspace,
pass that `workspace`, or check whether an `X-Deciduous-Workspace` header is
pinning you somewhere else.

## `deciduous remote status` says local holds more than the server

```
Drift: local holds more than the server. `deciduous remote push` to send it up.
```

Something wrote with the CLI (`deciduous add`) or the old stdio MCP server.
Those writes never reached the server. Run `deciduous remote push`, then write
through the HTTP MCP tools from now on. The
reverse message, the server holding more, means other agents have written. Run
`deciduous remote pull`.

## Your nodes are missing from the workspace you expected

They landed in another workspace. The usual causes are a `workspace` argument
set to a worktree's directory name instead of the repository's, a typo (unknown
names are created, not rejected), or no argument at all from outside a git
repository, which lands in `scratch`. Find them:

```
query_nodes(workspace: "*", search: "<part of the title>")
```

## A write is refused because another session holds the branch

Writes lock per workspace and branch. Another agent is writing on the same
branch right now, and the refusal names it. Use your own branch. Call
`check_activity` to see who is active where.

## HTTP 403 when scripting against the server directly

The public deployment sits behind Cloudflare, which rejected Python's default
`urllib` User-Agent with 403 before the request reached the server. Send a
User-Agent of your own. The same request with `User-Agent: something/1` passed.

## `Server not initialized`

```json
{"error":{"code":-32600,"data":{"message":"Server not initialized"},"message":"Invalid Request"}}
```

You posted `tools/list` or `tools/call` without an MCP session. Send
`initialize` first, keep the `mcp-session-id` response header, send
`notifications/initialized`, then send your call with that header. MCP clients
do this themselves. You only see this when you script HTTP by hand.

## The tools you expect are not there

`link_nodes`, `search_nodes` and `trace_chain` belong to the old `deciduous mcp` stdio server.
`add_edge`, `query_nodes` and `ask_graph` belong to the shared server. If the
user expects one and you see the other, the client is registered against the
wrong one. For Claude Code, `claude mcp get deciduous` shows which.
