# How agents share reasoning

A team gives each agent its own worktree and branch, then connects those agents to one Deciduous workspace. The workspace's Postgres graph holds the decisions, evidence, and links between their work. An agent can inspect another branch's reasoning without waiting for that branch to merge.

Deciduous does not create agents, assign tasks, merge code, or decide whether an idea is correct. You and your agent harness handle those jobs. The graph gives the team a record it can query and challenge.

## Workspace, branch, session

| Term | Meaning |
| --- | --- |
| Workspace | A named graph, normally one repository such as `example-app` |
| Branch | Metadata naming the Git branch where the work happened |
| MCP session | A connection session used by the server for tool calls and short write leases |
| Node | A goal, option, decision, action, outcome, observation, or revisit |
| Edge | A directed relationship between two nodes in the workspace |

Agree on the workspace name across worktrees and machines. Automatic directory names can differ when a worktree is called `example-app-agent-api`; an explicit workspace keeps that agent in the same graph as the rest of the team.

Branch metadata supports filtering and cross-branch credit. It does not check out a Git branch. The HTTP server relies on the client to pass the real branch and commit hash.

An MCP session is shorter-lived than the reasoning it records. Restarting an agent or reconnecting to the server does not remove its nodes.

## A useful reasoning chain

Record the requested result as a goal. Connect the approaches under consideration, explain the selected decision, then link the implementation and its observed result:

```text
goal -> option -> decision -> action -> outcome
```

Observations attach to the work that produced them. A revisit connects a previous approach to new evidence and a replacement decision. Preserve failed options and superseded choices when they explain why the team stopped pursuing them.

The graph does not enforce a single writing style. The `log_decision` helper uses a decision with outgoing `chosen` and `rejected` links to its options. Read edge types and rationales, not just visual position. Use explicit `add_node` and `add_edge` calls when you need the chain above.

## Credit the idea you reused

An agent should inspect the source node before adopting a teammate's approach. Record what changed and why that source was useful. A `took_from` edge runs from the source node to the node describing the reuse:

```text
agent-api's decision --took_from--> agent-tests' observation
```

This is cross-branch provenance. It neither copies the source branch's code nor asserts that both implementations are equivalent. Both nodes belong to the same workspace. For a cross-project reference, include the source workspace and full node identifier in the description; the server does not create edges across workspace boundaries.

## Record evidence at the right time

Before editing, record the action and the decision it implements. After testing, record the outcome and actual result. Keep a rejected experiment distinct from an untested proposal. A useful outcome says which test ran, what it demonstrated, and what remains uncertain.

Capture the relevant user request accurately, but redact secrets and material the team should not share. A graph is durable shared data. Avoid storing credentials, private conversation unrelated to the task, or a full transcript when a focused record will do.

Confidence is a recorded judgment from 0 to 100. It is not an automated correctness score. Statuses such as `active`, `completed`, `superseded`, and `abandoned` describe the record's place in the work; they do not replace evidence.

## Recover through connections

A new agent can read active goals and nearby decisions, use `show_node` for details, and follow ancestors to the earlier evidence. `check_activity` adds a view of current write leases and each branch's newest created node. The [recovery guide](recovery.md) turns this into a working routine.

`find_orphans` catches non-goal nodes with no incoming edge. It can reveal a missing link, but a graph without orphans can still contain wrong or unsupported reasoning. Read the sources and inspect the code before treating an old decision as current truth.

## Coordination has limits

The shared server's short advisory leases prevent two MCP sessions from interleaving a burst of writes on the same branch without a conflict response. Separate branches can proceed in parallel. A lease does not reserve files, prove that someone is still working, or provide task scheduling.

The event stream announces writes while a watcher is connected. It has no replay cursor or durable queue. After a disconnection, query the graph to recover state rather than assuming that no events occurred.

Workspaces group records within one trust boundary. They are not permissions. See [architecture and security](architecture.md) before exposing a server beyond your machine or trusted team.

## Shared Postgres and local SQLite

For the team workflow, the HTTP MCP service is the authoritative graph. A local SQLite copy can support the viewer and offline inspection, but transfers are explicit and incomplete. Ordinary CLI writes and the Rust stdio MCP stay local even after `remote init`.

Choose one primary write path for the team. Use [upgrading](upgrading.md) to migrate existing history, and [solo use](solo.md) if you want a separate offline or Git-synced graph. Avoid treating the two stores as an automatic bidirectional replica.
