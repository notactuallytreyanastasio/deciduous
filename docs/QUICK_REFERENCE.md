# Quick Reference

Fast lookup for common deciduous operations.

---

## Installation

```bash
cargo install deciduous
# or
brew install deciduous  # if published to homebrew
```

---

## Initialize a Project

```bash
deciduous init                    # Claude Code (default)
deciduous init --opencode         # OpenCode
deciduous init --windsurf         # Windsurf
deciduous init --both             # Claude Code + OpenCode
```

Creates: `.deciduous/`, `.claude/` (or `.opencode/`), hooks, CLAUDE.md section

---

## Node Types

| Type | Use When | Shape |
|------|----------|-------|
| `goal` | User wants something | House |
| `decision` | Choice must be made | Diamond |
| `option` | Possible choice | Box |
| `action` | Work being done | Rounded box |
| `outcome` | Work complete | Ellipse |
| `observation` | Something noticed | Note |
| `revisit` | Changing direction | Octagon |

---

## Add Nodes

```bash
# Basic
deciduous add goal "Add auth"
deciduous add decision "How to store sessions?"
deciduous add action "Implementing middleware"
deciduous add outcome "Auth working"
deciduous add observation "Redis adds complexity"
deciduous add revisit "Rethinking approach"

# With metadata
deciduous add action "Implementing X" \
  -c 85 \                           # Confidence 0-100
  --commit HEAD \                   # Link to current commit
  -f "src/auth.rs,src/session.rs" \ # Associated files
  -b feature-auth                   # Git branch

# With verbatim prompt (for goals)
deciduous add goal "Add auth" --prompt-stdin << 'EOF'
I need user authentication with email/password and OAuth...
EOF
```

---

## Link Nodes

```bash
# Basic (default: leads_to)
deciduous link 1 2

# With rationale
deciduous link 1 2 -r "Auth decision follows from goal"

# With edge type
deciduous link 1 2 -t chosen    # Option was selected
deciduous link 1 2 -t rejected  # Option was rejected
deciduous link 1 2 -t requires  # Dependency
```

---

## Query Graph

```bash
# List nodes
deciduous nodes
deciduous nodes --branch main
deciduous nodes --type goal
deciduous nodes --status active

# List edges
deciduous edges

# Show single node details
deciduous show 42
deciduous show 42 --json

# Full graph as JSON
deciduous graph
```

---

## Visualize

```bash
# Web viewer (opens browser)
deciduous serve
deciduous serve --port 8080

# Terminal UI
deciduous tui

# DOT export
deciduous dot > graph.dot
deciduous dot --png -o graph.png    # Requires graphviz
deciduous dot --auto                # Branch-specific filename
```

---

## Export & Sync

```bash
# Export for GitHub Pages
deciduous sync

# Creates:
# - docs/graph-data.json
# - docs/git-history.json
# - docs/index.html

# Then push to GitHub, enable Pages on /docs
```

---

## Multi-User Sync

```bash
# After git pull: import teammates' records, export yours, refresh docs/graph-data.json
deciduous sync

# What is pending? (exit 1 if anything)
deciduous sync --check

# Reconcile only, skip the GitHub Pages export
deciduous sync --no-pages

# Link to a teammate's node by change_id prefix (CHANGE column in `deciduous nodes`)
deciduous link a1b2c3d4 42 -r "implements their goal"

# Then commit the records
git add .deciduous/sync/
```

---

## Status Updates

```bash
deciduous status 42 active       # Currently in use
deciduous status 42 superseded   # Replaced by newer approach
deciduous status 42 abandoned    # Tried and rejected
deciduous status 42 completed    # Finished
```

---

## Update Prompts

```bash
# Update existing node's prompt
deciduous prompt 42 "The new prompt text"

# From stdin (multi-line)
deciduous prompt 42 << 'EOF'
The full verbatim prompt here...
EOF
```

---

## Roadmap Sync

```bash
# Initialize (parses ROADMAP.md)
deciduous roadmap init

# Sync with GitHub Issues
deciduous roadmap sync              # Dry run
deciduous roadmap sync --execute    # Apply changes

# List items
deciduous roadmap list
deciduous roadmap list --with-issues
```

