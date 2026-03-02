# Deciduous

**Decision graph tooling for AI-assisted development.** Track every goal, decision, and outcome. Survive context loss. Query your reasoning.

[![Crates.io](https://img.shields.io/crates/v/deciduous.svg)](https://crates.io/crates/deciduous)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)

**[Live Demo](https://notactuallytreyanastasio.github.io/deciduous/demo/)** (1,100+ decisions from building deciduous itself) · **[Interactive Tutorial](https://notactuallytreyanastasio.github.io/deciduous/tutorial/)** · **[Watch the Demo](https://asciinema.org/a/761574)**

---

## Why Deciduous?

You're building software with AI assistance. The LLM generates complex code fast. But then:

- **Sessions end.** Context compacts. The LLM forgets what was tried.
- **Decisions evaporate.** Six months later, no one remembers *why* approach A beat approach B.
- **PRs become opaque.** A 50-file diff tells you *what* changed, not *why*.
- **Onboarding is archaeology.** New teammates reverse-engineer decisions from code.

Deciduous creates a persistent, queryable graph of every decision made during development. Log decisions in real-time and they survive session boundaries, context compaction, and human memory.

Both you and your AI assistant can query past reasoning, see what was tried and rejected, trace any outcome back to the goal that spawned it, and recover full context after sessions end.

This isn't documentation written after the fact. It's a real-time record of *how* software gets built.

---

## Installation

### Homebrew

```bash
brew install notactuallytreyanastasio/tap/deciduous
```

### Pre-built Binaries

Download from [GitHub Releases](https://github.com/notactuallytreyanastasio/deciduous/releases):

| Platform | Binary |
|----------|--------|
| Linux (x86_64) | `deciduous-linux-amd64` |
| Linux (ARM64) | `deciduous-linux-arm64` |
| macOS (Intel) | `deciduous-darwin-amd64` |
| macOS (Apple Silicon) | `deciduous-darwin-arm64` |
| Windows | `deciduous-windows-amd64.exe` |

```bash
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

# Log a goal
deciduous add goal "Add user authentication" -c 90

# Explore options
deciduous add option "JWT tokens" -c 80
deciduous add option "Session cookies" -c 75
deciduous link 1 2 -r "Possible approach"
deciduous link 1 3 -r "Possible approach"

# Make a decision
deciduous add decision "Use JWT for API, sessions for web" -c 85
deciduous link 2 4 -r "Chosen approach"
deciduous link 3 4 -r "Also incorporated"

# View the graph
deciduous serve    # Web viewer at localhost:3000
```

### The Canonical Flow

Every decision follows this path through the graph:

```
goal → options → decision → actions → outcomes
```

- **Goals** lead to **options** (approaches to explore)
- **Options** lead to a **decision** (choosing which to pursue)
- **Decisions** lead to **actions** (implementing the chosen approach)
- **Actions** lead to **outcomes** (results of the implementation)
- **Observations** attach anywhere relevant
- **Revisits** connect old approaches to new ones when you pivot

---

## AI Assistant Integration

Deciduous works with multiple AI coding assistants. Each gets slash commands, enforcement hooks, and skills that make decision logging part of the natural workflow.

```bash
# Claude Code (default)
deciduous init

# OpenCode
deciduous init --opencode

# Windsurf (Codeium)
deciduous init --windsurf

# Multiple assistants at once
deciduous init --both              # Claude Code + OpenCode
deciduous init --both --windsurf   # All three
```

| Assistant | Flag | What Gets Created |
|-----------|------|-------------------|
| **Claude Code** | `--claude` (default) | `.claude/commands/`, `.claude/skills/`, `CLAUDE.md` |
| **OpenCode** | `--opencode` | `.opencode/plugins/`, `.opencode/commands/`, `AGENTS.md` |
| **Windsurf** | `--windsurf` | `.windsurf/hooks/`, `.windsurf/rules/` |

### What the Integration Gives Your AI

**Slash commands** — `/decision`, `/recover`, `/work`, `/document`, `/build-test`, `/serve-ui`, `/sync-graph`, `/decision-graph`, `/sync`

**Skills** — `/pulse` (map current architecture), `/narratives` (understand evolution), `/archaeology` (build queryable graph from history)

**Enforcement hooks** — Pre-edit hooks block code changes unless a recent goal or action node exists. Post-commit hooks remind the AI to link commits to the graph. This ensures decisions are captured *before* code is written, not after.

### Keeping Integration Updated

```bash
deciduous check-update    # Check if integration files need updating
deciduous update          # Update all detected assistants automatically
```

`update` auto-detects which assistants are installed and refreshes their commands, skills, hooks, and workflow sections — without touching your settings or configs.

---

## The Web Viewer

```bash
deciduous serve --port 3000
```

Five views for different ways of exploring your decision history:

| View | Purpose |
|------|---------|
| **Chains** | Decision chains by session — see the story of a feature |
| **Timeline** | Chronological view merged with git commits |
| **Graph** | Force-directed interactive visualization |
| **DAG** | Hierarchical goal → decision → outcome flowchart |
| **Archaeology** | Narrative-driven exploration with Q&A |

Features: branch filtering, full-text search with type filters, resizable panels, deep linking, keyboard navigation (j/k/g/G), and auto-refresh.

### Q&A Interface

The web viewer includes a built-in Q&A panel where you can ask questions about your decision graph in plain English:

> *"What was I working on before the session ended?"*
> *"Why did we switch from JWT to sessions for web auth?"*
> *"What connects the auth system to rate limiting?"*

Questions and answers are stored with full-text search (FTS5), so you can search past Q&A sessions later.

---

## Three Skills

Deciduous ships with three skills that give your AI assistant structured workflows for understanding a codebase.

### /pulse — Map the current design

Pulse maps the architecture *right now* as a decision tree. No history, no evolution — just the design choices that make the system work today. Use this before making changes to understand what decisions you might affect.

### /narratives — Understand how it evolved

Narratives are conceptual stories: how a subsystem evolved, what pivots happened, how different parts connect. Your AI looks at the current system, asks "how did this get this way?", and writes evolution stories to `.deciduous/narratives.md`.

### /archaeology — Turn history into a queryable graph

Archaeology takes narratives and structures them as nodes and edges. Every **pivot** becomes a `revisit` node connecting the old approach to the new one, with `observation` nodes capturing *why* things changed. After archaeology, you can query: *"What did we try before?"*, *"What led to this decision?"*, *"What are the pivot points?"*

---

## Document Attachments

Attach files to any decision node — architecture diagrams, specs, screenshots, PDFs.

```bash
deciduous doc attach 1 docs/architecture.png -d "System architecture diagram"
deciduous doc attach 1 screenshot.png --ai-describe    # AI-generated description
deciduous doc list 1                                    # What's attached?
deciduous doc open 3                                    # Open in default app
deciduous doc detach 3                                  # Soft-delete (recoverable)
deciduous doc gc --dry-run                              # Preview orphan cleanup
```

Documents are stored in `.deciduous/documents/` with content-hash naming for deduplication. The web viewer displays them in the node detail panel.

---

## Building Decision Graphs from History

The `/decision-graph` skill builds a full decision graph from your repository's commit history, working in four layers:

1. **Commit analysis** — Groups commits into logical narratives
2. **Code structure** — Identifies architectural decisions from the codebase
3. **Narrative construction** — Builds evolution stories with pivots and connections
4. **PR context** — Uses `gh` CLI to mine PR descriptions and review threads for decision rationale, alternatives considered, and trade-offs discussed

This is how you bootstrap a decision graph for an existing project that wasn't using deciduous from the start.

---

## Multi-User Sync

Share decisions across teammates. Each node has a globally unique `change_id` (UUID), so patches are idempotent:

```bash
# Export your branch's decisions
deciduous diff export --branch feature-x -o .deciduous/patches/my-feature.json

# Apply patches from teammates (safe to re-apply)
deciduous diff apply .deciduous/patches/*.json

# Preview what would change
deciduous diff apply --dry-run .deciduous/patches/teammate.json
```

**PR workflow:** Create nodes while working, export a patch, commit it with your PR. Teammates pull and apply after merge.

---

## Node Types and Statuses

| Type | Purpose | Example |
|------|---------|---------|
| `goal` | High-level objective | "Add user authentication" |
| `option` | Approach considered | "Use JWT tokens" |
| `decision` | Choice point | "Choose auth method" |
| `action` | Implementation step | "Added JWT middleware" |
| `outcome` | Result | "Auth working in prod" |
| `observation` | Discovery or insight | "JWT tokens too large for mobile" |
| `revisit` | Pivot point — old approach → new | "Reconsidering token strategy" |

| Status | Meaning |
|--------|---------|
| `active` | Current truth |
| `superseded` | Replaced by a newer approach |
| `abandoned` | Tried and rejected |

---

## GitHub Pages Deployment

Deploy your decision graph as a static site:

```bash
deciduous sync    # Export to docs/graph-data.json + docs/git-history.json
git add docs/
git push
```

Enable Pages in **Settings > Pages > Source > `gh-pages` branch**, and your graph is live at `https://<user>.github.io/<repo>/`.

`deciduous init` also creates a GitHub Action that automatically cleans up accumulated decision graph PNG/DOT files after PRs merge.

---

## Commands Reference

```bash
# Initialize & update
deciduous init [--opencode] [--windsurf] [--both]
deciduous update                    # Update integration files
deciduous check-update              # Check if update needed

# Add nodes
deciduous add <type> "Title" [-c confidence] [-p "prompt"] [-f "files"]
deciduous add action "Title" --commit HEAD    # Link to git commit
deciduous add goal "Title" --prompt-stdin     # Read prompt from stdin

# Connect, disconnect, delete
deciduous link <from> <to> -r "reason"
deciduous unlink <from> <to>
deciduous delete <id> [--dry-run]

# Update status
deciduous status <id> active|superseded|abandoned

# Query
deciduous nodes [-b branch] [--status active] [--type goal]
deciduous edges [--to <id>] [--from <id>]
deciduous graph                     # Full graph as JSON
deciduous commands                  # Recent command log

# Visualize
deciduous serve [--port 3000]       # Web viewer
deciduous dot [--png] [-o file]     # DOT/PNG export (requires graphviz)

# Export & sync
deciduous sync                      # Export to docs/ for GitHub Pages
deciduous writeup -t "Title" [-n 1-15]    # Generate PR writeup
deciduous backup                    # Database backup

# Multi-user sync
deciduous diff export [-b branch] -o patch.json
deciduous diff apply patches/*.json [--dry-run]
deciduous diff status
deciduous migrate                   # Add change_id columns

# Document attachments
deciduous doc attach <node> <file> [-d "desc"] [--ai-describe]
deciduous doc list [node]
deciduous doc show|open|detach|describe <id>
deciduous doc gc [--dry-run]

# Shell completion
deciduous completion bash|zsh|fish
```

---

## The Premises

1. **Decisions are institutional knowledge.** Code tells you *what*; the graph tells you *why*.
2. **Structured thinking produces better outcomes.** Naming a decision, assigning confidence, connecting it to goals — that forces you to think it through.
3. **Real-time beats retroactive.** By the time you write post-hoc docs, you've forgotten the options you rejected.
4. **Graphs beat documents.** Goals spawn decisions, decisions spawn actions, actions produce outcomes. A graph captures these relationships naturally.
5. **Context loss is inevitable.** Sessions end. Memory compacts. The graph survives.

---

## Why "deciduous"?

It almost has the word "decision" in it, and they're trees.

---

**[Tutorial](https://notactuallytreyanastasio.github.io/deciduous/tutorial/)** · **[Live Demo](https://notactuallytreyanastasio.github.io/deciduous/demo/)** · **[GitHub](https://github.com/notactuallytreyanastasio/deciduous)**
