# Work as a team of agents

Put agents working on the same project in one Deciduous workspace. Give each agent a Git branch and a bounded file scope. One agent can publish a finding while another is still implementing; the second agent can reuse that finding and leave an edge back to its source.

Deciduous records reasoning and coordinates short graph-write bursts. Your agent runner still assigns tasks, starts sessions, and manages code changes.

## Agree on the project and branch names

This guide uses server `http://127.0.0.1:4000`, workspace `example-app`, and two branches:

| Agent | Branch | Responsibility |
| --- | --- | --- |
| API agent | `agent-api` | Request validation and error responses |
| Test agent | `agent-tests` | Fixtures and integration tests |

Use [client configuration](clients.md) to pin both MCP clients to `example-app`. Set that same name with `deciduous remote init ... --workspace example-app` in each initialized CLI checkout. Worktree directory names often differ; relying on their basenames can split one project into several workspaces.

Pass the real branch name on each MCP write. The server cannot inspect your Git checkout and does not infer the branch from a local process.

Create a new workspace with one client connection or `remote init` before launching parallel sessions. The current server can race if several clients first create the same workspace at once. A successful `check_activity` from the first client confirms it is ready for the rest.

For a fresh exercise, run these commands from the code repository. The branch names and destination directories must not already exist; substitute unused names if needed:

```bash
git status --short
git worktree add -b agent-api ../example-app-api HEAD
git worktree add -b agent-tests ../example-app-tests HEAD
```

Worktrees start from the selected commit and do not copy uncommitted changes. Launch the API agent in `../example-app-api` and the test agent in `../example-app-tests`. Ensure each has the same MCP connection configuration. Give each its responsibility from the table and tell both to pass their branch name on graph writes. They share Postgres memory without sharing a SQLite file.

## A complete two-agent handoff

The examples below are MCP calls, not shell commands. Replace uppercase ID placeholders with the UUID returned by an earlier call. Do not send the placeholder text to the server.

### 1. The API agent opens the work

Check for another writer, then record the actual user request:

```javascript
check_activity({workspace: "example-app"})
add_node({
  workspace: "example-app", branch: "agent-api",
  node_type: "goal", status: "active",
  title: "Validate create-item requests",
  prompt: "Add request validation and tests for the create-item endpoint."
})
```

Save the returned `id` as `GOAL_ID`. Before implementation, record the choice:

```javascript
log_decision({
  workspace: "example-app", branch: "agent-api",
  parent_node_id: "GOAL_ID",
  title: "Choose the validation boundary",
  rationale: "Reject malformed requests before they reach the service layer.",
  chosen_option: {
    title: "Validate at the HTTP boundary",
    description: "Return 422 with stable field error codes."
  },
  rejected_options: [{
    title: "Validate after the database write",
    reason: "The service would receive invalid input and rely on rollback."
  }]
})
```

Save `decision_id` as `DECISION_ID`. The helper creates a decision and its option nodes, with `chosen` and `rejected` edges. Record the action and connect it:

```javascript
add_node({
  workspace: "example-app", branch: "agent-api",
  node_type: "action", status: "active",
  title: "Implement request validation at the route boundary",
  files: ["src/http/items.ts"]
})
add_edge({
  workspace: "example-app", branch: "agent-api",
  from_node_id: "DECISION_ID", to_node_id: "API_ACTION_ID",
  rationale: "Implement the selected validation boundary"
})
```

`API_ACTION_ID` is the ID returned by `add_node`. Send the test agent the workspace and `DECISION_ID`, along with its assigned files. A title alone is harder to follow than a linkable node.

### 2. The test agent reads before writing

```javascript
check_activity({workspace: "example-app", branches: 30})
query_nodes({
  workspace: "example-app", branch: "agent-api",
  type: "decision", search: "validation", limit: 10
})
show_node({node_id: "DECISION_ID"})
```

`query_nodes` returns summaries. `show_node` supplies the description, metadata, and edge rationales. Read those details before treating a title as an agreed contract.

The test agent uses the API agent's decision:

```javascript
log_observation({
  workspace: "example-app", branch: "agent-tests",
  title: "Use the API agent's field error contract in integration tests",
  description: "Assert status 422 and stable error codes, without coupling tests to message wording.",
  related_to: "GOAL_ID",
  took_from: "DECISION_ID",
  why: "The boundary decision defines the public response; a second contract would make the tests disagree with the implementation."
})
```

Save the returned `id` as `BORROW_ID`. `took_from` creates an edge from the source decision to this observation. It accepts a node UUID or a full `change_id`, within the same workspace, including a source on another branch. Use the full identifier, not a shortened display prefix.

### 3. The test agent implements and reports evidence

```javascript
add_node({
  workspace: "example-app", branch: "agent-tests",
  node_type: "action", status: "active",
  title: "Cover invalid create-item requests",
  files: ["tests/items.test.ts"]
})
add_edge({
  workspace: "example-app", branch: "agent-tests",
  from_node_id: "BORROW_ID", to_node_id: "TEST_ACTION_ID",
  rationale: "Test the shared error contract"
})
```

After running the tests, record the actual command, result, and limitations. This example assumes the named tests passed:

```javascript
close_thread({
  workspace: "example-app", branch: "agent-tests",
  parent_node_id: "TEST_ACTION_ID",
  title: "Invalid requests return the agreed field errors",
  description: "npm test -- tests/items.test.ts passed all 8 cases. Checked missing name, invalid quantity, and unknown fields. Full-suite run remains for integration.",
  success: true
})
```

The API agent records its own implementation outcome under `API_ACTION_ID`. Neither agent marks the shared goal complete until the coordinator has integrated the branches and verified the combined result. Deciduous does not merge code or run tests on your behalf.

## Read between milestones

Call `check_activity` before a burst of writes and after each milestone. Its `sessions` list shows unexpired write leases, not every running agent. Its `branches` list shows recent branch nodes even after the writer's lease expires; `branches_total` tells you whether the list is truncated.

The default is 20 branches, with a maximum of 200. Follow interesting nodes with `show_node`. Use `query_nodes` when you need a particular type or a search term.

## Treat locks as coordination

The default advisory lock key is `(workspace, branch)`, with a ten-second lease. Another write from the same MCP session renews it. Sessions on different branches can write at once; a conflicting session gets an error identifying the current holder.

On a conflict, read `check_activity`, wait for the other writer to finish, and retry with the same intended branch. Do not invent a new branch name to bypass a teammate's ownership. Missing `branch` values share the empty-branch lock bucket. A server operator can configure a workspace-wide lock, which makes all branches contend.

These locks do not protect source files, reserve a task for the duration of an agent session, or prevent raw database/import operations. Keep file ownership and code review in your team workflow.

## Watch the conversation in the graph

With the Deciduous 1.0 CLI configured for this workspace:

```bash
deciduous remote watch --types decision,observation,outcome
deciduous remote watch --branch agent-api --edges
```

The feed quotes titles and distinguishes updates from new nodes. It reconnects, but it does not replay events missed while disconnected. Query the graph after a gap. Do not count event frames as completed tasks.

`--url` prints a WebSocket URL containing the bearer token. Avoid it in screenshots, logs, and shared terminals. See [Troubleshooting](troubleshooting.md) for version and connection checks.
