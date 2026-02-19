# Deciduous Architecture

> Decision graph tooling for AI-assisted development. Track every goal, decision, and outcome. Survive context loss. Query your reasoning.

## The Core Problem

AI assistants have **context limits**. When you work on a complex feature across multiple sessions:

1. **Context compaction** - The AI summarizes to fit limits, losing nuance
2. **Session boundaries** - New sessions start fresh with no memory
3. **Decision amnesia** - Why did we choose approach A over B? Lost.
4. **Pivot confusion** - We changed direction, but the old code remains. Why?

**Deciduous solves this by externalizing the AI's reasoning into a queryable graph.**

---

## System Goals

### 1. Survive Context Loss

When an AI session ends or context is compacted, all the reasoning is gone. Deciduous captures decisions **in real-time** so they persist beyond any single session.

```
Session 1: "Let's use JWT tokens" → logged to graph
Session 2: "What auth approach did we choose?" → query the graph
```

### 2. Track Design Evolution

Codebases evolve. Decisions get revisited. Deciduous captures **pivots** - when and why you changed direction:

```
[Old Decision: JWT] → [Observation: Too large for mobile] → [REVISIT] → [New Decision: Session cookies]
```

### 3. Enable Context Recovery

Start a new session with `/recover` and the AI can rebuild its understanding from the graph:

```bash
deciduous nodes        # What decisions exist?
deciduous edges        # How are they connected?
deciduous show 42      # What was the reasoning for node 42?
```

### 4. Enforce Discipline

Through hooks, Deciduous **blocks** the AI from making code changes without first logging what it's doing:

```
[AI tries to edit code]
  ↓
[Hook checks: Is there a recent action node?]
  ↓
[No? Block the edit. Force the AI to log first.]
```

---

## How It All Fits Together

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           USER + AI ASSISTANT                                │
│                                                                              │
│  User: "Add authentication"                                                 │
│  AI:   Creates goal node → deciduous add goal "Add auth" --prompt "..."     │
│  AI:   Logs decision → deciduous add decision "JWT vs sessions"             │
│  AI:   Links them → deciduous link 1 2                                      │
│  AI:   Makes code changes                                                   │
│  AI:   Logs outcome → deciduous add outcome "Auth implemented"              │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
                                     │
                                     ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                              HOOKS LAYER                                     │
│                                                                              │
│  Pre-Tool-Use Hooks:                                                        │
│  ├── require-action-node.sh                                                 │
│  │   "Before Edit/Write, verify recent action node exists"                  │
│  │   Blocks: "You must log what you're doing first!"                        │
│  │                                                                          │
│  Post-Tool-Use Hooks:                                                       │
│  └── post-commit-reminder.sh                                                │
│      "After Bash commit, remind to link commit to graph"                    │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
                                     │
                                     ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                            DECIDUOUS CLI                                     │
│                                                                              │
│  Commands:                                                                  │
│  ├── add <type> <title>   → Create nodes (goal/decision/action/outcome)     │
│  ├── link <from> <to>     → Connect nodes with rationale                    │
│  ├── nodes/edges/graph    → Query the graph                                 │
│  ├── doc attach/list/show → Document attachments on nodes                   │
│  ├── serve                → Start web viewer                                │
│  ├── sync                 → Export for GitHub Pages                         │
│  └── diff export/apply    → Multi-user sync                                 │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
                                     │
                                     ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                           SQLITE DATABASE                                    │
│                                                                              │
│  .deciduous/deciduous.db                                                    │
│  ├── decision_nodes      → All nodes with types, titles, metadata           │
│  ├── decision_edges      → Connections with rationale                       │
│  ├── node_documents      → File attachments with metadata                   │
│  ├── command_log         → CLI operation history                            │
│  └── roadmap_items       → ROADMAP.md sync state                            │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
                                     │
                    ┌────────────────┼────────────────┐
                    ▼                ▼                ▼
         ┌─────────────────┐  ┌───────────┐  ┌──────────────────┐
         │   WEB VIEWER    │  │    TUI    │  │  GITHUB PAGES    │
         │                 │  │           │  │                  │
         │ React app with  │  │ Ratatui   │  │ Static export    │
         │ multiple views: │  │ terminal  │  │ via sync         │
         │ - Archaeology   │  │ interface │  │                  │
         │ - DAG          │  │           │  │ docs/             │
         │ - Chains       │  │           │  │ ├── index.html    │
         │ - Timeline     │  │           │  │ ├── graph.json    │
         │ - Roadmap      │  │           │  │ └── git-history   │
         └─────────────────┘  └───────────┘  └──────────────────┘
