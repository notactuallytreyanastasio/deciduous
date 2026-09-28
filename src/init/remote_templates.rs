//! Templates for a project whose graph lives on a shared server: its
//! `.deciduous/config.toml` has a `[remote]`. `deciduous init` and
//! `deciduous update` install these in place of the git-model ones in
//! [`super::templates`], which tell agents to commit `.deciduous/graph.json`,
//! `git pull --rebase` and run `deciduous sync`: none of that exists here.
//!
//! Each is the git-model template with its sync passages rewritten. The
//! test `remote_templates_track_the_git_ones` fails when a line the two
//! share changes in one and not the other.

/// CLAUDE.md section for a project whose graph lives on a server (`[remote]`).
pub const CLAUDE_MD_SECTION_REMOTE: &str = r#"
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
deciduous board read --unanswered <label>  # parallel agents: anything waiting for you?
git status                # Current state
```

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
"#;

/// /decision for a project whose graph lives on a server (`[remote]`).
pub const DECISION_MD_REMOTE: &str = r#"---
description: Manage decision graph - track algorithm choices and reasoning
allowed-tools: Bash(deciduous:*)
argument-hint: <action> [args...]
---

# Decision Graph Management

**Log decisions IN REAL-TIME as you work, not retroactively.**

## When to Use This

| You're doing this... | Log this type | Command |
|---------------------|---------------|---------|
| Starting a new feature | `goal` **with -p** | `/decision add goal "Add user auth" -p "user request"` |
| Choosing between approaches | `decision` | `/decision add decision "Choose auth method"` |
| Considering an option | `option` | `/decision add option "JWT tokens"` |
| About to write code | `action` | `/decision add action "Implementing JWT"` |
| Noticing something | `observation` | `/decision add obs "Found existing auth code"` |
| Finished something | `outcome` | `/decision add outcome "JWT working"` |
| Reconsidering a past decision | `revisit` | `/decision add revisit "Reconsidering auth"` |

## What NOT to Log - CRITICAL

**The decision graph records the USER'S project decisions, not your internal process.**

Do NOT create nodes for your own thinking, planning, reading, or tooling steps. Only log things the user would recognize as project milestones or decisions.

**Skip these (meta/process noise):**
- "Reading codebase to understand structure"
- "Planning implementation approach"
- "Running tests to check status"
- "Analyzing existing code"

**Log these (user-visible project work):**
- "Add user authentication" (goal)
- "Use JWT tokens" (option)
- "Implemented JWT middleware" (action)
- "JWT auth working, all tests pass" (outcome)

## Quick Commands

Based on $ARGUMENTS:

### View Commands
- `nodes` or `list` -> `deciduous nodes`
- `edges` -> `deciduous edges`
- `graph` -> `deciduous graph`
- `commands` -> `deciduous commands`

### Create Nodes (with optional metadata)
- `add goal <title>` -> `deciduous add goal "<title>" -c 90`
- `add decision <title>` -> `deciduous add decision "<title>" -c 75`
- `add option <title>` -> `deciduous add option "<title>" -c 70`
- `add action <title>` -> `deciduous add action "<title>" -c 85`
- `add obs <title> -d <description>` -> `deciduous add observation "<title>" -c 80 -d "<description>"`
  - **Observations MUST have both a title (short summary) and description (full detail)**
- `add outcome <title>` -> `deciduous add outcome "<title>" -c 90`
- `add revisit <title>` -> `deciduous add revisit "<title>" -c 75`

### Optional Flags for Nodes
- `-c, --confidence <0-100>` - Confidence level
- `-d, --description "..."` - Description (**REQUIRED for observations** - the detail behind the title)
- `-p, --prompt "..."` - Store the user prompt that triggered this node
- `-f, --files "file1.rs,file2.rs"` - Associate files with this node
- `-b, --branch <name>` - Git branch (auto-detected by default)
- `--no-branch` - Skip branch auto-detection
- `--commit <hash|HEAD>` - Link to a git commit (use HEAD for current commit)
- `--date "YYYY-MM-DD"` - Backdate node (for archaeology/retroactive logging)

### CRITICAL: Link Commits to Actions/Outcomes

**After every git commit, link it to the decision graph!**

```bash
git commit -m "feat: add auth"
deciduous add action "Implemented auth" -c 90 --commit HEAD
deciduous link <goal_id> <action_id> -r "Implementation"
```

## CRITICAL: Capture VERBATIM User Prompts

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

## Branch-Based Grouping

**Nodes are automatically tagged with the current git branch.** This enables filtering by feature/PR.

### How It Works
- When you create a node, the current git branch is stored in `metadata_json`
- Configure which branches are "main" in `.deciduous/config.toml`:
  ```toml
  [branch]
  main_branches = ["main", "master"]  # Branches not treated as "feature branches"
  auto_detect = true                    # Auto-detect branch on node creation
  ```
- Nodes on feature branches (anything not in `main_branches`) can be grouped/filtered

### CLI Filtering
```bash
# Show only nodes from specific branch
deciduous nodes --branch main
deciduous nodes --branch feature-auth
deciduous nodes -b my-feature

