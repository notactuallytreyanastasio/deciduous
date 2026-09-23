# Multi-User Sync

How several people (and several machines) share one decision graph through git.

## The model in one paragraph

Every machine has a private SQLite database, `.deciduous/deciduous.db`, which is
gitignored. The shared source of truth is `.deciduous/graph.json`, one file holding
the whole graph, committed with the code. Every graph write goes to the database and
to that file at the same time. `deciduous sync` makes the file and the database
agree, in both directions. That is the whole mechanism. There is no event log, no
checkpoint, no patch export, no directory of records, and nothing to compact.

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

## Identity: local ids vs change ids

| Field | Scope | Example | Use it for |
|-------|-------|---------|------------|
| `id` | one machine | `42` | typing quickly at your own prompt |
| `change_id` | everywhere | `a1b2c3d4-…` | anything that crosses machines |

Alice's goal is `#12` on her laptop and `#907` on Bob's. Its `change_id` is the same
on both. Records reference each other only by `change_id`; local ids are assigned
when a record is imported and never leave the machine.

Every command that takes a node id also takes a `change_id` prefix (four or more
characters, unique). `deciduous nodes` prints the first eight characters in the
CHANGE column:

```text
ID    CHANGE    TYPE         STATUS     TITLE
57    a1b2c3d4  goal         pending    Rate limit the public API
58    9f8e7d6c  action       pending    Add token bucket middleware
```

```bash
deciduous link a1b2c3d4 58 -r "implements the goal Alice logged"
deciduous status 9f8e7d6c completed
deciduous show a1b2
```

The MCP tools accept the same thing: pass `"node_id": "a1b2c3d4"` instead of an
integer.

## What a record looks like

One entry of `nodes`, keyed by its `change_id`:

```json
{
  "author": "Alice Example",
  "change_id": "a1b2c3d4-5e6f-4a7b-8c9d-0e1f2a3b4c5d",
  "created_at": "2026-09-02T10:15:00-04:00",
  "metadata": {
    "branch": "feat/rate-limit",
    "confidence": 90,
    "prompt": "add rate limiting to the public API"
  },
  "node_type": "goal",
  "status": "pending",
  "title": "Rate limit the public API",
  "updated_at": "2026-09-02T10:15:00-04:00"
}
```

Maps and keys are sorted and the file ends with a newline, so two machines holding
the same graph write byte-identical files and a diff shows only what changed.
`metadata` is the expanded form of the database's `metadata_json` string so diffs
stay readable.

An `edges` entry is the same idea. `edge_id` is a hash of
`(from_change_id, to_change_id, edge_type)`, so the same edge created on two
machines lands under one key.

A deleted record is not dropped. It is rewritten with `deleted_at` set (a
tombstone) and keeps its last fields. That way a deletion reaches machines that
already have the record, and `git log` still shows what was deleted.

## Why one file

0.17 kept one small JSON file per record under `.deciduous/sync/`. The reasoning was
that two people adding records never touch the same file, so git merges their
branches with no conflict at all, and `git log -- .deciduous/sync/nodes/<id>.json`
is the history of one decision.

Both of those are true. They were not worth what they cost:

| | Directory of records (0.17) | One file (0.18) |
|---|---|---|
| A real graph on disk | 2,781 files, 11 MB of mostly directory overhead | one file, ~1 MB |
| `git status` after a sync | a wall of paths | one path |
| A PR that touches the graph | hundreds of files in the diff | one file |
| Reading the graph | `read_dir` + open + parse per record | one read, one parse |
| A record's file renamed, or two files claiming one id | possible, and has to be detected and reported | not expressible |
| Concurrent adds on two branches | merge with no conflict | git conflict, resolved by the merge driver |

The last row is the trade. Adding a record used to be conflict-free; now every
concurrent change collides in git. That turned the merge driver from a nicety into
the mechanism, which is the honest place for it to be: the driver was already
required for the case people actually hit (two people editing the same decision),
and a mechanism exercised on every merge is one you find out about quickly, rather
than one that quietly rots until the day it matters.

`deciduous init` and `deciduous update` add
`.deciduous/graph.json linguist-generated=true` to `.gitattributes`, so GitHub folds
the file by default in pull request diffs. It is still there to expand and review.

## The workflow

```bash
git pull
deciduous sync              # 1. import their records  2. export anything missing  3. refresh docs/graph-data.json
# ... work; every add/link/status/delete writes the file immediately ...
git add .deciduous/graph.json docs/graph-data.json
git commit -m "graph: chose token bucket over leaky bucket"
git push
```

`deciduous sync --check` reports what is pending and exits 1 if anything is, which
makes it a usable pre-push hook. `--no-pages` skips the GitHub Pages export.

The AI assistant templates (`/sync`, `/recover`, `/decision`) tell the assistant to
run `deciduous sync` at session start and after any pull, and to use change_id
prefixes when linking to another person's nodes.

## Branches, old commits and resets

