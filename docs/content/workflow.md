# Record the reasoning while the work happens

A useful shared graph lets another agent answer two questions: what should I build on, and what has already been checked? Write enough to answer them while the context is fresh.

## Start from the shared goal

Read the goal's original request, then inspect the decisions and recent outcomes beneath it. Use `check_activity` to find current branch work. If a goal already covers your task, continue it instead of creating a second root with the same title.

For new work, create a `goal` with the user's exact request in `prompt`. Keep credentials out of captured prompts; if the request contains a secret, redact the secret and say that you did so.

MCP examples use tool names directly. Replace ID placeholders with returned server UUIDs:

```javascript
add_node({
  workspace: "example-app", branch: "agent-api",
  node_type: "goal", status: "active",
  title: "Bound retries for failed requests",
  prompt: "Stop retrying forever when the upstream API is down."
})
```

## Consider options before choosing

Record the viable options and the constraint that decides between them. A short rationale with a specific failure mode is more useful than "best practice."

```javascript
log_decision({
  workspace: "example-app", branch: "agent-api",
  parent_node_id: "GOAL_ID",
  title: "Bound the retry budget",
  rationale: "A 30-second deadline bounds total wait even when individual attempts are slow.",
  chosen_option: {title: "Use an overall deadline"},
  rejected_options: [{
    title: "Use only an attempt count",
    reason: "A slow attempt can exceed the user's wait budget."
  }]
})
```

`log_decision` returns `decision_id`, a chosen option, and any rejected options. Its edges run from the decision to the options. For finer control, use `add_node` and `add_edge`; each call returns an ID to connect. See [Concepts](concepts.md) for node and edge meanings.

These convenience helpers perform several writes. If one fails, inspect the graph before retrying; do not assume the whole call rolled back or create a duplicate chain.

## Record the action before editing

Create an `action` with the files you expect to change. Link it to the decision that authorized the approach:

```javascript
add_node({
  workspace: "example-app", branch: "agent-api",
  node_type: "action", status: "active",
  title: "Apply the deadline to the retry loop",
  description: "Carry the remaining budget into each attempt and stop before scheduling another delay.",
  files: ["src/retry.ts", "tests/retry.test.ts"]
})
add_edge({
  workspace: "example-app", branch: "agent-api",
  from_node_id: "DECISION_ID", to_node_id: "ACTION_ID",
  rationale: "Implement the deadline chosen for bounded waiting"
})
```

A teammate should be able to read this action and decide whether its file scope overlaps their work. The graph's branch lock does not lock those files.

## Publish findings that can change another agent's plan

Use `log_observation` for a measured result, a constraint found in code, or a failed assumption. Supply `related_to` to connect it to the relevant work. Add `took_from` and `why` when you reuse another agent's finding.

```javascript
log_observation({
  workspace: "example-app", branch: "agent-api",
  related_to: "ACTION_ID",
  title: "The client library already exposes an abort signal",
  description: "src/client.ts accepts AbortSignal. The retry loop can share one deadline controller instead of adding a second timeout implementation."
})
```

An observation can be useful before implementation is finished. Keep its confidence proportional to the evidence; distinguish an inspected API from a behavior you tested.

## Close work with a result

Report test commands, results, and gaps. Include the actual commit SHA in a node's `commit` field when there is a commit. The remote server stores the value you supply; it cannot resolve your local `HEAD`.

```javascript
close_thread({
  workspace: "example-app", branch: "agent-api",
  parent_node_id: "ACTION_ID",
  title: "Retry deadline passes the timeout cases",
  description: "npm test -- tests/retry.test.ts passed. A stalled attempt is cancelled at the deadline; no next attempt starts. Load testing remains outside this change.",
  success: true
})
```

Pass `goal_node_id` only when the whole goal is complete, including other agents' work and integration checks. `success: false` creates a rejected outcome; record what failed and what needs reconsideration.

Use `find_orphans({workspace: "example-app"})` before handoff. Connect missing non-goal nodes to their real cause, not to an unrelated root to make the warning disappear.

## Preserve a changed decision

If evidence invalidates the approach, add an observation explaining the failure and a `revisit` connected to the old decision. Record the new options and choice, then mark the old decision `superseded` with `update_node`. Keep the old reasoning so the next agent can see why it was rejected.

`update_node`'s top-level `branch` controls its write lock. It does not replace `metadata.branch`. Supplying `metadata` replaces that map, so read the current metadata and preserve its other keys if you need to change it.

## Keep exports separate from team writes

The shared MCP server is the team's normal write path. Pull its node and edge records into a local cache for CLI exports and the viewer. `deciduous sync` reconciles the local graph with Git records; it does not fetch the Postgres server.

A graph can contain prompts, internal paths, and customer details. Review exported content before putting it in a public repository, static site, or PR. [Evidence](evidence.md) covers attachment and export limits.