# Override auto-detection when creating nodes
deciduous add goal "Feature work" -b feature-x  # Force specific branch
deciduous add goal "Universal note" --no-branch  # No branch tag
```

### Web UI Branch Filter
The graph viewer shows a branch dropdown in the stats bar:
- "All branches" shows everything
- Select a specific branch to filter all views (Chains, Timeline, Graph, DAG)

### When to Use Branch Grouping
- **Feature work**: Nodes created on `feature-auth` branch auto-grouped
- **PR context**: Filter to see only decisions for a specific PR
- **Cross-cutting concerns**: Use `--no-branch` for universal notes
- **Retrospectives**: Filter by branch to see decision history per feature

### Create Edges
- `link <from> <to> [reason]` -> `deciduous link <from> <to> -r "<reason>"`

### Document Attachments
- `doc attach <node_id> <file>` -> `deciduous doc attach <node_id> <file>`
- `doc attach <node_id> <file> -d "desc"` -> attach with description
- `doc attach <node_id> <file> --ai-describe` -> attach with AI-generated description
- `doc list` -> `deciduous doc list` (all documents)
- `doc list <node_id>` -> `deciduous doc list <node_id>` (documents for one node)
- `doc show <id>` -> `deciduous doc show <id>`
- `doc describe <id> "desc"` -> `deciduous doc describe <id> "desc"`
- `doc describe <id> --ai` -> AI-generate description
- `doc open <id>` -> `deciduous doc open <id>` (open in default app)
- `doc detach <id>` -> `deciduous doc detach <id>` (soft-delete)
- `doc gc` -> `deciduous doc gc` (garbage-collect orphaned files)

### The graph server (this project has a `[remote]`)
- `sync` -> `deciduous remote status` (what differs between this copy and the server; exit 1 if anything)
- `pull` -> `deciduous remote pull` (send what is waiting, then refresh the local cache from the server)
- `push` -> `deciduous remote push` (send writes still waiting in `.deciduous/remote-log.jsonl`)
- Node references: every command that takes a node id also takes a `change_id` prefix (the CHANGE column in `deciduous nodes`). Use the prefix to point at a teammate's node, since local ids differ per machine.

### Message board (parallel agents)
- `board post --as <label> -s "subject" -m "body @other"` -> post; `--reply-to <id>` answers a message
- `board read --unanswered <label>` -> what is waiting for you; also `--since`, `--from`, `--to`, `-q`, `--json`
- `board show <id>` -> one message in full

### Export & Visualization
- `dot` -> `deciduous dot` (output DOT to stdout)
- `dot --png` -> `deciduous dot --png -o graph.dot` (generate PNG)
- `dot --nodes 1-11` -> `deciduous dot --nodes 1-11` (filter nodes)
- `writeup` -> `deciduous writeup` (generate PR writeup)
- `writeup -t "Title" --nodes 1-11` -> filtered writeup

## Node Types

| Type | Purpose | Example |
|------|---------|---------|
| `goal` | High-level objective | "Add user authentication" |
| `decision` | Choice point with options | "Choose auth method" |
| `option` | Possible approach | "Use JWT tokens" |
| `action` | Something implemented | "Added JWT middleware" |
| `outcome` | Result of action | "JWT auth working" |
| `observation` | Finding or data point (title + description) | Title: "Existing code uses sessions", -d: "The legacy auth uses express-session with cookie store, not token-based" |
| `revisit` | Pivot point / reconsideration | "Reconsidering auth approach" |

## Edge Types

| Type | Meaning |
|------|---------|
| `leads_to` | Natural progression |
| `chosen` | Selected option |
| `rejected` | Not selected (include reason!) |
| `requires` | Dependency |
| `blocks` | Preventing progress |
| `enables` | Makes something possible |
| `took_from` | Borrowed from that node, usually on another branch |

## Graph Integrity - CRITICAL

**Every node MUST be logically connected.** Floating nodes break the graph's value.

### Connection Rules (goal -> options -> decision -> actions -> outcomes)
| Node Type | MUST connect to | Example |
|-----------|----------------|---------|
| `goal` | Can be a root (no parent needed) | Root goals are valid orphans |
| `option` | Its parent goal | "Use JWT" -> links FROM "Add auth" |
| `decision` | The option(s) it chose between | "Choose JWT" -> links FROM "Use JWT" option |
| `action` | The decision that spawned it | "Implementing JWT" -> links FROM "Choose JWT" |
| `outcome` | The action that produced it | "JWT working" -> links FROM "Implementing JWT" |
| `observation` | Related goal/action/decision | "Found existing code" -> links TO relevant node |
| `revisit` | The decision/outcome being reconsidered | "Reconsidering auth" -> links FROM original decision |

### Audit Checklist
Ask yourself after creating nodes:
1. Does every **outcome** link back to the action that produced it?
2. Does every **action** link to the decision that spawned it?
3. Does every **option** link to its parent goal?
4. Does every **decision** link from the option(s) being chosen?
5. Are there **dangling outcomes** with no parent action?

### Find Disconnected Nodes
```bash
# List nodes with no incoming edges (potential orphans)
deciduous edges | cut -d'>' -f2 | cut -d' ' -f2 | sort -u > /tmp/has_parent.txt
deciduous nodes | tail -n+3 | awk '{print $1}' | while read id; do
  grep -q "^$id$" /tmp/has_parent.txt || echo "CHECK: $id"