The database is one per clone and is shared by every branch and commit you
check out; `graph.json` is versioned like any other file. `sync` treats a row
the database has and the file lacks as a local write not yet exported, and
exports it. It cannot tell that apart from a row git took out of the file by
switching branch or resetting, because the database keeps no record of which
rows have reached a file. What that means in practice:

- **A detached commit is read, not written.** On `git checkout <old commit>`,
  a bisect step or a CI checkout, `sync` imports what the commit's file adds
  and leaves the file exactly as the commit has it, so `git checkout main`
  still works afterwards. It says what it held back:
  `Note: HEAD is detached at 2848ae0, so the graph file was left as this
  commit has it: 1 node(s) not exported.` `sync --check` and the MCP `sync`
  and `sync_status` tools agree with it, and a commit that has no graph file
  is not given one. A
  stopped rebase, merge, cherry-pick or revert is detached too, but there the
  file is being rewritten, so `sync` writes it as usual.
- **Branches share nodes (not fixed).** A node added on `spike` is exported
  into `main`'s `graph.json` by the next `sync` on `main`. If that is not what
  you want on `main`, do not commit it there: after switching branch, run
  `deciduous sync --check`; if it reports exports you did not make on this
  branch, run `sync`, then `git checkout -- .deciduous/graph.json` before
  committing anything else, or stage the file with `git add -p`. The nodes
  stay in the database and in `spike`'s commits either way.
- **A reset brings rows back (not fixed).** `git reset --hard` to an earlier
  commit on a branch removes records from the file but not from the database,
  and the next `sync` exports them again. To remove a node for good, use
  `deciduous delete`, which writes a tombstone every clone applies.
- **A server does not see branches.** With a `[remote]`, every write reaches
  the workspace as it is made, whatever the branch.

Why the last three are not fixed: the fix that is safe is a local log of
writes not yet published, exporting only what it holds and letting the file
decide every other difference. Dropping rows missing from the file instead
would delete local data for anyone whose rows never reached a file (rows from
before the record store, rows written while `graph.json` was missing). That
log is a change to what `sync` means, with its own migration, not a patch.

## How reconcile decides

For each kind (nodes, themes, edges, tags), `deciduous sync` compares the file to
the database by `change_id`:

| File | Database | Result |
|------|----------|--------|
| record | missing | import (gets a fresh local id) |
| missing | row | export |
| record newer (`updated_at`) | row older | update the row |
| record older | row newer | rewrite the record |
| tombstone at or after the row's `updated_at` | row | delete locally |
| tombstone before the row's `updated_at` | row | the row was edited after the delete: rewrite the record (resurrect) |

Edges import once both endpoints exist locally. An edge whose endpoint has not
arrived yet is reported as pending (and `sync --check` exits 1 until it arrives) and
imports on a later sync. An edge that points at a tombstoned node is skipped.

Edges have no `updated_at`. An edge both sides have whose rationale or weight differs
(someone unlinked and relinked it) takes the file's version: local writes reach the
file as they happen, so a row that differs is stale.

Deleting a node tombstones its edges and tags with a `deleted_with` marker naming
that deletion. If the node comes back (edited elsewhere after the delete), those
tombstones no longer count and the edges and tags come back with it. A deliberate
unlink or untag has no marker and stays.

A local write is always stamped later than the version of its record already in the
file, even when that version's timestamp is ahead of this clock (`add --date` in the
future, a teammate whose clock runs fast). Otherwise last-writer-wins would keep the
file's version and the next sync would revert the edit.

Four details keep this honest:

- On an `updated_at` tie the file wins if the content differs. A merge-driver
  result keeps the winning side's `updated_at` but carries fields from both
  sides, and the database only has ours.
- A `graph.json` that does not parse stops the run. It is never treated as an
  empty graph, because that would export every local row over whatever is
  actually in there.
- A record filed under a key that disagrees with its own `change_id` (a hand
  edit) is reported and never written over.
- A record the database refuses (a constraint violation, say) is listed under
  "the database refused a record" and the rest of the run still completes.
  Same-named themes created on two machines before syncing are folded first:
  the smaller `change_id` becomes canonical everywhere and the other's tags
  are re-pointed to it.

Write-through follows the same rule in the other direction. When a local
`status`, `link`, or `tag` writes its record, it merges with whatever is already
in the file rather than replacing it, so a teammate's version that was pulled but
not yet synced into the database keeps its fields.

A sync that exports thousands of records rewrites `graph.json` once, not once per
record: `reconcile` runs inside `RecordStore::batch`, which keeps the document in
memory and writes on the way out.

Nothing here depends on the order records are read, so `deciduous sync` is
idempotent: a second run right after the first reports "already agree".

## Two people change the graph

Because everyone writes one file, git reports a conflict on every concurrent
change — and then never shows it to you, because `deciduous init`, `update`, and
`sync` register a merge driver in the clone's git config and `.gitattributes`
routes the file through it:

