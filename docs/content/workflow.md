# Record the work as goal, options, decision, action, outcome

A useful graph answers two questions for whoever comes next: what was asked for, and what has already been tried. Write each node at the moment it happens, on the server everyone shares: an agent through the server's MCP tools, a person through the `deciduous` CLI in a repository whose `.deciduous/config.toml` has a `[remote]`. Both land in the same workspace, with the same `change_id` on every node, so either side can link to what the other wrote.

```text
goal -> option(s) -> decision -> action -> outcome
                                      \-> observation (anywhere)
```

The examples use workspace `my-app`. Everything shown was run against a 1.1.0 server; ids are the ones it returned.

## The goal: the request, verbatim

Put the user's exact words in `prompt`. A summary loses what the next session needs to know.

MCP:

```javascript
add_node({
  workspace: "my-app", branch: "agent-api",
  node_type: "goal", status: "active",
  title: "Validate create-item requests",
  prompt: "Add request validation and tests for the create-item endpoint."
})
```

```json
{ "id": "32d05bcb-1b2b-4b7b-b3b8-170a8aa8b7fb", "change_id": "5ca33b18-c27e-4723-9610-3874a592fb7a",
  "created": true, "status": "active", "message": "Node created successfully" }
```

CLI (the branch is detected from git; `--prompt-stdin` takes a multi-line message from a heredoc):

```console
$ deciduous add goal "Bound retries for failed requests" -c 90 -p "Stop retrying forever when the upstream API is down."
Warning: Prompt is only 52 chars. This looks like a summary, not a full prompt.
Created node 1 (type: goal, title: Bound retries for failed requests) [confidence: 90%] [prompt: 52 chars] [branch: main]
```

The warning fires on any prompt under some length; it does not block the write. Nodes default to `pending`. `add_node` takes `status: "active"`; the CLI needs a second command, `deciduous status 1 active`.

Keep credentials out of prompts. Whoever holds the server token can read every prompt in every workspace (see [Architecture](architecture.md)).

## Options and the decision

The decision is only written once an option is actually chosen. Its title should say what it beat.

MCP has one call that writes the decision, its options and the edges in a single transaction:

```javascript
log_decision({
  workspace: "my-app", branch: "agent-api",
  parent_node_id: "32d05bcb-1b2b-4b7b-b3b8-170a8aa8b7fb",
  title: "Validate at the HTTP boundary, not after the write",
  rationale: "Reject malformed requests before they reach the service layer.",
  chosen_option: {title: "Validate at the HTTP boundary", description: "Return 422 with stable field error codes."},
  rejected_options: [{title: "Validate after the database write",
                      reason: "The service would receive invalid input and rely on rollback."}]
})
```

```json
{ "decision_id": "d63d2ce0-d91f-406e-9c4f-7253ee438c59",
  "chosen":   {"id": "894c65c3-...", "title": "Validate at the HTTP boundary"},
  "rejected": [{"id": "1644a09f-...", "title": "Validate after the database write"}] }
```

Know the shape it writes, because it is not the canonical flow: the edges run goal -> decision, then decision -> option (`chosen`) and decision -> option (`rejected`), with each `reason` as the edge rationale. The decision is created `completed`, the chosen option `completed`, the rejected one `rejected`.

`capture_conversation_turn` does the same for options inside a larger step. Pass the options as objects and mark the winner, or every option is recorded as rejected:

```javascript
options_considered: [{title: "Codes in a shared enum", chosen: true}, {title: "Codes as string literals"}],
decision: {title: "Shared enum over string literals", rationale: "Tests import the same enum"}
```

With `options_considered: ["RFC 7807 problem+json", "Ad-hoc {error: string}"]` (plain strings) both options came back with `"chosen": null` and both got a `rejected` edge from the decision, including the one that was picked.

The CLI writes one node per command and links them by hand, so it writes the flow in the canonical direction, option -> decision:

```console
$ deciduous add option "Cap the attempt count" -c 70
Created node 2 (type: option, title: Cap the attempt count) [confidence: 70%] [branch: main]
$ deciduous add option "Use an overall deadline" -c 80
Created node 3 (type: option, title: Use an overall deadline) [confidence: 80%] [branch: main]
$ deciduous link 1 2 -r "possible approach"
$ deciduous link 1 3 -r "possible approach"
$ deciduous add decision "Use an overall deadline, not an attempt cap" -c 80 -d "A slow attempt can exceed the wait budget under an attempt cap."
Created node 4 (type: decision, ...)
$ deciduous link 3 4 -t chosen -r "bounds total wait even when attempts are slow"
Created edge 3 (3 -> 4 via chosen)
$ deciduous link 2 4 -t rejected -r "a slow attempt can exceed the budget"
Created edge 4 (2 -> 4 via rejected)
```

The numbers are local ids, valid only in this checkout's cache. To refer to a node someone else wrote, use the 8-character `CHANGE` column from `deciduous nodes` or the server's UUID; `link`, `status`, `show` and `doc attach` accept all three.

## The action: before you edit

MCP: `parent_id` creates the node and its incoming edge in one transaction.

```javascript
add_node({
  workspace: "my-app", branch: "agent-api",
  node_type: "action", status: "active",
  title: "Implement request validation at the route boundary",
  files: ["src/http/items.ts"],
  parent_id: "d63d2ce0-d91f-406e-9c4f-7253ee438c59",
  rationale: "Implement the selected boundary"
})
```

Do not send `add_node` and an `add_edge` that needs its id in the same batch. The id does not exist yet, and the server says so:

```text
from_node_id is not a node id: "PLACEHOLDER". To link a node you are creating now, pass parent_id
to add_node instead; an id must come from a previous answer, not be written ahead of it
```

