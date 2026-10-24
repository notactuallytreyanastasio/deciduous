# Multi-User Sync

Several people, several machines, several agents, one graph. There are two
ways a project shares it, chosen when `deciduous init` asks where the graph
lives, and they do not mix:

| | Shared server (`[remote]` in `.deciduous/config.toml`) | Git (no `[remote]`) |
|---|---|---|
| The truth | PostgreSQL behind the server; one workspace per repository | `.deciduous/graph.json`, committed with the code |
| Agents write | straight to the server over MCP | to the local database, which mirrors `graph.json` |
| The CLI writes | the local database, then an op to the server | the local database and `graph.json` at once |
| Concurrent edits meet | on the server, compare-and-set field by field | in git, through the `deciduous merge-record` merge driver |
| Catch up | `deciduous remote pull` | `git pull`; the next command follows the file |
| Check | `deciduous remote status` (exit 1 on drift) | `deciduous sync --check` (exit 1 if pending) |
| Boards, documents | on the server (`remote push --seed` for documents) | the board in the main worktree's database; documents stay local |

In both models `.deciduous/deciduous.db` is a per-machine cache and is never
committed. `deciduous diff export` and `diff apply` do not exist; the patch
model was removed in 0.17. `.deciduous/sync/` was 0.17 only, and `deciduous
sync` folds a leftover one into `graph.json` and deletes it.

---

## Identity: local ids, change ids, server ids

| Field | Scope | Example | Use it for |
|-------|-------|---------|------------|
| `id` | one machine | `42` | typing quickly at your own prompt |
| `change_id` | everywhere | `8311856e-…` | anything that crosses machines |
| server id | the server | a UUID the MCP tools return | what agents quote; resolved last |

Alice's goal is `#12` on her laptop and `#907` on Bob's. Its `change_id` is the
same on both. Records reference each other only by `change_id`; local ids are
assigned on import and never leave the machine.

Every command that takes a node id also takes a `change_id` prefix (four or more
hex characters, unique). `deciduous nodes` prints the first eight in the CHANGE
column:

```
ID    CHANGE    TYPE         STATUS     TITLE
1     8311856e  goal         pending    Rate limit the public API
5     34367f53  action       pending    Add token bucket middleware
```

```bash
deciduous link 8311856e 5 -r "implements the goal Alice logged"
deciduous status 34367f53 completed
deciduous show 8311
```

The MCP tools accept the same thing: `"node_id": "8311856e"` instead of an
integer. A digit-only prefix is tried as a local id first; quote eight or
more characters of a server id when it could also be the start of a local
node's `change_id`.

## The shared server

Agents write through the server's MCP tools and never touch the local
database. The CLI writes locally first, so it works offline, and appends one
op per field change to `.deciduous/remote-log.jsonl`; the ops are replayed to
`POST /ops` before the command exits, and by `deciduous remote push` later.
The server applies each op at most once and only over the value it replaced,
so a `status` change queued while an agent retitled the node cannot put the
old title back; it is refused by name instead.

```bash
deciduous remote status        # waiting ops, and every node, edge, document that differs, field by field
deciduous remote pull          # send what is waiting, then take the server's rows (newer edit wins per node)
deciduous remote push          # send what is waiting; --seed for history from before the remote
deciduous remote watch         # one line per write, as agents make them
```

```
Log: 0 write(s) waiting, 0 rejected  (.deciduous/remote-log.jsonl)

                 local    server
  nodes            719       719
  edges            845       845

In sync: no writes waiting, and every node, edge and document matches field by field.
Themes and tags are not sent to the server, so they are not compared.
```

A git worktree whose `config.toml` lacks `[remote]` sends to its main
checkout's server (same repository, one workspace). The server has no
branches: a node written on any branch is in the workspace for every branch.
`init` and `update` on a project with a `[remote]` leave `.gitignore`,
`.gitattributes`, the merge driver and `graph.json` alone. The rest of the
server model, including how workspaces are named and what `check_activity`
and `took_from` are for, is in [remote-graph.md](remote-graph.md).

## Git: the graph file

```text
.deciduous/
├── deciduous.db            private cache (gitignored)
├── config.toml             shared
└── graph.json              shared: the graph
```

```json
{
  "version": 1,
  "nodes":  { "<change_id>": { … } },
  "edges":  { "<edge_id>":   { … } },
  "themes": { "<change_id>": { … } },
  "tags":   { "<node_change_id>--<theme_change_id>": { … } }
}
```

Every graph write goes to the database and to that file at the same time. When
git changes the file (a pull, a checkout, a reset, a stash), the database
follows it before the next command runs. `deciduous init` writes the rules:

