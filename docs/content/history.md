# Keep the approaches that were abandoned

When an approach turns out wrong, the useful record is not the corrected decision alone but the chain that replaced it: the old decision, the observation that killed it, a `revisit`, and the new decision, with the old one marked `superseded`. The same structure serves history reconstructed after the fact from commits. Both end up in the shared workspace, written either with `deciduous archaeology` from the CLI or node by node over MCP.

```text
old decision --> observation (why it failed) --> revisit --> new decision
  (superseded)
```

## A pivot from the CLI, in one command

`deciduous archaeology pivot` writes the observation, the revisit and the new decision, links them, and supersedes the old node. `--dry-run` shows it first:

```console
$ deciduous archaeology pivot 2465036d \
    "A fixed 30s deadline fails batch jobs that legitimately wait minutes" \
    "Per-caller deadline passed in the request context" \
    -r "One deadline cannot fit interactive and batch callers" -c 75
=== PIVOT CREATED ===

  #6 [observation] "A fixed 30s deadline fails batch jobs that legitimately wait minutes"
    └── linked from #3 "Use an overall deadline, not an attempt cap"
  #7 [revisit] "Reconsidering: Use an overall deadline, not an attempt cap"
    └── linked from #6 [observation]
  #8 [decision] "Per-caller deadline passed in the request context"
    └── linked from #7 [revisit]

  Superseded: #3 "Use an overall deadline, not an attempt cap" -> status: superseded

  Summary: 3 nodes created, 3 edges created, 1 node superseded
```

The first argument takes a local id, `change_id` prefix or server UUID. All of it reached the server; `remote status` afterwards said `In sync`. The new decision is `pending`, like any new node.

The work done under the old decision is history too. `archaeology supersede --cascade` marks a node and everything below it. Run it on the old decision's actions, never on the old decision itself once a pivot hangs under it: the observation, the revisit and the new decision are all below the old decision now, and the cascade follows them. On the old decision, `--dry-run` shows it would take the replacement with it:

```console
$ deciduous archaeology supersede 2465036d --cascade --dry-run
Would supersede node #3 "Use an overall deadline, not an attempt cap"
  Would also supersede (cascade):
    #4 "Apply a 30s deadline to the retry loop"
    #6 "A fixed 30s deadline fails batch jobs that legitimately wait minutes"
    #5 "Retry loop stops at the deadline"
    #7 "Reconsidering: Use an overall deadline, not an attempt cap"
    #8 "Per-caller deadline passed in the request context"
  Total: 6 node(s) would be marked superseded
```

On the action under it, only the work is marked:

```console
$ deciduous archaeology supersede 04e9c57d --cascade
Superseded node #4 "Apply a 30s deadline to the retry loop"
  Also superseded (cascade):
    #5 "Retry loop stops at the deadline"
  Total: 2 node(s) marked superseded
```

Always run `--dry-run` first and read the list before running the real thing.

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

Each call needs the id the previous one returned, so they cannot go in one batch. The CLI reads it back as a pivot once pulled (the six new nodes here are the `log_decision` that made the old decision and its two options, plus the three above):

```console
$ deciduous remote pull
Pulled: my-app <- https://deciduous.example.com
  fetched 14 nodes, 12 edges, 0 deletion(s)
  imported 6 new node(s), updated 0, removed 0; imported 5 edge(s), removed 0
$ deciduous narratives pivots -b agent-api
=== PIVOTS === (1 found)

Pivot #1: [revisit #9] "Reconsidering the error format"
  Old approach: #13 [decision] "Use RFC 7807 problem+json over an ad-hoc error object" (superseded)
  Triggered by: #11 [observation] "Mobile client cannot parse problem+json"
  New approach: #14 [decision] "problem+json for v2 clients, legacy body for v1"
```

