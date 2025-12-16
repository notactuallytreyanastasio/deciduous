# Deciduous

**Decision graphs for AI-assisted development.** Track goals, decisions, and outcomes. Survive context loss. Query your reasoning.

```
goal "Add auth" → decision "JWT vs sessions" → action "Implement JWT" → outcome "Auth working"
     ↑                    ↑                           ↑                        ↑
   why we started    what we chose              what we did             what happened
```

**[Live Demo](https://notactuallytreyanastasio.github.io/deciduous/demo/)** — 600+ decisions from building deciduous itself

---

## The Problem

LLMs write code fast. But sessions end, context compacts, and decisions evaporate.

Six months later, nobody—human or AI—remembers *why* you chose approach A over B.

## The Solution

Deciduous creates a persistent graph of every decision. Query past reasoning. See what was tried. Understand what got rejected and why.

Not documentation after the fact. A real-time record of *how* software gets built.

---

## Install

```bash
cargo install deciduous
```

## Quick Start

```bash
# Initialize in your project
deciduous init

# Log decisions as you work
deciduous add goal "Add rate limiting" -c 90
deciduous add decision "Redis vs in-memory" -c 75
deciduous link 1 2 -r "Choosing approach"

# View the graph
deciduous serve    # Web UI at localhost:8080
deciduous tui      # Terminal UI
```

That's it. The graph persists across sessions.

---

## Core Concepts

**Six node types:**

| Type | What it captures |
|------|------------------|
| `goal` | What you're trying to achieve |
| `decision` | A choice point |
| `option` | An approach you considered |
| `action` | Something you did |
| `outcome` | What happened |
| `observation` | Something you noticed |

**Nodes connect via edges.** Goals spawn decisions. Decisions have options. Options become actions. Actions produce outcomes.

```bash
deciduous add goal "Build search" -c 90
deciduous add decision "Elasticsearch vs Postgres FTS" -c 70
deciduous add option "Elasticsearch" -c 60
deciduous add option "Postgres full-text" -c 80
deciduous link 1 2 -r "Need to decide on search backend"
deciduous link 2 4 --edge-type chosen -r "Simpler ops, good enough for our scale"
deciduous link 2 3 --edge-type rejected -r "Overkill for current needs"
```

---

## Viewing the Graph

### Web Viewer

```bash
deciduous serve
```

Four visualization modes:
- **Chains** — Decision chains grouped by session
- **Timeline** — Chronological view merged with git commits
- **Graph** — Force-directed interactive visualization
- **DAG** — Hierarchical goal→decision→outcome flow

Deploy to GitHub Pages: `deciduous sync && git push`

### Terminal UI

```bash
deciduous tui
```

Vim-style navigation (`j`/`k`, `gg`/`G`), search (`/`), type filtering (`f`), branch filtering (`b`). Press `?` for help.

---

## AI Integration

Deciduous shines with AI coding assistants. Add the workflow to your project instructions:

```bash
deciduous init              # Claude Code (creates CLAUDE.md)
deciduous init --windsurf   # Windsurf/Cascade
```

The AI logs decisions in real-time. When context resets, it queries the graph to recover:

```bash
deciduous nodes             # What decisions exist?
deciduous edges             # How are they connected?
```

---

## Team Sync

Share decisions across teammates via patch files:

```bash
# Export your branch's decisions
deciduous diff export --branch feature-x -o .deciduous/patches/my-work.json

# Apply patches from teammates (idempotent)
deciduous diff apply .deciduous/patches/*.json
```

Each node has a stable UUID (`change_id`) that survives export/import.

---

## Commands

```bash
# Add nodes
deciduous add <type> "title" [-c confidence] [-p "prompt"] [-f "files"] [--commit HEAD]

# Connect nodes
deciduous link <from> <to> -r "reason" [--edge-type chosen|rejected|requires|blocks|enables]

# Query
deciduous nodes [-b branch]
deciduous edges
deciduous graph             # Full JSON export

# Visualize
deciduous serve             # Web UI
deciduous tui               # Terminal UI
deciduous dot --png         # PNG graph (requires graphviz)

# Export
deciduous sync              # Export for GitHub Pages
deciduous writeup -t "PR"   # Generate PR description
deciduous backup            # Backup database

# Shell completion
deciduous completion zsh|bash|fish|powershell
```

---

## Building from Source

```bash
git clone https://github.com/notactuallytreyanastasio/deciduous.git
cd deciduous
cargo build --release
```

macOS may need: `brew install libiconv && export LIBRARY_PATH="/opt/homebrew/opt/libiconv/lib:$LIBRARY_PATH"`

---

## Why "deciduous"?

It almost has "decision" in it. And they're trees.
