# Deciduous

**Decision graph tooling for AI-assisted development.** Track every goal, decision, and outcome. Survive context loss. Query your reasoning.

---

## See It In Action

**[Browse the Live Decision Graph](https://notactuallytreyanastasio.github.io/deciduous/demo/)** — 340+ decisions from building deciduous itself

**[Watch the Demo](https://asciinema.org/a/761574)** — Full session: initialization, decision logging, graph visualization, context recovery

---

## Why Deciduous?

LLMs generate complex code fast. Reviewing it, understanding it, and maintaining it? That's on you.

**The problem:** Sessions end. Memory compacts. Decisions evaporate. Six months later, no one—human or AI—remembers *why* you chose approach A over approach B.

**The solution:** Deciduous creates a persistent, queryable graph of every decision made during development. Both you and your AI assistant can query past reasoning, see what was tried, understand what was rejected and why.

This isn't documentation written after the fact. It's a real-time record of *how* software gets built, captured as decisions happen—by whoever is making them.

---

## The Premises

1. **Decisions are the unit of institutional knowledge.** Code tells you *what*, but decisions tell you *why*. Six months from now, you won't remember why you chose Redis over Postgres for that cache. The graph will.

2. **Structured thinking produces better outcomes.** The act of logging a decision—naming it, assigning confidence, connecting it to goals—forces you to think it through. It's rubber duck debugging for architecture.

3. **Real-time logging beats retroactive documentation.** Capture reasoning in the moment, not reconstructed from memory. By the time you write the post-hoc docs, you've already forgotten the options you rejected.

4. **Graphs beat documents.** Decisions connect—goals spawn decisions, decisions spawn actions, actions produce outcomes. A graph captures these relationships. You can trace any outcome back to the goal that spawned it.

5. **Complex PRs tell a story.** A 50-file PR is incomprehensible as a diff. But as a decision graph? You can see the goal, the key decisions, the rejected approaches, and how each file change connects to the larger purpose. Reviewers can understand *why*, not just *what*.

6. **Context loss is inevitable.** Sessions end. Memory compacts. The graph survives. When you come back to a project after months away, the graph is your memory.

7. **Humans and AI assistants both benefit.** You can query the graph to remember your own reasoning. The LLM can query it to understand decisions made before its context window. Either of you can log decisions. The graph doesn't care who's typing—it just preserves the reasoning.

8. **The graph is a shared workspace.** When the LLM makes a choice, you can see it. When you make a choice, the LLM can query it. Decisions flow between sessions, between humans and AI, between teammates.

---

## Who Uses It

**You, the developer:**
- Think through decisions more carefully by structuring them
- Remember why you made choices months later
- Review complex PRs by understanding the decision flow, not just the diff
- Onboard to unfamiliar codebases by reading the decision history

**Your AI assistant:**
- Recover context after session boundaries or compaction
- Understand decisions made before its context window
- Build on previous reasoning instead of starting fresh
- Leave a queryable trail for future sessions

**Your team:**
- Share decision context across PRs via patch files
- Review PRs with full visibility into the reasoning
- Build institutional knowledge that survives employee turnover

---

## Quick Start

### 1. Install

```bash
cargo install deciduous
```

### 2. Initialize in your project

```bash
cd your-project
deciduous init            # For Claude Code (default)
deciduous init --windsurf # For Windsurf/Cascade
deciduous init --opencode # For OpenCode
deciduous init --codex    # For Codex
```

This creates:
- `.deciduous/deciduous.db` — SQLite database for the graph
- Editor-specific tooling (`.claude/commands/`, `.windsurf/rules/`, `.opencode/command/`, or `.codex/prompts/`)
- `docs/` — Static web viewer (deployable to GitHub Pages)
- `CLAUDE.md` or `AGENTS.md` — Project instructions with the logging workflow

### 3. Start using

```bash
# Log a decision
deciduous add goal "Add user authentication" -c 90

# Connect decisions
deciduous add decision "Choose auth method" -c 75
deciduous link 1 2 -r "Deciding implementation approach"

# View the graph
deciduous serve          # Local web viewer
deciduous tui            # Terminal UI
deciduous sync           # Export for GitHub Pages
```

### 4. Deploy to GitHub Pages

```bash
git add docs/
git push
```

Enable Pages: **Settings > Pages > Source > Deploy from branch > `gh-pages`**

Your graph will be live at `https://<user>.github.io/<repo>/`

---

## The Workflow

```
SESSION START
    |
Run /recover → Query past decisions
    |
DO WORK → Log BEFORE each action
    |
AFTER CHANGES → Log outcomes, link nodes
    |
BEFORE PUSH → deciduous sync
    |
SESSION END → Graph survives
```

### During a session

```bash
# Starting a new feature
deciduous add goal "Add rate limiting" -c 90 -p "User asked: add rate limiting"

# Making a choice
deciduous add decision "Choose rate limiter approach" -c 75
deciduous link 1 2 -r "Deciding implementation"
deciduous add option "Redis-based" -c 80
deciduous add option "In-memory sliding window" -c 70

# Implementing
deciduous add action "Implementing Redis rate limiter" -c 85
deciduous link 2 5 --edge-type chosen -r "Scales across instances"

# Recording outcome
deciduous add outcome "Rate limiting working in prod" -c 95
deciduous link 5 6 -r "Implementation complete"
```

---

## Viewing the Graph

Two full-featured interfaces for browsing the graph—use whichever fits your workflow.

### Web Viewer

```bash
deciduous serve --port 3000
```

A browser-based interface with four visualization modes, branch filtering, and auto-refresh. Deploy to GitHub Pages for shareable, always-up-to-date graphs.

| View | Purpose |
|------|---------|
| **Chains** | Decision chains organized by session—see the story of a feature |
| **Timeline** | Chronological view merged with git commits—trace decisions to code |
| **Graph** | Force-directed interactive visualization—explore connections, zoom, pan |
| **DAG** | Hierarchical goal→decision→outcome flow—understand structure at a glance |

Features: branch dropdown filter, node search, stats bar with counts, click-to-expand details, recency sorting, responsive layout.

### Terminal UI

```bash
deciduous tui
```

A rich terminal interface for when you're already in the shell. Vim-style navigation, syntax-highlighted file previews, and integrated git diffs.

| Key | Action |
|-----|--------|
| `j`/`k`, `gg`/`G` | Navigate timeline |
| `Enter` | Toggle detail panel with connections, metadata, prompts |
| `/` | Search by title or description |
| `f` | Filter by node type (goal, decision, action, etc.) |
| `b`/`B` | Filter by branch / fuzzy branch search |
| `o` | Open associated files in your editor |
| `O` | View linked commit with full diff |
| `p`/`d` | Preview file content / show file diff (syntax highlighted) |
| `s` | Show goal story—hierarchical view from goal to outcomes |
| `?` | Help |

Features: auto-refresh on database changes, file browser panel, commit detail modal, syntax highlighting via the same engine as `bat`.

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

---

## Multi-User Sync

Share decisions across teammates working on the same codebase.

Each node has both a local ID and a globally unique `change_id` (UUID). Export patches to share:

```bash
# Export your branch's decisions
deciduous diff export --branch feature-x -o .deciduous/patches/my-feature.json

# Apply patches from teammates (idempotent—safe to re-apply)
deciduous diff apply .deciduous/patches/*.json

# Preview before applying
deciduous diff apply --dry-run .deciduous/patches/teammate.json
```

### PR Workflow

1. Create nodes while working
2. Export: `deciduous diff export --branch my-feature -o .deciduous/patches/my-feature.json`
3. Commit the patch file (not the database)
4. Open PR with patch file included
5. Teammates apply after pulling

---

## API Trace Capture

Capture Claude API traffic to analyze token usage, see thinking/response content, and correlate API calls with decision nodes.

```bash
# Run Claude through the deciduous proxy
deciduous proxy -- claude

# View traces
deciduous tui            # Press 't' for Trace view
deciduous serve          # Click "Traces" tab

# Manage traces
deciduous trace sessions              # List sessions
deciduous trace spans <session_id>    # List spans
deciduous trace link <sid> <node_id>  # Link to decision node
deciduous trace prune --days 30       # Clean up old traces
```

The proxy intercepts Anthropic API calls, recording:
- Token usage (input/output/cache)
- Model selection and duration
- Thinking blocks and responses
- Tool calls and their results

Link trace sessions to decision nodes to see exactly which API calls went into implementing a feature.

> **Inspiration:** The trace capture approach was inspired by [badlogic/lemmy/claude-trace](https://github.com/badlogic/lemmy/tree/main/apps/claude-trace).

---

## Commands Reference

```bash
# Initialize
deciduous init               # Claude Code (default)
deciduous init --windsurf    # Windsurf/Cascade
deciduous init --opencode    # OpenCode
deciduous init --codex       # Codex
deciduous update             # Update tooling to latest version

# Add nodes
deciduous add goal "Title" -c 90
deciduous add decision "Title" -c 75
deciduous add option "Title" -c 80
deciduous add action "Title" -c 85
deciduous add outcome "Title" -c 95
deciduous add observation "Title" -c 70

# Node metadata
-c, --confidence <0-100>     # Confidence level
-p, --prompt "..."           # User prompt (short, single-line)
--prompt-stdin               # Read prompt from stdin (multi-line, preferred)
-f, --files "a.rs,b.rs"      # Associated files
-b, --branch <name>          # Git branch (auto-detected)
--commit <hash|HEAD>         # Link to git commit

# Update prompts on existing nodes
deciduous prompt <id> "text" # Set prompt text
deciduous prompt <id>        # Read prompt from stdin

# Connect nodes
deciduous link <from> <to> -r "reason"
deciduous link 1 2 --edge-type chosen -r "Selected this approach"

# Query
deciduous nodes              # List all nodes
deciduous nodes -b main      # Filter by branch
deciduous edges              # List connections
deciduous graph              # Full graph as JSON
deciduous commands           # Recent command history

# Visualize
deciduous serve              # Web viewer
deciduous tui                # Terminal UI
deciduous dot --png          # Generate PNG (requires graphviz)
deciduous dot --auto         # Branch-specific filename

# Export
deciduous sync               # Export to docs/graph-data.json
deciduous writeup -t "Title" # Generate PR writeup
deciduous backup             # Create database backup

# Multi-user sync
deciduous diff export -o patch.json
deciduous diff apply patches/*.json
deciduous diff status
deciduous migrate            # Add change_id columns

# API trace capture
deciduous proxy -- claude    # Run with trace capture
deciduous trace sessions     # List trace sessions
deciduous trace spans <id>   # List spans in session
deciduous trace show <id>    # Show span content
deciduous trace link <s> <n> # Link session to node
deciduous trace prune        # Clean up old traces

# Shell completion
deciduous completion bash    # Generate bash completions
deciduous completion zsh     # Generate zsh completions
deciduous completion fish    # Generate fish completions
```

---

## Shell Completion

Enable tab completion for commands, options, and arguments.

**Zsh** (add to `~/.zshrc`):
```bash
source <(deciduous completion zsh)
```

**Bash** (add to `~/.bashrc`):
```bash
source <(deciduous completion bash)
```

**Fish** (add to `~/.config/fish/config.fish`):
```fish
deciduous completion fish | source
```

**PowerShell** (add to profile):
```powershell
deciduous completion powershell | Out-String | Invoke-Expression
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

```bash
deciduous nodes --branch main        # Filter by branch
deciduous add goal "Work" -b other   # Override auto-detection
deciduous add goal "Note" --no-branch # No branch tag
```

---

## GitHub Pages Deployment

`deciduous init` creates GitHub workflows that:
1. Deploy your graph viewer to GitHub Pages on push to main
2. Clean up branch-specific PNGs after PR merge

Enable Pages: **Settings > Pages > Source > Deploy from branch > `gh-pages`**

---

## Building from Source

```bash
git clone https://github.com/notactuallytreyanastasio/deciduous.git
cd deciduous
cargo build --release
./target/release/deciduous --help
```

### macOS Dependencies

The `syntect` crate (used for syntax highlighting in the TUI) requires `libiconv`:

```bash
brew install libiconv
export LIBRARY_PATH="/opt/homebrew/opt/libiconv/lib:$LIBRARY_PATH"
cargo build --release
```

Add the `LIBRARY_PATH` export to your shell profile (`.zshrc` or `.bashrc`) to make it permanent.

### Optional Dependencies

| Dependency | Required For | Install |
|------------|--------------|---------|
| graphviz | `deciduous dot --png` | `brew install graphviz` (macOS) / `apt install graphviz` (Ubuntu) |

---

## Nix Flake

A `flake.nix` is provided for reproducible builds and development environments.

```bash
# Build (full build with embedded web viewer)
nix build

# Build minimal (without rebuilding web viewer)
nix build .#minimal

# Run directly
nix run

# Enter development shell
nix develop

# Run all checks (build, clippy, test, fmt)
nix flake check
```

The devShell includes: Rust toolchain with rust-analyzer, Node.js 20, SQLite, graphviz, diesel-cli, cargo-watch, and all required dependencies for macOS/Linux.

---

## Why "deciduous"?

It almost has the word "decision" in it, and they're trees.
