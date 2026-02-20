# Project Instructions for OpenCode

> This file was converted from CLAUDE.md. OpenCode uses AGENTS.md for project instructions.
> See: https://opencode.ai/docs/rules/

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

# Map options (possible approaches from the goal)
deciduous add option "Identify users by auth token" -c 85
deciduous link 1 2 -r "possible_approach"

deciduous add option "Use IP-based rate limiting" -c 85
deciduous link 1 3 -r "possible_approach"
```

**Output:** Decision tree of the current model (goal -> options -> decisions).

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
| Possible approach | `option` |
| Choosing an approach | `decision` |
| What was learned | `observation` |
| **PIVOT** | `revisit` |

**Output:** Connected graph with Now ← revisit ← History.

---

## Slash Commands

All slash commands are bootstrapped by `deciduous init` and updated by `deciduous update`.

| Command | Purpose |
|---------|---------|
| `/decision` | Manage decision graph - add nodes, link edges, sync |
| `/recover` | Recover context from decision graph on session start |
| `/work` | Start a work transaction - creates goal node before implementation |
| `/document` | Generate comprehensive documentation for a file or directory |
| `/build-test` | Build the project and run the test suite |
| `/serve-ui` | Start the decision graph web viewer |
| `/sync-graph` | Export decision graph to GitHub Pages |
| `/decision-graph` | Build a decision graph from commit history (archaeology) |
| `/sync` | Multi-user sync - pull events, rebuild, push |

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

<!-- deciduous:start -->
## Decision Graph Workflow

**THIS IS MANDATORY. Log decisions IN REAL-TIME, not retroactively.**

### Available Slash Commands

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

### Available Skills

| Skill | Purpose |
|-------|---------|
| `/pulse` | Map current design as decisions (Now mode) |
| `/narratives` | Understand how the system evolved (History mode) |
| `/archaeology` | Transform narratives into queryable graph |

### The Node Flow Rule - CRITICAL

The canonical flow through the decision graph is:

```
goal -> options -> decision -> actions -> outcomes
```

- **Goals** lead to **options** (possible approaches to explore)
- **Options** lead to a **decision** (choosing which option to pursue)
- **Decisions** lead to **actions** (implementing the chosen approach)
- **Actions** lead to **outcomes** (results of the implementation)
- **Observations** attach anywhere relevant
- Goals do NOT lead directly to decisions -- there must be options first
- Options do NOT come after decisions -- options come BEFORE decisions
- Decision nodes should only be created when an option is actually chosen, not prematurely

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
| Exploring possible approaches | `option` | "Use Redux for state" |
| Choosing between approaches | `decision` | "Choose state management" |
| About to write/edit code | `action` | "Implementing Redux store" |
| Something worked or failed | `outcome` | "Redux integration successful" |
| Notice something interesting | `observation` | "Existing code uses hooks" |

### Document Attachments

Attach files (images, PDFs, diagrams, specs, screenshots) to decision graph nodes for rich context.

```bash
# Attach a file to a node
deciduous doc attach <node_id> <file_path>
deciduous doc attach <node_id> <file_path> -d "Architecture diagram"
deciduous doc attach <node_id> <file_path> --ai-describe

# List documents
deciduous doc list              # All documents
deciduous doc list <node_id>    # Documents for a specific node

# Manage documents
deciduous doc show <doc_id>     # Show document details
deciduous doc describe <doc_id> "Updated description"
deciduous doc describe <doc_id> --ai   # AI-generate description
deciduous doc open <doc_id>     # Open in default application
deciduous doc detach <doc_id>   # Soft-delete (recoverable)
deciduous doc gc                # Remove orphaned files from disk
```

**When to suggest document attachment:**

| Situation | Action |
|-----------|--------|
| User shares an image or screenshot | Ask: "Want me to attach this to the current goal/action node?" |
| User references an external document | Ask: "Should I attach a copy to the decision graph?" |
| Architecture diagram is discussed | Suggest attaching it to the relevant goal node |
| Files not in the project are dropped in | Attach to the most relevant active node |

**Do NOT aggressively prompt for documents.** Only suggest when files are directly relevant to a decision node. Files are stored in `.deciduous/documents/` with content-hash naming for deduplication.

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
should use JWT tokens with refresh token rotation.
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
deciduous prompt <node_id> "full verbatim prompt here"
cat prompt.txt | deciduous prompt <node_id>  # Multi-line from stdin
```

