# Decision graph records

Shared, git-tracked copy of this project's decision graph, one JSON file per
record. `deciduous` writes here on every change and `deciduous sync` reconciles
this directory with each machine's private SQLite database.

- `nodes/<change_id>.json`  goals, decisions, actions, outcomes...
- `edges/<edge_id>.json`    links between nodes (by change_id, not local id)
- `themes/`, `tags/`        theme definitions and node tags

Records with `deleted_at` are tombstones. Do not edit files by hand; run
`deciduous sync` after `git pull` and before `git push`.
