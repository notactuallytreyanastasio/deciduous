# Keep the approaches that were abandoned

When an approach turns out wrong, the useful record is not the corrected decision alone but the chain that replaced it: the old decision, the observation that killed it, a `revisit`, and the new decision, with the old one marked `superseded`. The same structure serves history reconstructed after the fact from commits. Both end up in the shared workspace, written either with `deciduous archaeology` from the CLI or node by node over MCP.

```text
old decision --> observation (why it failed) --> revisit --> new decision
  (superseded)
```

## A pivot from the CLI, in one command

`deciduous archaeology pivot` writes the observation, the revisit and the new decision, links them, and supersedes the old node. `--dry-run` shows it first:

```console
$ deciduous archaeology pivot 0a4240dc \
    "A fixed 30s deadline fails batch jobs that legitimately wait minutes" \
    "Per-caller deadline passed in the request context" \
    -r "One deadline cannot fit interactive and batch callers" -c 75
=== PIVOT CREATED ===

  #27 [observation] "A fixed 30s deadline fails batch jobs that legitimately wait minutes"
    └── linked from #4 "Use an overall deadline, not an attempt cap"
  #28 [revisit] "Reconsidering: Use an overall deadline, not an attempt cap"
    └── linked from #27 [observation]
  #29 [decision] "Per-caller deadline passed in the request context"
    └── linked from #28 [revisit]

  Superseded: #4 "Use an overall deadline, not an attempt cap" -> status: superseded

  Summary: 3 nodes created, 3 edges created, 1 node superseded
```

The first argument takes a local id, `change_id` prefix or server UUID. All of it reached the server; `remote status` afterwards listed only nodes other writers had added.

The work done under the old decision is history too. `archaeology supersede --cascade` marks a node and everything below it:

```console
$ deciduous archaeology supersede 29d960e4 --cascade
Superseded node #5 "Apply a 30s deadline to the retry loop"
  Also superseded (cascade):
    #6 "Retry loop stops at the deadline"
  Total: 2 node(s) marked superseded
```

`--dry-run` lists what would change; read it before running the real thing.

## The same pivot over MCP

The server has no pivot tool. Three `add_node` calls with `parent_id`, then `update_node`:

```javascript
add_node({workspace: "my-app", branch: "agent-api", node_type: "observation",
  title: "Mobile client cannot parse problem+json",
  description: "iOS 1.x hard-codes {error: string}; 12% of traffic",
  parent_id: "<old decision id>"})
add_node({workspace: "my-app", branch: "agent-api", node_type: "revisit",
  title: "Reconsidering the error format", parent_id: "<observation id>"})
add_node({workspace: "my-app", branch: "agent-api", node_type: "decision",
  title: "problem+json for v2 clients, legacy body for v1", parent_id: "<revisit id>"})
update_node({workspace: "my-app", branch: "agent-api", node_id: "<old decision id>", status: "superseded"})
```

Each call needs the id the previous one returned, so they cannot go in one batch. The CLI reads it back as a pivot once pulled:

```console
$ deciduous remote pull
$ deciduous narratives pivots -b agent-api
=== PIVOTS === (1 found)

Pivot #1: [revisit #40] "Reconsidering the error format"
  Old approach: #8 [decision] "Use RFC 7807 problem+json over an ad-hoc error object" (superseded)
  Triggered by: #39 [observation] "Mobile client cannot parse problem+json"
  New approach: #38 [decision] "problem+json for v2 clients, legacy body for v1"
```

