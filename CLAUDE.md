# Deciduous - Decision Graph Tooling

Decision graph tooling for AI-assisted development. Track every goal, decision, and outcome. Survive context loss. Query your reasoning.

---

## The Two Modes

Every system has two stories:

| Mode | Question | Skill |
|------|----------|-------|
| **Now** | "How does this work?" | `/pulse` |
| **History** | "How did we get here?" | `/narratives` → `/archaeology` |

**Now mode** maps current design as decisions. **History mode** captures evolution and pivots.

---

## Skills Overview

### /pulse - Map Current Design

Take the pulse of a system - what decisions define how it works TODAY.

```bash
# Start with a goal
deciduous add goal "API rate limiting behavior" -c 90

# Map the decisions
deciduous add decision "How to identify users?" -c 85
deciduous link 1 2 -r "leads_to"

deciduous add decision "What are the thresholds?" -c 85
deciduous link 1 3 -r "leads_to"
```

**Output:** Decision tree of the current model (like Dan's Suspense diagram).

### /narratives - Understand Evolution

Understand how the system evolved. Narratives are conceptual, not tied to commits.

1. Look at the current system
2. Ask "how did this get this way?"
3. Infer narratives from the design
4. Find evidence (commits, PRs, docs)
5. Identify pivots - where the model changed

**Output:** `.deciduous/narratives.md` with evolution stories.

### /archaeology - Structure for Query

Transform narratives into a queryable graph.

| Narrative Element | Graph Node |
|-------------------|------------|
| Title | `goal` |
| Design question | `decision` |
| Answer | `option` |
| What was learned | `observation` |
| **PIVOT** | `revisit` |

**Output:** Connected graph with Now ← revisit ← History.

---

## The Revisit Node

When a design approach is abandoned and replaced:

```
[Old Decision] ──► [Observation: why it failed] ──► [REVISIT] ──► [New Decision]
```

The revisit captures:
- WHAT is being reconsidered
- WHY (linked observations)
- Connects old approach to new

```bash
deciduous add observation "JWT too large for mobile"
deciduous add revisit "Reconsidering token strategy"
deciduous link <observation> <revisit> -r "forced rethinking"
deciduous status <old_decision> superseded
```

---

## Node Status

| Status | Meaning |
|--------|---------|
| `active` | Current truth |
| `superseded` | Replaced by newer approach |
| `abandoned` | Tried and rejected |

```bash
# Now mode - only active
deciduous nodes --status active

# History mode - everything
deciduous nodes

# Pivot points
deciduous nodes --type revisit
```

---

## Decision Graph Workflow

**THIS IS MANDATORY. Log decisions IN REAL-TIME, not retroactively.**

### The Core Rule

```
BEFORE you do something -> Log what you're ABOUT to do
AFTER it succeeds/fails -> Log the outcome
CONNECT immediately -> Link every node to its parent
AUDIT regularly -> Check for missing connections
```

### Behavioral Triggers - MUST LOG WHEN:

| Trigger | Log Type | Example |
|---------|----------|---------|
| User asks for a new feature | `goal` **with -p** | "Add dark mode" |
| Choosing between approaches | `decision` | "Choose state management" |
| About to write/edit code | `action` | "Implementing Redux store" |
| Something worked or failed | `outcome` | "Redux integration successful" |
| Notice something interesting | `observation` | "Existing code uses hooks" |

### CRITICAL: Capture VERBATIM User Prompts

**Prompts must be the EXACT user message, not a summary.** When a user request triggers new work, capture their full message word-for-word.

**BAD - summaries are useless for context recovery:**
```bash
# DON'T DO THIS - this is a summary, not a prompt
deciduous add goal "Add auth" -p "User asked: add login to the app"
```

**GOOD - verbatim prompts enable full context recovery:**
```bash
# Use --prompt-stdin for multi-line prompts
deciduous add goal "Add auth" -c 90 --prompt-stdin << 'EOF'
I need to add user authentication to the app. Users should be able to sign up
with email/password, and we need OAuth support for Google and GitHub. The auth
should use JWT tokens with refresh token rotation. Make sure to add rate limiting
on the login endpoint to prevent brute force attacks.
EOF

# Or use the prompt command to update existing nodes
deciduous prompt 42 << 'EOF'
The full verbatim user message goes here...
EOF
```

**When to capture prompts:**
- Root `goal` nodes: YES - the FULL original request
- Major direction changes: YES - when user redirects the work
- Routine downstream nodes: NO - they inherit context via edges

**Updating prompts on existing nodes:**
```bash
# Add or update a prompt retroactively
deciduous prompt <node_id> "full verbatim prompt here"

# Read from stdin for multi-line
cat prompt.txt | deciduous prompt <node_id>
```

Prompts are viewable in the TUI detail panel (`deciduous tui`) and web viewer detail panel.

### ⚠️ CRITICAL: Maintain Connections

**The graph's value is in its CONNECTIONS, not just nodes.**

| When you create... | IMMEDIATELY link to... |
|-------------------|------------------------|
| `outcome` | The action/goal it resolves |
| `action` | The goal/decision that spawned it |
| `option` | Its parent decision |
| `observation` | Related goal/action |

**Root `goal` nodes are the ONLY valid orphans.**

### Quick Commands

```bash
deciduous add goal "Title" -c 90 -p "User's original request"
deciduous add action "Title" -c 85
deciduous link FROM TO -r "reason"  # DO THIS IMMEDIATELY!
deciduous serve   # View live (auto-refreshes every 30s)
deciduous sync    # Export for static hosting

# Metadata flags
# -c, --confidence 0-100     Confidence level
# -p, --prompt "..."         Store user prompt (short, single-line)
# --prompt-stdin             Read prompt from stdin (multi-line, preferred)
# -f, --files "a.rs,b.rs"    Associate files
# -b, --branch <name>        Git branch (auto-detected)
# --commit <hash|HEAD>       Link to git commit (use HEAD for current commit)
# --date "YYYY-MM-DD"        Backdate node (RFC3339 or YYYY-MM-DD HH:MM:SS)

# Update prompts on existing nodes
deciduous prompt <node_id> "prompt text"   # Short prompt
deciduous prompt <node_id> << 'EOF'        # Multi-line from stdin
Full verbatim prompt here...
EOF

# Branch filtering
deciduous nodes --branch main
deciduous nodes -b feature-auth
```

### ⚠️ CRITICAL: Link Commits to Actions/Outcomes

**After every git commit, link it to the decision graph!**

```bash
# AFTER committing code, log an action/outcome with --commit HEAD
git commit -m "feat: add auth"
deciduous add action "Implemented auth feature" -c 90 --commit HEAD
deciduous link <goal_id> <action_id> -r "Implementation"

# Or log the outcome of a completed feature
deciduous add outcome "Auth feature merged" -c 95 --commit HEAD
```

This creates traceability between commits and decisions. The TUI and web viewer show commits linked to nodes.

### Branch-Based Grouping

Nodes are auto-tagged with the current git branch. Configure in `.deciduous/config.toml`:
```toml
[branch]
main_branches = ["main", "master"]
auto_detect = true
```

### Audit Checklist (Before Every Sync)

1. Does every **outcome** link back to what caused it?
2. Does every **action** link to why you did it?
3. Any **dangling outcomes** without parents?

### Session Start Checklist

```bash
deciduous nodes    # What decisions exist?
deciduous edges    # How are they connected? Any gaps?
git status         # Current state
```

### Multi-User Sync

Share decisions across teammates:

```bash
# Export your branch's decisions
deciduous diff export --branch feature-x -o .deciduous/patches/my-feature.json

# Apply patches from teammates (idempotent)
deciduous diff apply .deciduous/patches/*.json

# Preview before applying
deciduous diff apply --dry-run .deciduous/patches/teammate.json
```

PR workflow: Export patch → commit patch file → PR → teammates apply.
## Session Start Checklist

Every new session or after context recovery, run `/recover` or:

```bash
deciduous nodes           # What decisions exist?
deciduous edges           # How are they connected?
deciduous commands        # What happened recently?
git log --oneline -10     # Recent commits
git status                # Current state
```

---

## Quick Reference

```bash
# Build
cargo build --release

# Run tests
cargo test

# Initialize in a new project
deciduous init

# Start graph viewer
deciduous serve --port 3000

# Export graph
deciduous sync
deciduous graph > graph.json

# Generate DOT visualization
deciduous dot --png -o docs/decision-graph.dot

# Generate PR writeup
deciduous writeup -t "Feature X" --nodes 1-15 -o PR-WRITEUP.md
```

## Subagents - Domain-Specific Context

**Use subagents to scope work to specific parts of the codebase.**

When working on this project, identify which domain the work belongs to and use the appropriate subagent context. Subagent definitions are in `.claude/agents.toml`.

### Available Subagents

| Agent | Domain | Key Files |
|-------|--------|-----------|
| `rust-core` | CLI, database, export/sync | `src/main.rs`, `src/db.rs`, `src/export.rs` |
| `tui` | Terminal UI with Ratatui | `src/tui/**/*.rs` |
| `web` | React/TypeScript viewer | `web/src/**/*.{ts,tsx}` |
| `tooling` | Claude Code configuration | `.claude/`, `CLAUDE.md`, `src/init/` |
| `docs` | Documentation, guides | `docs/`, `README.md`, `ROADMAP.md` |
| `ci` | Build, Actions, releases | `.github/workflows/`, `scripts/` |

### How to Use Subagents

When spawning a Task for exploration or implementation:

1. **Identify the domain** from the file patterns in `.claude/agents.toml`
2. **Include the subagent context** in your Task prompt
3. **Scope file searches** to the relevant patterns

Example: For TUI work, spawn an Explore agent with:
```
"Focus on src/tui/. This is the TUI agent domain - Ratatui widgets, TEA pattern, vim navigation. See .claude/agents.toml for full context."
```

### Why Subagents Matter

- **Reduced context overhead**: Focus on relevant files only
- **Domain expertise**: Each agent has specialized instructions
- **Parallel work**: Multiple agents can work on different domains simultaneously
- **Consistency**: Same patterns applied across similar work

---

## Architecture

```
src/
├── main.rs              # CLI entry, command dispatch
├── lib.rs               # Public API exports
├── db.rs                # SQLite database via Diesel ORM
├── schema.rs            # Diesel table definitions
├── init.rs              # Project initialization (deciduous init)
├── serve.rs             # HTTP server for web UI
└── export.rs            # DOT export and PR writeup generation

web/                     # React/TypeScript web viewer source
├── src/
│   ├── utils/
│   │   └── graphProcessing.ts  # Chain building, session grouping algorithms
│   ├── types/
│   │   └── graph.ts            # TypeScript types for graph data
│   └── components/             # React components
└── dist/                       # Built output (singlefile HTML)
```

## Web Viewer Development

**When modifying web viewer code (TypeScript/React), you MUST rebuild and update the embedded HTML.**

### Key Files

| File | Purpose |
|------|---------|
| `web/src/utils/graphProcessing.ts` | Chain building, BFS traversal, session grouping |
| `web/src/types/graph.ts` | TypeScript interfaces for nodes, edges, chains |
| `src/viewer.html` | Embedded viewer served by `deciduous serve` |
| `docs/demo/index.html` | Static demo viewer for GitHub Pages |

### Rebuild Process

After modifying any `web/src/**` files:

```bash
# 1. Build the web viewer (outputs singlefile HTML)
cd web && npm run build && cd ..

# 2. Copy to embedded locations (use absolute paths)
cp /path/to/deciduous/web/dist/index.html /path/to/deciduous/src/viewer.html
cp /path/to/deciduous/web/dist/index.html /path/to/deciduous/docs/demo/index.html

# 3. Run Rust tests to ensure nothing broke
cargo test

# 4. Build release binary
cargo build --release
```

### Chain/Graph Processing Notes

The `buildChains` function in `graphProcessing.ts` uses BFS to traverse **full connected components**:
- Follows both outgoing AND incoming edges
- No artificial node limits (MAX_CHAIN_NODES = 0 means unlimited)
- Chains include all nodes reachable from any direction

This ensures viewing a single chain shows the entire decision tree, not a truncated subset.

## CLI Commands

| Command | Description |
|---------|-------------|
| `deciduous init` | Initialize deciduous in current directory |
| `deciduous add <type> "title"` | Add a node (goal/decision/option/action/outcome/observation/revisit) |
| `deciduous link <from> <to>` | Create edge between nodes |
| `deciduous status <id> <status>` | Update node status |
| `deciduous nodes` | List all nodes |
| `deciduous edges` | List all edges |
| `deciduous graph` | Output full graph as JSON |
| `deciduous commands` | Show recent command log |
| `deciduous backup` | Create database backup |
| `deciduous serve` | Start web viewer |
| `deciduous sync` | Export graph to JSON file |
| `deciduous dot` | Export graph as DOT format |
| `deciduous writeup` | Generate PR writeup markdown |
| `deciduous diff export` | Export nodes as a shareable patch |
| `deciduous diff apply` | Apply patches from teammates |
| `deciduous diff status` | List available patches |
| `deciduous migrate` | Add change_id columns for sync |

## DOT Export Options

```bash
deciduous dot [OPTIONS]

Options:
  -o, --output <FILE>     Output file (default: stdout)
  -r, --roots <IDS>       Root node IDs for BFS traversal (comma-separated)
  -n, --nodes <SPEC>      Specific node IDs or ranges (e.g., "1-11" or "1,3,5-10")
  -t, --title <TITLE>     Graph title
      --rankdir <DIR>     Graph direction: TB (top-bottom) or LR (left-right)
      --png               Generate PNG file (requires graphviz installed)
```

## Writeup Options

```bash
deciduous writeup [OPTIONS]

Options:
  -t, --title <TITLE>     PR title
  -r, --roots <IDS>       Root node IDs (comma-separated, traverses children)
  -n, --nodes <SPEC>      Specific node IDs or ranges
  -o, --output <FILE>     Output file (default: stdout)
      --png <FILENAME>    PNG file to embed (auto-detects GitHub repo/branch for URL)
      --no-dot            Skip DOT graph section
      --no-test-plan      Skip test plan section
```

**Recommended workflow with `--auto`:**

```bash
# 1. Generate branch-specific PNG (avoids merge conflicts!)
deciduous dot --auto --nodes 1-11

# 2. Commit and push
git add docs/decision-graph-*.dot docs/decision-graph-*.png
git commit -m "docs: add decision graph"
git push

# 3. Generate writeup with auto PNG detection
deciduous writeup --auto -t "My PR" --nodes 1-11

# 4. Update PR body
gh pr edit N --body "$(deciduous writeup --auto -t 'My PR' --nodes 1-11)"
```

The `--auto` flag generates branch-specific filenames (e.g., `docs/decision-graph-feature-foo.png`) which prevents merge conflicts when multiple PRs each have their own graph.

## Database Rules

**CRITICAL: NEVER delete the SQLite database (`.deciduous/deciduous.db`)**

The database contains the decision graph. If you need to clear data:
1. `deciduous backup` first
2. Ask the user before any destructive operation

---

## Multi-User Sync

**Problem**: Multiple users work on the same codebase, each with a local `.deciduous/deciduous.db` (gitignored). How to share decisions?

**Solution**: jj-inspired dual-ID model. Each node has:
- `id` (integer): Local database primary key, different per machine
- `change_id` (UUID): Globally unique, stable across all databases

### Export/Apply Workflow

```bash
# Export your branch's decisions as a patch
deciduous diff export --branch feature-x -o .deciduous/patches/alice-feature.json

# Export specific node IDs
deciduous diff export --nodes 172-188 -o .deciduous/patches/feature.json --author alice

# Apply patches from teammates (idempotent - safe to re-apply)
deciduous diff apply .deciduous/patches/*.json

# Preview what would change
deciduous diff apply --dry-run .deciduous/patches/bob-refactor.json

# Check patch status
deciduous diff status
```

### PR Workflow

1. Create nodes locally while working
2. Export: `deciduous diff export --branch my-feature -o .deciduous/patches/my-feature.json`
3. Commit the patch file (NOT the database)
4. Open PR with patch file included
5. Teammates pull and apply: `deciduous diff apply .deciduous/patches/my-feature.json`
6. **Idempotent**: Same patch applied twice = no duplicates

### Patch Format (JSON)

```json
{
  "version": "1.0",
  "author": "alice",
  "branch": "feature/auth",
  "nodes": [{ "change_id": "uuid...", "title": "...", ... }],
  "edges": [{ "from_change_id": "uuid1", "to_change_id": "uuid2", ... }]
}
```

---

## Development Rules

### Code Quality - MANDATORY

1. **ALWAYS run tests before committing:**
   ```bash
   cargo test
   ```
   Do NOT commit if tests fail.

2. **ALWAYS ensure code compiles:**
   ```bash
   cargo build --release
   ```
   Do NOT commit code that doesn't compile.

3. **Write tests for new functionality:**
   - New commands need tests
   - Bug fixes need regression tests
   - Edge cases need coverage

4. **Run clippy for lints:**
   ```bash
   cargo clippy
   ```

### Pre-Commit Checklist

```bash
cargo test              # All tests pass?
cargo build --release   # Compiles cleanly?
cargo clippy            # No warnings?
```

Only commit if ALL pass.

---

## Release Process - MANDATORY

### Semantic Versioning (SemVer)

Follow semver strictly: `MAJOR.MINOR.PATCH`

| Change Type | Version Bump | Example |
|-------------|--------------|---------|
| Breaking API change | MAJOR | 1.0.0 → 2.0.0 |
| New feature (backward compatible) | MINOR | 1.0.0 → 1.1.0 |
| Bug fix (backward compatible) | PATCH | 1.0.0 → 1.0.1 |

### Release Checklist

1. **Update version in Cargo.toml:**
   ```toml
   version = "X.Y.Z"
   ```

2. **Run full test suite:**
   ```bash
   cargo test
   cargo build --release
   ```

3. **Update CHANGELOG (if exists) or commit message with release notes**

4. **Commit the version bump:**
   ```bash
   git add Cargo.toml Cargo.lock
   git commit -m "release: vX.Y.Z - <brief description>"
   ```

5. **Create and push a git tag:**
   ```bash
   git tag -a vX.Y.Z -m "vX.Y.Z: <release notes>"
   git push origin main
   git push origin vX.Y.Z
   ```

6. **Publish to crates.io:**
   ```bash
   cargo publish
   ```

7. **Create GitHub Release:**
   ```bash
   gh release create vX.Y.Z --title "vX.Y.Z" --notes "<release notes>"
   ```
   Or use the GitHub UI: Releases → Draft new release → Choose tag → Add notes

### Release Notes Format

```markdown
## vX.Y.Z

### Added
- New feature A
- New feature B

### Changed
- Updated behavior of X

### Fixed
- Bug fix for Y
- Bug fix for Z

### Breaking Changes (if MAJOR bump)
- API change description
```

### Example Full Release

```bash
# 1. Bump version
sed -i '' 's/version = "0.3.4"/version = "0.3.5"/' Cargo.toml

# 2. Test
cargo test && cargo build --release

# 3. Commit
git add Cargo.toml Cargo.lock
git commit -m "release: v0.3.5 - fix detail panel layout"

# 4. Tag
git tag -a v0.3.5 -m "v0.3.5: Fix detail panel layout for connections

- Rationale text now displays on separate line
- Full node titles shown without truncation
- Improved readability of incoming/outgoing connections"

# 5. Push
git push origin main
git push origin v0.3.5

# 6. Publish
cargo publish

# 7. GitHub Release
gh release create v0.3.5 --title "v0.3.5" --notes "Fix detail panel layout for connections

- Rationale text now displays on separate line
- Full node titles shown without truncation
- Improved readability of incoming/outgoing connections"
```

---

## External Dependencies

### Required at Runtime

| Dependency | Required For | Install |
|------------|--------------|---------|
| None | Core functionality | - |

The deciduous binary is self-contained for core features.

### Optional Dependencies

| Dependency | Required For | Install |
|------------|--------------|---------|
| graphviz | `--png` flag (DOT → PNG) | `brew install graphviz` / `apt install graphviz` |

If graphviz is not installed, `deciduous dot --png` will fail with a helpful error message.

---

## GitHub Action for PNG Cleanup

When you run `deciduous init`, a GitHub workflow is created at `.github/workflows/cleanup-decision-graphs.yml`. This workflow:

1. Triggers after any PR is merged
2. Finds decision graph PNG/DOT files
3. Creates a cleanup branch and removes them
4. Auto-merges the cleanup PR

This keeps your repo clean of accumulated visualization files while still having nice graphs in PRs.
