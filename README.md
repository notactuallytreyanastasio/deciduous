# Deciduous

**Decision graph tooling for AI-assisted development.** Track every goal, decision, and outcome. Survive context loss. Query your reasoning.

[![Crates.io](https://img.shields.io/crates/v/deciduous.svg)](https://crates.io/crates/deciduous)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)

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

### Homebrew (Recommended)

```bash
brew tap notactuallytreyanastasio/tap
brew install deciduous
```

### Pre-built Binaries

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
deciduous add option "JWT tokens" -c 80
deciduous add option "Session cookies" -c 75
deciduous link 1 2 -r "Possible approach"
deciduous link 1 3 -r "Possible approach"
deciduous add decision "Use JWT for API, sessions for web" -c 85
deciduous link 2 4 -r "Chosen approach"
deciduous link 3 4 -r "Also incorporated"

# View the graph
deciduous serve    # Web viewer at localhost:3000
```

That's it. Your first decision graph is live.

### The Canonical Flow

Every decision follows this path through the graph:

```
goal → options → decision → actions → outcomes
```

- **Goals** lead to **options** (possible approaches to explore)
- **Options** lead to a **decision** (choosing which option to pursue)
- **Decisions** lead to **actions** (implementing the chosen approach)
- **Actions** lead to **outcomes** (results of the implementation)
- **Observations** attach anywhere relevant
- **Revisits** connect old approaches to new ones when you pivot

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

### MCP Server

Deciduous includes a built-in [MCP](https://modelcontextprotocol.io/) server that works with Claude Code, Claude Desktop, and any MCP-compatible client. Instead of shelling out to the CLI, the AI gets direct access to 32 tools for managing and querying the decision graph.

> **Claude Cowork:** Coming soon. Cowork agents can't yet load custom MCP servers — track progress on [anthropics/claude-code#48909](https://github.com/anthropics/claude-code/issues/48909).

**Claude Code** (project-level):

Add to `.claude/settings.local.json`:

```json
{
  "mcpServers": {
    "deciduous": {
      "command": "deciduous",
      "args": ["mcp"]
    }
  }
}
```

**Claude Desktop**:

Add to `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS) or `%APPDATA%\Claude\claude_desktop_config.json` (Windows):

```json
{
  "mcpServers": {
    "deciduous": {
      "command": "/Users/you/.cargo/bin/deciduous",
      "args": ["mcp"],
      "env": {
        "DECIDUOUS_DB_PATH": "/path/to/your/project/.deciduous/deciduous.db"
      }
    }
  }
}
```

Use the **absolute path** to the `deciduous` binary (run `which deciduous` to find it) — Claude Desktop doesn't inherit your shell `PATH`. Point `DECIDUOUS_DB_PATH` at the SQLite database in your project's `.deciduous/` directory, then restart Claude Desktop.

**Claude Code (user-level)**:

Add the project-level config above to `~/.claude/settings.json` instead of `.claude/settings.local.json` to make the server available across all projects.

Once configured, the AI gets tools for:

| Category | Tools | What they do |
|----------|-------|-------------|
| **Graph CRUD** | `add_node`, `link_nodes`, `delete_node`, ... | Create and manage decision nodes and edges |
| **Querying** | `list_nodes`, `search_nodes`, `show_node`, ... | Find and filter nodes across the graph |
| **Analysis** | `trace_chain`, `get_node_context`, `get_pulse`, ... | Traverse chains, get neighborhood context, find orphans |
| **Sessions** | `start_session`, `end_session`, `resume_session`, ... | Each conversation gets its own decision tree |
| **Export** | `export_dot`, `generate_writeup` | DOT visualization and PR writeups |

Sessions persist to disk and auto-resume across server restarts, so long-running conversations keep their decision trees intact.

**[Full MCP documentation and tutorial](https://deciduous.dev/mcp)**

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

# Considering options
deciduous add option "Redis-based distributed" -c 80
deciduous add option "In-memory sliding window" -c 70
deciduous link 1 2 -r "Possible approach"
deciduous link 1 3 -r "Possible approach"

# Attach the design spec to the goal
deciduous doc attach 1 docs/rate-limiting-spec.pdf -d "Rate limiting design spec"

# Making a choice
deciduous add decision "Use Redis rate limiter" -c 85
deciduous link 2 4 --edge-type chosen -r "Scales across instances"
deciduous link 3 4 --edge-type rejected -r "Doesn't scale horizontally"

# Implementing the chosen approach
deciduous add action "Implementing Redis rate limiter" -c 85
deciduous link 4 5 -r "Implementation"

# Attach the architecture diagram to the action
deciduous doc attach 5 docs/redis-arch.png --ai-describe

# Recording the outcome
deciduous add outcome "Rate limiting working in prod" -c 95
deciduous link 5 6 -r "Implementation complete"

# Share it: records in .deciduous/sync/ are written as you go;
# sync imports teammates' records and refreshes docs/graph-data.json
deciduous sync
```

### Document Attachments

Attach files to any decision node — architecture diagrams, specs, screenshots, PDFs.

```bash
# Attach a diagram to a goal
deciduous doc attach 1 docs/architecture.png -d "System architecture diagram"

# AI-generate a description
deciduous doc attach 1 screenshot.png --ai-describe

# List what's attached
deciduous doc list 1

# Open a document
deciduous doc open 3

# Soft-delete (recoverable)
deciduous doc detach 3

# Clean up orphaned files
deciduous doc gc
```

Documents are stored in `.deciduous/documents/` with content-hash naming for deduplication. The web viewer displays attached documents in the node detail panel. Soft-delete with `doc detach`; garbage-collect orphaned files with `doc gc --dry-run` to preview.

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

## Skills: Archaeology, Decision-Graph, and Narratives

Deciduous ships with skills that give your AI assistant structured ways to understand and work with a codebase's decision history.

### /decision-graph — Visiting and documenting the past

The `/decision-graph` skill builds a full decision graph from your repository's commit history — perfect for bootstrapping a graph on an existing project that wasn't using deciduous from the start. It works in four layers:

1. **Commit analysis** — Groups commits into logical narratives
2. **Code structure** — Identifies architectural decisions from the codebase
3. **Narrative construction** — Builds evolution stories with pivots and connections
4. **PR context** — Uses `gh` CLI to mine PR descriptions and review threads for decision rationale, alternatives considered, and trade-offs discussed

### /archaeology — Revisit, correct, execute

Archaeology is for revisiting past decisions in the graph—finding what went wrong, correcting the record, and executing on a new direction. When you discover a past decision was flawed, archaeology gives you the workflow to trace it back, mark it superseded, and connect the new approach.

```bash
# Find the old decision and mark it
deciduous nodes --status active --type decision
deciduous add observation "Mobile Safari 4KB cookie limit breaking JWT auth"
deciduous link 1 2 -r "Discovered in production"

# Revisit: pivot from old approach to new
deciduous add revisit "Reconsidering auth token strategy"
deciduous link 2 3 -r "Cookie limits forced rethink"
deciduous status 1 superseded

# Execute the new direction
deciduous add decision "Hybrid: JWT for API, sessions for web"
deciduous link 3 4 -r "New approach"
deciduous add action "Implementing hybrid auth" -c 85
deciduous link 4 5 -r "Implementation"
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

### /pulse — Map the current design

Pulse maps the current architecture as a decision tree — the design choices that make the system work today. Your agent reads the code, identifies the design questions that had to be answered, and logs them as nodes. Useful for understanding what decisions you might affect before making changes.

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

Features: branch filtering, full-text search with type filters, resizable panels, deep linking, click-to-expand details, keyboard navigation (j/k/g/G/Space), Q&A panel, and auto-refresh.

---

## Node Types

| Type | Purpose | Example |
|------|---------|---------|
| `goal` | High-level objective | "Add user authentication" |
| `option` | Approach considered | "Use JWT tokens" |
| `decision` | Choice point | "Choose auth method" |
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
| `.opencode/commands/*.md` | Command templates |
| `.opencode/skills/*/SKILL.md` | Skill definitions |
| `.opencode/agents/*.md` | Custom deciduous agent |
| `.opencode/tools/*.ts` | Custom deciduous tool |
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

Deciduous always checks [crates.io](https://crates.io/crates/deciduous) for new versions once per 24 hours via a lightweight hook. The notification varies by severity:

- **Patch updates** (e.g., 0.13.10 → 0.13.11): quiet one-line notification
- **Minor/major updates** (e.g., 0.13.x → 0.14.0): prominent banner encouraging upgrade

You can also check manually at any time:

```bash
$ deciduous check-update
Update available: Integration files are v0.9.4, binary is v0.9.5. Run 'deciduous update'.
```

The check is rate-limited (once per 24h), has a 3-second timeout, and never blocks your workflow. Results are cached in `.deciduous/.latest_version`.

---

## How the Hooks/Plugins Work

Each AI assistant integration includes hooks and plugins that enforce the decision graph workflow:

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

### Version-Check Hook (Notifies of new versions)

Checks crates.io once per 24 hours for newer versions of deciduous. Patch updates get a quiet one-liner; minor/major updates get a prominent banner encouraging upgrade. Always-on, non-blocking.

```
Session starts → Hook checks cached version → If stale, queries crates.io (3s timeout) → AI tells user
```

### Assistant-Specific Implementation

| Assistant | Pre-Edit | Post-Commit | Version Check |
|-----------|----------|-------------|---------------|
| **Claude Code** | `PreToolUse` on `Edit\|Write` | `PostToolUse` on `Bash` | `PreToolUse` on `Bash` |
| **OpenCode** | TypeScript `pre-edit` | TypeScript `post-commit` | TypeScript `pre-tool` |
| **Windsurf** | Cascade `pre_write_code` | Cascade `post_run_command` | Cascade `pre_write_code` |

Pre-edit and post-commit hooks use **exit code 2** to block/alert. Version-check hooks use **exit code 0** (informational, never blocks).

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
deciduous nodes --status active
deciduous nodes --type goal
deciduous edges              # List connections
deciduous edges --to <id>    # Edges pointing to a node
deciduous edges --from <id>  # Edges from a node
deciduous graph              # Full graph as JSON
deciduous commands           # Recent command log

# Visualize
deciduous serve              # Web viewer
deciduous dot --png          # Generate PNG (requires graphviz)

# Sync + export
deciduous sync               # Reconcile .deciduous/sync/ with the DB, export to docs/
deciduous sync --check       # Anything pending? (exit 1 if so)
deciduous writeup -t "Title" # Generate PR writeup
deciduous backup             # Create database backup

# Document attachments
deciduous doc attach <node_id> <file>          # Attach file to node
deciduous doc attach <node_id> <file> -d "..." # With description
deciduous doc attach <node_id> <file> --ai-describe  # AI description
deciduous doc list                             # List all documents
deciduous doc list <node_id>                   # Documents for a node
deciduous doc show <id>                        # Document details
deciduous doc describe <id> "text"             # Set description
deciduous doc describe <id> --ai               # AI-generate description
deciduous doc open <id>                        # Open in default app
deciduous doc detach <id>                      # Soft-delete
deciduous doc gc                               # Clean orphaned files
deciduous doc gc --dry-run                     # Preview cleanup

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
- Use `/archaeology` to revisit and correct past decisions
- Use `/decision-graph` to build decision graphs from commit history
- Use `/document` to generate comprehensive docs with test examples
- Ask deep questions via the Q&A interface grounded in actual graph data
- Attach relevant documents (diagrams, screenshots, specs) to decision nodes

**Your team:**
- Share decision context through `.deciduous/sync/`, one git-tracked JSON record per decision; concurrent edits of the same record merge field by field
- Review PRs with full visibility into reasoning
- Build institutional knowledge that survives turnover
- Search past Q&A interactions to find answers that were already given

---

## Why "deciduous"?

It almost has the word "decision" in it, and they're trees.

---

**[Tutorial](https://notactuallytreyanastasio.github.io/deciduous/tutorial/)** · **[Live Demo](https://notactuallytreyanastasio.github.io/deciduous/demo/)** · **[GitHub](https://github.com/notactuallytreyanastasio/deciduous)**