`ask_graph` follows revisits and superseded nodes when the question asks about history or pivots, so "why did we change the retry deadline?" comes back with both the dead decision and its replacement ([Recovery](recovery.md#what-the-agent-is-told)).

## Statuses

`superseded` means replaced by a newer approach; `abandoned` means tried and dropped with nothing replacing it. `deciduous status <id> superseded` and `abandoned` work, although the 1.1.0 `status --help` lists only `pending, active, completed, rejected`. Over MCP, `update_node` takes all six. Query the current state with `query_nodes` and `status: "active"`, the history with no status filter, or `type: "revisit"` for the pivots.

## Reconstructing history from commits

For a codebase that was built before anyone logged decisions, the graph can be filled in from git. Pick one bounded question (why retries have a cap, why an endpoint has two response shapes) rather than the whole repository, and read the evidence first:

```bash
git log --oneline -- src/retry.rs
git show <sha> -- src/retry.rs
```

A commit proves what changed, not why. Record the author's stated reason as such, mark your inference as inference, and say what you did not look at. Record options only where the evidence shows they were considered; alternatives you can think of now are not history.

The CLI can backdate a node and tie it to the commit:

```console
$ deciduous add decision "Retry with exponential backoff (historical)" \
    -d "Reconstructed from commit message" --date 2024-03-01 --commit <sha>
Created node 37 (type: decision, ...) [commit: 4c3ba126...] [branch: main] [date: 2024-03-01T00:00:00-05:00]
```

The date reaches the server: `query_nodes` returned `"created_at": "2024-03-01T05:00:00.000000Z"` for it. `--date` takes `YYYY-MM-DD`, `YYYY-MM-DD HH:MM:SS` or RFC 3339, read in local time. MCP's `add_node` has no date argument; an agent reconstructing history puts the date and full hash in the description, or uses the CLI. (An older page said a backdated CLI node stays local. With a `[remote]` it does not.)

`deciduous archaeology timeline` lists nodes in creation order, so backdated ones sort where they belong:

```console
$ deciduous archaeology timeline -b main
=== TIMELINE === (11 nodes)

  2024-03-01  #37  [decision]      Retry with exponential backoff (historical) pending
  2026-11-08  #1   [goal]          Bound retries for failed requests        active        90%
  2026-11-08  #4   [decision]      Use an overall deadline, not an attempt cap superseded    80%
  2026-11-08  #27  [observation]   A fixed 30s deadline fails batch jobs that legitimately wait minutes pending
  2026-11-08  #28  [revisit]       Reconsidering: Use an overall deadline, not an attempt cap pending
  2026-11-08  #29  [decision]      Per-caller deadline passed in the request context pending  75%
```

It reads the local cache; `remote pull` first. `-t` filters by type, `-l` limits to the most recent N, `--json` for scripts.

## Narratives

A narrative is the prose version: how one part of the system got the way it is, with its pivots. `deciduous narratives init` writes `.deciduous/narratives.md` with one section per goal that is not superseded or abandoned (pending and completed goals included) to fill in:

```console
$ deciduous narratives init
Created .../.deciduous/narratives.md with 2 goal section(s)
$ deciduous narratives show
# Narratives
...
## Bound retries for failed requests
> Node #1 | Status: active

**Current state:** [How it works today]

**Evolution:**
1. [First approach] - [why]
2. **PIVOT:** [what changed] - [why it changed]
3. [Current approach] - [why this is better]
```

That file is local. It is not sent to the server, and the `.gitignore` that `deciduous init` writes ignores everything in `.deciduous/` except `config.toml` and `graph.json`, so git does not carry it either. Teammates see the graph, not the narrative; put anything they need into nodes, or commit the file somewhere outside `.deciduous/` with `-o`.

## The commands and skills init installs

`deciduous init` writes `.claude/commands/decision-graph.md` (`/decision-graph`: build a graph from commit history, with narratives as the source of truth and commits as evidence) and three instruction files, `.claude/skills/pulse.md`, `narratives.md` and `archaeology.md`, for mapping the current design, writing narratives, and turning them into nodes. All of them drive the CLI (`deciduous add`, `link`, `status`, `--date`), so in a project with a `[remote]` what they write reaches the server. Review them before handing them to an agent: `/decision-graph` assumes the binary is at `~/.cargo/bin/deciduous`.

## Check the result

`find_orphans` finds nodes nothing points at; an empty list says the graph is connected, not that it is true. Have a second agent check the important claims against the commits they cite.
