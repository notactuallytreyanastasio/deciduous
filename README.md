# Deciduous

**Persistent decision memory for AI coding assistants.** When Claude's context compacts, your reasoning survives.

## The Problem

Claude Code loses context. Sessions end. Memory compacts. Six months later, nobody remembers *why* you chose approach A over approach B. The decisions that shaped your codebase evaporate.

## The Solution

Deciduous creates a persistent, queryable graph of every decision made during development. When a new Claude session starts—or when context compacts mid-session—Claude can query the graph to recover reasoning it never saw.

This isn't documentation written after the fact. It's a real-time record of *how* software gets built, captured as decisions happen.

---

## Quick Start

### 1. Install

```bash
cargo install deciduous
```

### 2. Initialize in your project

```bash
cd your-project
deciduous init
```

This creates:
- `.deciduous/deciduous.db` — SQLite database for the graph
- `.claude/commands/` — Slash commands for Claude Code
- `.windsurf/rules/` — Rules for Windsurf/Cascade
- `docs/` — Static web viewer (deployable to GitHub Pages)
- `CLAUDE.md` — Project instructions with the logging workflow

### 3. Start using with Claude

Tell Claude to use the decision graph, or just start working—the CLAUDE.md instructions will guide it.

---

## What It Does

**For context compaction:**
- Claude logs decisions to a SQLite database as it works
- When context compacts, the graph survives
- New sessions run `/context` to query past decisions
- Claude picks up where it left off, with full reasoning intact

**For long-term decision tracking:**
- Every goal, decision, action, and outcome is timestamped and linked
- Confidence scores (0-100) show certainty at decision time
- Edge types capture relationships: `chosen`, `rejected`, `requires`, `blocks`
- The graph is queryable: "What did we decide about auth?" "Why did we reject Redux?"

**For multi-user collaboration:**
- Export decision patches to share with teammates
- Import patches from others (idempotent—safe to apply multiple times)
- Each node has a globally unique ID for conflict-free merging

**For PR generation:**
- Generate decision graph visualizations for PRs
- Auto-generate writeups from the graph
- Show reviewers *why* you made the choices you made

---

## Usage Flow

### Session Start

Claude runs `/context` to recover past decisions:

```
> /context

Current branch: feature/auth
Decision graph shows:
- Goal #12: "Add user authentication" (confidence: 90)
  └─> Decision #13: "Choose auth method" (confidence: 75)
        ├─> Option #14: "JWT tokens" (chosen)
        └─> Option #15: "Session cookies" (rejected: "stateless preferred")

Last action: #18 "Implementing JWT refresh flow"
Status: in_progress
```

### During Work

Claude logs decisions in real-time:

```bash
# Starting a new feature
deciduous add goal "Add rate limiting" -c 90

# Making a choice
deciduous add decision "Choose rate limiter approach" -c 75
deciduous add option "Redis-based" -c 80
deciduous add option "In-memory with sliding window" -c 70

# Implementing
deciduous add action "Implementing Redis rate limiter" -c 85
deciduous link 21 23 --edge-type chosen -r "Scales across instances"

# Recording outcome
deciduous add outcome "Rate limiting working in prod" -c 95
deciduous link 23 24 -r "Implementation complete"
```

### Before Push

```bash
deciduous sync   # Export graph to JSON for the web viewer
```

### PR Time

```bash
# Generate branch-specific visualization
deciduous dot --auto --nodes 20-24

# Generate PR writeup
deciduous writeup --auto -t "Add rate limiting" --nodes 20-24
```

---

## Multi-User Sync

Share decisions across team members working on the same codebase.

### The Problem

Each user has a local `.deciduous/deciduous.db` (gitignored). How do you share decisions?

### The Solution

Export/import patches using globally unique change IDs (inspired by jj/Jujutsu):

```bash
# Export your branch's decisions
deciduous diff export --branch feature-x -o .deciduous/patches/alice-feature.json

# Apply patches from teammates (idempotent)
deciduous diff apply .deciduous/patches/*.json

# Preview before applying
deciduous diff apply --dry-run .deciduous/patches/bob.json

# Check available patches
deciduous diff status
```

### PR Workflow

1. Create nodes while working
2. Export: `deciduous diff export --branch my-feature -o .deciduous/patches/my-feature.json`
3. Commit the patch file (NOT the database)
4. Open PR with patch file included
5. Teammates apply: `deciduous diff apply .deciduous/patches/my-feature.json`

Same patch applied twice = no duplicates.

---

## Viewing the Graph

### Terminal UI (TUI)

```bash
deciduous tui
```

A rich, vim-style terminal interface for browsing your decision graph.

**Navigation:**
| Key | Action |
|-----|--------|
| `j`/`k` | Move down/up in timeline |
| `gg` | Jump to top |
| `G` | Jump to bottom |
| `Ctrl+d`/`Ctrl+u` | Page down/up |
| `Enter` | Toggle detail panel |
| `q` | Quit |

**Filtering & Search:**
| Key | Action |
|-----|--------|
| `/` | Search by title/description |
| `f` | Cycle through type filters |
| `b` | Cycle through branch filters |
| `B` | Fuzzy branch search |
| `R` | Toggle timeline order (newest/oldest first) |
| `Ctrl+c` | Clear all filters |