done
```
Note: Root goals are VALID orphans. Outcomes/actions/options usually are NOT.

### Fix Missing Connections
```bash
deciduous link <parent_id> <child_id> -r "Retroactive connection - <why>"
```

### When to Audit
- After `deciduous remote pull` brings in other people's nodes
- After creating multiple nodes quickly
- At session end
- When the web UI graph looks disconnected

## Git Staging Rules - CRITICAL

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

## The Graph Server

This project has a `[remote]` in `.deciduous/config.toml`. The server holds the graph; `.deciduous/deciduous.db` is this machine's cache of it. There is no `.deciduous/graph.json` to commit, no merge driver, and `deciduous sync` is not the step here.

Every `add`, `link`, `status`, `delete` is queued in `.deciduous/remote-log.jsonl` and sent before the command exits. A write made while the server is down waits in the log and goes with the next write or `deciduous remote push`. MCP tools write to the server directly.

```bash
deciduous remote status   # every node, edge and document that differs, with the command that fixes it
deciduous remote pull     # send what is waiting, then refresh the local cache (deletions included)
deciduous remote push     # send what is waiting; --retry-rejected once a refusal's cause is fixed
```

**Linking to a teammate's node:** local ids differ per machine, so use the change_id prefix shown in the CHANGE column of `deciduous nodes`, or the server id an agent quotes:
```bash
deciduous nodes                          # 57   a1b2c3d4  goal  ...  (a1b2c3d4 is stable everywhere)
deciduous link a1b2c3d4 58 -r "builds on their goal"
```

## Parallel Agents: The Message Board

When more than one agent works at once, coordinate through deciduous, never through a scratch or markdown file. A file has no ids to answer, no way to ask what is waiting for you, and it is gone with the worktree it was written in.

- **Post** interface changes, questions and answers: `post_message` (MCP) or `deciduous board post --as <your-label> -s "subject" -m "body"`. Address other agents with `@label`.
- **Read** what is waiting for you: `read_messages` with `unanswered_for: "<your-label>"`, or `deciduous board read --unanswered <your-label>`. Do it at start, before touching a file another agent may be changing, and before finishing.
- **Reply** with `reply_to: <id>` (`--reply-to <id>`). A reply is what takes a question off the asker's list; a new post does not.

One board serves every git worktree of the repository, or the server when the project has a `[remote]`. Messages are not graph nodes: not in `graph.json`, not exported, not synced. With `DECIDUOUS_AGENT_LABEL` set in a session's environment, `board post` uses it for `--as`, and the `board-mentions.sh` hook shows that label's unanswered messages at session start and as new ones arrive.

## The Rule

```
LOG BEFORE YOU CODE, NOT AFTER.
CONNECT EVERY NODE TO ITS PARENT.
AUDIT FOR ORPHANS REGULARLY.
THE SERVER IS THE TRUTH: `deciduous remote pull` TO REFRESH, `deciduous remote status` TO CHECK.
COORDINATE PARALLEL AGENTS ON THE BOARD, NEVER IN A SCRATCH FILE.
```

**Live graph**: https://notactuallytreyanastasio.github.io/deciduous/
"#;

/// /recover for a project whose graph lives on a server (`[remote]`).
pub const RECOVER_MD_REMOTE: &str = r#"---
description: Recover context from decision graph and recent activity - USE THIS ON SESSION START
allowed-tools: Bash(deciduous:*, git:*, cat:*, tail:*)
argument-hint: [focus-area]
---

# Context Recovery

**RUN THIS AT SESSION START.** The decision graph is your persistent memory.

## Step 1: Query the Graph

```bash
# The graph lives on the server; refresh this machine's cache first
deciduous remote pull
deciduous remote status

