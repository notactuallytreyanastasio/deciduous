# Deciduous Concepts

The graph records why the code is the way it is: what was asked, which
approaches were on the table, which one won and why, what was done about it,
and what happened. Since 1.1.0 every node can also say what it is backed by.
This page is the model. How the pieces are built is
[ARCHITECTURE.md](ARCHITECTURE.md); the three skills for using it are
[narratives.md](narratives.md).

---

## Why externalize reasoning

An agent's context ends with the session, or is compacted mid-way. The code
that results says what, not why: not that per-IP rate limiting was tried and
rejected, not that it was rejected because mobile clients share carrier NAT
addresses. Six months later the only trace of that reasoning is in whoever
remembers it. The graph is that memory, outside any one session, queryable.

## The graph

A directed graph. Nodes are things that happened; edges say how they relate.

### Node types

Seven. `deciduous add --help` lists six; `revisit` is accepted too.

| Type | What it records | From a scratch project |
|---|---|---|
| `goal` | What was asked for. Carries the user's words verbatim in `metadata.prompt`. | `Rate limit the public API` |
| `option` | One approach that was on the table | `Token bucket per API key`, `Fixed window per IP` |
| `decision` | The choice, titled with what it beat | `Token bucket per key over fixed window per IP` |
| `action` | Work done: code written, a commit made | `Add token bucket middleware` |
| `outcome` | What happened: a test result, a number, an error | `Middleware passes load test at 10k rps` |
| `observation` | A fact that bears on the plan; the description holds the detail | `Shared IPs behind carrier NAT` |
| `revisit` | An earlier choice being reconsidered: the pivot point | `Reconsidering: Token bucket per key over fixed window per IP` |

### The canonical flow

```
goal -> options -> decision -> actions -> outcomes
```

Goals lead to options, options to a decision, the decision to actions, actions
to outcomes. Observations attach wherever they bear. Goals do not lead
straight to decisions; a decision with no options recorded is a decision whose
alternatives are lost. A decision node is created when an option is chosen,
not before.

```
$ deciduous edges
ID    FROM   TO     TYPE         RATIONALE
----------------------------------------------------------------------
1     1      2      leads_to     possible approach
2     1      3      leads_to     possible approach
3     2      4      chosen       survives NAT
4     3      4      rejected     carrier NAT
5     4      5      leads_to     implementation
6     5      6      leads_to     result
7     7      3      blocks       why per-IP fails
```

### Edge types

| Type | Meaning | Typical direction |
|---|---|---|
| `leads_to` | the default: one thing follows from another | goal → option, decision → action, action → outcome |
| `chosen` | this option won | option → decision |
| `rejected` | this option lost | option → decision |
| `requires` | a dependency | action → action |
| `blocks` | prevents progress | observation → option or action |
| `enables` | makes something possible | action → outcome |
| `took_from` | a borrow: `from` is the node taken from, `to` is the node that used it; crosses branches on purpose | another branch's decision → your observation |

Every edge can carry a rationale (`-r`), which is where "why" lives for the
connection itself.

### Status

Six. `deciduous status --help` lists four; the binary accepts all six and
names them when refused:

```
$ deciduous status 3 bogus
Error: unknown status 'bogus'; expected one of: pending, active, completed, rejected, superseded, abandoned
```

| Status | Meaning |
|---|---|
| `pending` | the default on creation |
| `active` | current truth: this is how things work now |
| `completed` | finished |
| `rejected` | an option that lost |
| `superseded` | replaced by a later approach (a pivot) |
| `abandoned` | tried and dropped, not replaced |

There is no `nodes --status` filter. `deciduous nodes` filters by branch
(`-b`), type (`-t`) and theme (`--theme`); the status is a column in its
output and in `show`.

### The pivot

When direction changes, the graph keeps the old path and says why it was left:

```
[old decision] ──► [observation: what was learned] ──► [revisit] ──► [new decision]
      status: superseded
```

`deciduous archaeology pivot <old_id> "<observation>" "<new approach>"` writes
all of that in one command (three nodes, three edges, the old node
superseded); `--dry-run` shows it first. `deciduous narratives pivots` lists
every revisit with its chain.

## Identity

Three ids name one node, and every command that takes a node takes any of
them:

| Id | Looks like | Scope |
|---|---|---|
| local id | `4`, or `#4` | this machine's database; differs on every clone |
| `change_id` | `918ffbe9-8004-4ab8-bfdf-0f995cead45e`, or 4+ hex characters of it | everywhere: the same in every clone, in `graph.json`, on the server |
| server id | a UUID the MCP tools return | the shared server; resolved last, only with a `[remote]` |

`deciduous nodes` prints the first eight characters of the `change_id` in its
CHANGE column. To link to a teammate's node, use that, never their local id:
`deciduous link 8311856e 42 -r "implements their goal"`.

## The prompt

The most valuable field for recovery is the user's exact words. A summary
("user asked for auth") throws away the constraints that shaped every decision
under it. `add goal -p "..."` stores it; `--prompt-stdin` takes several lines;
`deciduous prompt <id>` adds or replaces it later. The CLI warns when a prompt
looks like a summary:

