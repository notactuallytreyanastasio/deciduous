<!-- Generated from content/concepts.md by scripts/docs/build.mjs. Edit the source. -->

# Graph concepts

A deciduous graph is typed nodes joined by typed, directed edges, kept per workspace (one per repository) on a shared server. Seven node types, seven edge types, six statuses, and a confidence from 0 to 100. Every node has two identities: a server UUID that MCP tools use, and a change_id that survives every copy and every machine. The CLI and the server reject anything outside these lists, by name.

## Node types

| Type | Records |
| --- | --- |
| `goal` | What someone asked for. Carries the request verbatim in `prompt`. The only node that may have no parent. |
| `option` | One way to do it, considered before choosing. |
| `decision` | The choice between options. A useful title names what it beat. |
| `action` | Code or config changed to carry out a decision. |
| `outcome` | What happened: the test count, the error, the number. |
| `observation` | A fact learned along the way that matters to the plan. Attaches anywhere. |
| `revisit` | An earlier choice being reconsidered: links the old decision, the observation that undid it, and the new decision. |

The flow has two shapes, depending on how the decision was written. Built node by node (`deciduous add`, `add_node`), it is `goal -> option -> decision -> action -> outcome` with `leads_to` edges. Built with MCP's `log_decision`, the decision comes first and its options hang under it: `goal -> decision -> option`, with a `chosen` or `rejected` edge from the decision to each option. Do not then link the options back to the decision; the server refuses an edge that reverses an existing one (see [Edge types](#edge-types)). Observations attach where they were learned. The graph does not enforce either order; `find_orphans` (MCP) reports any non-goal node with no incoming edge, which is the usual sign of a missing link.

In 1.1.0, `add --help` leaves out `revisit` and `status --help` lists only four statuses; both are accepted. The next release lists them all. Anything outside the lists is refused:

```text
Error: unknown node type 'bogus'; expected one of: goal, decision, option, action, outcome, observation, revisit
```

The server's `query_nodes` also accepts `feedback`, a legacy type kept so old imported graphs stay readable. New nodes cannot be created with it. Likewise the status `done`: old graphs carry it, so imports and `query_nodes` accept it, but no write sets it (`add_node` with `status: "done"` answers `status must be one of pending, active, completed, rejected, superseded, abandoned; got "done"; nothing was written`). Read it as `completed`.

## Edge types

An edge runs from one node to another in the same workspace. The two may be on different branches. The server refuses an edge that already exists, and an edge whose reverse exists, because two nodes cannot be each other's parent; `took_from` is exempt from both. After `log_decision`, linking the chosen option back to its decision gets:

```text
bb3060b2-... -> 45a3019e-... (chosen) already exists; 45a3019e-... -> bb3060b2-... would make the two nodes each other's parent, and nothing was written. A decision's options hang under it with chosen/rejected edges (decision -> option); to put a decision under its goal, link goal -> decision.
```

| Type | From -> to |
| --- | --- |
| `leads_to` | The default: parent to child along the flow. |
| `chosen` | Decision to the option it took. |
| `rejected` | Decision to an option it turned down. |
| `requires` | A node to something it depends on. |
| `blocks` | A node to something it prevents. |
| `enables` | A node to something it makes possible. |
| `took_from` | The node an idea came from, to the node that reused it. Usually across branches, between agents. |

`log_decision` draws `chosen` and `rejected` edges from the decision to its options; `log_observation` with `took_from` draws the edge from the source to the new observation. From the CLI: `deciduous link <from> <to> -t took_from -r "why"`.

## Status

`pending` (the default), `active`, `completed`, `rejected`, `superseded`, `abandoned`. In 1.1.0, `add --help` leaves out `revisit` and `status --help` lists only four statuses; both are accepted. The next release lists them all. Nothing sets `active` on its own: new nodes are `pending` until someone changes them. Anything else is refused:

```text
$ deciduous status 1 done
Error: unknown status 'done'; expected one of: pending, active, completed, rejected, superseded, abandoned
```

`superseded` is for a decision that was right once and has been replaced (`deciduous archaeology supersede` sets it, with `--cascade` for descendants); `abandoned` is for something tried and dropped. Both keep the node, which is the point: the graph is most useful about the paths not taken. Deleting a node (`deciduous delete`, MCP `delete_node`) is a soft delete on the server.

## Confidence

An integer from 0 to 100, set by whoever wrote the node. It is a judgement, not a measurement. The CLI refuses anything outside the range:

```text
error: invalid value '150' for '--confidence <CONFIDENCE>': 150 is not in 0..=100
```

## Identity: id, change_id, local number

| Identity | Looks like | Where it works |
| --- | --- | --- |
| Server `id` | `8889e255-4305-4fc5-a4f2-d316db4e316e` | Every MCP tool that takes `node_id`, `parent_id`, `from_node_id`, `to_node_id`. The CLI accepts it too. |
| `change_id` | `96096527-2046-4a39-863a-7e67c3cf194e` | The same on every machine, in `graph.json`, and on the server. The CLI accepts a prefix (`deciduous link 5e906850 96096527`); `deciduous nodes` prints the first 8 characters in the CHANGE column. Over MCP, `log_observation`'s `took_from` and `related_to` accept a full change_id; `show_node` does not. |
| Local number | `1`, `2`, `3` | One machine's database only. Two machines number the same node differently. |

MCP tools do not accept prefixes: `show_node` with `"11111111"` answers `node_id is not a node id: "11111111"`.

A change_id also makes a write safe to retry. `add_node` with a `change_id` the workspace already holds, for a node of the same type and title, returns that node with `created: false` instead of making a second one. The same change_id with a different title is refused:

```text
change_id 11111111-2222-4333-8444-555555555555 is already node 3bcdac84-4e0d-410f-890f-d1f8f32712a4, action "idempotent"; this call names a action "different". Nothing was written. Use another change_id, or update_node to change that node.
```

## Workspace

A workspace is one repository's graph on a server. Its name comes from `workspace` under `[remote]` in `.deciduous/config.toml`, which `remote init` and `remote setup` write: by default the repository's root directory name, lowercased (`Docs_Ref_Upper` becomes `docs_ref_upper`), or whatever `--workspace` says. The server trims and lowercases names and refuses anything that looks like a path.

The first repository to write to a workspace claims it by its root commit ids. Clones and worktrees of that repository share the root commit and are let in; an unrelated repository with the same directory name is refused until it picks another name or names this one explicitly (see [Troubleshooting](content/troubleshooting.md#belongs-to-another-repository)).

Over MCP, the workspace for a call is decided in this order: the `X-Deciduous-Workspace` header if the client sends one, then the call's `workspace` argument, then `scratch`. Tools that name a node by id (`update_node`, `delete_node`, `delete_edge`) act in that node's workspace instead; a header that pins a different workspace makes them refuse. Read tools also accept `workspace: "*"` for every workspace at once; writes refuse it. A workspace is a name for routing, not a permission: anyone holding the server's token can read and write all of them. See [Architecture](content/architecture.md#the-trust-boundary).

## Branch

Every node records the git branch it was written on, in its metadata. The CLI detects it (`--branch` overrides, `--no-branch` skips); MCP clients pass `branch` on each write, because the server cannot see the agent's checkout. Branch is for filtering (`deciduous nodes -b feature-x`, `query_nodes` with `branch`) and for `check_activity`'s view of who is writing where. Nothing is locked by branch: two agents on one branch both write, and their nodes interleave.