```
# .gitignore
.deciduous/*
!.deciduous/config.toml
!.deciduous/graph.json

# .gitattributes
.deciduous/graph.json merge=deciduous linguist-generated=true
```

and registers the merge driver in the clone's git config
(`merge.deciduous.driver = deciduous merge-record %O %A %B`). `linguist-generated`
folds the file in pull request diffs; it is still there to expand.

### What a record looks like

One entry of `nodes`, keyed by its `change_id`:

```json
{
  "author": "Alice Example",
  "change_id": "8311856e-5e6f-4a7b-8c9d-0e1f2a3b4c5d",
  "created_at": "2026-10-24T12:47:10-04:00",
  "metadata": {
    "branch": "main",
    "confidence": 90,
    "prompt": "Add rate limiting to the public API..."
  },
  "node_type": "goal",
  "status": "pending",
  "title": "Rate limit the public API",
  "updated_at": "2026-10-24T12:47:10-04:00"
}
```

Maps and keys are sorted and the file ends with a newline, so two machines
holding the same graph write byte-identical files and a diff shows only what
changed. `metadata` is the expanded form of the database's `metadata_json`.

An `edges` entry is the same idea. Its key is a hash of
`(from_change_id, to_change_id, edge_type)`, so the same edge created on two
machines lands under one key.

A deleted record is not dropped. It is rewritten with `deleted_at` set (a
tombstone) and keeps its last fields, so the deletion reaches machines that
already have the record and `git log` still shows what was deleted.

### Why one file

0.17 kept one small JSON file per record under `.deciduous/sync/`, so two
people adding records never touched the same file and git merged with no
conflict. It worked, and it cost more than it solved: a real graph was 2,781
files and 11 MB of mostly directory overhead for ~1 MB of content, `git status`
after a sync was a wall of paths, and a PR touching the graph was hundreds of
files. 0.18 folded it into one file. The trade is that every concurrent change
now collides in git, which turned the merge driver from a nicety into the
mechanism, and a mechanism exercised on every merge is one you find out about
quickly rather than one that rots until the day it matters.

### The workflow

```bash
git pull
deciduous sync              # optional since 1.0.11: any command follows graph.json first
# ... work; every add/link/status/delete writes the file immediately ...
git add .deciduous/graph.json
git commit -m "graph: chose token bucket over fixed window"
git push
```

```
$ deciduous sync --check
Checked .deciduous/graph.json (8 nodes, 7 edges, 1 themes, 1 tags on disk)
  Database and records already agree
```

`sync --check` exits 1 if `sync` would write to `graph.json` or something is
unresolved (an edge waiting for a node, a record that will not read, an
unfinished merge), which makes it a usable pre-push hook. A local database
that is merely behind the file is not pending: the next command catches it up.

`deciduous sync` no longer writes `docs/graph-data.json`; that export went
with GitHub Pages in 1.0.5.

### Branches, old commits and resets

The database is one per clone and shared by every branch you check out;
`graph.json` is versioned like any other file. Since 1.0.11 the database
follows the file: before any command (or, in a running `deciduous mcp`, before
any tool call), if `graph.json` is not the file the database last mirrored,
the database is brought to it.

- **A branch's nodes stay on that branch.** A node added on `spike` leaves the
  local graph on `git checkout main` and comes back on `git checkout spike`,
  under the same local id, with its documents. It is never written into
  `main`'s file.
- **Git can take things out.** `git reset --hard`, `git stash` and
  `git checkout -- .deciduous/graph.json` discard graph changes as they
  discard code changes; `git stash pop` brings them back.
- **A detached commit is read, not written.** On `git checkout <old commit>`,
  a bisect step or a CI checkout, the local graph shows that commit's graph and
  the file is left as the commit has it. Adds and edits made there are kept in
  the database as unpublished and published into the next branch you check
  out; deletions there are refused by name. A stopped rebase, merge,
  cherry-pick or revert is detached too, but there the file is being
  rewritten, so writes go into it as usual.

The database records the digest of the file it mirrors; git never updates
it, so a checkout shows up as a digest that differs. Writes made while HEAD is
detached, or whose file write failed, are recorded in a local `unpublished`
table, which is how a local write the file never got is told from a row git
took out.

### How reconcile decides

For nodes, themes, edges and tags, `deciduous sync` compares the file to the
database by `change_id`:

| File | Database | Result |
|------|----------|--------|
| record | missing | import (gets a fresh local id) |
| missing | row | export |
| record newer (`updated_at`) | row older | update the row |
| record older | row newer | rewrite the record |
| tombstone at or after the row's `updated_at` | row | delete locally |
| tombstone before the row's `updated_at` | row | the row was edited after the delete: rewrite the record |

