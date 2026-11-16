# MCP tools

The shared server's 18 tools in 1.0.2, generated from its own `tools/list`
reply, not written by hand. The descriptions are the first sentence the server
gives your client. Your client shows you the rest. Node ids are UUIDs.

If your tool list has `link_nodes`, `search_nodes` or `trace_chain` instead,
the client is running the old local `deciduous mcp` stdio server. Register the
HTTP server instead ([Shared graph](shared-graph.md)).

## Shared server

Every write tool takes `workspace` and `branch`. Pass both on every call (see
[Shared graph](shared-graph.md#3-which-workspace-a-call-lands-in)). Read tools
also accept `workspace: "*"`.

| Tool | Required | Optional | What it does |
|------|----------|----------|--------------|
| `add_edge` | `from_node_id`, `to_node_id` | `branch`, `edge_type`, `rationale`, `workspace` | Create a directed edge between two nodes. |
| `add_node` | `node_type`, `title` | `branch`, `commit`, `confidence`, `description`, `edge_type`, `files`, `parent_id`, `prompt`, `rationale`, `status`, `workspace` | Add a new node to the decision graph. |
| `ask_graph` | `question` | `include_context`, `scope`, `workspace` | Ask a natural language question about the decision graph. |
| `capture_conversation_turn` | `summary` | `action`, `branch`, `confidence`, `decision`, `goal`, `observations`, `options_considered`, `outcome`, `parent_node_id`, `workspace` | Capture the reasoning from a conversation exchange into the decision graph. |
| `check_activity` | - | `branches`, `workspace` | List active write sessions in a workspace — which branches have a session actively writing right now, which client, and whether it's this session — and the most recent node on every branch, so one call shows what everyon |
| `close_thread` | `title` | `branch`, `description`, `goal_node_id`, `lessons_learned`, `next_steps`, `parent_node_id`, `success`, `workspace` | Close out a reasoning thread with an outcome. |
| `delete_edge` | `from_node_id`, `to_node_id` | `branch`, `edge_type` | Remove an edge between two nodes. |
| `delete_node` | `node_id` | `branch` | Soft-delete a decision graph node. |
| `find_orphans` | - | `workspace` | Find nodes with no incoming edges that aren't goals. |
| `get_ancestors` | `node_id` | `max_depth`, `max_nodes` | Walk the graph backward from a node to find all ancestor nodes. |
| `get_descendants` | `node_id` | `max_depth`, `max_nodes` | Walk the graph forward from a node to find all descendant nodes. |
| `get_graph` | - | `branch`, `include_details`, `max_nodes`, `workspace` | Get the decision graph (nodes, edges, themes, documents) for one workspace. |
| `list_workspaces` | - | - | List every project in the shared decision graph, with node and edge counts, most populated first. |
| `log_decision` | `title`, `chosen_option` | `branch`, `confidence`, `parent_node_id`, `rationale`, `rejected_options`, `workspace` | Record a decision point with the options that were considered and which was chosen. |
| `log_observation` | `title` | `branch`, `description`, `related_to`, `tags`, `took_from`, `why`, `workspace` | Quickly log an insight, learning, or discovery as an observation node. |
| `query_nodes` | - | `branch`, `limit`, `search`, `status`, `type`, `workspace` | Search and filter decision graph nodes. |
| `show_node` | `node_id` | - | Get detailed information about a single node, including connected edges, documents, and themes. |
| `update_node` | `node_id` | `branch`, `description`, `metadata`, `status`, `title` | Update an existing decision graph node's title, description, or status. |

Things the schemas do not say:

- Every call has a 60-second deadline. A call that runs past it is stopped,
  its uncommitted writes are rolled back, and it is answered with an error
  instead of hanging.
- `get_graph` refuses graphs over `max_nodes` (default 2000). Use `query_nodes`
  with `search`, `type`, `branch` and `limit` instead.
- `add_node` with `parent_id` checks the parent before it writes anything. A
  parent that is missing, or in another workspace, fails the whole call:
  `parent_id 00000000-... is not a node in this workspace; nothing was created.`
- `log_decision` with `parent_node_id` and `rejected_options` crashed with
  `request handler crashed` on 1.0.2 and still left its decision node behind,
  unlinked. Write the decision, its options and their edges with `add_node` and
  `parent_id` instead, and run `find_orphans` if a call crashed.
- `edge_type` is one of `leads_to` (default), `chosen`, `rejected`, `requires`,
  `blocks`, `enables`, `took_from`.
