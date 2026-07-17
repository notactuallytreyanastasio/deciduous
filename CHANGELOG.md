# Changelog

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