A non-goal written without `parent_id` is accepted, and the reply carries `suggested_parents`: up to five ranked guesses with the signals behind each (same branch, type fit, text similarity, open status). Nothing is linked for you.

CLI:

```console
$ deciduous add action "Apply a 30s deadline to the retry loop" -c 85 -f "src/retry.rs,tests/retry.rs"
$ deciduous link 4 5 -r "implement the chosen option"
```

`--commit` takes `HEAD`, a branch, a tag or a hash prefix and stores the full hash; a rev git cannot resolve is refused. The server cannot see your checkout, so over MCP pass the real hash in `commit`; it stores whatever string you send.

## Observations: what changes the plan

```javascript
log_observation({
  workspace: "my-app", branch: "agent-tests",
  title: "Reuse the RFC 7807 shape in fixtures",
  related_to: "32d05bcb-1b2b-4b7b-b3b8-170a8aa8b7fb",
  took_from: "183e9e9a-153a-4141-9fb2-8312fe486012",
  why: "The API side already emits problem+json; fixtures that match it keep the tests honest."
})
```

`related_to` and `took_from` take a server UUID or a full `change_id`. An 8-character prefix is refused over MCP (`took_from is not a node id: "9ad1b0b7"`), although the CLI accepts one. `took_from` draws a `took_from` edge from the source node to the new observation, with `why` as its rationale; [Teams](teams.md#borrowing-across-branches-tookfrom) covers it.

## The outcome: the number or the error

```javascript
close_thread({
  workspace: "my-app", branch: "agent-api",
  parent_node_id: "de6ccf4c-877b-406d-a41a-5474d8c764ec",
  title: "Invalid requests return 422 with field codes",
  description: "npm test -- tests/items.test.ts: 8 passed.",
  success: true,
  lessons_learned: ["The router already parses JSON before handlers run"]
})
```

`success: true` writes a `completed` outcome under the parent; `success: false` writes a `rejected` one. Each `lessons_learned` entry becomes an observation under the outcome; each `next_steps` entry becomes a new goal. `next_steps` entries must be objects (`{"title": "...", "description": "..."}`); unlike `lessons_learned`, a plain string is refused with `next_steps[0] must be an object ... nothing was written`. `goal_node_id` marks that goal `completed`, so pass it only when the whole goal is done, not when your part is.

CLI:

```console
$ deciduous add outcome "Retry loop stops at the deadline" -c 90 -d "cargo test retry: 4 passed"
$ deciduous link 5 6 -r "test result"
$ deciduous status 6 completed
```

`deciduous status` accepts `pending, active, completed, rejected, superseded, abandoned`. Its 1.1.0 `--help` lists only the first four.

## One step, one call

`capture_conversation_turn` writes a step's goal, observations, options, decision, action and outcome together, all or nothing, linked to each other and to `parent_node_id`:

```javascript
capture_conversation_turn({
  workspace: "my-app", branch: "agent-api",
  parent_node_id: "32d05bcb-1b2b-4b7b-b3b8-170a8aa8b7fb",
  summary: "Chose the error format and wrote it",
  observations: ["Clients already parse RFC 7807 problem documents"],
  options_considered: [{title: "RFC 7807 problem+json", chosen: true}, {title: "Ad-hoc {error: string}"}],
  decision: "Use RFC 7807 problem+json over an ad-hoc error object",
  action: "Emit problem+json from the validation middleware",
  outcome: "Validation errors render as problem+json; 3 snapshot tests pass"
})
```

It wrote parent -> observation, parent -> decision, decision -> each option, decision -> action -> outcome.

## Retries and typos

A tool refuses an argument it does not have and names the ones it does, and nothing is written:

```text
add_node has no argument "parent" (did you mean parent_id?). Its arguments are: branch, change_id,
commit, confidence, description, edge_type, files, node_type, parent_id, prompt, rationale, status,
title, workspace; nothing was written
```

To make a retried `add_node` safe, give it a `change_id` (any UUID). The second call with the same one returns the first node, `"created": false`, and does not duplicate the edge:

```json
{ "id": "c2bc238e-...", "created": false,
  "message": "Node already exists with change_id 11111111-2222-4333-8444-555555555555; nothing new was created (linked under parent_id)" }
```

## Changing a node

`update_node` changes title, description, status or metadata. `metadata` is merged: keys you send are set, a key sent as `null` is removed, the rest are kept. Sending `{"commit": "0123abc"}` to an action with `branch` and `files` left all three in place. (Older docs said it replaced the map. It does not.)

When an approach is abandoned, do not edit the old decision into the new one. Record why and pivot; [History](history.md) shows both the one-command CLI pivot and the MCP calls.

## Before you stop

```javascript
find_orphans({workspace: "my-app"})
```

```json
{ "count": 1, "orphans": [{ "id": "e0ff0733-...", "node_type": "observation", "title": "Orphan probe" }] }
```

Link each orphan to its real cause, not to whichever root makes the list empty. `suggest_parents: true` adds ranked guesses for the first 25.

From the CLI, `deciduous remote status` is the check: it exits 0 and prints `In sync` when every write reached the server, and exits 1 listing what differs otherwise. Writes are sent before each command exits, so it normally says:

```console
$ deciduous remote status
Remote: https://deciduous.example.com
Workspace: my-app
Log: 0 write(s) waiting, 0 rejected  (.../.deciduous/remote-log.jsonl)

                 local    server
  nodes              6         6
  edges              6         6

In sync: no writes waiting, and every node, edge and document matches field by field.
Themes and tags are not sent to the server, so they are not compared.
```

Nodes an agent wrote over MCP show up as `Only on the server` until you run `deciduous remote pull`; that is a stale cache, not lost work. [Recovery](recovery.md) covers reading the graph back.