`ask_graph` follows revisits and superseded nodes when the question uses a history word (`changed`, `pivot`, `history`, `previously`, `replaced` and the like; `change` is not one). "How has the retry deadline changed?" takes that route and comes back with both the dead decision and its replacement. "Why did we change the retry deadline?" goes by the rationale route instead, because of `why` ([Recovery](recovery.md#what-the-agent-is-told)).

## Statuses

`superseded` means replaced by a newer approach; `abandoned` means tried and dropped with nothing replacing it. `deciduous status <id> superseded` and `abandoned` work. In 1.1.0, `add --help` leaves out `revisit` and `status --help` lists only four statuses; both are accepted. The next release lists them all. Over MCP, `update_node` takes all six.

`active` is only set when someone sets it: new nodes are `pending`, and so is the new decision `archaeology pivot` writes. So `query_nodes` with `status: "active"` usually returns `{"count":0,"nodes":[]}`, even in a workspace with a current decision. For the current state, query with no status filter and drop `superseded`, `abandoned` and `rejected` nodes, or use `consolidation_report`, which treats any decision not in those three as current. For the history, use no filter; for the pivots, `type: "revisit"`.

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
Created node 9 (type: decision, ...) [commit: 64551157...] [branch: main] [date: 2024-03-01T00:00:00-05:00]
```

The date reaches the server: `query_nodes` returned `"created_at": "2024-03-01T05:00:00.000000Z"` for it. `--date` takes `YYYY-MM-DD`, `YYYY-MM-DD HH:MM:SS` or RFC 3339, read in local time. MCP's `add_node` has no date argument; an agent reconstructing history puts the date and full hash in the description, or uses the CLI. (An older page said a backdated CLI node stays local. With a `[remote]` it does not.)

`deciduous archaeology timeline` lists nodes in creation order, so backdated ones sort where they belong:

```console
$ deciduous archaeology timeline -b main
=== TIMELINE === (9 nodes)

  2024-03-01  #9   [decision]      Retry with exponential backoff (historical) pending
  2026-11-08  #1   [goal]          Bound retries for failed requests        pending       90%
  2026-11-08  #3   [decision]      Use an overall deadline, not an attempt cap superseded    80%
  2026-11-08  #4   [action]        Apply a 30s deadline to the retry loop   superseded
  2026-11-08  #6   [observation]   A fixed 30s deadline fails batch jobs that legitimately wait minutes pending
  2026-11-08  #7   [revisit]       Reconsidering: Use an overall deadline, not an attempt cap pending
  2026-11-08  #8   [decision]      Per-caller deadline passed in the request context pending       75%
```

It reads the local cache; `remote pull` first. `-t` filters by type, `-l` limits to the most recent N, `--json` for scripts.

## Narratives

A narrative is the prose version: how one part of the system got the way it is, with its pivots. `deciduous narratives init` writes `.deciduous/narratives.md` with one section per goal that is not superseded or abandoned (pending and completed goals included) to fill in:

```console
$ deciduous narratives init
Created .../.deciduous/narratives.md with 1 goal section(s)
$ deciduous narratives show
# Narratives
...
## Bound retries for failed requests
> Node #1 | Status: pending

**Current state:** [How it works today]

**Evolution:**
1. [First approach] - [why]
2. **PIVOT:** [what changed] - [why it changed]
3. [Current approach] - [why this is better]
```

That file is local. It is not sent to the server, and the `.gitignore` that `deciduous init` writes ignores everything in `.deciduous/` except `config.toml` and `graph.json`, so git does not carry it either. Teammates see the graph, not the narrative; put anything they need into nodes, or commit the file somewhere outside `.deciduous/` with `-o`.

## The commands and skills init installs

`deciduous init` writes `.claude/commands/decision-graph.md` (`/decision-graph`: build a graph from commit history, with narratives as the source of truth and commits as evidence) and three instruction files, `.claude/skills/pulse.md`, `narratives.md` and `archaeology.md`, for mapping the current design, writing narratives, and turning them into nodes. Only `/decision-graph` is a slash command. Claude Code loads project skills from `.claude/skills/<name>/SKILL.md` ([Claude Code docs](https://code.claude.com/docs/en/skills)); init writes flat `.md` files with no frontmatter, so there is no `/pulse`, `/narratives` or `/archaeology`, although the generated `CLAUDE.md` lists them as skills. Point an agent at them by path: "read `.claude/skills/archaeology.md` and follow it". All of them drive the CLI (`deciduous add`, `link`, `status`, `--date`), so in a project with a `[remote]` what they write reaches the server. Review them before handing them to an agent: `/decision-graph` assumes the binary is at `~/.cargo/bin/deciduous`.

## Check the result

`find_orphans` finds nodes nothing points at; an empty list says the graph is connected, not that it is true. Have a second agent check the important claims against the commits they cite.