# See all decisions (look for recent ones and pending status)
deciduous nodes

# Filter by current branch (useful for feature work)
deciduous nodes --branch $(git rev-parse --abbrev-ref HEAD)

# See how decisions connect
deciduous edges

# What commands were recently run?
deciduous commands

# Check for attached documents
deciduous doc list
```

**Branch-scoped context**: If working on a feature branch, filter nodes to see only decisions relevant to this branch. Main branch nodes are tagged with `[branch: main]`.

## Step 1.5: Audit Graph Integrity

**CRITICAL: Check that all nodes are logically connected.**

```bash
# Find nodes with no incoming edges (potential missing connections)
deciduous edges | cut -d'>' -f2 | cut -d' ' -f2 | sort -u > /tmp/has_parent.txt
deciduous nodes | tail -n+3 | awk '{print $1}' | while read id; do
  grep -q "^$id$" /tmp/has_parent.txt || echo "CHECK: $id"
done
```

**Review each flagged node (flow: goal -> options -> decision -> actions -> outcomes):**
- Root `goal` nodes are VALID without parents
- `option` nodes MUST link to their parent goal
- `decision` nodes MUST link from the option(s) being chosen
- `action` nodes MUST link to their parent decision
- `outcome` nodes MUST link back to their action

**Fix missing connections:**
```bash
deciduous link <parent_id> <child_id> -r "Retroactive connection - <reason>"
```

## Step 1.7: Check the Message Board

If other agents are working on this project at the same time, read what they have asked you before doing anything else:

```bash
deciduous board read --unanswered <your-label>   # or read_messages with unanswered_for
deciduous board read --since <last id you saw>    # everything new, addressed to you or not
```

Answer each with `deciduous board post --as <your-label> --reply-to <id> ...` (or `post_message` with `reply_to`). Coordinate there, never in a scratch or markdown file.

## Step 2: Check Git State

```bash
git status
git log --oneline -10
git diff --stat
```

## Step 3: Check Session Log

```bash
cat git.log | tail -30
```

## After Gathering Context, Report:

1. **Current branch** and pending changes
2. **Branch-specific decisions** (filter by branch if on feature branch)
3. **Recent decisions** (especially pending/active ones)
4. **Last actions** from git log and command log
5. **Open questions** or unresolved observations
6. **Attached documents** - diagrams, specs, or screenshots on key nodes
7. **Suggested next steps**
8. **Unanswered board messages** addressed to you, if other agents are working in parallel

### Branch Configuration

Check `.deciduous/config.toml` for branch settings:
```toml
[branch]
main_branches = ["main", "master"]  # Which branches are "main"
auto_detect = true                    # Auto-detect branch on node creation
```

---

## REMEMBER: Real-Time Logging Required

After recovering context, you MUST follow the logging workflow:

```
EVERY USER REQUEST -> Log goal/decision first
BEFORE CODE CHANGES -> Log action
AFTER CHANGES -> Log outcome, link nodes
BEFORE ENDING -> deciduous remote status
```

**The user is watching the graph live.** Log as you go, not after.

### Quick Logging Commands

```bash
# Root goal with user prompt (capture what the user asked for)
deciduous add goal "What we're trying to do" -c 90 -p "User asked: <their request>"

