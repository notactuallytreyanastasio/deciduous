# Deciduous - Decision Graph Tooling

Decision graph tooling for AI-assisted development. Track every goal, decision, and outcome. Survive context loss. Query your reasoning.

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
| `tooling` | Editor integrations | `.claude/`, `.windsurf/`, `src/init.rs` |
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
├── export.rs            # DOT export and PR writeup generation
├── interceptor.rs       # Embedded JS interceptor for API tracing
└── tui/
    ├── app.rs           # TUI application state
    ├── views/
    │   ├── trace.rs     # Trace session/span viewer
    │   └── roadmap.rs   # Roadmap view
    └── ...

web/                     # React/TypeScript web viewer source
├── src/
│   ├── views/
│   │   └── TraceView.tsx       # Trace sessions/spans UI
│   ├── types/
│   │   ├── graph.ts            # TypeScript types for graph data
│   │   └── trace.ts            # TypeScript types for traces
│   └── utils/
│       └── graphProcessing.ts  # Chain building, session grouping
└── dist/                       # Built output (singlefile HTML)

trace-interceptor/       # Node.js API interceptor (bundled into binary)
├── src/
│   ├── index.ts         # Fetch interception logic
│   ├── deciduous.ts     # CLI client for recording spans
│   └── stream-parser.ts # SSE response parsing
└── dist/bundle.js       # Bundled output (embedded in Rust)
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
| `deciduous tui` | Interactive terminal UI |
| `deciduous dot` | Export graph as DOT format |
| `deciduous writeup` | Generate PR writeup markdown |
| `deciduous diff export` | Export nodes as a shareable patch |
| `deciduous diff apply` | Apply patches from teammates |
| `deciduous diff status` | List available patches |
| `deciduous migrate` | Add change_id columns for sync |
| `deciduous proxy -- <cmd>` | Run command with API trace capture |
| `deciduous trace sessions` | List trace sessions |
| `deciduous trace spans <id>` | List spans in a session |
| `deciduous trace link <sid> <nid>` | Link session to node |
| `deciduous trace prune` | Delete old trace data |

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

## API Trace Capture

Capture and analyze Claude API traffic to understand token usage, model selection, and correlate API calls with decision nodes.

### Quick Start

```bash
# Run Claude through the deciduous proxy
deciduous proxy -- claude

# View traces in TUI
deciduous tui   # Press 't' for Trace view

# View traces in web viewer
deciduous serve  # Click "Traces" tab
```

### How It Works

The `deciduous proxy` command:
1. Starts a trace session
2. Injects a JavaScript interceptor via `NODE_OPTIONS`
3. Captures all Anthropic API requests/responses
4. Records token usage, model, duration, thinking/response content
5. Updates session totals in real-time

### Trace Commands

```bash
# Start proxy (recommended way to capture traces)
deciduous proxy -- claude
deciduous proxy -- claude --dangerously-skip-permissions

# Manual trace management
deciduous trace sessions              # List all sessions
deciduous trace spans <session_id>    # List spans in a session
deciduous trace show <span_id>        # Show full span content
deciduous trace link <session_id> <node_id>    # Link session to decision node
deciduous trace unlink <session_id>   # Unlink session
deciduous trace prune --days 30       # Delete old traces (keeps linked)
```

### TUI Trace View

Press `t` in the TUI to access the Trace view:

| Key | Action |
|-----|--------|
| `j/k` | Navigate sessions/spans |
| `Enter` | Expand session → spans → detail |
| `Esc` | Go back |
| `Tab` | Switch detail tabs (Thinking/Response/Tools) |
| `l` | Link session to a node |
| `u` | Unlink session |
| `r` | Refresh |

### Web Trace View

The web viewer's "Traces" tab shows:
- Session list with token totals and duration
- Span list when session expanded
- Detail panel with thinking, response, and tool content

### Data Stored

**Sessions** track:
- Session ID, start/end time, duration
- Working directory, git branch
- Total tokens (input/output/cache read/cache write)
- Linked decision node (optional)

**Spans** track per-API-call:
- Sequence number, model, duration
- Token counts, cache stats
- Stop reason, tool names used
- Previews of user message, thinking, response

**Content** stores full text:
- Complete thinking blocks
- Full response text
- Tool inputs and outputs

### Linking Traces to Decisions

Link a trace session to a decision node to correlate API activity with your decision graph:

```bash
# After a session, link it to the goal you were working on
deciduous trace link abc123 42

# Or use TUI: navigate to session, press 'l', select node
```

Linked sessions show a green badge in the UI and are preserved during pruning.

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
