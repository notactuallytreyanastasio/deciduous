# Changelog

## [0.18.0] - 2026-09-18

### Changed
- **The shared graph is one file again: `.deciduous/graph.json`.** It holds every node, edge, theme, and tag, keyed by `change_id`. 0.17's `.deciduous/sync/` directory of per-record files is folded into it once by the next `sync`, `init`, or `update`, and then deleted — `git rm -r .deciduous/sync` afterwards. A record the file already holds in a *newer* version is not overwritten by the older file, so folding in a directory that arrived with a pull does not lose the pulled work.
- **Why the per-record store was undone.** The design was sound — two people adding records touch different files, so git merges them with no conflict — and the bill was 2,781 files and 11MB of directory overhead for a graph whose actual content is ~1MB. `git status` after a sync was a wall of paths, a PR touching the graph was hundreds of files, and a rename could put a record in a file named after a different one.
- **The merge driver is now the mechanism, not a nicety.** One file means every concurrent change conflicts in git, so `deciduous merge-record` runs on every merge. It merges the two documents record by record: a record only one side has survives (both people's new nodes are kept), and a record both sides changed still merges field by field with the ancestor as the tiebreak. `deciduous sync` repairs a file left with conflict markers by a clone that has no driver registered.
- `.gitignore` now tracks `.deciduous/graph.json` and `.gitattributes` routes it through `merge=deciduous`. `deciduous update` replaces the 0.17 rules rather than stacking on them.

### Fixed
- A `graph.json` that will not parse stops a sync instead of reading as an empty graph — which would have exported every local row over whatever was actually in the file.
- A sync that exports thousands of records writes the file once, not once per record.

### Added
- `RecordStore::batch` — apply many writes with a single write-through at the end.

## [0.17.1] - 2026-09-04

### Fixed
- `deciduous nodes` panicked on an observation whose description contains multibyte characters (an ellipsis, for instance).

## [0.17.0] - 2026-09-04

### Changed
- **One multi-user sync mechanism.** `.deciduous/sync/` held one JSON record per node, edge, theme, and tag, tracked in git. The event log, checkpoint, and patch export were removed. (Superseded by the single graph file in 0.18.0.)
- Records were written by the database layer, so CLI, MCP, and HTTP API all published automatically.
- `deciduous sync` reconciled both ways then exported `docs/graph-data.json`; `--check` exited 1 if anything was pending.
- Node references accept a `change_id` prefix everywhere (CLI and MCP).
- Concurrent edits of one record merge field by field through the `deciduous merge-record` git merge driver, registered by `init`/`update`/`sync`.
- Legacy JSONL event logs are imported once (tolerating corrupted lines) and removed; `deciduous events` became a deprecated alias.

## [0.16.0] - 2026-07-17

### Added
- **Multi-graph HTTP API daemon (`deciduous serve --api`)** — serves many independent decision graphs to remote clients over HTTP with bearer-token auth, one SQLite file per graph. This is what lets a graph live centrally and be written to by clients that have no local `.deciduous/` (e.g. a federated fleet all pointing at one daemon).
- **`GET /health`** — unauthenticated, side-effect-free liveness endpoint for reverse proxies and uptime probes.
- **`DECIDUOUS_API_DATA_DIR`** env var as an alternative to `--data-dir` (joining the existing `DECIDUOUS_API_TOKEN`), for clean container/systemd configuration.
- **Deployment artifacts** (`Dockerfile`, `deploy/`) — a container + Caddy auto-TLS reverse proxy, WAL-safe backup script, and a runbook for running the daemon behind a public subdomain.

### Security
- **The HTTP API is append-and-read only.** `delete_node`, `unlink_nodes`, `update_status`, and `update_prompt` are refused at the daemon (403), so a shared graph reachable by a bearer token cannot have its history erased or rewritten by any token holder.
- **`graph_id` is validated before any filesystem access**, closing a latent path-handling surface.

### Fixed
- **Cache/disk wedge** — a graph whose data directory was deleted under a running daemon left `PUT` answering 201 while every subsequent write 404'd forever. The filesystem is authoritative now: a vanished graph is re-created instead of wedging.


## [0.3.5] - 2025-12-10

### Fixed
- **Critical: Database path resolution now walks up directory tree** - Previously, `deciduous` used relative paths based on current working directory. Running commands from subdirectories or different directories would use/create a different database, making it appear like data was lost. Now `deciduous` walks up the directory tree to find `.deciduous/` folder, similar to how `git` finds `.git/`. This means:
  - Running `deciduous nodes` from `project/src/` correctly uses `project/.deciduous/deciduous.db`
  - Running commands from any subdirectory of an initialized project works correctly
  - No more "phantom" databases created in wrong directories

### Technical Details
- Modified `get_db_path()` in `src/db.rs` to traverse parent directories
- `DECIDUOUS_DB_PATH` env var still takes priority if set
- If no `.deciduous/` found anywhere up the tree, defaults to current directory (for `deciduous init`)

## [0.3.4] - 2025-12-10

### Added
- `deciduous sync` exports to `docs/graph-data.json` for GitHub Pages integration

## [0.3.3] - 2025-12-09

### Added
- `deciduous dot` command for DOT/PNG graph export
- `deciduous writeup` command for PR writeup generation
- `--auto` flag for branch-specific filenames

## [0.3.2] - 2025-12-09

### Added
- Initial public release
- Core decision graph functionality
- Web viewer with multiple visualization modes
- GitHub Pages deployment support