deciduous add action "What I'm about to implement" -c 85
deciduous add outcome "What happened" -c 95
deciduous link FROM TO -r "Connection reason"

# Capture prompt when user redirects mid-stream
deciduous add action "Switching approach" -c 85 -p "User said: use X instead"

deciduous remote status  # anything the server has not got?
```

**When to use `--prompt`:** On root goals (always) and when user gives new direction mid-stream. Downstream nodes inherit context via edges.

---

## Focus Areas

If $ARGUMENTS specifies a focus, prioritize context for:

- **auth**: Authentication-related decisions
- **ui** / **graph**: UI and graph viewer state
- **cli**: Command-line interface changes
- **api**: API endpoints and data structures

---

## The Memory Loop

```
SESSION START
    |
Run /recover -> See past decisions
    |
AUDIT -> Fix any orphan nodes first!
    |
DO WORK -> Log BEFORE each action
    |
CONNECT -> Link new nodes immediately
    |
AFTER CHANGES -> Log outcomes, observations
    |
AUDIT AGAIN -> Any new orphans?
    |
BEFORE ENDING -> deciduous remote status
    |
PUSH -> Live graph updates
    |
SESSION END -> Final audit
    |
(repeat)
```

**Live graph**: https://notactuallytreyanastasio.github.io/deciduous/

---

## The Graph Server

This project has a `[remote]` in `.deciduous/config.toml`: teammates' and agents' decisions are on the server, not in a file git carries. `deciduous remote pull` brings them into your local cache; `deciduous remote status` lists what differs, each with the command that fixes it. There is no `.deciduous/graph.json` to commit and no `deciduous sync` step. To link to a teammate's node use its change_id prefix (CHANGE column in `deciduous nodes`), not its local id.

## Why This Matters

- Context loss during compaction loses your reasoning
- The graph survives - query it early, query it often
- Retroactive logging misses details - log in the moment
- The user sees the graph live - show your work
- The graph server shares reasoning with every teammate and agent as it is written
"#;

/// /document for a project whose graph lives on a server (`[remote]`).
pub const DOCUMENT_MD_REMOTE: &str = r#"---
description: Document a file or directory comprehensively - shaking the tree to truly understand it
allowed-tools: Read, Glob, Grep, Bash(ls:*, find:*, wc:*, git log:*, git blame:*, deciduous:*), Write, Edit
argument-hint: <file-or-directory>
---

# Document

**Comprehensive documentation that shakes the tree to understand everything.**

This skill generates in-depth documentation for a file or directory, focusing on:
- Human readability while covering ALL surface area
- Linking to tests as working examples
- Refining tests to look more real-world if needed
- Integration with the deciduous decision graph

---

## Step 1: Create Documentation Goal Node

Before documenting, log what you're about to do:

```bash
deciduous add goal "Document $ARGUMENTS" -c 90 --prompt-stdin << 'EOF'
[User's verbatim documentation request]
EOF
```

Store the goal ID for linking later.

---

## Step 2: Understand the Target

### For a File

1. **Read the file completely**
   - Understand every function, class, type, and export
   - Note all imports and dependencies
   - Identify the file's role in the larger system

2. **Find tests for this file**
   - Look for test files with similar names
   - Search for imports/references in test directories
   - These will become working examples in the docs

3. **Trace callers/callees**
   - Who calls this file?
   - What does this file call?
   - Map the dependency graph

### For a Directory

1. **Map the structure**
   - List all files and their purposes
   - Identify the public API (index/mod files)
   - Find the entry point

2. **Understand relationships**
   - How do files in this directory interact?
   - What's the data flow?

3. **Find related tests**
   - Test directories that cover this code
   - Integration tests that exercise the whole module

---

## Step 3: Document Each Component

For each file/component, document:

### 3.1 Purpose
- One sentence: what does this do?
- Why does it exist? (The "why" is more important than the "what")

### 3.2 API Surface
For every public function/method/class:

```markdown
### `function_name(param1: Type, param2: Type) -> ReturnType`

**Purpose:** What this does and why you'd call it.

**Parameters:**
- `param1` - Description and valid values
- `param2` - Description and valid values

**Returns:** What the return value means

**Throws/Errors:** What can go wrong

**Example:**
```code
// From: tests/example_test.rs:42
let result = function_name("input", 42);
assert_eq!(result, expected);
```

**Related:** Links to related functions
```

### 3.3 Internal Architecture
- How does it work internally?
- What are the key data structures?
- What are the invariants?

### 3.4 Dependencies
- What does this depend on?
- What depends on this?

### 3.5 Tests as Examples

For each relevant test:
- Show the test as a working example
- Explain what the test demonstrates
- **If test is too synthetic/artificial, REFINE IT:**
  - Make variable names descriptive
  - Add comments explaining the scenario
  - Use realistic values instead of "foo", "bar", 123
  - Create a deciduous observation node noting the refinement

---

## Step 4: Create Documentation File

**Output location:**
- For file `src/auth/jwt.rs` -> `docs/src/auth/jwt.rs.md`
- For directory `src/auth/` -> `docs/src/auth/README.md`

**Document structure:**

```markdown
# <Component Name>

> One-sentence description

## Overview

High-level explanation of what this does and why it exists.

## Quick Start

```code
// Most common usage pattern - from real tests
```

## API Reference

[For each public function/type/constant]

### `function_name(...)`

[Generated from Step 3.2]

## Architecture

[Internal design, data flow, key invariants - from Step 3.3]

## Examples

[Tests converted to examples - from Step 3.5]

## Dependencies

[What this depends on, what depends on this - from Step 3.4]

## Related Documentation

- Links to other relevant docs
- Links to test files
```

---

## Step 5: Refine Tests If Needed

If tests are too synthetic (meaningless variable names, unrealistic values):

1. Read the test file
2. Improve it with:
   - Descriptive variable names (user_email, order_total, not x, y)
   - Realistic values (not "foo", "bar", 123)
   - Comments explaining the scenario
3. Edit the test file with improvements
4. Create observation node:

```bash
deciduous add observation "Refined tests for <component>" -c 85 -d "Made tests more real-world: descriptive variable names, realistic values, scenario comments added"
deciduous link <action_id> <observation_id> -r "Test improvements during documentation"
```

---

## Step 6: Link to Decision Graph

After documentation is complete:

```bash
# Create documentation action (if not already created)
deciduous add action "Documented <target>" -c 95 -f "<files-created>"

# Link to goal
deciduous link <goal_id> <action_id> -r "Documentation complete"

# Create outcome
deciduous add outcome "Documentation complete for <target>" -c 95
deciduous link <action_id> <outcome_id> -r "Successfully documented"

# Anything the graph server has not got?
deciduous remote status
```

---

## Step 7: Verify Coverage

**Checklist before completing:**

- [ ] Every public function documented
- [ ] Every parameter explained
- [ ] Every return value explained
- [ ] Every error case documented
- [ ] At least one example per function (from tests)
- [ ] Architecture overview included
- [ ] Dependencies mapped
- [ ] Links to tests included
- [ ] Tests refined if they were synthetic

If anything is missing, go back and fill it in. **Do not miss any surface area.**

---

## Decision Criteria

**What to document:**
- Public APIs (always)
- Complex internal logic (when it's not obvious)
- Design decisions (why, not just what)
- Edge cases and error handling
- Integration points

**What NOT to document:**
- Trivial getters/setters
- Auto-generated code
- Implementation details obvious from code

**How deep to go:**
- Deep enough that someone new could understand and use the code
- Deep enough that someone could modify it without breaking things
- Capture the "why" behind design decisions

---

## Example Usage

```bash
# Document a single file
/document src/auth/jwt.rs

# Document a directory
/document src/auth/

# Document the whole project
/document .
```

**What happens:**
1. Goal node created
2. Code analyzed thoroughly
3. Tests found and used as examples
4. Tests refined if synthetic
5. Documentation written to docs/
6. Action/outcome nodes created
7. Graph synced

---

## Integration with Documentation Enforcement

When documentation is created, the `require-documentation.sh` hook will recognize it exists. This creates a virtuous cycle:

1. Can't edit code without documentation (hook blocks)
2. Run `/document` to create documentation
3. Now code edits are allowed
4. When code changes significantly, re-run `/document`

---

## Quick Reference

```bash
# Document and generate docs
/document <path>

# After documenting, you can edit the file
# The require-documentation.sh hook will allow it
```

**Always creates:**
- Goal node (before starting)
- Action node (for the documentation work)
- Outcome node (on completion)
- Observation nodes (for test refinements)

**Now document: $ARGUMENTS**
"#;

/// /work for a project whose graph lives on a server (`[remote]`).
pub const WORK_MD_REMOTE: &str = r#"---
description: Start a work transaction - creates goal node BEFORE any implementation
allowed-tools: Bash(deciduous:*)
argument-hint: <goal title>
---

# Work Transaction

**USE THIS BEFORE STARTING ANY IMPLEMENTATION.**

This skill creates the required deciduous nodes BEFORE you write any code. The Edit/Write hooks will BLOCK you if you don't have a recent node.

## Step 1: Create the Goal Node

Based on $ARGUMENTS (or the user's most recent request), create a goal node:

```bash
# Create goal with the user's request captured verbatim
deciduous add goal "$ARGUMENTS" -c 90 --prompt-stdin << 'EOF'
[INSERT THE EXACT USER REQUEST HERE - VERBATIM, NOT SUMMARIZED]
EOF
```

**IMPORTANT**: The prompt must be the user's EXACT words, not your summary.

## Step 2: Announce the Goal ID

After creating the goal, tell the user:
- The goal ID that was created
- What you're about to implement
- That you'll create action nodes as you work

## Step 3: Before Each Major Edit

Before editing files, create an action node:

```bash
deciduous add action "What you're about to implement" -c 85 -f "file1.rs,file2.rs"
deciduous link <goal_id> <action_id> -r "Implementation step"
```

**Other agents working at the same time?** Before editing a file another agent may be changing, read `deciduous board read --unanswered <your-label>`, and post an interface change on the board (`deciduous board post --as <your-label> -s "..." -m "@owner ..."`) before you make it. Never coordinate through a scratch or markdown file.

## Step 4: After Completion

When the work is done:

```bash
# After committing
deciduous add outcome "What was accomplished" -c 95 --commit HEAD
deciduous link <action_id> <outcome_id> -r "Implementation complete"

# Anything the graph server has not got?
deciduous remote status
```

Before you report the work finished, check the board once more (`deciduous board read --unanswered <your-label>`) and answer what is there with `--reply-to <id>`.

## Step 5: Attach Supporting Documents (Optional)

If the work produced or referenced important files (diagrams, specs, screenshots):

```bash
deciduous doc attach <goal_id> path/to/diagram.png -d "Architecture diagram"
deciduous doc attach <action_id> path/to/spec.pdf --ai-describe
```

If the user shares images or drops in files not in the project, attach them to the most relevant active node.

## The Transaction Model

```
/work "Add feature X"
    |
Goal node created (ID: N)
    |
Action node before each edit (links to goal)
    |
Implementation happens (Edit/Write now allowed)
    |
git commit
    |
Outcome node with --commit HEAD (links to action)
    |
Attach supporting documents (optional)
    |
deciduous remote status
```

## What NOT to Log

**Only create nodes for user-visible project work, not your internal process.**

Do NOT create action nodes for reading code, running tests to check status, exploring the codebase, or planning your approach. These are your process, not the user's project decisions.

Action nodes should describe code being written or changed. Outcome nodes should describe results the user cares about. If the user wouldn't put it in a PR description, don't log it.

## Why This Matters

- **Hooks will block you** if no recent action/goal exists
- **Commits will remind you** to link them to the graph
- **The graph captures your reasoning** for future sessions
- **Context recovery works** because the graph has everything

## Quick Reference

```bash
# Start work
deciduous add goal "Feature title" -c 90 -p "User request"

# Before editing (required!)
deciduous add action "What I'm implementing" -c 85
deciduous link <goal> <action> -r "Implementation"

# After committing
deciduous add outcome "Result" -c 95 --commit HEAD
deciduous link <action> <outcome> -r "Complete"

# Attach documents (optional)
deciduous doc attach <goal> diagram.png -d "Description"

# Parallel agents: coordinate on the board, not in a file
deciduous board read --unanswered <label>
deciduous board post --as <label> -s "subject" -m "@other body" [--reply-to <id>]

# Before finishing: anything the server has not got?
deciduous remote status
```

**Now create the goal node for: $ARGUMENTS**
"#;

/// /sync for a project whose graph lives on a server (`[remote]`).
pub const SYNC_MD_REMOTE: &str = r#"---
description: Check this project's copy of the decision graph against the graph server and refresh it
allowed-tools: Bash(deciduous:*)
---

# Sync with the Graph Server

This project has a `[remote]` in `.deciduous/config.toml`. Its graph lives on a shared server, and `.deciduous/deciduous.db` is this machine's cache of it. There is no `.deciduous/graph.json` to pull, commit or merge, and `deciduous sync` is not the step here: CLI writes are queued in `.deciduous/remote-log.jsonl` and sent before each command exits, MCP tools write to the server directly.

## Step 1: What differs?

```bash
deciduous remote status
```

Lists the writes waiting in the log, the ones the server refused, and every node, edge and document that differs between this copy and the server, each with the command that fixes it. Exits 1 when anything differs.

## Step 2: Refresh

```bash
deciduous remote pull
```

Sends what is waiting, then refreshes the local cache from the server, deletions included.

## Step 3: Send what is stuck

```bash
deciduous remote push                   # writes still waiting (the server was down)
deciduous remote push --seed            # rows only this copy has, such as history from before the remote
deciduous remote push --retry-rejected  # ops the server refused, once the cause is fixed
```

## Linking across users

Local ids differ per machine. Refer to someone else's node by its change_id prefix (the CHANGE column in `deciduous nodes`) or by the server id an agent quotes:

```bash
deciduous link a1b2c3d4 42 -r "our action implements their goal"
```

Agent messages (`deciduous board`) are not part of the graph; with a `[remote]` they live on the server.

## Quick Reference

| Command | What it does |
|---------|--------------|
| `deciduous remote status` | What differs, and the command that fixes each; exit 1 if anything |
| `deciduous remote pull` | Send what is waiting, then refresh the local cache |
| `deciduous remote push` | Send what is waiting in the log |
| `deciduous nodes` | Shows the CHANGE prefix to use for cross-user links |
"#;
