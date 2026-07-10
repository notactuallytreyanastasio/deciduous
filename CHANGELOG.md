# Changelog

## [0.15.0] - 2026-05-06

### Added
- Built-in MCP server: `deciduous mcp` exposes 31 tools over Model Context Protocol
- Works with Claude Code, Claude Desktop, and cowork
- Graph CRUD, querying, full-text search, chain tracing, node context, pulse, and orphan detection via MCP
- Session management: each conversation gets its own decision tree (start/end/resume), persisted across server restarts
- `/query` slash command for natural language reports from the decision graph

## [0.14.0] - 2026-04-08

### Changed
- Observations now require both a title and description (`-d` flag); CLI warns when the description is missing
- Observation descriptions shown inline in `deciduous nodes` listing
- Web viewer detail panel shows observation description prominently below title
- All templates and instructions updated for the observation title+description convention

## [0.13.0] - 2026-02-19

Covers the 0.13.x series (0.13.0 through 0.13.15, 2026-02-19 to 2026-03-22).

### Added
- Document attachments: attach files (images, PDFs, diagrams) to decision nodes
- 7 `doc` subcommands: attach, list, show, describe, open, detach, gc - with AI-generated descriptions (`--ai-describe`), content-hash deduplication, and soft-delete with garbage collection
- Theme system: tag and group nodes by theme
- REST API: `/api/documents` and `/api/documents/file/<id>` endpoints
- Custom deciduous agent and tool for OpenCode; modern OpenCode directory conventions (0.13.2-0.13.5)
- decision-graph skill mines PRs via `gh` CLI for design context and review discussion (0.13.6)
- "What NOT to Log" guidance in templates to reduce meta-process noise (0.13.15)

### Changed
- Version checking is now always-on with quiet patch notifications and prominent minor/major banners; `deciduous auto-update on/off` deprecated (0.13.10-0.13.11)
- Web viewer defaults to showing all goals and sorts narratives by most recent activity (0.13.12-0.13.13)
- Post-commit hook is now advisory instead of blocking (0.13.14)

### Fixed
- `deciduous update` no longer deletes user content after the workflow section in CLAUDE.md; added `<!-- deciduous:start/end -->` markers (0.13.1)
- OpenCode plugins write to `.deciduous/plugin.log` instead of stdout/stderr, which corrupted the TUI (0.13.7-0.13.9)

## [0.12.0] - 2026-02-05

Covers the 0.12.x series (0.12.0 through 0.12.2).

### Added
- Redesigned web viewer with hierarchical narrative view and smart narrative detection
- D3 DAG flowchart visualization with left-to-right hierarchical layout
- Full-text search with type filter buttons; resizable panels
- All slash commands bootstrapped via init/update: /document, /build-test, /serve-ui, /sync-graph, /decision-graph, /sync (0.12.1)

### Changed
- Removed 20k+ lines of old scattered components for unified App.tsx
- README rewritten to lead with /pulse, /archaeology, /narratives skills (0.12.2)

## [0.11.0] - 2026-01-22

Covers the 0.11.x series (0.11.0 through 0.11.2).

### Added
- OpenCode integration support via `deciduous init` and `deciduous update`
- New `integration-status` command to check Claude Code and OpenCode setup
- Pre-built binaries for Linux (x64/ARM64), macOS, and Windows on GitHub Releases with SHA256 checksums (0.11.1)

### Fixed
- `deciduous opencode install` now works standalone without `deciduous init` (0.11.2)

## [0.10.0] - 2026-01-14

Covers the 0.10.x series (0.10.0 through 0.10.3).

### Added
- Archaeology view is now the default at `deciduous serve` - narrative-focused exploration with AI-powered explanations
- Session-based card stack UI with keyboard navigation and mobile touch gestures
- Deep linking with parameterized routes and Q&A history view with full-text search (0.10.2)
- Prompt modal for viewing verbatim user prompts on nodes (0.10.2)

### Fixed
- Card stack rendering fixes and removal of node count limits (0.10.1, 0.10.3)

## [0.9.0] - 2026-01-12

Covers the 0.9.x series (0.9.0 through 0.9.6).

### Added
- New `--date` flag for backdating nodes (archaeology workflow support)
- New `revisit` node type for pivots and direction changes (0.9.2)
- Skills added to init/update: /pulse, /narratives, /archaeology (0.9.3)
- New `check-update` command with version tracking via `.deciduous/.version` (0.9.5)
- Embedded changelog shows what's new when upgrading (0.9.6)

### Changed
- `update` no longer overwrites user configs (settings.json, config.toml, docs/) (0.9.5)

## [0.8.0] - 2025-12-11

Covers the 0.8.x series (0.8.0 through 0.8.25, 2025-12-11 to 2026-01-09).

### Added
- TUI enhancements: rich detail panel, syntax highlighting, modals
- Commit linking via `--commit` flag (0.8.3)
- Git history export and DAG default view in web viewer (0.8.5, 0.8.6)
- Light theme web UI (0.8.7)
- `audit` command for retroactive commit association (0.8.8)
- Git guard for staging safety (0.8.22)
- New `delete` and `unlink` commands (0.8.25)

### Changed
- Focused on Claude Code integration, removed multi-editor support (0.8.23)

## [0.7.0] - 2025-12-11

Covers the 0.7.x series (0.7.0 through 0.7.2).

### Added
- Terminal user interface (TUI) for decision graph exploration

### Fixed
- NULL change_ids backfilled on database open (0.7.1)
- Terminal cleanup runs even on TUI error (0.7.2)

## [0.6.0] - 2025-12-10

### Added
- Multi-user graph sync with diff/patch model (`deciduous diff export/apply/status`)
- Multi-user sync documentation and init templates

## [0.5.0] - 2025-12-10

Covers the 0.5.x series (0.5.0 through 0.5.4).

### Added
- Reference configs for Claude Code and Windsurf
- Branch-scoped decision graphs (0.5.1)
- Graph integrity auditing (0.5.2)
- New `update` command and `.deciduous/config.toml` config file (0.5.3)
- Branch dropdown in web viewer (0.5.4)

### Fixed
- Foreign key error when linking nodes (0.5.3)

## [0.4.0] - 2025-12-10

Covers the 0.4.x series (0.4.0 through 0.4.3).

### Added
- Editor-specific init flags: `--claude` and `--windsurf`
- `--prompt` and `--files` flags for node metadata tracking
- 30-second auto-refresh in web viewer when running locally

### Fixed
- Windsurf rules YAML format and activation triggers

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