Prompts are viewable in the web viewer.

### CRITICAL: Maintain Connections

**The graph's value is in its CONNECTIONS, not just nodes.**

| When you create... | IMMEDIATELY link to... |
|-------------------|------------------------|
| `outcome` | The action that produced it |
| `action` | The decision that spawned it |
| `decision` | The option(s) it chose between |
| `option` | Its parent goal |
| `observation` | Related goal/action |
| `revisit` | The decision/outcome being reconsidered |

**Root `goal` nodes are the ONLY valid orphans.**

### Quick Commands

```bash
deciduous add goal "Title" -c 90 -p "User's original request"
deciduous add action "Title" -c 85
deciduous link FROM TO -r "reason"  # DO THIS IMMEDIATELY!
deciduous serve   # View live (auto-refreshes every 30s)
deciduous sync    # Export for static hosting

# Metadata flags
# -c, --confidence 0-100   Confidence level
# -p, --prompt "..."       Store the user prompt (use when semantically meaningful)
# -f, --files "a.rs,b.rs"  Associate files
# -b, --branch <name>      Git branch (auto-detected)
# --commit <hash|HEAD>     Link to git commit (use HEAD for current commit)
# --date "YYYY-MM-DD"      Backdate node (for archaeology)

# Branch filtering
deciduous nodes --branch main
deciduous nodes -b feature-auth
```

### CRITICAL: Link Commits to Actions/Outcomes

**After every git commit, link it to the decision graph!**

```bash
git commit -m "feat: add auth"
deciduous add action "Implemented auth" -c 90 --commit HEAD
deciduous link <goal_id> <action_id> -r "Implementation"
```

The `--commit HEAD` flag captures the commit hash and links it to the node. The web viewer will show commit messages, authors, and dates.

### Git History & Deployment

```bash
# Export graph AND git history for web viewer
deciduous sync

# This creates:
# - docs/graph-data.json (decision graph)
# - docs/git-history.json (commit info for linked nodes)
```

To deploy to GitHub Pages:
1. `deciduous sync` to export
2. Push to GitHub
3. Settings > Pages > Deploy from branch > /docs folder

Your graph will be live at `https://<user>.github.io/<repo>/`

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

### Git Staging Rules - CRITICAL

**NEVER use broad git add commands that stage everything:**
- ❌ `git add -A` - stages ALL changes including untracked files
- ❌ `git add .` - stages everything in current directory
- ❌ `git add -a` or `git commit -am` - auto-stages all tracked changes
- ❌ `git add *` - glob patterns can catch unintended files

**ALWAYS stage files explicitly by name:**
- ✅ `git add src/main.rs src/lib.rs`
- ✅ `git add Cargo.toml Cargo.lock`
- ✅ `git add .opencode/commands/decision.md`

**Why this matters:**
- Prevents accidentally committing sensitive files (.env, credentials)
- Prevents committing large binaries or build artifacts
- Forces you to review exactly what you're committing
- Catches unintended changes before they enter git history

### Session Start Checklist

```bash
deciduous check-update    # Update needed? Run 'deciduous update' if yes
deciduous nodes           # What decisions exist?
deciduous edges           # How are they connected? Any gaps?
deciduous doc list        # Any attached documents to review?
git status                # Current state
```

### Multi-User Sync

Sync decisions with teammates via event logs:

```bash
# Check sync status
deciduous events status

# Apply teammate events (after git pull)
deciduous events rebuild

# Compact old events periodically
deciduous events checkpoint --clear-events
```

Events auto-emit on add/link/status commands. Git merges event files automatically.
<!-- deciduous:end -->
## Session Start Checklist

Every new session or after context recovery, run `/recover` or:

