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
deciduous add observation "JWT too large for mobile" -d "Mobile clients hit payload size limits with full JWT claims. Token refresh adds 2-3s on slow connections."
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
| `/decision-graph` | Build a decision graph from commit history |
| `/sync` | Check this copy against the graph server and refresh it |

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

Through the MCP tools, create and link in one call: `add_node` with
`parent_id` set to the node it belongs under. Never send `add_edge` in the
same batch as the `add_node` whose id it needs: the id does not exist yet,
and whatever stands in for it (a placeholder, a guess) is refused.

### Behavioral Triggers - MUST LOG WHEN:

| Trigger | Log Type | Example |
|---------|----------|---------|
| User asks for a new feature | `goal` **with -p** | "Add dark mode" |
| Exploring possible approaches | `option` | "Use Redux for state" |
| Choosing between approaches | `decision` | "Choose state management" |
| About to write/edit code | `action` | "Implementing Redux store" |
| Something worked or failed | `outcome` | "Redux integration successful" |
| Notice something interesting | `observation` | "Existing code uses hooks" |

### What NOT to Log - CRITICAL

**The decision graph records the USER'S project decisions, not your internal process.**

Nodes should capture what the user is building, choosing, and accomplishing. Do NOT create nodes for your own thinking, planning, or tooling steps.

**DO NOT create nodes for:**
- Reading/exploring the codebase ("Analyzing project structure", "Reading config files")
- Your planning process ("Planning implementation approach", "Evaluating options internally")
- Tool usage ("Running tests to check status", "Checking git log")
- Context gathering ("Understanding existing auth code", "Reviewing PR comments")
- Meta-commentary ("Starting work on this task", "Preparing to implement")

**DO create nodes for:**
- What the user asked for (goals)
- Concrete approaches being considered (options)
- Choices made between approaches (decisions)
- Code being written or changed (actions)
- Results of implementation (outcomes)
- Technical findings that affect decisions (observations)

**Rule of thumb:** If a node describes something the user would put on a project timeline or in a PR description, log it. If it describes your internal process of reading and thinking, don't.

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
deciduous remote status   # What this copy and the graph server disagree on

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

### Branch-Based Grouping

Nodes are auto-tagged with the current git branch. Configure in `.deciduous/config.toml`:
```toml
[branch]
main_branches = ["main", "master"]
auto_detect = true
```

### Audit Checklist (Before Ending a Session)

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
- ✅ `git add .claude/commands/decision.md`

**Why this matters:**
- Prevents accidentally committing sensitive files (.env, credentials)
- Prevents committing large binaries or build artifacts
- Forces you to review exactly what you're committing
- Catches unintended changes before they enter git history

### Session Start Checklist

```bash
deciduous remote pull     # Refresh the local cache from the graph server
deciduous remote status   # Anything this copy has that the server lacks?
deciduous check-update    # Update needed? Run 'deciduous update' if yes
                          # (auto-checked every 24h if auto-update is on)
deciduous nodes           # What decisions exist?
deciduous edges           # How are they connected? Any gaps?
deciduous doc list        # Any attached documents to review?
deciduous transcript list # Sessions not yet in the graph? import them
deciduous board read --unanswered <label>  # parallel agents: anything waiting for you?
git status                # Current state
```

### The Transcript Is the Log

Logging in real time is the rule, and the transcript is the backstop. Claude Code writes every prompt, edit, command and result of a session to a JSONL file; `deciduous transcript import` derives goals (the prompt, verbatim), actions (edit runs, commits) and outcomes (test and build runs with their exit, the closing report) from it, each node carrying `metadata.evidence` as `proven` (a tool call and its result) or `inferred` (the assistant said so). Importing twice adds nothing.

```bash
deciduous transcript list                 # this project's sessions: imported or new
deciduous transcript import <session-id>  # or a path to the .jsonl; --dry-run to see counts
```

What the import cannot derive is a decision or an option: whether one approach was chosen over another is a judgement about prose. Log those as you make them; the import fills in what happened around them.

### Parallel Agents: The Message Board

When more than one agent works at once, coordinate through deciduous, never through a scratch or markdown file. A file has no ids to answer, no way to ask what is waiting for you, and it is gone with the worktree it was written in.