```
Warning: Prompt is only 110 chars. This looks like a summary, not a full prompt.
```

Transcript-derived goals carry the prompt verbatim without anyone typing it.

## Evidence (1.1.0)

A node written by hand or by an agent is a claim. A node derived from a Claude
Code transcript says what backs it, in `metadata.evidence`:

| `evidence.kind` | Means | Where it comes from |
|---|---|---|
| `proven` | the transcript recorded the thing happening | a tool call and its result (an `Edit`, a `cargo test` exit, a `git commit` hash), or the user's own words |
| `inferred` | the assistant said so | the closing text of a turn |

Both kinds sit side by side, labelled, so "all tests pass" in the assistant's
report and the test runner's own result are two nodes and the graph says which
is which:

```
$ deciduous show 14
Node #14 outcome
Title: cargo test passed
Description: 5 passed, 0 failed
  Evidence: proven transcript, session 0f1e2d3c, 1 tool call(s), exit ok

$ deciduous show 16
Node #16 outcome
Title: Done. The widget is in and all 5 tests pass.
  Evidence: inferred transcript, session 0f1e2d3c
```

Proven actions also carry `metadata.consulted` (the files the agent `Read`
since its previous action) and `metadata.file_hashes` (git blob ids of the
files it touched, `git:<sha1>`, the id `git log --find-object` takes;
`sha256:` without git). `show --json` prints the metadata raw:

```json
"evidence": {
  "kind": "proven",
  "session": "0f1e2d3c-0000-4000-8000-000000000001",
  "source": "transcript",
  "tool_use_ids": ["t2", "t3"],
  "uuids": ["a3", "a4"]
}
```

Nodes written through the CLI or MCP have no `evidence` field. The web viewer
does not show evidence or file hashes yet; `show` does. Decisions and options
are never derived from a transcript: see [transcripts.md](transcripts.md).

## Where the graph lives

Two models, chosen when `deciduous init` asks.

**A shared server** (the default since 1.0.11: PostgreSQL on this machine,
or in Docker, or someone else's). The server is the truth, one workspace per
repository. Agents write to it over MCP. The CLI writes `.deciduous/deciduous.db`
and replays each write to the server as an op; `remote pull` refreshes the
local copy; `remote status` shows the gap. Nothing about the graph is
committed except `[remote] url` and `workspace` in `.deciduous/config.toml`.

**Git** (no `[remote]`). The shared graph is `.deciduous/graph.json`, committed
with the code and merged record by record by a git merge driver. The local
database mirrors it: every write lands in both, and a checkout, pull or reset
is followed by the next command, so the graph you see is the graph of the
commit you have checked out. `deciduous sync` reconciles the two and `sync
--check` exits 1 when anything is pending.

In both, `.deciduous/deciduous.db` is a per-machine cache and is never
committed. [MULTI_USER_SYNC.md](MULTI_USER_SYNC.md) has both workflows.

## Themes and tags

A theme is a named colour (`deciduous themes create rate-limiting -c "#ef4444"`);
a tag puts a node under it (`deciduous tag add 1 rate-limiting`). `nodes
--theme <name>` filters, and the viewer offers each theme as a filter chip.
Themes and tags travel in `graph.json`; they are not sent to the server.

## Documents

Files attach to nodes: `deciduous doc attach <node> <file> -d "..."`. They are
stored under `.deciduous/documents/` as `<filename>.<first 8 hex of sha256>`,
so the same file attached twice is one file on disk. Detach is a soft delete;
`doc gc` removes files nothing references. The viewer shows a node's documents
in its detail panel. Documents are not in `graph.json`; with a `[remote]`,
`remote push --seed` sends them. The full process is
[DECIDUOUS_DOCUMENT_ADDITION_PROCESS.md](DECIDUOUS_DOCUMENT_ADDITION_PROCESS.md).

## What is not graph

**Messages.** Agents working in parallel coordinate on the board (`deciduous
board`, `post_message`/`read_messages`). Messages have ids, replies and
`@label` addressing; they are never in `graph.json`, never exported, never
synced. One board serves every worktree of a repository, or the server when
there is a `[remote]`.

**Roadmap items** sync through `ROADMAP.md` itself (`deciduous roadmap`).

**The Q&A history** of the viewer's ask panel stays in the local database.

## When to write

| Moment | Node |
|---|---|
| The user asks for something | `goal`, with the message verbatim |
| More than one way to do it | one `option` each |
| One is picked | `decision`, titled with what it beat |
| Code or config is about to change | `action` |
| It works, or fails | `outcome`, with the number or the error |
| A fact changes the plan | `observation` |
| An earlier choice turns out wrong | `revisit` |

Not: reading, searching, planning, running tests to look. Those are the
agent's process, not the project's decisions, and the transcript import keeps
them only as an action's `consulted` list. Link every node when you create it
(`add_node` with `parent_id` over MCP, `link` on the CLI); root goals are the
only valid orphans.
