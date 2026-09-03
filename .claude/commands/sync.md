---
description: Sync decision graph with teammates - reconcile .deciduous/sync/ records with the local database
allowed-tools: Bash(deciduous:*, git:*)
---

# Multi-User Sync

The shared decision graph lives in `.deciduous/sync/` as one JSON file per node, edge, theme, and tag. Your SQLite database is a private cache of it. `deciduous sync` makes the two agree, in both directions.

## Step 1: Pull

```bash
git pull --rebase
```

## Step 2: Sync

```bash
deciduous sync
```

This does, in order:
1. Creates `.deciduous/sync/` if it does not exist yet
2. Imports the pre-0.17 JSONL event log / checkpoint if present, then removes them
3. Imports records you do not have (teammates' nodes get *local* ids here)
4. Exports database rows that have no record yet
5. Regenerates `docs/graph-data.json` for GitHub Pages

Read the summary. "Pending" edges are waiting for a node that has not been pulled yet; they import on a later sync.

## Step 3: Link across users if needed

Local ids differ per machine. Refer to a teammate's node by the change_id prefix shown in the CHANGE column:

```bash
deciduous nodes
deciduous link a1b2c3d4 42 -r "our action implements their goal"
```

## Step 4: Commit and push

```bash
git add .deciduous/sync/ docs/graph-data.json docs/git-history.json
git commit -m "graph: <what was decided>"
git push
```

`deciduous sync --check` exits non-zero if anything is still pending, so it works as a pre-push guard.

## Merge conflicts

- **A file under `.deciduous/sync/`**: two people edited the same record. Normally git merges it field by field through the `deciduous` merge driver (registered by `deciduous sync`), so you never see this. If a merge was done without the driver and the file has `<<<<<<<` markers, just run `deciduous sync`: it merges the sides the same way and imports the result.
- **`docs/graph-data.json`**: never hand-merge it. Take either side and run `deciduous sync` to regenerate.

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| Teammate's nodes missing | `git pull` then `deciduous sync` |
| "No node has a change_id starting with ..." | You have not synced their record yet |
| Record file unreadable | `git checkout -- <file>` or fix the JSON; sync skips it and continues |
| `.deciduous/sync/` not in git | `deciduous update` fixes `.gitignore` |

## Quick Reference

| Command | What it does |
|---------|--------------|
| `deciduous sync` | Reconcile both ways + export Pages data |
| `deciduous sync --check` | Report pending changes, exit 1 if any |
| `deciduous sync --no-pages` | Reconcile only |
| `deciduous nodes` | Shows the CHANGE prefix to use for cross-user links |
