# Multi-User Sync

How several people (and several machines) share one decision graph through git.

## The model in one paragraph

Every machine has a private SQLite database, `.deciduous/deciduous.db`, which is
gitignored. The shared source of truth is `.deciduous/sync/`, a directory of small
JSON files, one per record, committed with the code. Every graph write goes to the
database and to its record file at the same time. `deciduous sync` makes the
directory and the database agree, in both directions. That is the whole mechanism.
There is no event log, no checkpoint, no patch export, and nothing to compact.

```text
.deciduous/
├── deciduous.db            private cache (gitignored)
├── config.toml             shared
└── sync/                   shared, one file per record
    ├── README.md
    ├── nodes/<change_id>.json
    ├── edges/<edge_id>.json
    ├── themes/<change_id>.json
    └── tags/<node_change_id>--<theme_change_id>.json
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

`nodes/<change_id>.json`:

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

Keys are sorted and the file ends with a newline, so two machines writing the same
record produce byte-identical files. `metadata` is the expanded form of the
database's `metadata_json` string so diffs stay readable.

`edges/<edge_id>.json` is the same idea. `edge_id` is a hash of
`(from_change_id, to_change_id, edge_type)`, so the same edge created on two
machines lands in one file.

A deleted record is not removed. It is rewritten with `deleted_at` set (a
tombstone) and keeps its last fields. That way a deletion reaches machines that
already have the record, `git log` still shows what was deleted, and checking out an
older branch never looks like a mass deletion.

## Why files instead of a log

The previous design appended events to per-author JSONL files and periodically
compacted them into a `checkpoint.json`. It had four problems that the record store
does not:

| Problem | JSONL log | Record store |
|---------|-----------|--------------|
| Same author on two branches (or a rebased PR) | Both append at the end of one file: conflict | Different records are different files: no conflict |
| Checkpoint | One giant file, rewritten by whoever compacts: conflict | None needed |
| Ordering | Replay by wall-clock timestamp; events "older than the checkpoint" are dropped, so clock skew loses data | Each record carries its own `updated_at`; newer wins, per record |
| Concurrent writers on one machine | Two processes could interleave and corrupt a line, which broke every later rebuild | Temp file + rename; a bad file is reported and skipped |

It also makes history legible: `git log -- .deciduous/sync/nodes/<id>.json` is the
history of one decision and `git blame` shows who changed it.

Pull request diffs list every touched record. `deciduous init` and `deciduous
update` add `.deciduous/sync/** linguist-generated=true` to `.gitattributes`, so
GitHub folds them by default. They are still there to expand and review.

## The workflow

```bash
git pull
deciduous sync              # 1. import their records  2. export anything missing  3. refresh docs/graph-data.json
# ... work; every add/link/status/delete writes its record immediately ...
git add .deciduous/sync/ docs/graph-data.json
git commit -m "graph: chose token bucket over leaky bucket"
git push
```

`deciduous sync --check` reports what is pending and exits 1 if anything is, which
makes it a usable pre-push hook. `--no-pages` skips the GitHub Pages export.

The AI assistant templates (`/sync`, `/recover`, `/decision`) tell the assistant to
run `deciduous sync` at session start and after any pull, and to use change_id
prefixes when linking to another person's nodes.

## How reconcile decides

For each kind (nodes, themes, edges, tags), `deciduous sync` compares the store to
the database by `change_id`:

| Store | Database | Result |
|-------|----------|--------|
| record | missing | import (gets a fresh local id) |
| missing | row | export |
| record newer (`updated_at`) | row older | update the row |
| record older | row newer | rewrite the record |
| tombstone at or after the row's `updated_at` | row | delete locally |
| tombstone before the row's `updated_at` | row | the row was edited after the delete: rewrite the record (resurrect) |

Edges import once both endpoints exist locally. An edge whose endpoint has not
arrived yet is reported as pending and imports on a later sync. An edge that points
at a tombstoned node is skipped.

Three details keep this honest:

- On an `updated_at` tie the store wins if the content differs. A merge-driver
  result keeps the winning side's `updated_at` but carries fields from both
  sides, and the database only has ours.
- A record file that exists but does not parse is reported and never written
  over. Export only creates files that are missing.
- A record the database refuses (a constraint violation, say) is listed under
  "the database refused a record" and the rest of the run still completes.
  Same-named themes created on two machines before syncing are folded first:
  the smaller `change_id` becomes canonical everywhere and the other's tags
  are re-pointed to it.

Write-through follows the same rule in the other direction. When a local
`status`, `link`, or `tag` writes its record, it merges with whatever is
already in the file rather than replacing it, so a teammate's version that was
pulled but not yet synced into the database keeps its fields.

Nothing here depends on the order files are read, so `deciduous sync` is idempotent:
a second run right after the first reports "already agree".

## Two people edit the same record

Adding records never conflicts. When two branches change the *same* record, git
does not have to stop either: `deciduous init`, `update`, and `sync` register a
merge driver for `.deciduous/sync/**` in the clone's git config, and
`.gitattributes` routes record files through it:

```text
.gitattributes:   .deciduous/sync/** merge=deciduous linguist-generated=true
git config:       merge.deciduous.driver = deciduous merge-record %O %A %B
```

Git hands the driver the common ancestor of the file (`%O`) along with both
sides. That ancestor is the fingerprint that tells a one-sided change from a real
collision, so the driver merges field by field:

| Situation | Result |
|-----------|--------|
| Only one side changed a field | that side's value |
| Both changed `metadata` | merged key by key with the same rules (Alice's `confidence` and Bob's `commit` both survive) |
| Both changed the same field to different values | the side whose record has the later `updated_at` |
| `updated_at` / `deleted_at` | the later one; `created_at` the earlier |
| One side deleted, the other edited **after** the delete | the edit wins, the record lives |
| One side deleted, the other edited **before** the delete | the tombstone stands, keeping the edited fields |
| Both sides created the record independently (no ancestor) | every differing field is a collision: later `updated_at` wins, `metadata` still unions |

The driver exits non-zero if either side is not valid JSON, and git then falls
back to an ordinary conflict.

Git config is per clone, so a clone that has never run `deciduous sync` (or a
GitHub web merge) can still produce conflict markers inside a record file. That
is not fatal: `deciduous sync` finds files containing markers, applies the same
merge (using the `|||||||` base section when git's `merge.conflictStyle` is
`diff3` or `zdiff3`, otherwise a two-way merge), rewrites the file, and imports
the result. `deciduous sync --check` lists such files and exits 1.

`docs/graph-data.json` is a generated export and contains local ids. If it
conflicts, take either side and run `deciduous sync` to regenerate it.

## Where writes come from

Records are written by the database layer, so every entry point publishes: the
CLI, the MCP server (`add_node`, `link_nodes`, `update_status`, `delete_node` and
the rest), the `deciduous pivot`/`supersede` archaeology commands, and the HTTP API
daemon if a `sync/` directory sits next to a graph's database. A write to the store
that fails is reported on stderr and never fails the database write; the next
`deciduous sync` exports whatever is missing.

## Upgrading from the JSONL event log

Run `deciduous update` (fixes `.gitignore`, which used to hide all of
`.deciduous/`) and then `deciduous sync`. The first sync reads
`.deciduous/sync/events/*.jsonl` and `checkpoint.json`, replays them, writes the
result as records, and deletes the legacy files. Lines that held two JSON objects
glued together (a bug in the old appender) are split and both objects recovered.
If any line is truly unreadable the legacy files are kept and the offending lines
are listed, so nothing is dropped silently. Afterwards `git rm -r
.deciduous/sync/events .deciduous/sync/checkpoint.json`.

`deciduous events …` still works as a deprecated alias for the equivalent
`deciduous sync` behaviour. The `diff export` / `diff apply` patch commands were
removed earlier; `.deciduous/patches/` is no longer read.

## Not synced (yet)

- **Documents** (`.deciduous/documents/`): attachment metadata and files stay local.
- **Sessions and the command log**: local by design.
- **Roadmap items**: synced through `ROADMAP.md` itself.