```bash
deciduous check-update    # Update needed? Run 'deciduous update' if yes
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

# Update Claude integration files to latest version
deciduous update

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

When working on this project, identify which domain the work belongs to and use the appropriate subagent context. Subagent definitions are in `.opencode/agents.toml`.

### Available Subagents

| Agent | Domain | Key Files |
|-------|--------|-----------|
| `rust-core` | CLI, database, export/sync | `src/main.rs`, `src/db.rs`, `src/export.rs` |
| `web` | React/TypeScript viewer | `web/src/**/*.{ts,tsx}` |
| `tooling` | OpenCode configuration | `.opencode/`, `AGENTS.md`, `src/init/` |
| `docs` | Documentation, guides | `docs/`, `README.md`, `ROADMAP.md` |
| `ci` | Build, Actions, releases | `.github/workflows/`, `scripts/` |

### How to Use Subagents

When spawning a Task for exploration or implementation:

1. **Identify the domain** from the file patterns in `.opencode/agents.toml`
2. **Include the subagent context** in your Task prompt
3. **Scope file searches** to the relevant patterns

Example: For web viewer work, spawn an Explore agent with:
```
"Focus on web/src/. This is the web agent domain - React components, D3/Dagre graphs. See .opencode/agents.toml for full context."
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

.deciduous/
├── deciduous.db         # SQLite database (decision graph)
├── documents/           # Content-hash named document attachments
├── config.toml          # Branch/feature configuration
├── sync/                # Event-based multi-user sync files
└── narratives.md        # Narrative tracking
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
| `deciduous update` | Update Claude integration files to latest version |
| `deciduous check-update` | Check if integration files need updating |
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
| `deciduous doc attach <node_id> <file>` | Attach file to a decision node |
| `deciduous doc list [node_id]` | List attached documents (all or per-node) |
| `deciduous doc show <id>` | Show document details |
| `deciduous doc describe <id> [text]` | Set/update document description (`--ai` for AI) |
| `deciduous doc open <id>` | Open document in default application |
| `deciduous doc detach <id>` | Soft-delete a document attachment |
| `deciduous doc gc` | Garbage collect orphaned document files |

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

### Git Staging Rules - CRITICAL

**NEVER use broad git add commands that stage everything:**
- ❌ `git add -A` - stages ALL changes including untracked files
- ❌ `git add .` - stages everything in current directory
- ❌ `git add -a` or `git commit -am` - auto-stages all tracked changes
- ❌ `git add *` - glob patterns can catch unintended files

**ALWAYS stage files explicitly by name:**
- ✅ `git add src/main.rs src/lib.rs`
- ✅ `git add Cargo.toml Cargo.lock`
- ✅ `git add .opencode/commands/decision.md`

**Why this matters:**
- Prevents accidentally committing sensitive files (.env, credentials)
- Prevents committing large binaries or build artifacts
- Forces you to review exactly what you're committing
- Catches unintended changes before they enter git history

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

2. **Update embedded changelog in `src/changelog.rs`:**
   ```rust
   // Add new release at TOP of RELEASES array (newest first)
   Release {
       version: "X.Y.Z",
       highlights: &[
           "Brief description of feature 1",
           "Brief description of feature 2",
       ],
   },
   ```
   This is REQUIRED - users see these notes when upgrading!

3. **Run full test suite:**
   ```bash
   cargo test
   cargo build --release
   ```

4. **Commit the version bump:**
   ```bash
   git add Cargo.toml Cargo.lock src/changelog.rs
   git commit -m "release: vX.Y.Z - <brief description>"
   ```

5. **Create and push a git tag:**
   ```bash
   git tag -a vX.Y.Z -m "vX.Y.Z: <release notes>"
   git push --no-verify origin main
   git push origin vX.Y.Z
   ```

6. **Publish to crates.io:**
   ```bash
   cargo publish --allow-dirty
   ```

7. **Create GitHub Release:**
   ```bash
   gh release create vX.Y.Z --title "vX.Y.Z" --notes "<release notes>"
   ```

8. **Update local version file:**
   ```bash
   deciduous update
   ```

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