```

---

## Node Types and Their Meaning

| Type | Shape | Purpose | Example |
|------|-------|---------|---------|
| **goal** | House | What you're trying to achieve | "Add user authentication" |
| **decision** | Diamond | A choice point with options | "How to identify users?" |
| **option** | Box | Possible choice for a decision | "Use JWT tokens" |
| **action** | Rounded box | Work being done | "Implementing auth middleware" |
| **outcome** | Ellipse | Result of work | "Auth working in staging" |
| **observation** | Note | Something noticed | "Sessions scale better for mobile" |
| **revisit** | Octagon | Reconsidering past decisions | "Rethinking token strategy" |

### The Revisit Pattern (Pivots)

When you change direction, the graph captures **why**:

```
[Decision: Use JWT]
       │
       ▼
[Option: JWT chosen] ──────► [Action: Implement JWT]
       │                              │
       ▼                              ▼
[Observation: JWT payloads         [Outcome: JWT works but
 too large for mobile]              mobile has issues]
       │                              │
       └──────────┬───────────────────┘
                  ▼
           [REVISIT: Reconsidering
            token strategy]
                  │
                  ▼
           [Decision: Session-based auth]
                  │
                  ▼
           [Option: Server sessions chosen]
```

---

## The Three Interfaces

### 1. CLI (`deciduous <command>`)

For AI assistants and power users. Fast, scriptable, integrates with hooks.

```bash
# Add a decision
deciduous add decision "How to structure the API?" -c 85

# Link it to a goal
deciduous link 1 2 -r "API design is part of auth goal"

# See the graph
deciduous nodes
deciduous edges
deciduous graph | jq '.nodes | length'
```

### 2. TUI (`deciduous tui`)

Interactive terminal interface for exploring the graph. Vim-style navigation.

- **Views**: Timeline, DAG, Roadmap
- **Features**: File picker, syntax highlighting, git diff viewer
- **Pattern**: Elm Architecture (TEA) - messages, update, render

### 3. Web Viewer (`deciduous serve` or GitHub Pages)

React application with multiple browsing modes:

| View | Purpose |
|------|---------|
| **Archaeology** | Default. Shows pivots and narratives - how design evolved |
| **DAG** | Directed graph visualization with hierarchical layout |
| **Chains** | Connected component chains rooted at goals |
| **Timeline** | Chronological view of nodes + git commits |
| **Story** | Goal-focused tree showing full decision tree from a goal |
| **Roadmap** | ROADMAP.md items synced with GitHub Issues |

---

## Multi-User Sync

The database is local (`.deciduous/deciduous.db` is gitignored). How do teammates share decisions?

### The Dual-ID Model (jj-inspired)

Every node has two IDs:
- **`id`** (integer): Local database primary key, different on each machine
- **`change_id`** (UUID): Globally unique, stable across all machines

### Patch Workflow

```bash
# Alice exports her branch's decisions
deciduous diff export --branch feature-auth -o .deciduous/patches/alice-auth.json

# Bob applies Alice's patch (idempotent - safe to re-apply)
deciduous diff apply .deciduous/patches/alice-auth.json

# Merge conflicts? Patches use change_id, not local IDs
```

**PR Workflow:**
1. Export patch file
2. Commit patch file to repo (NOT the database)
3. Open PR
4. Teammates pull and apply patches

---

## AI Assistant Integration

Deciduous supports multiple AI assistants:

| Assistant | Integration Directory | Config File |
|-----------|----------------------|-------------|
| Claude Code | `.claude/` | `CLAUDE.md` |
| OpenCode | `.opencode/` | `AGENTS.md` |
| Windsurf | `.windsurf/` | `rules/deciduous.md` |

### Hook Enforcement

```toml
# .deciduous/config.toml
[hooks]
enabled = true

[[hooks.pre_tool_use]]
name = "require-action-node"
matcher = "Edit|Write"
description = "Block code edits without recent action node"
enabled = true

[[hooks.post_tool_use]]
name = "post-commit-reminder"
matcher = "Bash"
description = "Remind to link commits to graph"
enabled = true
```

When the AI tries to edit code:
1. Hook script runs
2. Checks: Is there an action node in the last 15 minutes?
3. No? **Blocks the edit** with a message telling the AI to log first

---

## Key Data Flows

### Flow 1: Real-Time Decision Capture

```
User says "Add dark mode"
         │
         ▼
AI creates goal node:
deciduous add goal "Add dark mode" --prompt "Add dark mode..."
         │
         ▼
AI decides approach:
deciduous add decision "How to implement theming?" -c 85
deciduous link 1 2 -r "Theme decision for dark mode"
         │
         ▼
AI implements:
deciduous add action "Implementing CSS variables theme system"
[Code changes happen]
deciduous add outcome "Dark mode working" --commit HEAD
```

### Flow 2: Context Recovery

```
New session starts
         │
         ▼
