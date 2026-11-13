<!-- Generated from content/mcp.md by scripts/docs/build.mjs. Edit the source. -->

# MCP tools

There are two MCP servers with different tool sets, and an agent gets whichever one its client is configured for. The graph server answers at `<base>/mcp` over HTTP with 21 tools and writes PostgreSQL directly; this is what `remote setup` registers with Claude Code and what this page is mostly about. `deciduous mcp` is a stdio server with a different set of 34 tools. Seven share a name with a graph-server tool. Five of those take different arguments and ids; `post_message` and `read_messages` take the same arguments on both, and in a project with a `[remote]` they reach the same board, with the same message ids. It writes the local database, and in a project with a `[remote]` those writes go on to the server through the CLI's op log, exactly like `deciduous add`. Nothing configures it for you, and it does not send the server's logging instructions. The two lists below are from `tools/list` against deciduous 1.1.0 on 2026-11-08.

## Which one an agent has

| | Graph server (`<base>/mcp`) | `deciduous mcp` (stdio) |
| --- | --- | --- |
| Transport | Streamable HTTP, `Authorization: Bearer <token>` | stdin/stdout, a process per session |
| Configured by | `remote setup` (Claude Code, user scope, if none is registered), or by hand: [Connect your agents](content/clients.md) | you, in the client: `{"command": "deciduous", "args": ["mcp"]}`, started inside the project (see below) |
| Writes go to | PostgreSQL, at once, visible to every agent | the local database; in a project with a `[remote]`, also queued in `.deciduous/remote-log.jsonl` and sent to the server, as CLI writes are. Without a `[remote]`, they stay local. |
| Reads come from | the server | the local database, which is a cache: it shows what other agents wrote only after `deciduous remote pull` |
| Tools | 21 | 34 |
| Prompts | 1 (`always_capture`) | none (`prompts/list` answers `-32601 Unknown method`) |
| `initialize` reply | `serverInfo {name: deciduous-mcp, version}` plus `instructions` (the logging rules agents follow) | `serverInfo {name: deciduous, version}`, no `instructions` |

The stdio server finds its database the way the CLI does: `DECIDUOUS_DB_PATH` if set, otherwise the first `.deciduous/` in the directory it was started in or above. If there is none, it creates `.deciduous/deciduous.db` in that directory and writes there, local-only. A client that starts it from your home directory, `/` or its own application directory writes a separate graph the project never sees. Start it inside the project, or give it the path:

```json
{"command": "deciduous", "args": ["mcp"],
 "env": {"DECIDUOUS_DB_PATH": "/abs/path/to/my-app/.deciduous/deciduous.db"}}
```

Use the graph server for agents. The stdio server is for a project with no server at all ([Without a shared server](content/local.md)) or for the local-only tools it has that the graph server lacks (themes, DOT export, writeups, sessions).

A stdio write in a project with a `[remote]` was checked: `add_node` over `deciduous mcp` returned `Created observation node #3`, and `deciduous remote status` then said In sync with the server holding the node.

## Talking to the graph server by hand

An MCP client does this for you. With `curl`, the sequence is: `initialize` without a session id, read `mcp-session-id` from the response headers, send `notifications/initialized` with that id, then make calls with it. `Accept` must list both `application/json` and `text/event-stream`.

```bash
BASE=https://deciduous.example.com
H=(-H "Authorization: Bearer $DECIDUOUS_MCP_TOKEN" -H 'Content-Type: application/json'
   -H 'Accept: application/json, text/event-stream')

curl -s -D headers.txt -X POST "$BASE/mcp" "${H[@]}" -d '{"jsonrpc":"2.0","id":1,"method":"initialize",
  "params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"probe","version":"0"}}}'
SID=$(grep -i '^mcp-session-id' headers.txt | awk '{print $2}' | tr -d '\r')

curl -s -X POST "$BASE/mcp" "${H[@]}" -H "mcp-session-id: $SID" \
  -d '{"jsonrpc":"2.0","method":"notifications/initialized"}'
curl -s -X POST "$BASE/mcp" "${H[@]}" -H "mcp-session-id: $SID" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'
```