Edges import once both endpoints exist locally; one whose endpoint has not
arrived is reported as pending (`sync --check` exits 1) and imports later. An
edge that points at a tombstoned node is skipped. Edges have no `updated_at`:
an edge both sides have whose rationale differs takes the file's version.

Deleting a node tombstones its edges and tags with a `deleted_with` marker. If
the node comes back (edited elsewhere after the delete), those tombstones no
longer count and the edges and tags come back with it. A deliberate unlink or
untag has no marker and stays.

A local write is always stamped later than the version of its record already
in the file, even when that version's timestamp is ahead of this clock
(`add --date` in the future, a fast clock elsewhere); otherwise the next sync
would revert the edit. A `graph.json` that does not parse stops the run rather
than being read as an empty graph, which would export every local row over
whatever is actually there. A record filed under a key that disagrees with its
own `change_id` (a hand edit) is reported and never written over. Same-named
themes created on two machines are folded: the smaller `change_id` becomes
canonical and the other's tags are re-pointed.

`deciduous sync` is idempotent: a second run reports "already agree".

### Two people change the graph

Everyone writes one file, so git reports a conflict on every concurrent
change, and never shows it to you, because the merge driver runs. Git hands
it the common ancestor (`%O`) and both sides, and it merges record by record:

| Situation | Result |
|-----------|--------|
| Only one side has a record | keep it: both people's new nodes survive |
| Both sides have it, unchanged | keep it |
| Both sides changed it | merge field by field, below |
| One side dropped it, the other left it alone | it goes |
| One side dropped it, the other edited it | the edit wins |

For the record both sides changed:

| Situation | Result |
|-----------|--------|
| Only one side changed a field | that side's value |
| Both changed `metadata` | merged key by key (Alice's `confidence` and Bob's `commit` both survive) |
| Both changed the same field to different values | the side whose record has the later `updated_at` |
| `updated_at` / `deleted_at` | the later one; `created_at` the earlier |
| One side deleted, the other edited after the delete | the edit wins, the record lives |
| One side deleted, the other edited before the delete | the tombstone stands, keeping the edited fields |
| Both created the record independently (no ancestor) | every differing field is a collision: later `updated_at` wins, `metadata` still unions |

A version that still carries conflict markers is resolved by merging its own
sides first. An ancestor that will not parse fails the driver, because a merge
without it would hand every differing field to the newer record; the error
names the way to do that knowingly (`deciduous merge-record /dev/null <ours>
<theirs>`).

When the driver fails, or git cannot run it (`deciduous` not on the PATH git
sees: GUI clients, CI images), git does not fall back to an ordinary conflict.
It leaves our side in the file with no markers and marks the path unmerged
(`UU` in `git status`). That file looks clean; committing it silently drops the
other side. `deciduous sync --check` reports the unmerged state and exits 1,
and `deciduous sync` finishes the merge from git's own three versions
(`git ls-files -u`), keeping local writes made since, and stages the result.
While a merge, rebase or cherry-pick is stopped, `sync` also folds the
incoming commit's version (`MERGE_HEAD`, `REBASE_HEAD`, `CHERRY_PICK_HEAD`) into
the working file.

A clone that never ran `deciduous sync`, or a GitHub web merge, can still
produce conflict markers inside `graph.json`. `deciduous sync` reconstructs
both sides from the markers (using the `|||||||` base when
`merge.conflictStyle` is `diff3` or `zdiff3`), applies the same merge, rewrites
the file and imports the result; `sync --check` reports it and exits 1 without
touching the file.

### Where writes come from

The graph file is written by the database layer, so every entry point
publishes: the CLI, `deciduous mcp`, the archaeology commands, and the HTTP API
daemon when a `graph.json` sits next to a graph's database. A write to the
file that fails is reported on stderr and never fails the database write; the
next `deciduous sync` exports whatever is missing.

### Upgrading

From **0.17** (`.deciduous/sync/`): `deciduous update`, then `deciduous sync`.
The first sync folds every record into `graph.json` and deletes the directory;
a record the file already has in a newer version is not overwritten. If any
file will not parse, the directory is kept and the files listed. Afterwards
`git rm -r .deciduous/sync`.

From **pre-0.17** (`.deciduous/sync/events/*.jsonl` and `checkpoint.json`):
the same `deciduous sync` replays them into `graph.json` and deletes them.
`deciduous events` still exists as a hidden, deprecated alias so old scripts
do not break.

## Not synced through git

- **Documents** (`.deciduous/documents/`): metadata and files stay on the
  machine that attached them. With a `[remote]`, `remote push --seed` sends
  them to the server.
- **The message board**: coordination, not graph; never in `graph.json` or on
  any export.
- **Sessions, the Q&A history and the command log**: local by design.
- **Roadmap items**: synced through `ROADMAP.md` itself.