User runs /recover (or AI reads CLAUDE.md instructions)
         │
         ▼
AI queries graph:
├── deciduous nodes --branch main
├── deciduous edges
└── git log --oneline -10
         │
         ▼
AI rebuilds understanding:
"Ah, we're working on auth. We chose JWT but there were mobile
 issues logged as observations. There's a revisit node suggesting
 we might switch to sessions."
```

### Flow 3: Graph Export for Visualization

```
deciduous serve --port 3000
         │
         ▼
HTTP server starts with embedded React app
         │
         ├── GET /api/graph        → Full graph JSON
         ├── GET /api/git-history  → Commit info
         └── GET /api/roadmap      → ROADMAP items
         │
         ▼
React app loads data
         │
         ├── buildChains()    → Find connected components
         ├── buildSessions()  → Group by time
         └── findPivots()     → Detect revisit patterns
         │
         ▼
User browses with multiple views
```

### Flow 4: Static Export for GitHub Pages

```
deciduous sync
         │
         ├── docs/graph-data.json     → Decision graph
         ├── docs/git-history.json    → Commit metadata
         ├── docs/roadmap-items.json  → Roadmap state
         └── docs/index.html          → Embedded viewer
         │
         ▼
git push → GitHub Pages serves static files
         │
         ▼
Anyone can browse: https://user.github.io/repo/
```

### Flow 5: Document Attachment

```
User: "Attach this diagram to the auth goal"
         │
         ▼
AI runs: deciduous doc attach 42 architecture.png -d "Auth architecture"
         │
         ▼
File hashed (SHA-256), copied to .deciduous/documents/
         │
         ▼
Record created in node_documents table
(change_id for sync, content_hash for dedup)
         │
         ├── Web viewer shows document in node detail panel
         ├── GET /api/documents?node_id=42 → document list
         └── GET /api/documents/file/1 → serve file content
```

---

## Source Code Organization

```
src/
├── main.rs           # CLI command dispatcher (clap)
├── lib.rs            # Public API exports
├── db.rs             # SQLite database (Diesel ORM)
├── schema.rs         # Database table definitions
├── config.rs         # .deciduous/config.toml loader
├── init/             # deciduous init
│   ├── mod.rs        # Project initialization
│   └── templates.rs  # File templates (CLAUDE.md, hooks, etc.)
├── serve.rs          # HTTP server for web viewer
├── export.rs         # DOT export, PR writeup generation
├── diff.rs           # Multi-user sync patches
├── hooks.rs          # Claude Code hook management
├── opencode.rs       # OpenCode integration
├── github.rs         # GitHub API client
├── roadmap.rs        # ROADMAP.md parsing and sync
├── changelog.rs      # Embedded release notes
└── tui/              # Terminal UI
    ├── mod.rs        # Event loop, file watcher
    ├── app.rs        # Application state
    ├── msg.rs        # TEA messages
    ├── update.rs     # State transitions
    ├── state.rs      # Pure state transformations
    ├── ui.rs         # Rendering
    ├── events.rs     # Keyboard/mouse handling
    └── views/        # Timeline, DAG, Roadmap views

web/                  # React/TypeScript web viewer
├── src/
│   ├── App.tsx       # Router setup, data loading
│   ├── types/        # TypeScript interfaces
│   ├── utils/        # Graph algorithms
│   │   ├── graphProcessing.ts      # Chain/session building
│   │   └── archaeologyProcessing.ts # Pivot detection
│   ├── hooks/        # React custom hooks
│   ├── components/   # Reusable UI components
│   └── views/        # Browsing mode views
└── vite.config.ts    # Build configuration
```

---

## Testing

```bash
# Run all tests
cargo test

# Integration tests use temporary databases
# See: tests/cli_integration.rs
```

Tests verify:
- CLI commands work correctly
- Database operations are consistent
- Graph queries return expected results
- Export formats are valid

---

## Development Workflow

```bash
# Build
cargo build --release

# Run
./target/release/deciduous <command>

# Web viewer development
cd web && npm run dev

# Rebuild embedded viewer after web changes
cd web && npm run build
cp dist/index.html ../src/viewer.html
cp dist/index.html ../docs/demo/index.html
cargo build --release
```

---

## Summary

Deciduous is **external memory for AI-assisted development**. It:

1. **Captures decisions in real-time** before context is lost
2. **Enforces discipline** through hooks that block unlogged work
3. **Enables recovery** by providing a queryable graph of past reasoning
4. **Tracks evolution** by capturing pivots when direction changes
5. **Enables collaboration** through patch-based multi-user sync
6. **Visualizes everything** through web and terminal interfaces

The system works because it's integrated at the workflow level - AI assistants are trained (via CLAUDE.md) to use deciduous commands, and hooks prevent them from skipping the logging step.