Skip `notifications/initialized` and the session is dropped: the next call gets HTTP 404 `Session not found` with `This session expired or the server restarted. Send a new initialize request without a session id to start another.` The same reply follows a server restart. `GET /mcp` answers 405; there is no server-to-client stream.

Tool results come back as JSON text inside `result.content[0].text`. A tool error is a JSON-RPC `error` whose `message` says what was wrong, including any argument the tool does not have:

```text
query_nodes has no argument "bogus". Its arguments are: branch, file, limit, search, status, type, workspace
```

## Workspace and branch on every call

Every tool except `list_workspaces` takes `workspace`. The server decides which workspace a call acts on in this order:

For tools that take a workspace rather than a node:

1. The `X-Deciduous-Workspace` header, if the client sends one (a repository's `.mcp.json` can). It wins over the argument: an `add_node` sent with `"workspace": "elsewhere-xyz"` on a connection pinned to `my-app` landed in `my-app`.
2. The `workspace` argument: the `workspace` under `[remote]` in `.deciduous/config.toml`, which is what the CLI uses.
3. `scratch`.

Read tools accept `"workspace": "*"` for all workspaces at once; on a pinned connection `*` reads the pinned workspace only, without an error. Writes refuse it. A read naming a workspace that does not exist is an error; the first write creates it.

Tools that take a node id (`show_node`, `update_node`, `delete_node`, `delete_edge`, `get_ancestors`, `get_descendants`) always act in that node's own workspace, and never fall back to `scratch`. Their `workspace` argument is only a guard against acting on the wrong one:

```text
node e2eee285-... is in workspace "wf-ref-fix-s1", not "wf-ref-fix-s2". Pass the node's own workspace, or none
```

On a pinned connection, a node outside the pin is refused: `node e2eee285-... belongs to another workspace than the one this client is pinned to`.

Pass `branch` on every write. The server cannot see the agent's checkout, and `check_activity` shows writers by branch. Nothing is locked: since 1.0.8 there are no branch leases, and two sessions writing one branch both succeed.

## The 21 graph-server tools

Required arguments in bold. Node ids are full server UUIDs (the `id` in every result); prefixes are refused.

### Write

| Tool | Arguments | Notes |
| --- | --- | --- |
| `add_node` | **`node_type`**, **`title`**, `description`, `status`, `confidence`, `prompt`, `files`, `commit`, `branch`, `parent_id`, `edge_type`, `rationale`, `change_id` | `parent_id` creates and links in one transaction (`edge_type`, `rationale` apply to that edge). A non-goal node without a parent comes back with `suggested_parents`, never linked for you. A repeated `change_id` returns the existing node with `created: false`. |
| `add_edge` | **`from_node_id`**, **`to_node_id`**, `edge_type`, `rationale`, `branch` | For `took_from`, `from` is the node the idea came from. Never in the same batch as the `add_node` whose id it needs. |
| `update_node` | **`node_id`**, `title`, `description`, `status`, `metadata`, `branch` | `metadata` is merged: keys sent are set, a key sent as `null` is removed, others are kept. |
| `delete_node` | **`node_id`**, `branch` | Soft delete. |
| `delete_edge` | **`from_node_id`**, **`to_node_id`**, `edge_type`, `branch` | `edge_type` defaults to `leads_to`. |
| `log_observation` | **`title`**, `description`, `related_to`, `took_from`, `why`, `tags`, `branch` | `related_to` and `took_from` take a UUID or a full change_id. `why` becomes the `took_from` edge's rationale. |
| `log_decision` | **`title`**, **`chosen_option`**, `rejected_options`, `rationale`, `parent_node_id`, `confidence`, `branch` | One transaction: decision, options, `chosen`/`rejected` edges. Options are objects or plain titles. |
| `capture_conversation_turn` | **`summary`**, `goal`, `observations`, `options_considered`, `decision`, `action`, `outcome`, `parent_node_id`, `confidence`, `branch` | One step's nodes and edges, all or nothing. `parent_node_id` hangs it under the goal in progress. |
| `close_thread` | **`title`**, `description`, `parent_node_id`, `goal_node_id`, `success`, `lessons_learned`, `next_steps`, `branch` | Writes the outcome; `goal_node_id` marks the goal completed; lessons become observations, next steps become goals. |

### Read

| Tool | Arguments | Notes |
| --- | --- | --- |
| `list_workspaces` | none | Every workspace with node and edge counts. |
| `check_activity` | `branches` (default 20) | Who wrote to this workspace in the last five minutes (branch, client, whether it is you), and the newest node on each branch. Call before a burst of writes. |
| `query_nodes` | `type`, `status`, `branch`, `search`, `file`, `limit` (default 100) | `file` matches `metadata.files`. `type` also accepts legacy `feedback`, `status` legacy `done`. |
| `show_node` | **`node_id`** | The node, its edges, documents, themes, and up to 10 `related` nodes sharing a file or commit. Takes a UUID, not a change_id. |
| `get_graph` | `branch`, `include_details`, `max_nodes` (default 2000) | Refuses a larger graph rather than truncating. |
| `get_ancestors` / `get_descendants` | **`node_id`**, `max_depth` (default 50), `max_nodes` (default 1000) | Walk edges backward / forward. |
| `find_orphans` | `suggest_parents` | Non-goal nodes with no incoming edge; with `suggest_parents`, ranked guesses for the first 25. |
| `ask_graph` | **`question`**, `scope` (`all`, `active`, `decisions`, `goals`, `observations`, `recent`), `include_context` (default true) | Text search (trigram and full text), then follows the edges the question implies. Says how each result was reached, and stops early when a distinctive word in the question matches nothing. Not a language model. |
| `consolidation_report` | `similarity` (0.6), `stale_days` (14), `max_nodes`, `max_pairs`, `max_items` | Proposes merges, missing revisits, stale actions and parents. Changes nothing. |

### Message board

| Tool | Arguments |
| --- | --- |
| `post_message` | **`author`**, **`subject`**, **`body`**, `reply_to`, `branch` |
| `read_messages` | `unanswered_for`, `to`, `author`, `branch`, `query`, `since_id`, `id`, `limit` (default 50) |

Messages are per workspace and are not graph nodes. See [The message board](content/board.md).

### Prompt

The graph server also offers one MCP prompt, `always_capture`, with an optional `verbosity` argument: `full` logs every exchange, `decisions_only` only decision points and outcomes, `minimal` only major milestones. Claude Code lists MCP prompts as commands, named `/servername:promptname (MCP)` ([Claude Code: Use MCP prompts as commands](https://code.claude.com/docs/en/mcp)), so with the server registered as `deciduous` it is `/deciduous:always_capture`. The stdio server has no prompts.

## The 34 stdio tools

`deciduous mcp` exposes: `add_node`, `attach_document`, `create_theme`, `delete_node`, `end_session`, `export_dot`, `find_orphans`, `generate_writeup`, `get_branch_summary`, `get_graph`, `get_node_context`, `get_pulse`, `get_session`, `get_timeline`, `link_nodes`, `list_documents`, `list_edges`, `list_nodes`, `list_sessions`, `list_themes`, `post_message`, `read_messages`, `resume_session`, `search_nodes`, `show_node`, `start_session`, `sync`, `sync_status`, `tag_node`, `trace_chain`, `unlink_nodes`, `untag_node`, `update_prompt`, `update_status`.

Where a name matches a graph-server tool, other than the two board tools, the arguments and ids still differ: stdio tools take a local node number (`Created observation node #3`) or a change_id prefix, never a server UUID (`No node has a change_id starting with 'e2eee285-...'`, even though the CLI accepts one); the graph server takes UUIDs. Where the job matches, the name differs: `link_nodes` vs `add_edge`, `list_nodes`/`search_nodes` vs `query_nodes`, `update_status` vs `update_node`. An agent moved from one server to the other with instructions written for the first will hit `has no argument` and unknown-tool errors; [For agents](content/for-agents.md) is written for the graph server.
