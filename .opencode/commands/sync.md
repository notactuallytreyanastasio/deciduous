---
description: Sync decision graph with teammates - pull events, rebuild, push
arguments: []
---

# Multi-User Sync

The shared decision graph lives in one file, `.deciduous/graph.json`, holding every node, edge, theme, and tag. Your SQLite database is a private cache of it. `deciduous sync` makes the two agree, in both directions.

## Step 1: Pull

```bash
git pull --rebase
```

## Step 2: Sync

```bash
deciduous sync
```

This creates `.deciduous/graph.json` if needed, folds in a 0.17 `.deciduous/sync/` directory or a pre-0.17 JSONL event log once, imports records you do not have (teammates' nodes get *local* ids here), exports database rows that have no record yet, and regenerates `docs/graph-data.json`. "Pending" edges are waiting for a node that has not been pulled yet.

## Step 3: Link across users if needed

Local ids differ per machine. Refer to a teammate's node by the change_id prefix shown in the CHANGE column:

```bash
deciduous nodes
deciduous link a1b2c3d4 42 -r "our action implements their goal"
```

## Step 4: Commit and push

```bash
git add .deciduous/graph.json docs/graph-data.json docs/git-history.json
git commit -m "graph: <what was decided>"
git push
```

`deciduous sync --check` exits non-zero if anything is still pending, so it works as a pre-push guard.

## Merge conflicts

- **`.deciduous/graph.json`**: two people changed the graph. Normally git merges it record by record through the `deciduous` merge driver, so you never see this. If it has `<<<<<<<` markers, run `deciduous sync`: it merges the sides the same way and imports the result.
- **`docs/graph-data.json`**: never hand-merge it. Take either side and run `deciduous sync` to regenerate.

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| Teammate's nodes missing | `git pull` then `deciduous sync` |
| "No node has a change_id starting with ..." | You have not synced their record yet |
| Record file unreadable | `git checkout -- <file>` or fix the JSON; sync skips it and continues |
| `.deciduous/graph.json` not in git | `deciduous update` fixes `.gitignore` |