- **Post** interface changes, questions and answers: `post_message` (MCP) or `deciduous board post --as <your-label> -s "subject" -m "body"`. Address other agents with `@label`.
- **Read** what is waiting for you: `read_messages` with `unanswered_for: "<your-label>"`, or `deciduous board read --unanswered <your-label>`. Do it at start, before touching a file another agent may be changing, and before finishing.
- **Reply** with `reply_to: <id>` (`--reply-to <id>`). A reply is what takes a question off the asker's list; a new post does not.

One board serves every git worktree of the repository, or the server when the project has a `[remote]`. Messages are not graph nodes: not in `graph.json`, not exported, not synced. With `DECIDUOUS_AGENT_LABEL` set in a session's environment, `board post` uses it for `--as`, and the `board-mentions.sh` hook shows that label's unanswered messages at session start and as new ones arrive.

### The Graph Server

This project has a `[remote]` in `.deciduous/config.toml`: its graph lives on a shared server, and `.deciduous/deciduous.db` is this machine's cache of it. There is no `.deciduous/graph.json` to commit or merge, and `deciduous sync` is not the step here. Every CLI write is queued in `.deciduous/remote-log.jsonl` and sent before the command exits; MCP tools write to the server directly (pass the `workspace` named under `[remote]`).

```bash
deciduous remote status   # what differs between this copy and the server (exit 1 if anything)
deciduous remote pull     # send what is waiting, then refresh the local cache from the server
deciduous remote push     # send writes still waiting in the log (the server was down)
```

Local node ids differ per machine. To point at someone else's node, use its change_id prefix (the CHANGE column in `deciduous nodes`) or the server id an agent quotes: `deciduous link a1b2c3d4 <id> -r "..."`.
<!-- deciduous:end -->
## Session Start Checklist

Every new session or after context recovery, run `/recover` or:

```bash
deciduous check-update    # Update needed? Run 'deciduous update' if yes
                          # (always-on, checks every 24h)
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

When working on this project, identify which domain the work belongs to and use the appropriate subagent context. Subagent definitions are in `.claude/agents.toml`.

### Available Subagents

| Agent | Domain | Key Files |
|-------|--------|-----------|
| `rust-core` | CLI, database, export/sync | `src/main.rs`, `src/db.rs`, `src/export.rs` |
| `web` | React/TypeScript viewer | `web/src/**/*.{ts,tsx}` |
| `tooling` | Claude Code configuration | `.claude/`, `CLAUDE.md`, `src/init/` |
| `docs` | Documentation, guides | `docs/`, `README.md`, `ROADMAP.md` |
| `ci` | Build, Actions, releases | `.github/workflows/`, `scripts/` |

### How to Use Subagents

When spawning a Task for exploration or implementation:

1. **Identify the domain** from the file patterns in `.claude/agents.toml`
2. **Include the subagent context** in your Task prompt
3. **Scope file searches** to the relevant patterns

Example: For web viewer work, spawn an Explore agent with:
```
"Focus on web/src/. This is the web agent domain - React components, D3/Dagre graphs. See .claude/agents.toml for full context."
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
├── db.rs                # SQLite via Diesel: nodes, edges, documents, themes, the command log
├── schema.rs            # Diesel table definitions
├── records.rs           # .deciduous/graph.json: records, reconcile, the merge driver
├── oplog.rs             # .deciduous/remote-log.jsonl: CLI writes queued for the server
├── remote.rs            # deciduous remote: setup, login, status, push, pull, watch
├── server.rs            # Installing and running this machine's server as a user service
├── local_server.rs      # The server's PostgreSQL on localhost:5432
├── transcript.rs        # deciduous transcript: the graph derived from a Claude Code transcript
├── archaeology.rs       # pivot, timeline, supersede
├── pulse.rs, narratives.rs
├── board.rs             # The message board (deciduous board)
├── mcp/                 # deciduous mcp: the local stdio MCP server (34 tools)
├── api.rs               # deciduous serve --api: the multi-graph JSON daemon
├── serve.rs, viewer.html # deciduous serve: the web viewer, embedded
├── export.rs            # DOT export and PR writeups
├── init/                # deciduous init / update: templates, remote templates, the guard
│   └── known_templates.rs   # @generated: sha256 of every template ever shipped
├── hooks.rs, opencode.rs, config.rs, changelog.rs, github.rs, roadmap.rs
├── events.rs, watch.rs  # Server events over WebSocket
└── demo_swarm/          # deciduous demo-swarm

