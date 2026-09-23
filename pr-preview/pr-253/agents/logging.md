# Logging while you work

The graph is only worth reading if it is written while the work happens. A
graph reconstructed at the end records what you remember, not what you chose.
Nothing enforces this. Write at the moments below, as they happen, one node per
thing someone would want to know later.

## At the start of a session

Read before you write. Find out what is already in flight on this branch and
what the last session left open:

- Claude Code with the slash commands installed: run `/recover`.
- `query_nodes` with `workspace` and `branch`, then `ask_graph`
  with the topic of the user's request. `check_activity` shows who else is
  writing to the workspace.

If the request continues an existing goal, attach new nodes under that goal.
Do not start a parallel one.

## The shape

```
goal -> option(s) -> decision -> action(s) -> outcome(s)
                  observations attach anywhere
```

| Type | Write it when | Title is |
|------|---------------|----------|
| `goal` | The user asks for something | What they asked for, as a result |
| `option` | You see more than one way to do it | The approach, in a phrase |
| `decision` | You pick an option | The choice **and** what it beat |
| `action` | You are about to change code or config | What you are changing |
| `outcome` | The change worked or failed | The result, with the number or error |
| `observation` | You learned a fact that affects the work | The fact, not "noticed something" |
| `revisit` | An earlier decision turned out wrong | What is being reconsidered |

Rules that keep it readable:

- A goal carries the user's **verbatim** message in `prompt`, not a summary.
  Do the same for any message that changes direction.
- Options come before the decision. A goal never points straight at a decision.
- Every node except a root goal has a parent. Create it with its parent in
  one call (below).
- Titles are claims someone can check later: "Trigram index: search 85 ms -> 3 ms",
  not "Improved search".
- Put the why in `description`. That is the part a later session cannot recover
  from the code.

Do **not** log your own process: reading files, running tests to look around,
planning, "starting work". A useful test: would the user put this on a project
timeline or in a PR description? If not, leave it out.

## Create and link in one call

`add_node` takes `parent_id` and creates the node and its
edge in one transaction:

```json
{
  "node_type": "observation",
  "title": "remote init writes url + workspace, but CLI adds still land locally",
  "description": "local 1 / remote 0 after `deciduous add`; needs `remote push`",
  "parent_id": "5b39d6e3-de1f-480d-b09e-5ca43931d8ed",
  "rationale": "Setup fact the docs rest on",
  "workspace": "deciduous",
  "branch": "docs-for-agents-and-people",
  "confidence": 90
}
```

The reply contains `"message":"Node created and linked"`, the new `id`, and
`edge_id`.

**Never send `add_node` and an `add_edge` that needs its id in the same
parallel batch.** The id does not exist yet, and whatever stands in for it (a
placeholder, a guess, a made-up UUID) is refused. If a node needs a second
parent, send `add_edge` after the `add_node` reply has arrived.

## Commits

After a commit, log an `action` or `outcome` that carries it:

- `add_node` with `commit` set to the hash from `git rev-parse HEAD`.

## Several actions at once

`log_decision` writes a decision, its options, and `chosen`/`rejected` edges in
one call. `capture_conversation_turn` writes a goal, observations, decision,
action and outcome from one exchange. `close_thread` writes the outcome that
ends a line of work and can mark its goal completed. They save round trips.
They do not replace logging as you go.

## When you finish

- Mark finished goals `completed` and dead ends `abandoned` with `update_node`.
- Run `find_orphans`. Link anything it lists, except root goals.
- Tell the user which nodes you added, and anything you left `pending`.
