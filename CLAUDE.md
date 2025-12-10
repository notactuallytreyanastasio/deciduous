# Deciduous - Decision Graph Tooling

Decision graph tooling for AI-assisted development. Track every goal, decision, and outcome. Survive context loss. Query your reasoning.

---

## ⚠️ MANDATORY: Decision Graph Workflow

**THIS IS NOT OPTIONAL. The decision graph is watched live by the user. Every step must be logged IN REAL-TIME, not retroactively.**

### The Core Rule

```
BEFORE you do something → Log what you're ABOUT to do
AFTER it succeeds/fails → Log the outcome
ALWAYS → Sync frequently so the live graph updates
```

### Behavioral Triggers - MUST LOG WHEN:

| Trigger | Log Type | Example |
|---------|----------|---------|
| User asks for a new feature | `goal` | "Add dark mode to UI" |
| You're choosing between approaches | `decision` | "Choose state management approach" |
| You identify multiple ways to do something | `option` (for each) | "Option A: Redux", "Option B: Context" |
| You're about to write/edit code | `action` | "Implementing Redux store" |
| You notice something interesting | `observation` | "Existing code uses hooks pattern" |
| Something worked or failed | `outcome` | "Redux integration successful" |
| You complete a git commit | `action` with `--commit` | Include the commit hash |

### The Loop - Follow This EVERY Time

```
1. USER REQUEST RECEIVED
   ↓
   Log: goal or decision (what are we trying to do?)

2. BEFORE WRITING ANY CODE
   ↓
   Log: action "About to implement X"
   Link: Connect action to its parent goal/decision IMMEDIATELY

3. AFTER EACH SIGNIFICANT CHANGE
   ↓
   Log: outcome "X completed" or observation "Found Y"
   Link: Connect outcome back to its action/goal IMMEDIATELY

4. AUDIT CONNECTIONS
   ↓
   Ask: Does every outcome link to what caused it?
   Ask: Does every action link to why I did it?
   Fix: Any missing connections before continuing

5. BEFORE EVERY GIT PUSH
   ↓
   Run: deciduous sync
   Commit: Include graph-data.json

6. REPEAT - The user is watching the graph live
```

### ⚠️ CRITICAL: Maintain the Logical Thread

**The graph's value is in its CONNECTIONS, not just its nodes.**

Every time you create a node, IMMEDIATELY link it:

| When you create... | Link it to... | Why |
|-------------------|---------------|-----|
| `outcome` | The action/goal it resolves | Shows what work produced this result |
| `action` | The goal/decision that spawned it | Shows why you did this work |
| `option` | Its parent decision | Shows what choice this relates to |
| `observation` | Related goal/action | Shows context for the finding |
| `decision` | Parent goal (if any) | Shows what prompted the choice |

**Root `goal` nodes are the ONLY valid orphans.** Everything else needs a parent.

### Audit Checklist (Do This Regularly)

Before `deciduous sync`, ask yourself:
1. Does every **outcome** link back to what caused it?
2. Does every **action** link to why I did it?
3. Are there any **dangling outcomes** floating without parents?
4. Can I trace from any outcome back to a root goal?

```bash
# Find nodes that might need connections
deciduous edges | cut -d'>' -f2 | cut -d' ' -f2 | sort -u > /tmp/has_parent.txt
deciduous nodes | tail -n+3 | awk '{print $1}' | while read id; do
  grep -q "^$id$" /tmp/has_parent.txt || echo "CHECK: $id"
done
```

### Quick Commands

```bash
# Log nodes (use -c/--confidence 0-100)
deciduous add goal "Title" -c 90
deciduous add decision "Title" -c 75
deciduous add action "Title" -c 85
deciduous add observation "Title" -c 70
deciduous add outcome "Title" -c 95

# Link nodes
deciduous link FROM_ID TO_ID -r "Reason for connection"
deciduous link 1 2 --edge-type chosen -r "Selected this approach"

# View graph
deciduous nodes           # List all nodes
deciduous edges           # List all edges
deciduous graph           # Full graph as JSON

# Sync and export
deciduous sync            # Export to .deciduous/web/graph-data.json

# DOT export for visualizations
deciduous dot                              # Output DOT to stdout
deciduous dot -o graph.dot                 # Output to file
deciduous dot --png -o graph.dot           # Generate PNG (requires graphviz)
deciduous dot --auto --nodes 1-11          # Branch-specific filename (docs/decision-graph-{branch}.png)
deciduous dot --roots 1,5 --png            # Filter from root nodes (BFS)

# PR writeup generation
deciduous writeup -t "PR Title"            # Generate markdown writeup
deciduous writeup -t "Title" --nodes 1-11  # Writeup for specific nodes
deciduous writeup --auto --nodes 1-11      # Use branch-specific PNG (best for PRs!)
deciduous writeup --png docs/graph.png     # Explicit PNG path
deciduous writeup --no-dot --no-test-plan  # Skip sections

# Makefile shortcuts
make goal T="Title" C=90
make decision T="Title" C=75
make action T="Title" C=85
make obs T="Title" C=70
make outcome T="Title" C=95
make link FROM=1 TO=2 REASON="why"
make dot NODES=1-11 PNG=1
make writeup TITLE="PR Title" NODES=1-11
```

### Confidence Levels

- **90-100**: Certain, proven, tested
- **70-89**: High confidence, standard approach
- **50-69**: Moderate confidence, some unknowns
- **30-49**: Experimental, might change
- **0-29**: Speculative, likely to revisit

### Why This Matters

1. **The user watches the graph live** - They see your reasoning as you work
2. **Context WILL be lost** - The graph survives compaction, you don't
3. **Retroactive logging misses details** - Log in the moment or lose nuance
4. **Future sessions need this** - Your future self (or another session) will query this
5. **Public accountability** - The graph is published at the live URL

---

## Session Start Checklist

Every new session or after context recovery, run `/context` or:

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
```

## CLI Commands

| Command | Description |
|---------|-------------|
| `deciduous init` | Initialize deciduous in current directory |
| `deciduous add <type> "title"` | Add a node (goal/decision/option/action/outcome/observation) |
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