**File Operations:**
| Key | Action |
|-----|--------|
| `o` | Open associated files in editor |
| `O` | View commit details (split modal with diff) |
| `F` | Toggle file browser in detail panel |
| `n`/`N` | Next/previous file (when in file browser) |
| `p` | Preview file content with syntax highlighting |
| `d` | Show file diff with syntax highlighting |

**Other:**
| Key | Action |
|-----|--------|
| `s` | Show goal story (hierarchy view) |
| `r` | Refresh graph from database |
| `?` | Show help |

The TUI includes syntax highlighting for file previews and diffs, using the same highlighting engine as `bat`.

### Web Viewer

```bash
deciduous serve --port 3000
```

Or deploy to GitHub Pages (workflow included with `deciduous init`).

**Four visualization modes:**

| View | Purpose |
|------|---------|
| **Chains** | Decision chains with flow visualization |
| **Timeline** | Chronological view of all nodes |
| **Graph** | Force-directed interactive graph |
| **DAG** | Hierarchical directed acyclic graph |

### CLI Queries

```bash
deciduous nodes              # List all nodes
deciduous nodes -b feature-x # Filter by branch
deciduous edges              # List all connections
deciduous graph              # Full graph as JSON
deciduous commands           # Recent command history
```

---

## Node Types

| Type | Purpose | Example |
|------|---------|---------|
| `goal` | High-level objective | "Add user authentication" |
| `decision` | Choice point | "Choose auth method" |
| `option` | Approach considered | "Use JWT tokens" |
| `action` | Implementation step | "Added JWT middleware" |
| `outcome` | Result | "Auth working in prod" |
| `observation` | Discovery or insight | "Existing code uses sessions" |

## Edge Types

| Type | Meaning |
|------|---------|
| `leads_to` | Natural progression |
| `chosen` | Selected this option |
| `rejected` | Did not select (include reason) |
| `requires` | Dependency |
| `blocks` | Preventing progress |
| `enables` | Makes something possible |

## Confidence Scores

| Range | Meaning |
|-------|---------|
| 90-100 | Certain, proven, tested |
| 70-89 | High confidence, standard approach |
| 50-69 | Moderate, some unknowns |
| 30-49 | Experimental, might change |
| 0-29 | Speculative, likely to revisit |

---

## Commands Reference

```bash
# Initialize
deciduous init

# Add nodes
deciduous add goal "Title" -c 90
deciduous add decision "Title" -c 75
deciduous add option "Title" -c 80
deciduous add action "Title" -c 85
deciduous add outcome "Title" -c 95
deciduous add observation "Title" -c 70

# Optional metadata
deciduous add goal "Title" -c 90 -p "User prompt" -f "src/file.rs"
deciduous add goal "Title" -b feature-x    # Override branch
deciduous add goal "Title" --no-branch     # No branch tag

# Connect nodes
deciduous link 1 2 -r "Reason"
deciduous link 1 2 --edge-type chosen -r "Selected this"

# Query
deciduous nodes
deciduous nodes -b main                    # Filter by branch
deciduous edges
deciduous graph
deciduous commands

# Multi-user sync
deciduous diff export -o patch.json --branch feature-x
deciduous diff apply .deciduous/patches/*.json
deciduous diff apply --dry-run patch.json
deciduous diff status
deciduous migrate                          # Add change_id columns

# Visualize
deciduous serve
deciduous dot --png -o graph.dot
deciduous dot --auto --nodes 1-11

# Export
deciduous sync
deciduous writeup -t "Title" --nodes 1-11
deciduous backup
```

---

## Branch-Based Grouping

Nodes are automatically tagged with the current git branch.

**Configuration** (`.deciduous/config.toml`):
```toml
[branch]
main_branches = ["main", "master"]
auto_detect = true
```

**Usage:**
```bash
deciduous nodes --branch main        # Filter by branch
deciduous nodes -b feature-auth
deciduous add goal "Work" -b other   # Override auto-detection
deciduous add goal "Note" --no-branch # No branch tag
```

The web UI has a branch dropdown filter in the stats bar.

---

## GitHub Pages Deployment

`deciduous init` creates a GitHub workflow that:
1. Deploys your graph viewer to GitHub Pages on push
2. Cleans up branch-specific PNGs after PR merge

Enable Pages: **Settings > Pages > Source > Deploy from branch > `gh-pages`**

Your graph will be live at `https://<username>.github.io/<repo>/`

---

## Live Example

See a real decision graph: **[deciduous_example](https://notactuallytreyanastasio.github.io/deciduous_example/graph/)**

This shows 50+ decision nodes tracking a complete project with goals flowing through decisions to outcomes.

---

## Building from Source

```bash
git clone https://github.com/notactuallytreyanastasio/deciduous.git
cd deciduous
cargo build --release
./target/release/deciduous --help
```

**Optional dependency:** graphviz (for `--png` flag)
```bash
brew install graphviz    # macOS
apt install graphviz     # Ubuntu/Debian
```

---

## Why "Deciduous"?

Deciduous trees shed their leaves seasonally but their structure persists. Like Claude's context, the leaves (working memory) fall away—but the decision graph (trunk and branches) remains, ready to support new growth.

---

## License

MIT
