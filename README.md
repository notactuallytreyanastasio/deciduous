# Deciduous

**Decision graph tooling for AI-assisted development.** Track every goal, decision, and outcome. Survive context loss. Query your reasoning.

[![Crates.io](https://img.shields.io/crates/v/deciduous.svg)](https://crates.io/crates/deciduous)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

---

## See It In Action

**[Browse the Live Decision Graph](https://notactuallytreyanastasio.github.io/deciduous/demo/)** — 1,100+ decisions from building deciduous itself

**[Interactive Tutorial](https://notactuallytreyanastasio.github.io/deciduous/tutorial/)** — Learn the workflow in 15 minutes

**[Watch the Demo](https://asciinema.org/a/761574)** — Full session walkthrough

---

## The Problem

You're building software with AI assistance. The LLM generates complex code fast. But then:

- **Sessions end.** Context compacts. The LLM loses memory of what was tried.
- **Decisions evaporate.** Six months later, no one remembers *why* you chose approach A over B.
- **PRs become incomprehensible.** A 50-file diff tells you *what* changed, not *why*.
- **Onboarding is archaeology.** New teammates reverse-engineer decisions from code.

The code tells you *what*. But decisions tell you *why*.

## The Solution

Deciduous creates a persistent, queryable graph of every decision made during development. Log decisions in real-time—as they happen—and they survive session boundaries, context compaction, and human memory.

```
1,174 nodes • 1,024 edges • Real development history from building this tool
```

Both you and your AI assistant can:
- **Query past reasoning** before making new decisions
- **See what was tried** and what was rejected
- **Trace any outcome** back to the goal that spawned it
- **Recover context** after sessions end or memory compacts

This isn't documentation written after the fact. It's a real-time record of *how* software gets built.

---

## Installation

### Pre-built Binaries (Recommended)

Download the latest release for your platform from [GitHub Releases](https://github.com/notactuallytreyanastasio/deciduous/releases):

| Platform | Binary |
|----------|--------|
| Linux (x86_64) | `deciduous-linux-amd64` |
| Linux (ARM64) | `deciduous-linux-arm64` |
| macOS (Intel) | `deciduous-darwin-amd64` |
| macOS (Apple Silicon) | `deciduous-darwin-arm64` |
| Windows | `deciduous-windows-amd64.exe` |

```bash
# Example: Linux/macOS
curl -LO https://github.com/notactuallytreyanastasio/deciduous/releases/latest/download/deciduous-darwin-arm64
chmod +x deciduous-darwin-arm64
sudo mv deciduous-darwin-arm64 /usr/local/bin/deciduous
```

### Via Cargo

```bash
cargo install deciduous
```

### From Source

```bash
git clone https://github.com/notactuallytreyanastasio/deciduous.git
cd deciduous
cargo build --release
# Binary at target/release/deciduous
```

---

## Quick Start

```bash
# Initialize in your project
cd your-project
deciduous init

# Start logging decisions
deciduous add goal "Add user authentication" -c 90
deciduous add decision "Choose auth method" -c 75
deciduous link 1 2 -r "Deciding implementation approach"

# View the graph
deciduous serve    # Web viewer at localhost:3000
```

That's it. Your first decision graph is live.

### Multi-Assistant Support

Deciduous integrates with multiple AI coding assistants:

```bash
# Claude Code (default)
deciduous init

# OpenCode
deciduous init --opencode

# Windsurf (Codeium)
deciduous init --windsurf

# Multiple assistants
deciduous init --both              # Claude Code + OpenCode
deciduous init --windsurf          # + Windsurf (auto-creates .windsurf/)
deciduous init --both --windsurf   # All three
```

| Assistant | Flag | Integration Files |
|-----------|------|-------------------|
| **Claude Code** | `--claude` (default) | `.claude/`, `CLAUDE.md` |
| **OpenCode** | `--opencode` | `.opencode/`, `AGENTS.md` |
| **Windsurf** | `--windsurf` | `.windsurf/hooks/`, `.windsurf/rules/` |

**Auto-detection:** `deciduous update` auto-detects which assistants are installed (`.claude/`, `.opencode/`, `.windsurf/`) and updates them all. Windsurf is also auto-detected during `init` if `.windsurf/` already exists.

---

## The Workflow

```
BEFORE you do something → Log what you're ABOUT to do
AFTER it succeeds/fails → Log the outcome
CONNECT immediately → Link every node to its parent
```

### Example Session

```bash
# Starting a new feature
deciduous add goal "Add rate limiting" -c 90 -p "User asked: add rate limiting to the API"

# Making a choice
deciduous add decision "Choose rate limiter approach" -c 75
deciduous link 1 2 -r "Deciding implementation"

# Considering options
deciduous add option "Redis-based distributed" -c 80
deciduous add option "In-memory sliding window" -c 70
deciduous link 2 3 -r "Option A"
deciduous link 2 4 -r "Option B"

# Implementing the chosen approach
deciduous add action "Implementing Redis rate limiter" -c 85
deciduous link 3 5 --edge-type chosen -r "Scales across instances"

# Recording the outcome
deciduous add outcome "Rate limiting working in prod" -c 95
deciduous link 5 6 -r "Implementation complete"

# Sync for GitHub Pages
deciduous sync
```

### Session Recovery

When context compacts or you start a new session:

```bash
deciduous nodes           # What decisions exist?
deciduous edges           # How are they connected?
deciduous commands        # What happened recently?
```

Or open the web viewer and ask a question in plain English:

> *"What was I working on before the session ended?"*
> *"What approach did we take for rate limiting and why?"*

The graph remembers what you don't. The Q&A interface lets you ask it.

---

## Three Skills: Pulse, Archaeology, and Narratives

Deciduous ships with three skills that give your AI assistant structured ways to understand a codebase. These aren't just CLI commands—they're workflows the agent follows to build and query the graph.

### /pulse — What decisions define this system right now?

Pulse maps the current architecture as a decision tree. No history, no evolution—just the design choices that make the system work today.

Your agent reads the code, identifies the design questions that had to be answered, and logs them:

```bash
deciduous add goal "API rate limiting behavior" -c 90
deciduous add decision "What identifies a user for rate limiting?" -c 85
deciduous link 1 2 -r "leads_to"
deciduous add decision "What happens when limit is exceeded?" -c 85
deciduous link 1 3 -r "leads_to"
deciduous add option "Return 429 with Retry-After header" -c 90
deciduous link 3 4 -r "resolved_by"
```

Output: A decision tree of the current model. Use this before making changes to understand what decisions you might affect.

### /archaeology — Turn narratives into a queryable graph

Archaeology takes the conceptual stories from `/narratives` and structures them as nodes and edges you can traverse. Every **PIVOT** in a narrative becomes a `revisit` node that connects the old approach to the new one, with `observation` nodes capturing *why* things changed.

```bash
# The pivot pattern: old approach → observation → revisit → new approach
deciduous add decision "JWT for all auth" -c 85 --date "2023-01-20"
deciduous add observation "Mobile Safari 4KB cookie limit breaking JWT auth"
deciduous link 2 3 -r "Discovered in production"
deciduous add revisit "Reconsidering auth token strategy"
deciduous link 3 4 -r "Cookie limits forced rethink"
deciduous status 2 superseded
deciduous add decision "Hybrid: JWT for API, sessions for web"
deciduous link 4 5 -r "New approach"
```

After archaeology, you can query: "What did we try before?" (`--status superseded`), "What led to this decision?" (`edges --to <id>`), "What are the pivot points?" (`--type revisit`).

### /narratives — Understand how the system evolved

Narratives are the conceptual stories—how a subsystem evolved over time, what pivots happened, and how different parts of the system connect.

```markdown
## Authentication
> How users prove identity.

**Current state:** JWT for API, sessions for web.

**Evolution:**
1. Started with JWT everywhere
2. **PIVOT:** Mobile hit 4KB cookie limits
3. Added sessions for web, kept JWT for API

**Connects to:** "Rate Limiting"
```

Output: `.deciduous/narratives.md` with evolution stories that archaeology can transform into graph structure.

---

## Deep Q&A Interface

The web viewer includes a built-in Q&A interface where you can ask questions about your decision graph and get answers grounded in your actual development history.

```
POST /api/ask
{
  "question": "Why did we switch from JWT to sessions for web auth?",
  "context": {
    "selected_node_id": 42,
    "branch": "main"
  }
}
```

The Q&A system:

- **Sends your question + graph context to Claude** — it sees the relevant nodes, edges, and narrative context
- **Archaeology-aware** — when asking from the archaeology view, the agent gets full narrative context including pivots, superseded approaches, and GitHub links
- **Stores every interaction** — questions and answers are saved with full-text search (FTS5), so you can search past Q&A sessions
- **Searchable history** — `GET /api/qa/search?q=auth` finds past conversations about authentication decisions

This turns the graph into a conversational interface. Instead of manually traversing nodes, ask: *"What was tried before the current approach?"* or *"What connects the auth system to rate limiting?"*

```bash
# Browse Q&A history
GET /api/qa?offset=0&limit=20

# Search past questions
GET /api/qa/search?q=rate+limiting&limit=10

# Get a specific interaction
GET /api/qa/42
```

---

## Viewing the Graph

### Web Viewer

```bash
deciduous serve --port 3000
```

Five views:

| View | Purpose |
|------|---------|
| **Chains** | Decision chains by session—see the story of a feature |
| **Timeline** | Chronological view merged with git commits |
| **Graph** | Force-directed interactive visualization |
| **DAG** | Hierarchical goal→decision→outcome flow |
| **Archaeology** | Narrative-driven exploration with Q&A |

Features: branch filtering, node search, click-to-expand details, Q&A panel, auto-refresh.

---

## Node Types

| Type | Purpose | Example |
|------|---------|---------|
| `goal` | High-level objective | "Add user authentication" |
| `decision` | Choice point | "Choose auth method" |
| `option` | Approach considered | "Use JWT tokens" |
| `action` | Implementation step | "Added JWT middleware" |
| `outcome` | Result | "Auth working in prod" |
| `observation` | Discovery or insight | "JWT tokens too large for mobile" |
| `revisit` | Pivot point—connects old approach to new | "Reconsidering token strategy" |

## Node Status

| Status | Meaning |
|--------|---------|
| `active` | Current truth—how things work today |
| `superseded` | Replaced by a newer approach |
| `abandoned` | Tried and rejected—dead end |

```bash
deciduous status <node_id> superseded
deciduous nodes --status active    # Now mode
deciduous nodes --status superseded # What was tried
```

## Edge Types

| Type | Meaning |
|------|---------|
| `leads_to` | Natural progression |
| `chosen` | Selected this option |
| `rejected` | Did not select (with reason) |
| `requires` | Dependency |
| `blocks` | Preventing progress |
| `enables` | Makes possible |
| `supersedes` | New approach replaces old (via revisit) |

---

## Graph Maintenance

Made a mistake? Fix it:

```bash
# Remove an edge
deciduous unlink 5 12

# Delete a node (cascades to connected edges)
deciduous delete 42

# Preview before deleting
deciduous delete 42 --dry-run
```

---

## Multi-User Sync

Share decisions across teammates. Each node has a globally unique `change_id` (UUID):

```bash
# Export your branch's decisions
deciduous diff export --branch feature-x -o .deciduous/patches/my-feature.json

# Apply patches from teammates (idempotent)
deciduous diff apply .deciduous/patches/*.json

# Preview before applying
deciduous diff apply --dry-run .deciduous/patches/teammate.json
```

### PR Workflow

1. Create nodes while working
2. Export: `deciduous diff export -o .deciduous/patches/my-feature.json`
3. Commit the patch file (not the database)
4. PR includes the patch; teammates apply after merge

---

## GitHub Pages Deployment

`deciduous init` creates workflows that deploy your graph viewer automatically:

```bash
deciduous sync    # Export to docs/graph-data.json
git add docs/
git push
```

Enable Pages: **Settings > Pages > Source > `gh-pages` branch**

Your graph is live at `https://<user>.github.io/<repo>/`

---

## Keeping AI Integration Updated

When deciduous releases new features, your existing projects can get the latest integration files:

```bash
# Check if an update is needed
deciduous check-update

# Update integration files (auto-detects installed assistants)
deciduous update
```

The `update` command auto-detects which assistants are installed and updates them:

### Claude Code (`.claude/`)

| Files | What's Updated |
|-------|----------------|
| `.claude/commands/*.md` | Slash commands (`/decision`, `/recover`, `/work`, `/document`, `/build-test`, `/serve-ui`, `/sync-graph`, `/decision-graph`, `/sync`) |
| `.claude/skills/*.md` | Skills (`/pulse`, `/narratives`, `/archaeology`) |
| `.claude/hooks/*.sh` | Enforcement hooks |
| `.claude/agents.toml` | Subagent configurations |
| `CLAUDE.md` | Decision Graph Workflow section (preserves custom content) |

### OpenCode (`.opencode/`)

| Files | What's Updated |
|-------|----------------|
| `.opencode/plugins/*.ts` | TypeScript hooks (pre-edit, post-commit) |
| `.opencode/opencode.json` | Plugin configuration |
| `AGENTS.md` | Decision Graph Workflow section |

### Windsurf (`.windsurf/`)

| Files | What's Updated |
|-------|----------------|
| `.windsurf/hooks.json` | Cascade hooks configuration |
| `.windsurf/hooks/*.sh` | Hook scripts (pre-write, post-command) |
| `.windsurf/rules/deciduous.md` | Always-on rules for Cascade |

**Not touched:** Settings files, `.deciduous/config.toml`, `docs/` - your configs stay intact.

### Automatic Version Checking

The `check-update` command compares `.deciduous/.version` with the binary version:

```bash
$ deciduous check-update
Update available: Integration files are v0.9.4, binary is v0.9.5. Run 'deciduous update'.
```

Add this to your session start routine to catch updates automatically.

---

## How the Hooks Work

Each AI assistant integration includes hooks that enforce the decision graph workflow:

### Pre-Edit Hook (Blocks edits without context)

Before the AI can edit files, it must have logged a recent goal or action node (within 15 minutes). This ensures decisions are captured *before* code is written.

```
AI tries to edit → Hook checks for recent node → Blocks if missing → AI logs decision → Edit proceeds
```

### Post-Commit Hook (Reminds to link commits)

After any `git commit`, the AI is reminded to:
1. Create an outcome or action node with `--commit HEAD`
2. Link it to the parent goal/action

This connects your git history to the decision graph.

### Assistant-Specific Implementation

| Assistant | Pre-Edit Hook | Post-Commit Hook |
|-----------|---------------|------------------|
| **Claude Code** | `PreToolUse` on `Edit\|Write` | `PostToolUse` on `Bash` (git commit) |
| **OpenCode** | TypeScript plugin `pre-edit` | TypeScript plugin `post-commit` |
| **Windsurf** | Cascade `pre_write_code` | Cascade `post_run_command` |

All hooks use **exit code 2** to block actions and provide guidance to the AI.

---

## The Premises

1. **Decisions are the unit of institutional knowledge.** Code tells you *what*, but decisions tell you *why*. Six months from now, you won't remember why you chose Redis over Postgres for that cache. The graph will.

2. **Structured thinking produces better outcomes.** The act of logging a decision—naming it, assigning confidence, connecting it to goals—forces you to think it through.

3. **Real-time logging beats retroactive documentation.** Capture reasoning in the moment. By the time you write post-hoc docs, you've forgotten the options you rejected.

4. **Graphs beat documents.** Goals spawn decisions, decisions spawn actions, actions produce outcomes. A graph captures these relationships. You can trace any outcome to its origin.

5. **Complex PRs tell a story.** A 50-file diff is incomprehensible. A decision graph shows the goal, the key decisions, the rejected approaches, and how each change connects to purpose.

6. **Context loss is inevitable.** Sessions end. Memory compacts. The graph survives.

7. **The graph is a shared workspace.** Decisions flow between sessions, between humans and AI, between teammates. The graph doesn't care who's typing—it preserves the reasoning.

---

## Commands Reference

```bash
# Initialize
deciduous init               # Initialize with Claude Code (default)
deciduous init --opencode    # Initialize with OpenCode
deciduous init --windsurf    # Initialize with Windsurf
deciduous init --both        # Initialize with Claude Code + OpenCode
deciduous init --both --windsurf  # All three assistants
deciduous update             # Update tooling (auto-detects installed assistants)
deciduous check-update       # Check if update is needed

# Add nodes
deciduous add goal "Title" -c 90
deciduous add decision "Title" -c 75
deciduous add action "Title" -c 85 --commit HEAD  # Link to git commit

# Node options
-c, --confidence <0-100>     # Confidence level
-p, --prompt "..."           # User prompt that triggered this
--prompt-stdin               # Read prompt from stdin (multi-line)
-f, --files "a.rs,b.rs"      # Associated files
--commit <hash|HEAD>         # Link to git commit
--date "YYYY-MM-DD"          # Backdate node (for archaeology)

# Connect and disconnect
deciduous link <from> <to> -r "reason"
deciduous unlink <from> <to>

# Delete nodes
deciduous delete <id>
deciduous delete <id> --dry-run

# Query
deciduous nodes              # List all nodes
deciduous nodes -b main      # Filter by branch
deciduous edges              # List connections
deciduous graph              # Full graph as JSON

# Visualize
deciduous serve              # Web viewer
deciduous dot --png          # Generate PNG (requires graphviz)

# Export
deciduous sync               # Export to docs/
deciduous writeup -t "Title" # Generate PR writeup
deciduous backup             # Create database backup

# Multi-user sync
deciduous diff export -o patch.json
deciduous diff apply patches/*.json
deciduous migrate            # Add change_id columns for sync

# Shell completion
deciduous completion zsh     # Add: source <(deciduous completion zsh)
deciduous completion bash
deciduous completion fish
```

---

## Who Uses Deciduous

**You, the developer:**
- Think through decisions by structuring them
- Remember why you made choices months later
- Review PRs by understanding the decision flow
- Onboard to codebases by reading decision history
- Ask questions about your own project's history and get grounded answers

**Your AI assistant:**
- Recover context after compaction or session boundaries
- Build on previous reasoning instead of starting fresh
- Leave a queryable trail for future sessions
- Use `/pulse` to map current architecture before making changes
- Use `/archaeology` to understand why things are the way they are
- Use `/document` to generate comprehensive docs with test examples
- Use `/decision-graph` to build decision graphs from commit history
- Use `/sync` to synchronize decision graphs across teammates
- Ask deep questions via the Q&A interface grounded in actual graph data

**Your team:**
- Share decision context via patch files
- Review PRs with full visibility into reasoning
- Build institutional knowledge that survives turnover
- Search past Q&A interactions to find answers that were already given

---

## Why "deciduous"?

It almost has the word "decision" in it, and they're trees.

---

**[Tutorial](https://notactuallytreyanastasio.github.io/deciduous/tutorial/)** · **[Live Demo](https://notactuallytreyanastasio.github.io/deciduous/demo/)** · **[GitHub](https://github.com/notactuallytreyanastasio/deciduous)**