web/                     # The web viewer: React + TypeScript + D3, one App.tsx
├── src/App.tsx
├── src/types/           # graph.ts, archaeology.ts, qa.ts, generated/ (from the Rust types)
└── dist/                # Built output, a single HTML file

deciduous_mcp/           # The shared graph server: Elixir, PostgreSQL, 21 MCP tools

.deciduous/
├── config.toml          # [remote] names the server and workspace; [branch] controls tagging
├── deciduous.db         # SQLite: with a [remote], this machine's cache; without, the graph
├── remote-log.jsonl     # Writes waiting for the server
├── graph.json           # The git-tracked graph for a project with no server
├── documents/           # Attached files, content-hash named
└── narratives.md        # Narrative tracking
```

## Web Viewer Development

**When modifying web viewer code (TypeScript/React), you MUST rebuild and update the embedded HTML.**

### Key Files

| File | Purpose |
|------|---------|
| `web/src/App.tsx` | The viewer: narrative modes, tree and detail panel, D3 graph, search, themes, the Ask panel |
| `web/src/types/graph.ts` | TypeScript interfaces for nodes, edges, themes |
| `web/src/types/generated/` | Types generated from the Rust side; `scripts/validate-types.sh` checks they match |
| `src/viewer.html` | Embedded viewer served by `deciduous serve` |
| `docs/demo/index.html` | Static demo viewer served on deciduous.dev |

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

The viewer does not yet show `metadata.evidence` or `file_hashes` on transcript-derived nodes; `deciduous show` does.

## CLI Commands

| Command | Description |
|---------|-------------|
| `deciduous init` | Initialize deciduous in current directory |
| `deciduous update` | Update Claude integration files to latest version |
| `deciduous check-update` | Check if integration files need updating |
| `deciduous auto-update on\|off` | The once-a-day crates.io version check |
| `deciduous add <type> "title"` | Add a node (goal/decision/option/action/outcome/observation/revisit) |
| `deciduous link <from> <to>` | Create edge between nodes |
| `deciduous status <id> <status>` | Update node status |
| `deciduous nodes` | List all nodes |
| `deciduous edges` | List all edges |
| `deciduous graph` | Output full graph as JSON |
| `deciduous commands` | Recent CLI writes, with exit status |
| `deciduous transcript list` | This project's Claude Code sessions: imported or new |
| `deciduous transcript import <path\|session-id>` | Derive the graph from a transcript, with evidence on every node (`--dry-run`, `--no-hash`, `--json`) |
| `deciduous backup` | Create database backup |
| `deciduous serve` | Start web viewer |
| `deciduous sync` | Reconcile `.deciduous/graph.json` with the local DB (projects with no server) |
| `deciduous dot` | Export graph as DOT format |
| `deciduous writeup` | Generate PR writeup markdown |
| `deciduous sync --check` | Report pending sync changes, exit 1 if any |
| `deciduous migrate` | Add change_id columns for sync |
| `deciduous doc attach <node_id> <file>` | Attach file to a decision node |
| `deciduous doc list [node_id]` | List attached documents (all or per-node) |
| `deciduous doc show <id>` | Show document details |
| `deciduous doc describe <id> [text]` | Set/update document description (`--ai` for AI) |
| `deciduous doc open <id>` | Open document in default application |
| `deciduous doc detach <id>` | Soft-delete a document attachment |
| `deciduous doc gc` | Garbage collect orphaned document files |
| `deciduous remote setup\|status\|push\|pull\|watch` | The shared graph server |
| `deciduous board post\|read\|show` | The message board for parallel agents |
| `deciduous archaeology pivot\|supersede\|timeline` | Revisit a decision that turned out wrong |
| `deciduous themes`, `deciduous tag` | Themes and tags |
| `deciduous audit` | Associate commits with nodes |

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

## Sharing the graph

This repository has a `[remote]` in `.deciduous/config.toml`: the graph lives on the shared server, `.deciduous/deciduous.db` is this machine's cache of it, and MCP tools write to the server directly. CLI writes are queued in `.deciduous/remote-log.jsonl` and sent before the command exits.

```bash
deciduous remote status   # what this copy and the server disagree on; exit 1 if anything
deciduous remote pull     # send what is waiting, then refresh the cache
deciduous remote push     # resend writes the server did not get (it was down)
deciduous remote watch    # one line per write as it lands, from every agent
```

A project with no server shares its graph through the git-tracked `.deciduous/graph.json` instead: `deciduous sync` reconciles it with the local database both ways (newer `updated_at` wins, deletes are tombstones), and the `deciduous` merge driver registered by init/update merges concurrent edits record by record. `sync` has not exported `docs/graph-data.json` since 1.0.5; the copy in this repository is a hand-made export from 2026-09-04 that the static demo reads.

Node references anywhere: local id **or** change_id prefix (`deciduous link a1b2c3d4 58`). `.deciduous/patches/`, `deciduous diff` and `.deciduous/sync/` are from 0.16 and 0.17 and do not exist.

Full design: `docs/MULTI_USER_SYNC.md` and `docs/remote-graph.md`.

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
- ✅ `git add .claude/commands/decision.md`

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

Semver: a new command or a new field in the graph is MINOR; a fix is PATCH. 1.1.0 (2026-10-24) was the last MINOR.

A release is a tag on main that `release.yml` builds and publishes. What the workflow checks before it publishes anything, and what to do in order:

1. **Bump both package versions.** `Cargo.toml` and `deciduous_mcp/mix.exs` must carry the tag's version; the workflow refuses a tag whose two versions disagree (`Require both package versions to match the publishing tag`). `cargo build` updates `Cargo.lock`.
2. **Both changelogs.** `CHANGELOG.md` (dated, Added / Changed / Fixed / Not done) and `src/changelog.rs` (the `RELEASES` array `check-update` shows on upgrade, newest first). 0.16.0 through 0.17.1 were once missing from both.
3. **Test and package locally**: `cargo test`, `cargo clippy --all-targets`, `cargo build --release`, `cargo publish --dry-run`. The toolchain is pinned to 1.92.0 in `rust-toolchain.toml` and the workflows; bump it on purpose, not by letting stable move.
4. **Server deps**: `cd deciduous_mcp && mix hex.audit`. A Hex advisory against any dependency fails the self-contained server build and release acceptance; 1.0.10 was tagged and never published for this, and 1.1.0 needed bandit, plug_crypto and decimal bumped on release day.
5. **PR, merge, then tag by hand** from the merge commit:
   ```bash
   git tag -a vX.Y.Z <merge-sha> -m "vX.Y.Z: ..."
   git push origin vX.Y.Z
   ```
   `Auto Release` also fires on the merge and either pushes the tag with `GITHUB_TOKEN` or fails on the duplicate; a tag pushed by the bot does not trigger `release.yml`, one pushed by a person does. If the bot got there first, `gh workflow run release.yml -f tag=vX.Y.Z -f publish=true`.
6. **Watch the run to the end.** `release.yml` builds five CLI binaries and five server executables, runs release acceptance (an isolated PostgreSQL; the readiness wait was fixed in #319 after it raced on 1.1.0's run), then `release`, `publish-crate` (crates.io, secret `CARGO_REGISTRY_TOKEN`) and `update-homebrew` (the tap repository; the checked-in `Formula/deciduous.rb` here is a stale copy). A release looks done when nothing was published: confirm `gh release view vX.Y.Z`, crates.io, and the tap formula's version.
7. `deciduous update` in this repository afterwards, and commit what it changed.

Stacked PRs: never `gh pr merge --delete-branch` on a PR another open PR is based on. GitHub closes the dependent PR instead of retargeting it, and a closed PR whose base is gone cannot be reopened. Merge without deleting, `gh pr edit N --base main` for the next one, and dispatch `type-check.yml` on its branch by hand (the CI workflow only runs for PRs that were opened against main). Delete branches at the end.

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
