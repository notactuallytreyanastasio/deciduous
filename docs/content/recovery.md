# Recover the work, not a chat transcript

Start a new agent with the server endpoint, workspace name, current branch, and the active goal's ID. Those references let it retrieve the reasoning that survived the previous session.

## Read before changing anything

Use the same shared MCP connection as the rest of the team. These are MCP calls; replace the ID placeholders with server UUIDs:

```javascript
check_activity({workspace: "example-app", branches: 30})
query_nodes({workspace: "example-app", type: "goal", status: "active", limit: 20})
query_nodes({workspace: "example-app", branch: "agent-api", limit: 30})
show_node({node_id: "GOAL_ID"})
get_descendants({node_id: "GOAL_ID"})
```

`query_nodes` returns summaries, newest created nodes first. It does not include the full rationale. Follow relevant IDs with `show_node`. Defaults matter: `add_node` defaults to `pending`, so also query pending goals if an active-goal search is empty.

Use `get_ancestors({node_id: "ACTION_ID"})` to trace an unfinished action back to the choices that led to it. Read the source node of each `took_from` edge before reusing it. A superseded decision may still be visible in the history.

## Reconcile the graph with the checkout

The graph preserves what someone recorded. Check the code and tests before assuming the recorded outcome still applies:

```bash
git status --short
git branch --show-current
git log -5 --oneline
```

Match the branch and commit in node metadata to the checkout. Review uncommitted changes without discarding them. If the graph says a test passed but the relevant code has changed since, run the test again and record a fresh result.

Write a short recovery observation linked to the active action or goal. State what you confirmed and what remains unknown. Continue the existing chain instead of logging the original request as a new goal.

## Handoff checklist

The outgoing agent should leave:

- The workspace and goal ID, plus the current action and latest outcome IDs.
- Its branch, changed files, and commit SHA if committed.
- The decisions another agent must preserve, including source IDs for borrowed findings.
- Test commands and their actual results, followed by any checks still needed.
- The next bounded step and any ownership conflict or missing permission.

Keep credentials out of the handoff. The receiving agent gets access through its configured MCP connection.

## Recover the local viewer

After confirming the CLI's workspace and URL:

```bash
deciduous remote status
deciduous remote pull
deciduous nodes
deciduous edges
deciduous serve
```

Local integer node IDs can differ from the server UUIDs and from another machine's IDs. Use the full `change_id` to identify the same node across stores. MCP tools that ask for a node UUID should receive the server `id`; `log_observation` also accepts a full `change_id` for `related_to` and `took_from`.

`remote pull` merges node and edge records into the local cache. It does not download document bytes or provide a complete backup of the remote database. Equal counts in `remote status` mean only that counts match, not that every record matches.

If local-only work exists, preserve a backup before recovery. Do not delete the database or push it over the server to resolve uncertainty. Follow [Upgrading](upgrading.md) for a reviewed import.

## After a dropped event stream

`deciduous remote watch` reconnects, but notifications issued during a connection gap can be lost. Call `check_activity` and query the relevant branches to catch up. A quiet stream does not establish that nobody changed the graph.

## If the server is unavailable

Keep any local cache and the code checkout intact. Record new offline work as local-only and avoid claiming teammates can see it. Restore the server connection before depending on shared updates. The [solo/offline guide](solo.md) explains that alternative; [Troubleshooting](troubleshooting.md) covers health, credentials, and stale sessions.

Infrastructure recovery requires the operator's Postgres backup and restore process. A local `deciduous backup` protects SQLite only.