---

## Hooks

```bash
# Install hooks from config
deciduous hooks install

# Uninstall
deciduous hooks uninstall

# Check status
deciduous hooks status

# Show all integration status
deciduous integration
```

---

## Database Operations

```bash
# Backup
deciduous backup
deciduous backup -o backup.db

# Delete a node (and its edges)
deciduous delete 42
deciduous delete 42 --dry-run

# Remove an edge
deciduous unlink 1 2
```

---

## PR Writeup

```bash
# Generate PR description from graph
deciduous writeup --title "Add auth" --nodes 1-15 -o PR.md

# With embedded graph
deciduous dot --auto --nodes 1-15 --png
git add docs/decision-graph-*.png
deciduous writeup --auto --title "Add auth" --nodes 1-15
```

---

## Session Recovery

```bash
# Check for updates (always-on, checked every 24h)
deciduous check-update

# Show recent decisions
deciduous nodes
deciduous edges

# Show recent commands
deciduous commands --limit 20

# View git state
git log --oneline -10
git status
```

---

## Common Patterns

### Start New Feature

```bash
deciduous add goal "Feature name" --prompt-stdin << 'EOF'
User's full request here...
EOF
# Note the ID (e.g., 42)

deciduous add decision "Key design question" -c 85
deciduous link 42 43 -r "Design question for feature"
```

### Record a Pivot

```bash
deciduous add observation "Problem with current approach"
deciduous add revisit "Reconsidering X"
deciduous link <observation_id> <revisit_id> -r "Caused rethinking"
deciduous link <revisit_id> <new_decision_id> -r "New approach"
deciduous status <old_decision_id> superseded
```

### After Committing Code

```bash
git commit -m "feat: add auth"
deciduous add action "Implemented auth" -c 90 --commit HEAD
deciduous link <goal_id> <action_id> -r "Implementation"
```

### Before a PR

```bash
# Export graph for the PR
deciduous dot --auto --nodes 1-15 --png

# Generate writeup
deciduous writeup --auto -t "My PR" --nodes 1-15

# Update PR
gh pr edit N --body "$(deciduous writeup --auto -t 'My PR' --nodes 1-15)"
```

---

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `DECIDUOUS_DB_PATH` | Override database path |
| `GITHUB_TOKEN` | GitHub API access for roadmap sync |

---

## Slash Commands

All bootstrapped by `deciduous init` and updated by `deciduous update`.

| Command | Purpose |
|---------|---------|
| `/decision` | Manage decision graph - add nodes, link edges, sync |
| `/recover` | Recover context from decision graph on session start |
| `/work` | Start a work transaction - creates goal node before implementation |
| `/document` | Generate comprehensive documentation for a file or directory |
| `/build-test` | Build the project and run the test suite |
| `/serve-ui` | Start the decision graph web viewer |
| `/sync-graph` | Export decision graph to GitHub Pages |
| `/decision-graph` | Build a decision graph from commit history |
| `/sync` | Multi-user sync - pull events, rebuild, push |

## Skills

| Skill | Purpose |
|-------|---------|
| `/pulse` | Map current design as decisions (Now mode) |
| `/narratives` | Understand how the system evolved (History mode) |
| `/archaeology` | Transform narratives into queryable graph |

---

## File Locations

| Path | Purpose |
|------|---------|
| `.deciduous/deciduous.db` | SQLite database (gitignored) |
| `.deciduous/config.toml` | Configuration |
| `.deciduous/.version` | Binary version for update detection |
| `.deciduous/.latest_version` | Cached latest version from crates.io |
| `.deciduous/.last_version_check` | Timestamp of last version check |
| `.deciduous/sync/` | Shared graph records, one JSON file each (tracked) |
| `.claude/hooks/` | Claude Code hooks |
| `.claude/commands/` | Claude Code slash commands |
| `.claude/skills/` | Claude Code skills |
| `docs/graph-data.json` | Exported graph for GitHub Pages |