```text
.gitattributes:   .deciduous/graph.json merge=deciduous linguist-generated=true
git config:       merge.deciduous.driver = deciduous merge-record %O %A %B
```

Git hands the driver the common ancestor of the file (`%O`) along with both sides.
The driver merges the two documents **record by record**:

| Situation | Result |
|-----------|--------|
| Only one side has a record | keep it — both people's new nodes survive |
| Both sides have it, unchanged | keep it |
| Both sides changed it | merge field by field, below |
| One side dropped it from the map, the other left it alone | it goes |
| One side dropped it, the other edited it | the edit wins |

For the one record both sides changed, the ancestor is the fingerprint that tells a
one-sided change from a real collision:

| Situation | Result |
|-----------|--------|
| Only one side changed a field | that side's value |
| Both changed `metadata` | merged key by key with the same rules (Alice's `confidence` and Bob's `commit` both survive) |
| Both changed the same field to different values | the side whose record has the later `updated_at` |
| `updated_at` / `deleted_at` | the later one; `created_at` the earlier |
| One side deleted, the other edited **after** the delete | the edit wins, the record lives |
| One side deleted, the other edited **before** the delete | the tombstone stands, keeping the edited fields |
| Both sides created the record independently (no ancestor) | every differing field is a collision: later `updated_at` wins, `metadata` still unions |

A version that still carries conflict markers (someone committed a conflicted file)
is resolved by merging its own sides first, so it does not poison every later merge
whose ancestor it is. An ancestor that will not parse at all fails the driver: a
merge without it would hand every field the sides differ on to the newer record and
silently lose the older side's edits. The error names the way to do that anyway,
knowingly (`deciduous merge-record /dev/null <ours> <theirs>`: an empty base is the
two-way merge).

When the driver does fail (a side is not JSON), or git cannot run it (`deciduous` is
not on the PATH git sees: GUI clients, CI images), git does **not** fall back to an
ordinary conflict. It leaves our side in the file untouched, with no markers, and
marks the path unmerged (`UU` in `git status`). That file parses and looks clean;
committing it silently drops the other side. `deciduous sync --check` reports the
unmerged state and exits 1, and `deciduous sync` finishes the merge from git's own
three versions (`git ls-files -u`), keeping any local writes made to the working file
since, and stages the result.

Staging that file by hand (`git add`) clears the unmerged state but not the problem.
So while a merge, rebase or cherry-pick is stopped, `sync` and `sync --check` also
fold the incoming commit's version (`MERGE_HEAD`, `REBASE_HEAD`, `CHERRY_PICK_HEAD`)
into the working file, measured from its base. For a file the driver merged this
changes nothing; for one that is missing the other side, `--check` exits 1 and
`sync` merges and stages it. A merge already committed is out of reach.

Git config is per clone, so a clone that has never run `deciduous sync` (or a GitHub
web merge) can still produce conflict markers inside `graph.json`. That is not
fatal: `deciduous sync` reconstructs both sides from the markers, applies the same
merge (using the `|||||||` base section when git's `merge.conflictStyle` is `diff3`
or `zdiff3`, otherwise a two-way merge), rewrites the file, and imports the result.
`deciduous sync --check` reports it and exits 1 without touching the file.

`docs/graph-data.json` is a generated export and contains local ids. If it
conflicts, take either side and run `deciduous sync` to regenerate it.

## Where writes come from

The graph file is written by the database layer, so every entry point publishes: the
CLI, the MCP server (`add_node`, `link_nodes`, `update_status`, `delete_node` and
the rest), the `deciduous pivot`/`supersede` archaeology commands, and the HTTP API
daemon if a `graph.json` sits next to a graph's database. A write to the file that
fails is reported on stderr and never fails the database write; the next
`deciduous sync` exports whatever is missing.

## Upgrading

From **0.17** (`.deciduous/sync/` full of per-record files): run `deciduous update`
and then `deciduous sync`. The first sync folds every record into `graph.json` and
deletes the directory. A record the file already has in a *newer* version is not
overwritten by the older file, so folding in a directory that arrived with a pull
does not lose the pulled work. If any file will not parse, the directory is kept and
the offending files are listed, so nothing is dropped silently. Afterwards
`git rm -r .deciduous/sync`.

From **pre-0.17** (`.deciduous/sync/events/*.jsonl` and `checkpoint.json`): the same
`deciduous sync` replays them, writes the result into `graph.json`, and deletes the
legacy files. Lines that held two JSON objects glued together (a bug in the old
appender) are split and both objects recovered.

`deciduous events …` still works as a deprecated alias for the equivalent
`deciduous sync` behaviour. The `diff export` / `diff apply` patch commands were
removed earlier; `.deciduous/patches/` is no longer read.

## Not synced (yet)

- **Documents** (`.deciduous/documents/`): attachment metadata and files stay local.
- **Sessions and the command log**: local by design.
- **Roadmap items**: synced through `ROADMAP.md` itself.
