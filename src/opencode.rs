//! OpenCode integration
//!
//! Generates and installs OpenCode configuration files for decision graph integration.
//! OpenCode uses TypeScript/JavaScript plugins and tools for hooks and automation.
//!
//! Directory structure:
//! - `.opencode/plugins/` - TypeScript plugins (hooks)
//! - `.opencode/commands/` - Custom slash commands (markdown)
//! - `.opencode/skills/<name>/SKILL.md` - Skills with OpenCode frontmatter
//! - `.opencode/agents/` - Custom agent definitions (markdown)
//! - `.opencode/tools/` - Custom tools (TypeScript)
//! - `opencode.json` - Configuration file
//! - `AGENTS.md` - Project instructions (equivalent to CLAUDE.md)

use crate::config::Config;
use colored::Colorize;
use serde_json::json;
use std::fs;
use std::path::Path;

/// OpenCode plugin for requiring action nodes before edits
pub const PLUGIN_REQUIRE_ACTION_NODE: &str = r#"// OpenCode Plugin: Require Action Node
// Checks for recent action/goal nodes before file edits
// This enforces the decision graph workflow: log BEFORE you code

import type { Plugin } from "@opencode-ai/plugin"

export const RequireActionNode: Plugin = async ({ $ }) => {
  return {
    "tool.execute.before": async (input, output) => {
      // Only check on edit and write tools
      if (input.tool !== "edit" && input.tool !== "write") {
        return
      }

      try {
        // Check if deciduous is initialized
        const fs = await import("fs")
        if (!fs.existsSync(".deciduous")) {
          return // No deciduous in this project, allow all edits
        }

        // Get recent nodes from deciduous
        const result = await $`deciduous nodes 2>/dev/null | tail -5`.quiet()
        const stdout = result.stdout.toString()
        const lines = stdout.trim().split("\n").filter((l: string) => l.trim())

        // Check for any goal or action node
        let hasRecentNode = false
        for (const line of lines) {
          if (line.match(/goal|action/i)) {
            hasRecentNode = true
            break
          }
        }

        if (!hasRecentNode && lines.length > 2) {
          // Write reminder to log file instead of console (console output corrupts TUI)
          const path = await import("path")
          const logFile = path.join(".deciduous", "plugin.log")
          const msg = `[${new Date().toISOString()}] REMINDER: No recent action/goal node found. Run: deciduous add goal "..." or deciduous add action "..."\n`
          fs.appendFileSync(logFile, msg)
        }
      } catch (error) {
        // If deciduous isn't available, continue silently
      }
    }
  }
}
"#;

/// OpenCode plugin for post-commit reminders
pub const PLUGIN_POST_COMMIT_REMINDER: &str = r#"// OpenCode Plugin: Post-Commit Reminder
// Reminds to link commits to the decision graph after git commit
// This ensures commits are connected to the reasoning that led to them

import type { Plugin } from "@opencode-ai/plugin"

export const PostCommitReminder: Plugin = async ({ $ }) => {
  return {
    "tool.execute.after": async (input) => {
      // Only check bash tool
      if (input.tool !== "bash") {
        return
      }

      // Check if deciduous is initialized
      const fs = await import("fs")
      if (!fs.existsSync(".deciduous")) {
        return
      }

      // Check if this was a git commit command
      const command = input.args?.command || ""
      if (!command.match(/^git commit/)) {
        return
      }

      try {
        // Get the latest commit info
        const hashResult = await $`git rev-parse --short HEAD 2>/dev/null`.quiet()
        const msgResult = await $`git log -1 --format=%s 2>/dev/null`.quiet()

        const commitHash = hashResult.stdout.toString().trim()
        const commitMsg = msgResult.stdout.toString().trim().slice(0, 50)

        // Write reminder to log file instead of console (console output corrupts TUI)
        const path = await import("path")
        const logFile = path.join(".deciduous", "plugin.log")
        const msg = `[${new Date().toISOString()}] POST-COMMIT: ${commitHash} "${commitMsg}" - Run: deciduous add outcome "..." --commit HEAD\n`
        fs.appendFileSync(logFile, msg)
      } catch (error) {
        // If git commands fail, skip the reminder
      }
    }
  }
}
"#;

/// OpenCode command template: /work
pub const COMMAND_WORK: &str = r#"---
description: Start a work transaction - creates goal node BEFORE any implementation
arguments:
  - name: GOAL
    description: The goal you're working towards
    required: true
---

# Work Transaction

**USE THIS BEFORE STARTING ANY IMPLEMENTATION.**

This skill creates the required deciduous nodes BEFORE you write any code. The Edit/Write hooks will BLOCK you if you don't have a recent node.

## Step 1: Create the Goal Node

Based on $GOAL (or the user's most recent request), create a goal node:

```bash
# Create goal with the user's request captured verbatim
deciduous add goal "$GOAL" -c 90 --prompt-stdin << 'EOF'
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

## Step 4: After Completion

When the work is done:

```bash
# After committing
deciduous add outcome "What was accomplished" -c 95 --commit HEAD
deciduous link <action_id> <outcome_id> -r "Implementation complete"

# Sync the graph
deciduous sync
```

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
deciduous sync
```

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

# Always sync
deciduous sync
```

**Now create the goal node for: $GOAL**
"#;

/// OpenCode command template: /recover
pub const COMMAND_RECOVER: &str = r#"---
description: Recover context from decision graph and recent activity - USE THIS ON SESSION START
arguments:
  - name: FOCUS
    description: Optional focus area to filter by (e.g. auth, ui, cli, api)
    required: false
---

# Context Recovery

**RUN THIS AT SESSION START.** The decision graph is your persistent memory.

## Step 1: Query the Graph

```bash
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
BEFORE GIT PUSH -> deciduous sync
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

deciduous sync  # Do this frequently!
```

**When to use `--prompt`:** On root goals (always) and when user gives new direction mid-stream. Downstream nodes inherit context via edges.

---

## Focus Areas

If $FOCUS specifies a focus, prioritize context for:

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
BEFORE PUSH -> deciduous sync
    |
PUSH -> Live graph updates
    |
SESSION END -> Final audit
    |
(repeat)
```

---

## Multi-User Sync

If working in a team, sync decision graphs automatically via events:

```bash
# Check sync status
deciduous events status

# Apply teammate events (after git pull)
deciduous events rebuild

# Periodic maintenance (compact old events)
deciduous events checkpoint --clear-events
```

Events are auto-emitted when you use `add`, `link`, `status`, etc.
Git handles merging everyone's event files automatically.

## Why This Matters

- Context loss during compaction loses your reasoning
- The graph survives - query it early, query it often
- Retroactive logging misses details - log in the moment
- The user sees the graph live - show your work
- Patches share reasoning with teammates
"#;

/// OpenCode command template: /decision
pub const COMMAND_DECISION: &str = r#"---
description: Manage decision graph - track algorithm choices and reasoning
arguments:
  - name: ACTION
    description: "Command: add <type> <title>, link <from> <to>, nodes, edges, sync, etc."
    required: true
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

## Quick Commands

Based on $ACTION:

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
- `add obs <title>` -> `deciduous add observation "<title>" -c 80`
- `add outcome <title>` -> `deciduous add outcome "<title>" -c 90`
- `add revisit <title>` -> `deciduous add revisit "<title>" -c 75`

### Optional Flags for Nodes
- `-c, --confidence <0-100>` - Confidence level
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

### Sync Graph
- `sync` -> `deciduous sync`

### Multi-User Sync (Event-Based) - RECOMMENDED
- `events init` -> `deciduous events init` (initialize event-based sync)
- `events status` -> `deciduous events status` (show pending events)
- `events rebuild` -> `deciduous events rebuild` (apply teammate events)
- `events checkpoint` -> `deciduous events checkpoint` (create snapshot)
- `events checkpoint --clear-events` -> snapshot and clear old events

### Multi-User Sync (Legacy Diff/Patch)
- `diff export -o <file>` -> `deciduous diff export -o <file>` (export nodes as patch)
- `diff export --nodes 1-10 -o <file>` -> export specific nodes
- `diff export --branch feature-x -o <file>` -> export nodes from branch
- `diff apply <file>` -> `deciduous diff apply <file>` (apply patch, idempotent)
- `diff apply --dry-run <file>` -> preview without applying
- `diff status` -> `deciduous diff status` (list patches in .deciduous/patches/)
- `migrate` -> `deciduous migrate` (add change_id columns for sync)

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
| `observation` | Finding or data point | "Existing code uses sessions" |
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
- Before every `deciduous sync`
- After creating multiple nodes quickly
- At session end
- When the web UI graph looks disconnected

## Git Staging Rules - CRITICAL

**NEVER use broad git add commands that stage everything:**
- `git add -A` - stages ALL changes including untracked files
- `git add .` - stages everything in current directory
- `git add -a` or `git commit -am` - auto-stages all tracked changes
- `git add *` - glob patterns can catch unintended files

**ALWAYS stage files explicitly by name:**
- `git add src/main.rs src/lib.rs`
- `git add Cargo.toml Cargo.lock`
- `git add .claude/commands/decision.md`

**Why this matters:**
- Prevents accidentally committing sensitive files (.env, credentials)
- Prevents committing large binaries or build artifacts
- Forces you to review exactly what you're committing
- Catches unintended changes before they enter git history

## Multi-User Sync

**Problem**: Multiple users work on the same codebase, each with a local `.deciduous/deciduous.db` (gitignored). How to share decisions?

**Solution**: Event-based sync with append-only logs. Each user has their own event file that git merges automatically.

### Event-Based Sync (Recommended)

**Setup (once per repo):**
```bash
deciduous events init
git add .deciduous/sync/
git commit -m "feat: enable event-based sync"
```

**Daily workflow:**
```bash
git pull                    # Get teammate events
deciduous events rebuild    # Apply to local DB
# Work normally - events auto-emit on add/link/etc.
git add .deciduous/sync/ && git commit -m "sync" && git push
```

**Periodic maintenance:**
```bash
deciduous events checkpoint --clear-events  # Compact old events
git add .deciduous/sync/ && git commit -m "checkpoint"
```

### Legacy Patch Workflow

For manual control, use the older patch system:

```bash
# Export nodes as a patch file
deciduous diff export --branch feature-x -o .deciduous/patches/my-feature.json

# Apply patches from teammates
deciduous diff apply .deciduous/patches/*.json
```

## The Rule

```
LOG BEFORE YOU CODE, NOT AFTER.
CONNECT EVERY NODE TO ITS PARENT.
AUDIT FOR ORPHANS REGULARLY.
SYNC BEFORE YOU PUSH.
EXPORT PATCHES FOR YOUR TEAMMATES.
```

**Live graph**: https://notactuallytreyanastasio.github.io/deciduous/
"#;

/// OpenCode command template: /build-test
pub const COMMAND_BUILD_TEST: &str = r#"---
description: Build the project and run the test suite
arguments:
  - name: PATTERN
    description: Optional test pattern to filter tests
    required: false
---

# Build and Test

Build the project and run the test suite.

## Instructions

1. Run the full build and test cycle:
   ```bash
   cargo build --release && cargo test
   ```

2. If tests fail, analyze the failures and explain:
   - Which test failed
   - What it was testing
   - Likely cause of failure
   - Suggested fix

3. If all tests pass, report success and any warnings from the build.

4. If the user specifies a specific test pattern, run only those tests:
   ```bash
   cargo test $PATTERN
   ```

## Test categories in this project
- `test_public_exports` - API verification
- `test_filter_graph` - Graph filtering
- `test_extract_commit` - Commit extraction from metadata
- `test_extract_confidence` - Confidence extraction from metadata
- `test_graph_to_dot` - DOT export
- `test_generate_writeup` - PR writeup generation
"#;

/// OpenCode command template: /serve-ui
pub const COMMAND_SERVE_UI: &str = r#"---
description: Start the decision graph web viewer
arguments:
  - name: PORT
    description: Port to run the server on (default 3000)
    required: false
---

# Start Decision Graph Viewer

Launch the deciduous web server for viewing and navigating the decision graph.

## Instructions

1. Start the server:
   ```bash
   deciduous serve --port ${PORT:-3000}
   ```

2. Inform the user:
   - The server is running at http://localhost:${PORT:-3000}
   - The graph auto-refreshes every 30 seconds
   - They can browse decisions, chains, and timeline views
   - Changes made via CLI will appear automatically

3. The server will run in the foreground. Remind user to stop it when done (Ctrl+C).

## UI Features
- **Chains View**: See decision chains grouped by goals
- **Timeline View**: Chronological view of all decisions
- **Graph View**: Interactive force-directed graph
- **DAG View**: Directed acyclic graph visualization
- **Detail Panel**: Click any node to see full details including:
  - Node metadata (confidence, commit, prompt, files)
  - Connected nodes (incoming/outgoing edges)
  - Timestamps and status
  - Attached documents

## Alternative: Static Hosting

For GitHub Pages or other static hosting:
```bash
deciduous sync  # Exports to docs/graph-data.json
```

Then push to GitHub - the graph is viewable at your GitHub Pages URL.
"#;

/// OpenCode command template: /sync-graph
pub const COMMAND_SYNC_GRAPH: &str = r#"---
description: Sync the decision graph to GitHub Pages
arguments: []
---

# Sync Decision Graph to GitHub Pages

Export the current decision graph to docs/graph-data.json so it's deployed to GitHub Pages.

## Steps

1. Run `deciduous sync` to export the graph
2. Show the user how many nodes/edges were exported
3. If there are changes, stage them: `git add docs/graph-data.json`

This should be run before any push to main to ensure the live site has the latest decisions.
"#;

/// OpenCode command template: /document
pub const COMMAND_DOCUMENT: &str = r#"---
description: Document a file or directory comprehensively - shaking the tree to truly understand it
arguments:
  - name: TARGET
    description: File or directory path to document
    required: true
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
deciduous add goal "Document $TARGET" -c 90 --prompt-stdin << 'EOF'
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
deciduous add observation "Refined tests for <component> - made more real-world" -c 85
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

# Sync
deciduous sync
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

**Now document: $TARGET**
"#;

/// OpenCode command template: /sync
pub const COMMAND_SYNC: &str = r#"---
description: Sync decision graph with teammates - pull events, rebuild, push
arguments: []
---

# Multi-User Sync

Synchronize decision graph with your team using event-based sync.

## Step 1: Pull Latest

```bash
git pull --rebase
```

## Step 2: Check Sync Status

```bash
deciduous events status
```

Look for:
- **Pending events**: Events from teammates not yet in your local DB
- **Event files**: Each teammate has their own `.jsonl` file

## Step 3: Rebuild if Needed

If there are pending events:

```bash
# Preview what would change
deciduous events rebuild --dry-run

# Apply teammate events to your local database
deciduous events rebuild
```

## Step 4: Push Your Changes

```bash
# Stage sync files (events are auto-committed to your event file)
git add .deciduous/sync/

# Commit and push
git commit -m "sync: decision graph events"
git push
```

## Checkpoint (Periodic Maintenance)

To prevent repo bloat, periodically create a checkpoint:

```bash
# Create checkpoint and clear old events
deciduous events checkpoint --clear-events

# Commit the checkpoint
git add .deciduous/sync/
git commit -m "checkpoint: compact decision graph events"
git push
```

**When to checkpoint:**
- After major milestones
- When event files get large (>100KB)
- Before releases

## Troubleshooting

### Events not syncing?

1. Make sure `.deciduous/sync/` is tracked in git
2. Check that `deciduous events init` was run
3. Verify events are being emitted: `deciduous events status`

### Merge conflicts in event files?

Event files are append-only JSONL. Git should auto-merge them.
If conflicts occur, accept both versions (both sets of events are valid).

### Missing nodes after rebuild?

Nodes reference each other by `change_id` (UUID), not local `id`.
If edges fail, the referenced node may be in a teammate's events
that haven't been pulled yet. Pull and rebuild again.

## Quick Reference

| Command | What it does |
|---------|--------------|
| `deciduous events status` | Show pending events, authors, file sizes |
| `deciduous events rebuild` | Apply all events to local DB |
| `deciduous events rebuild --dry-run` | Preview without applying |
| `deciduous events checkpoint` | Snapshot current state |
| `deciduous events checkpoint --clear-events` | Snapshot + delete old events |
| `deciduous events emit <id>` | Manually emit event for a node |
"#;

/// OpenCode command template: /decision-graph
pub const COMMAND_DECISION_GRAPH: &str = r#"---
description: Build a deciduous decision graph capturing design evolution from commit history
arguments:
  - name: REPO
    description: Path to the repository to analyze (default current directory)
    required: false
---

# Decision Graph Construction

You are building a **deciduous decision graph** - a DAG that captures the evolution of design decisions in a codebase.

**Target repository:** $REPO (if provided), otherwise the current directory.

Use the `deciduous` CLI (at ~/.cargo/bin/deciduous) to build the graph. Run deciduous commands in the current directory (not inside the source repo).

For git commands to explore commit history, use `git -C <repo-path>` to target the source repo.

**CRITICAL: Only use information from the repository itself (commits, code, comments, tests). Do not use your prior knowledge about the project. Everything must be grounded in what you find in the repo.**

## Commit Exploration

Use a layered strategy to find all relevant commits:

**Layer 1: See all commits.** Start with the full list when building narratives.

```bash
git log --oneline --after="..." --before="..." -- path/
```

**Layer 2: Keyword expansion.** Once you have narratives, search for spelling variations and related terms you might have missed (e.g., "cache" -> "caching", "cached", "LRU", "invalidate"). For each key identifier in your narratives, trace its full lifecycle:

- Introduction
- Changes and modifications
- Renames
- Deprecation or removal
- Replacement by other mechanisms
- Becoming stable/public API

If there's a feature flag controlling the feature, search for commits mentioning that flag.

**Layer 3: Follow authors.** If a narrative has a key author, check their commits +/-1 month from known commits. They often work on related things.

**Layer 4: Pull request context.** If the `gh` CLI is available, use it to find PRs associated with key commits and paths. PRs often contain design discussion, review comments, and rationale that never appears in commit messages.

```bash
# Find PRs that touched a specific path
gh pr list --state merged --search "path/to/module" --limit 50

# Find the PR that introduced a specific commit
gh pr list --state merged --search "<commit-sha>" --limit 5

# Read PR description and review comments for context
gh pr view <pr-number>
gh api repos/{owner}/{repo}/pulls/<pr-number>/comments

# Search PR titles/bodies for keywords from your narratives
gh pr list --state merged --search "<keyword>" --limit 30
```

PR descriptions and review threads are goldmines for understanding **why** decisions were made. Reviewers often challenge approaches, surface alternatives that were considered, and document trade-offs - exactly the kind of reasoning that belongs in the decision graph.

When you find relevant PR discussion:
- Use it to enrich observation and decision node descriptions
- Quote reviewer comments as evidence (e.g., "PR #42 review: 'We should use Redis instead because...'")
- Note when a PR was blocked or revised - these are often pivot points

### DO NOT:

- `git log ... | head -100` -- **NO.** You will miss commits in the middle.
- `git log ... | tail -200` -- **NO.** Same problem.
- Start with keyword filtering -- **NO.** You'll miss things with unexpected names.

### DO:

- See all commits first, filter mentally while building narratives
- Include "remove", "delete", "disable", "deprecate" in keyword searches -- removals explain transitions
- Check the commit count first (`| wc -l`), but then see them all
- **Read full commit messages** for any commit whose title mentions an identifier or concept relevant to your narrative -- you need precise understanding of what happened to each one you care about

## Finding the Story

Not every commit matters. Look for commits that change **the model** - how the system conceptualizes the problem:

- Existing tests modified (contract changing, not just bugs fixed)
- Data structures replaced or reworked
- Heuristics changed significantly
- New abstractions introduced
- API behavior shifts

Skip commits that are pure implementation (same model, different code) or routine fixes that just add tests.

Among model-changing commits, find the **spine**: what question keeps getting re-answered? What approach keeps getting replaced or refined? That's your central thread - build the graph around it.

## Narrative Tracking

**Don't build the graph as you explore.** First, collect commits into narratives.

Maintain `narratives.md` as you explore:

1. For each significant commit, read `narratives.md`
2. Ask: "Does this commit evolve an existing narrative?"
3. If yes: append the commit to that narrative's section
4. If no: add a new narrative section

Example `narratives.md`:

```
## Cache Strategy
- a1b2c3d: Add in-memory cache
- e4f5g6h: Cache invalidation issues
- i7j8k9l: Switch to Redis

## API Rate Limiting
- m1n2o3p: Add basic throttling
- ...
```

**Before building the graph**, take a critical pass over `narratives.md`:

- Merge narratives that are essentially the same evolving thing
- Ensure each narrative clearly explains how one independent piece evolved
- Note where narratives branch from or feed into each other

## Hardening Phase

After building initial narratives, harden them to ensure nothing is missed.

### Step 1: Extract concepts per narrative

For each narrative, list the key concepts/APIs/identifiers and their lifecycle stage:

- **Introduced**: First appearance of the concept
- **Changed**: Modifications to behavior or implementation
- **Renamed/Deprecated/Removed**: End of life or replacement
- **Marked stable**: Became public API or removed "unstable_" prefix

Example addition to narrative:

```
## Cache Strategy
Concepts: cache, LRUCache, cacheTimeout, invalidate

Lifecycle:
- cache: introduced (a1b2c3d), changed (e4f5g6h), renamed to LRUCache (x1y2z3)
- cacheTimeout: introduced (e4f5g6h), removed (i7j8k9l)
- LRUCache: introduced via rename (x1y2z3), marked stable (p1q2r3)

Commits:
- a1b2c3d: Add in-memory cache
- ...
```

### Step 2: Exhaustive search per concept

For each concept, search full commit messages (not just subject lines):

```bash
git log --all --after="..." --before="..." --grep="<concept>" --format="%H %s" -- path/
```

For each match, read the full commit message:

```bash
git show <sha> --format="%B" --no-patch
```

### Step 3: Rewrite narratives with gaps filled

Rewrite `narratives.md` integrating any newly discovered commits. The rewritten version should:

- Include ALL commits found for each concept
- Update the lifecycle tracking for each concept
- Ensure the arc is complete (if something was introduced, when was it changed/removed?)

If a concept has an incomplete arc (e.g., introduced but never removed, yet it's not in current code), investigate further.

## Cross-Narrative Connections

When building the graph, don't just branch everything from the goal. Capture how narratives relate:

**Branch from the spine, not goal:** If a narrative arose from work in another narrative, branch from that work.

- Wrong: `goal -> "How to preserve state?"`
- Right: `outcome("timeout works") -> "How to preserve state?"` (the question arose after implementing timeout)

**Observations feed back:** If an observation in one narrative influenced decisions in another, add an edge.

- Example: An observation about nested boundary timing might inform heuristics refinement in the main narrative

**Keep truly independent things from goal:** If a narrative is genuinely a separate concern that doesn't arise from other work, branching from goal is appropriate.

After consolidating, build the graph - one decision chain per narrative, with cross-links where they connect.

## Node Types

| Type            | Purpose                                               |
| --------------- | ----------------------------------------------------- |
| **goal**        | High-level objective being pursued                    |
| **decision**    | A choice point with multiple possible paths           |
| **option**      | A possible approach to a decision                     |
| **observation** | Learning, insight, or new information discovered      |
| **action**      | Something that was done (must reference a commit)     |
| **outcome**     | Result or consequence of an action                    |
| **revisit**     | Pivot point where a previous approach is reconsidered |

## CLI Commands

```bash
# Add nodes (returns node ID)
deciduous add goal "Title of the goal"
deciduous add decision "The question or choice point"
deciduous add option "One possible approach"
deciduous add observation "Something learned or discovered"
deciduous add action "Descriptive title of what was done"
deciduous add outcome "What resulted from the action"
deciduous add revisit "Reconsidering previous approach"

# Add nodes with descriptions (use -d for explanations and sources)
deciduous add action "Title" -d "Explanation of what happened and why.

Sources:
- abc123: 'Relevant quote from commit message'"

# Set status on options
deciduous status <id> rejected    # option that wasn't chosen
deciduous status <id> completed   # option that was chosen

# Connect nodes (from -> to means "from leads_to to")
deciduous link <from-id> <to-id>
deciduous link <from-id> <to-id> -r "Why this led to that"

# View/restructure
deciduous nodes           # list all
deciduous edges           # list connections
deciduous unlink <from> <to>   # remove edge
deciduous delete <id>          # remove node and edges
```

## Narrative Discipline

You're not collecting facts - you're crafting a story. Every node needs a _raison d'etre_.

Before adding a node, stop and ask: **Why does this exist? What prompted it?**

- Is this commit evolving something that's already in the graph? Then connect it there - it's a continuation, not a new branch.
- Is this a response to an observation about existing work? Then chain from that observation - something was learned, and this is the reaction.
- Did the winds change? Look for the moment the team realized the old approach wasn't working. That's an observation node, and it's the bridge to what came next.

**Don't branch from the goal unless it's genuinely new.** If you're about to draw an edge from the root goal, ask: does this replace or refine something we already designed? If yes, find that thing and connect there instead.

The test: can someone read your graph and understand not just _what_ happened, but _why_ each thing happened? Every node should feel inevitable given what came before it.

Think of commits as chapters in a story. Each chapter exists because of what happened in previous chapters. Your job is to find those causal threads and make them explicit.

## Temporal Rule

**Time flows forward. Past influences future, never reverse.**

Options under a decision are alternatives considered _at the same time_. If an approach was tried, failed, and a new approach was designed later - that's a **new decision node**, connected by observations about why the old approach failed.

Example - DON'T model sequential attempts as parallel options:

```
# WRONG - these were decided years apart, not simultaneously
decision: "How to handle caching?"
  |-> option: in-memory cache (2019)
  |-> option: Redis (2020)
  |-> option: CDN (2021)
```

Example - DO model as chain of decisions with learning:

```
# RIGHT - each attempt informs the next, options are simultaneous alternatives
decision: "How to handle caching?" (2019)
  |-> option: in-memory cache [chosen]
  |-> option: no caching [rejected] "Perf requirements too strict"
        |
option: in-memory cache -> action -> outcome
        |
observation: "Doesn't scale across instances"
        |
decision: "How to share cache across instances?" (2020)
  |-> option: Redis [chosen] "Team has Redis experience"
  |-> option: Memcached [rejected] "Less feature-rich"
  |-> option: database caching [rejected] "Adds DB load"
        |
option: Redis -> action -> outcome
        |
observation: "Latency too high for hot paths"
        |
decision: "How to reduce latency for static assets?" (2021)
  |-> option: CDN [chosen]
```

Multiple observations can converge into one decision. Multiple options can branch from one decision. But the graph flows forward in time.

## Edge Types

Use specific edge types to show relationships:

```bash
deciduous link <from> <to> -t chosen -r "Why this was selected"
deciduous link <from> <to> -t rejected -r "Why this wasn't selected"
deciduous link <from> <to> -t leads_to -r "How this led to that"
```

- `decision --chosen--> option` - This option was selected
- `decision --rejected--> option` - This option was considered but not selected (with rationale)
- `decision --leads_to--> option` - Lists available options

For post-hoc abandonment (tried something, it failed later):

- Mark the option status as `rejected`: `deciduous status <id> rejected`
- Create an observation explaining why it failed
- Link observation to the new decision it triggered

## Link Patterns (goal -> options -> decision -> actions -> outcomes)

- `goal -> option` - Goal leads to possible approaches
- `option -> decision` - Options lead to choosing (use chosen/rejected edge types)
- `decision -> action` - Chosen option leads to implementation
- `action -> outcome` - Action produces result
- `outcome -> observation` - Result reveals new insight
- `observation -> option` - Insight suggests new approach (feeds back to options)
- `observation -> revisit` - Insight forces reconsideration of previous approach
- `revisit -> option` - Pivot leads to exploring new options

When a design approach is abandoned and replaced:

```bash
deciduous add observation "JWT too large for mobile"
deciduous add revisit "Reconsidering token strategy"
deciduous link <observation> <revisit> -r "forced rethinking"
deciduous status <old_decision> superseded
```

Revisit nodes connect old approaches to new ones, capturing WHY things changed.

## Grounding Requirements

1. **Actions must cite commits**: Every action node must reference a real commit SHA in its description. Use `-d` when adding the node.

2. **Observations from evidence**: Observations should come from commit messages, code comments, or test descriptions you find in the repo.

3. **No speculation**: If you can't find evidence for something in the repo, don't include it. An incomplete but grounded graph is better than a complete but speculative one.

4. **Quote sources**: When possible, quote or paraphrase the actual commit message or comment that supports a node.

## Rich Node Content

The graph is an **alternative interface to browsing commit history**. Someone reading a node should understand what happened without looking up commits.

**Every node needs a description** - especially outcome and observation nodes. The description should be readable to someone exploring the graph who doesn't have the commits open.

### Structure: Explanation first, then sources

1. **Start with a readable explanation** that makes sense within the narrative. What happened? Why? How does it connect to what came before?
2. **Add sources below** with direct quotes from commits that support the explanation.

If you find your explanation doesn't make sense in context - something feels like a leap or a gap - that's a signal to dig deeper. There's probably a missing commit or transition you haven't found yet.

The relationship is many-to-many:

- One node may reference multiple commits (a decision informed by several changes)
- One commit may appear in multiple nodes (a large commit touching several concerns)

### Example

```bash
deciduous add decision "Should we switch from SQL to a document store?" -d "The team decided to switch from SQL to a document store.
This eliminated the impedance mismatch between the object model and storage,
at the cost of losing ad-hoc query capability (which wasn't being used anyway).

Sources:
- a1b2c3d: 'Our access patterns are almost entirely key-value lookups. The
  JOIN operations we wrote are never actually used in production.'
- e4f5g6h: 'Document store removes the ORM layer entirely - one less thing
  to maintain and debug.'"
```

### DO NOT:

- Leave nodes with just a title and no description
- Put quotes first without explaining what they mean
- Write explanations that don't make sense in the narrative flow (this signals missing context)

### DO:

- Write explanations that flow naturally from previous nodes
- Include direct quotes that support the explanation
- Treat gaps in the story as prompts to investigate further

## Output

When done, run `deciduous graph > graph.json` to export.
"#;

/// OpenCode skill template: /pulse
pub const SKILL_PULSE: &str = r#"---
description: Map the current model as decisions - no history, just now
arguments:
  - name: SCOPE
    description: "What part of the system to map (e.g., 'Authentication', 'API rate limiting')"
    required: true
---

# Pulse

**Map the current model as decisions. No history, just now.**

## Step 1: Get current state

```bash
deciduous pulse
```

Review the report: active goals, coverage gaps, orphan nodes. This tells you what's already mapped and what needs attention.

## Step 2: Pick a scope

What part of the system are you taking the pulse of?

- A feature ("Suspense fallback behavior")
- A subsystem ("Authentication")
- A boundary ("API request lifecycle")

Scope: **$SCOPE**

## Step 3: Ask "What decisions define this?"

Read the code. For the thing you're scoping, ask:

> "What design questions had to be answered for this to work?"

Not implementation questions ("which library?") - model questions ("what's the behavior?")

## Step 4: Build the goal -> options -> decisions

```bash
# Create the root goal
deciduous add goal "$SCOPE: <Core question>" -c 90

# Add options (possible approaches from the goal)
deciduous add option "<Possible approach>" -c 85
deciduous link <goal> <option> -r "possible_approach"

# When an option is chosen, create a decision
deciduous add decision "Chose <approach>" -c 90
deciduous link <option> <decision> -r "chosen"
```

If a question is still open, leave it as option nodes without a decision.

## Step 5: Review

```bash
# Check the pulse again to see what's mapped
deciduous pulse

# Check for coverage gaps
deciduous pulse --summary

# View visually
deciduous serve
```

## Check for Supporting Documents

If the system has architecture diagrams, specs, or reference docs relevant to the scope:

```bash
deciduous doc list <goal_id>
deciduous doc attach <goal_id> docs/architecture.png -d "Current architecture"
```

## Decision Criteria

- **Worth capturing?** Does it define BEHAVIOR, not implementation?
- **How deep?** Stop when decisions become implementation details
- **Option vs Decision?** Option = possible approach. Decision = choosing which option.

## Connecting to History

Pulse gives you the "Now". For history, run `/narratives` then `/archaeology`.
"#;

/// OpenCode skill template: /narratives
pub const SKILL_NARRATIVES: &str = r#"---
description: Understand how a system evolved - narratives are the source of truth
arguments:
  - name: FOCUS
    description: "What part of the system to trace (e.g., 'auth', 'caching', 'API')"
    required: true
---

# Narrative Tracking

**Narratives are the source of truth. Commits are just evidence.**

## Step 1: Initialize narratives file

```bash
deciduous narratives init
```

This creates `.deciduous/narratives.md` pre-populated with your active goal titles.

## Step 2: Understand the system first

Before looking at git, read the code. Ask: **What are the major pieces of this system?**

Each major piece probably has a narrative behind it.

Focus area: **$FOCUS**

## Step 3: Fill in the narratives

Edit `.deciduous/narratives.md`. For each section:

1. Describe the **current state** (how it works today)
2. Infer the **evolution** (how it likely got this way)
3. Identify **PIVOTs** (when the conceptual model changed)
4. Find evidence (PRs, commits, docs) - optional
5. Check attached documents (`deciduous doc list`) - diagrams or specs may provide evidence

Signs of a pivot:
- Two approaches coexisting (migration in progress)
- Comments explaining "we used to do X"
- Config for old + new system
- Deprecation warnings

## Step 4: Review narratives

```bash
deciduous narratives show
```

## Step 5: Check existing pivots

```bash
deciduous narratives pivots
```

This shows all revisit nodes already in the graph with their full chains.

## Output Format

Each narrative section in `.deciduous/narratives.md`:

```markdown
## <Name>
> <One sentence: what this piece of the system does>

**Current state:** <How it works today>

**Evolution:**
1. <First approach> - <why>
2. **PIVOT:** <what changed> - <why it changed>
3. <Current approach> - <why this is better>

**Evidence:** <Optional: PRs, commits, docs>
**Connects to:** <Other narratives this influenced>
**Status:** active | superseded | abandoned
```

## What Makes a Good Narrative

- Coherent story about ONE design aspect
- Explains HOW something works and WHY it evolved
- Would help a new team member understand the system
- NOT a list of commits or feature changelog

## Next Step

After narratives are written, run `/archaeology` to transform them into a queryable decision graph.
"#;

/// OpenCode skill template: /archaeology
pub const SKILL_ARCHAEOLOGY: &str = r#"---
description: Transform narratives into a queryable decision graph
arguments: []
---

# Archaeology

**Transform narratives into a queryable decision graph.**

Run `/narratives` first to create `.deciduous/narratives.md`.

## Step 1: Read the narratives

```bash
deciduous narratives show
```

For each narrative, you'll create a subgraph.

## Step 2: Create root goals

For each narrative, create a backdated goal:

```bash
deciduous add goal "<Narrative title>" -c 90 --date "YYYY-MM-DD"
```

## Step 3: Build initial approaches

```bash
deciduous add decision "<First approach>" -c 85 --date "YYYY-MM-DD"
deciduous link <goal> <decision> -r "Initial design"
```

## Step 4: Create pivots with `archaeology pivot`

For each **PIVOT** in a narrative, use the atomic pivot command:

```bash
# One command replaces 7 manual add/link/status commands
deciduous archaeology pivot <from_id> "<what was learned>" "<new approach>" -c 85 -r "<why it failed>"
```

This automatically creates:
- observation node (what was learned)
- revisit node (reconsidering the old approach)
- decision node (the new approach)
- All 3 linking edges
- Marks the old approach as superseded

Preview before executing:
```bash
deciduous archaeology pivot <from_id> "observation" "new approach" --dry-run
```

## Step 5: Connect narratives

When narratives reference each other:

```bash
deciduous link <auth_observation> <ratelimit_decision> \
  -r "Auth failures drove rate limit redesign"
```

## Step 6: Mark superseded paths

For nodes that were replaced but not part of a pivot:

```bash
# Single node
deciduous archaeology supersede <id>

# Node and all descendants
deciduous archaeology supersede <id> --cascade
```

## Step 7: Review the timeline

```bash
# See all nodes chronologically
deciduous archaeology timeline

# Filter by type
deciduous archaeology timeline --type revisit

# See existing pivot chains
deciduous narratives pivots

# Visual exploration
deciduous serve
```

## Attach Evidence Documents

If you find diagrams, screenshots, or specs that support the archaeology:

```bash
deciduous doc attach <goal_id> evidence/old-architecture.png -d "Architecture before refactor"
deciduous doc attach <revisit_id> evidence/perf-report.pdf --ai-describe
```

Documents provide visual/tangible evidence alongside commit-based grounding.

## Querying the Graph

```bash
# Current state
deciduous pulse

# Pivot points
deciduous narratives pivots

# Timeline
deciduous archaeology timeline

# By status
deciduous nodes --type revisit
```

## What NOT to Do

- **Don't create nodes for every commit.** Commits are evidence, not graph nodes.
- **Don't create implementation nodes.** The graph is about the MODEL, not the code.
- **Don't over-structure.** Simple narratives might just be: goal -> option -> decision.
"#;

/// OpenCode skill: /pulse (SKILL.md format for .opencode/skills/pulse/SKILL.md)
pub const SKILL_PULSE_OPENCODE: &str = r#"---
name: pulse
description: Map the current model as decisions - no history, just now
compatibility: opencode
---

# Pulse

**Map the current model as decisions. No history, just now.**

## Step 1: Get current state

```bash
deciduous pulse
```

Review the report: active goals, coverage gaps, orphan nodes. This tells you what's already mapped and what needs attention.

## Step 2: Pick a scope

What part of the system are you taking the pulse of?

- A feature ("Suspense fallback behavior")
- A subsystem ("Authentication")
- A boundary ("API request lifecycle")

## Step 3: Ask "What decisions define this?"

Read the code. For the thing you're scoping, ask:

> "What design questions had to be answered for this to work?"

Not implementation questions ("which library?") - model questions ("what's the behavior?")

## Step 4: Build the goal -> options -> decisions

```bash
# Create the root goal
deciduous add goal "<Scope>: <Core question>" -c 90

# Add options (possible approaches from the goal)
deciduous add option "<Possible approach>" -c 85
deciduous link <goal> <option> -r "possible_approach"

# When an option is chosen, create a decision
deciduous add decision "Chose <approach>" -c 90
deciduous link <option> <decision> -r "chosen"
```

If a question is still open, leave it as option nodes without a decision.

## Step 5: Review

```bash
deciduous pulse
deciduous pulse --summary
deciduous serve
```

## Check for Supporting Documents

```bash
deciduous doc list <goal_id>
deciduous doc attach <goal_id> docs/architecture.png -d "Current architecture"
```

## Decision Criteria

- **Worth capturing?** Does it define BEHAVIOR, not implementation?
- **How deep?** Stop when decisions become implementation details
- **Option vs Decision?** Option = possible approach. Decision = choosing which option.

## Connecting to History

Pulse gives you the "Now". For history, run `/narratives` then `/archaeology`.
"#;

/// OpenCode skill: /narratives (SKILL.md format for .opencode/skills/narratives/SKILL.md)
pub const SKILL_NARRATIVES_OPENCODE: &str = r#"---
name: narratives
description: Understand how a system evolved - narratives are the source of truth
compatibility: opencode
---

# Narrative Tracking

**Narratives are the source of truth. Commits are just evidence.**

## Step 1: Initialize narratives file

```bash
deciduous narratives init
```

This creates `.deciduous/narratives.md` pre-populated with your active goal titles.

## Step 2: Understand the system first

Before looking at git, read the code. Ask: **What are the major pieces of this system?**

Each major piece probably has a narrative behind it.

## Step 3: Fill in the narratives

Edit `.deciduous/narratives.md`. For each section:

1. Describe the **current state** (how it works today)
2. Infer the **evolution** (how it likely got this way)
3. Identify **PIVOTs** (when the conceptual model changed)
4. Find evidence (PRs, commits, docs) - optional
5. Check attached documents (`deciduous doc list`)

Signs of a pivot:
- Two approaches coexisting (migration in progress)
- Comments explaining "we used to do X"
- Config for old + new system
- Deprecation warnings

## Step 4: Review narratives

```bash
deciduous narratives show
```

## Step 5: Check existing pivots

```bash
deciduous narratives pivots
```

## Output Format

Each narrative section in `.deciduous/narratives.md`:

```markdown
## <Name>
> <One sentence: what this piece of the system does>

**Current state:** <How it works today>

**Evolution:**
1. <First approach> - <why>
2. **PIVOT:** <what changed> - <why it changed>
3. <Current approach> - <why this is better>

**Evidence:** <Optional: PRs, commits, docs>
**Connects to:** <Other narratives this influenced>
**Status:** active | superseded | abandoned
```

## What Makes a Good Narrative

- Coherent story about ONE design aspect
- Explains HOW something works and WHY it evolved
- Would help a new team member understand the system
- NOT a list of commits or feature changelog

## Next Step

After narratives are written, run `/archaeology` to transform them into a queryable decision graph.
"#;

/// OpenCode skill: /archaeology (SKILL.md format for .opencode/skills/archaeology/SKILL.md)
pub const SKILL_ARCHAEOLOGY_OPENCODE: &str = r#"---
name: archaeology
description: Transform narratives into a queryable decision graph
compatibility: opencode
---

# Archaeology

**Transform narratives into a queryable decision graph.**

Run `/narratives` first to create `.deciduous/narratives.md`.

## Step 1: Read the narratives

```bash
deciduous narratives show
```

For each narrative, you'll create a subgraph.

## Step 2: Create root goals

For each narrative, create a backdated goal:

```bash
deciduous add goal "<Narrative title>" -c 90 --date "YYYY-MM-DD"
```

## Step 3: Build initial approaches

```bash
deciduous add decision "<First approach>" -c 85 --date "YYYY-MM-DD"
deciduous link <goal> <decision> -r "Initial design"
```

## Step 4: Create pivots with `archaeology pivot`

For each **PIVOT** in a narrative, use the atomic pivot command:

```bash
deciduous archaeology pivot <from_id> "<what was learned>" "<new approach>" -c 85 -r "<why it failed>"
```

This automatically creates:
- observation node (what was learned)
- revisit node (reconsidering the old approach)
- decision node (the new approach)
- All 3 linking edges
- Marks the old approach as superseded

Preview before executing:
```bash
deciduous archaeology pivot <from_id> "observation" "new approach" --dry-run
```

## Step 5: Connect narratives

When narratives reference each other:

```bash
deciduous link <auth_observation> <ratelimit_decision> \
  -r "Auth failures drove rate limit redesign"
```

## Step 6: Mark superseded paths

For nodes that were replaced but not part of a pivot:

```bash
# Single node
deciduous archaeology supersede <id>

# Node and all descendants
deciduous archaeology supersede <id> --cascade
```

## Step 7: Review the timeline

```bash
# See all nodes chronologically
deciduous archaeology timeline

# Filter by type
deciduous archaeology timeline --type revisit

# See existing pivot chains
deciduous narratives pivots

# Visual exploration
deciduous serve
```

## Attach Evidence Documents

If you find diagrams, screenshots, or specs that support the archaeology:

```bash
deciduous doc attach <goal_id> evidence/old-architecture.png -d "Architecture before refactor"
deciduous doc attach <revisit_id> evidence/perf-report.pdf --ai-describe
```

Documents provide visual/tangible evidence alongside commit-based grounding.

## Querying the Graph

```bash
# Current state
deciduous pulse

# Pivot points
deciduous narratives pivots

# Timeline
deciduous archaeology timeline

# By status
deciduous nodes --type revisit
```

## What NOT to Do

- **Don't create nodes for every commit.** Commits are evidence, not graph nodes.
- **Don't create implementation nodes.** The graph is about the MODEL, not the code.
- **Don't over-structure.** Simple narratives might just be: goal -> option -> decision.
"#;

/// OpenCode agent definition for .opencode/agents/deciduous.md
pub const AGENT_DECIDUOUS: &str = r#"---
description: Deciduous decision graph specialist - manages nodes, edges, and graph operations
mode: subagent
---

# Deciduous Agent

You are a specialized agent for managing the deciduous decision graph. Use the `deciduous` CLI to manage nodes, edges, and graph operations.

## Core Commands

```bash
# Add nodes
deciduous add goal "Title" -c 90 -p "User request"
deciduous add option "Approach" -c 70
deciduous add decision "Choice" -c 85
deciduous add action "Implementation" -c 85 -f "file1.rs,file2.rs"
deciduous add outcome "Result" -c 95 --commit HEAD
deciduous add observation "Finding" -c 80
deciduous add revisit "Reconsidering" -c 75

# Connect nodes (ALWAYS do this immediately)
deciduous link <from> <to> -r "reason"

# Query
deciduous nodes
deciduous edges
deciduous graph
deciduous pulse

# Sync and export
deciduous sync
deciduous dot --png
```

## Node Flow Rule

```
goal -> options -> decision -> actions -> outcomes
```

- Goals lead to options (possible approaches)
- Options lead to decisions (choosing which option)
- Decisions lead to actions (implementation)
- Actions lead to outcomes (results)
- Observations attach anywhere relevant
- Root goals are the ONLY valid orphans

## Connection Rules

| When you create... | IMMEDIATELY link to... |
|-------------------|------------------------|
| `option` | Its parent goal |
| `decision` | The option(s) it chose between |
| `action` | The decision that spawned it |
| `outcome` | The action that produced it |
| `observation` | Related goal/action |
| `revisit` | The decision being reconsidered |

## After Git Commits

```bash
deciduous add outcome "What was accomplished" -c 95 --commit HEAD
deciduous link <action_id> <outcome_id> -r "Implementation complete"
```
"#;

/// OpenCode custom tool for .opencode/tools/deciduous.ts
pub const TOOL_DECIDUOUS: &str = r#"// OpenCode Custom Tool: Deciduous Decision Graph
// Wraps the deciduous CLI for direct graph operations from OpenCode
//
// This tool allows agents to interact with the decision graph without
// needing to use the bash tool directly.

import { tool } from "@opencode-ai/plugin"

export default tool({
  description: "Manage the deciduous decision graph - add nodes, create edges, query the graph, and sync",
  args: {
    command: tool.schema.string().describe(
      "The deciduous subcommand and arguments to run. Examples: " +
      "'add goal \"Title\" -c 90', " +
      "'link 1 2 -r \"reason\"', " +
      "'nodes', 'edges', 'graph', 'pulse', 'sync'"
    ),
  },
  async execute(args, context) {
    const proc = Bun.spawn(["sh", "-c", `deciduous ${args.command}`], {
      cwd: context.directory,
      stdout: "pipe",
      stderr: "pipe",
    })

    const stdout = await new Response(proc.stdout).text()
    const stderr = await new Response(proc.stderr).text()
    const exitCode = await proc.exited

    if (exitCode !== 0) {
      return `Error (exit ${exitCode}):\n${stderr}\n${stdout}`
    }

    return stdout || "(no output)"
  },
})
"#;

/// Default configuration file content (duplicated from init/templates.rs to avoid circular deps)
const DEFAULT_CONFIG: &str = r#"# Deciduous Configuration
# This file controls branch detection and grouping behavior

[branch]
# Branches considered "main" - nodes on these branches won't trigger feature-branch grouping
main_branches = ["main", "master"]

# Automatically detect and store git branch when creating nodes
auto_detect = true
"#;

/// Ensure core deciduous infrastructure exists (.deciduous/, config, database, docs/)
/// This allows `deciduous opencode install` to work standalone in a fresh project
fn ensure_core_infrastructure(project_root: &Path) -> Result<(), String> {
    let deciduous_dir = project_root.join(".deciduous");

    // Create .deciduous directory if missing
    if !deciduous_dir.exists() {
        fs::create_dir_all(&deciduous_dir)
            .map_err(|e| format!("Could not create .deciduous: {}", e))?;
        println!("   {} .deciduous/", "Creating".green());
    }

    // Create config.toml if missing
    let config_path = deciduous_dir.join("config.toml");
    if !config_path.exists() {
        fs::write(&config_path, DEFAULT_CONFIG)
            .map_err(|e| format!("Could not write config.toml: {}", e))?;
        println!("   {} .deciduous/config.toml", "Creating".green());
    }

    // Write version file
    let version_path = deciduous_dir.join(".version");
    let version = env!("CARGO_PKG_VERSION");
    fs::write(&version_path, version)
        .map_err(|e| format!("Could not write version file: {}", e))?;
    println!(
        "   {} .deciduous/.version ({})",
        "Creating".green(),
        version
    );

    // Set up database path (database is created lazily on first use)
    let db_path = deciduous_dir.join("deciduous.db");
    if !db_path.exists() {
        println!(
            "   {} .deciduous/deciduous.db (will be created on first use)",
            "Preparing".green()
        );
    }
    std::env::set_var("DECIDUOUS_DB_PATH", &db_path);

    // Add .deciduous to .gitignore
    let gitignore_path = project_root.join(".gitignore");
    let entry = ".deciduous/";
    if gitignore_path.exists() {
        let existing = fs::read_to_string(&gitignore_path)
            .map_err(|e| format!("Could not read .gitignore: {}", e))?;
        if !existing
            .lines()
            .any(|line| line.trim() == entry || line.trim() == ".deciduous")
        {
            let new_content = format!(
                "{}\n\n# Deciduous database (local)\n{}\n",
                existing.trim_end(),
                entry
            );
            fs::write(&gitignore_path, new_content)
                .map_err(|e| format!("Could not update .gitignore: {}", e))?;
            println!("   {} .gitignore (added {})", "Updated".green(), entry);
        }
    } else {
        let content = format!("# Deciduous database (local)\n{}\n", entry);
        fs::write(&gitignore_path, content)
            .map_err(|e| format!("Could not create .gitignore: {}", e))?;
        println!("   {} .gitignore", "Creating".green());
    }

    // Create docs/ directory for GitHub Pages viewer
    let docs_dir = project_root.join("docs");
    if !docs_dir.exists() {
        fs::create_dir_all(&docs_dir).map_err(|e| format!("Could not create docs/: {}", e))?;
        println!("   {} docs/", "Creating".green());
    }

    // Create empty graph-data.json
    let graph_data_path = docs_dir.join("graph-data.json");
    if !graph_data_path.exists() {
        let empty_graph = r#"{"nodes":[],"edges":[]}"#;
        fs::write(&graph_data_path, empty_graph)
            .map_err(|e| format!("Could not write graph-data.json: {}", e))?;
        println!("   {} docs/graph-data.json", "Creating".green());
    }

    // Create .nojekyll for GitHub Pages
    let nojekyll_path = docs_dir.join(".nojekyll");
    if !nojekyll_path.exists() {
        fs::write(&nojekyll_path, "").map_err(|e| format!("Could not write .nojekyll: {}", e))?;
        println!("   {} docs/.nojekyll", "Creating".green());
    }

    Ok(())
}

/// Install OpenCode configuration and plugins
pub fn install_opencode(project_root: &Path) -> Result<(), String> {
    println!("\n{}", "Installing OpenCode integration...".cyan().bold());
    println!("   Directory: {}\n", project_root.display());

    // First, ensure core deciduous infrastructure exists
    ensure_core_infrastructure(project_root)?;

    // Migrate old singular dirs before creating anything
    migrate_opencode_dirs(project_root)?;

    let config = Config::load();

    let opencode_dir = project_root.join(".opencode");
    let plugin_dir = opencode_dir.join("plugins");
    let command_dir = opencode_dir.join("commands");
    let skill_dir = opencode_dir.join("skills");
    let agent_dir = opencode_dir.join("agents");
    let tool_dir = opencode_dir.join("tools");

    // Create directories
    for dir in [
        &opencode_dir,
        &plugin_dir,
        &command_dir,
        &skill_dir,
        &agent_dir,
        &tool_dir,
    ] {
        if !dir.exists() {
            fs::create_dir_all(dir).map_err(|e| format!("Could not create {:?}: {}", dir, e))?;
            println!("   {} {:?}", "Creating".green(), dir);
        }
    }

    // Install plugins (hooks)
    if config.hooks.enabled {
        for hook in &config.hooks.pre_tool_use {
            if hook.enabled && hook.name == "require-action-node" {
                let plugin_path = plugin_dir.join("require-action-node.ts");
                fs::write(&plugin_path, PLUGIN_REQUIRE_ACTION_NODE)
                    .map_err(|e| format!("Could not write plugin: {}", e))?;
                println!(
                    "   {} .opencode/plugins/require-action-node.ts",
                    "Installed".green()
                );
            }
        }

        for hook in &config.hooks.post_tool_use {
            if hook.enabled && hook.name == "post-commit-reminder" {
                let plugin_path = plugin_dir.join("post-commit-reminder.ts");
                fs::write(&plugin_path, PLUGIN_POST_COMMIT_REMINDER)
                    .map_err(|e| format!("Could not write plugin: {}", e))?;
                println!(
                    "   {} .opencode/plugins/post-commit-reminder.ts",
                    "Installed".green()
                );
            }
        }
    }

    // Install commands
    let commands = [
        ("work.md", COMMAND_WORK),
        ("recover.md", COMMAND_RECOVER),
        ("decision.md", COMMAND_DECISION),
        ("build-test.md", COMMAND_BUILD_TEST),
        ("serve-ui.md", COMMAND_SERVE_UI),
        ("sync-graph.md", COMMAND_SYNC_GRAPH),
        ("document.md", COMMAND_DOCUMENT),
        ("sync.md", COMMAND_SYNC),
        ("decision-graph.md", COMMAND_DECISION_GRAPH),
    ];

    for (name, content) in commands {
        let cmd_path = command_dir.join(name);
        fs::write(&cmd_path, content)
            .map_err(|e| format!("Could not write command {}: {}", name, e))?;
        println!("   {} .opencode/commands/{}", "Installed".green(), name);
    }

    // Install skills in proper directory structure (.opencode/skills/<name>/SKILL.md)
    let skills = [
        ("pulse", SKILL_PULSE_OPENCODE),
        ("narratives", SKILL_NARRATIVES_OPENCODE),
        ("archaeology", SKILL_ARCHAEOLOGY_OPENCODE),
    ];

    for (name, content) in skills {
        let skill_subdir = skill_dir.join(name);
        if !skill_subdir.exists() {
            fs::create_dir_all(&skill_subdir)
                .map_err(|e| format!("Could not create {:?}: {}", skill_subdir, e))?;
        }
        let skill_path = skill_subdir.join("SKILL.md");
        fs::write(&skill_path, content)
            .map_err(|e| format!("Could not write skill {}: {}", name, e))?;
        println!(
            "   {} .opencode/skills/{}/SKILL.md",
            "Installed".green(),
            name
        );
    }

    // Install custom agent
    let agent_path = agent_dir.join("deciduous.md");
    fs::write(&agent_path, AGENT_DECIDUOUS).map_err(|e| format!("Could not write agent: {}", e))?;
    println!("   {} .opencode/agents/deciduous.md", "Installed".green());

    // Install custom tool
    let tool_path = tool_dir.join("deciduous.ts");
    fs::write(&tool_path, TOOL_DECIDUOUS).map_err(|e| format!("Could not write tool: {}", e))?;
    println!("   {} .opencode/tools/deciduous.ts", "Installed".green());

    // Generate opencode.json config
    // Plugins are auto-loaded from .opencode/plugins/
    // Tools are auto-loaded from .opencode/tools/
    // Instructions/rules go in AGENTS.md, referenced via the 'instructions' key
    let opencode_config = json!({
        "$schema": "https://opencode.ai/config.json",
        "instructions": ["AGENTS.md"]
    });

    let config_path = project_root.join("opencode.json");
    if !config_path.exists() {
        fs::write(
            &config_path,
            serde_json::to_string_pretty(&opencode_config).unwrap(),
        )
        .map_err(|e| format!("Could not write opencode.json: {}", e))?;
        println!("   {} opencode.json", "Created".green());
    } else {
        println!("   {} opencode.json (already exists)", "Skipped".yellow());
    }

    // Generate or update AGENTS.md
    let agents_md_path = project_root.join("AGENTS.md");
    if !agents_md_path.exists() {
        // Check if CLAUDE.md exists to convert from
        let claude_md_path = project_root.join("CLAUDE.md");
        if claude_md_path.exists() {
            // Read CLAUDE.md and adapt for OpenCode
            let claude_content = fs::read_to_string(&claude_md_path)
                .map_err(|e| format!("Could not read CLAUDE.md: {}", e))?;

            let agents_content = convert_claude_to_agents_md(&claude_content);
            fs::write(&agents_md_path, agents_content)
                .map_err(|e| format!("Could not write AGENTS.md: {}", e))?;
            println!(
                "   {} AGENTS.md (converted from CLAUDE.md)",
                "Created".green()
            );
        } else {
            // Create basic AGENTS.md
            let agents_content = generate_basic_agents_md();
            fs::write(&agents_md_path, agents_content)
                .map_err(|e| format!("Could not write AGENTS.md: {}", e))?;
            println!("   {} AGENTS.md", "Created".green());
        }
    } else {
        // AGENTS.md exists - append workflow section if not present
        let existing_content = fs::read_to_string(&agents_md_path)
            .map_err(|e| format!("Could not read AGENTS.md: {}", e))?;

        if existing_content.contains("## Decision Graph Workflow") {
            println!(
                "   {} AGENTS.md (workflow section already present)",
                "Skipped".yellow()
            );
        } else {
            // Append the workflow section
            let workflow_section = get_agents_workflow_section();
            let new_content = format!("{}\n{}", existing_content.trim_end(), workflow_section);
            fs::write(&agents_md_path, new_content)
                .map_err(|e| format!("Could not update AGENTS.md: {}", e))?;
            println!(
                "   {} AGENTS.md (appended workflow section)",
                "Updated".green()
            );
        }
    }

    println!("\n{}", "OpenCode integration installed!".green().bold());
    println!();
    println!("Plugins installed in .opencode/plugins/");
    println!("Commands installed in .opencode/commands/");
    println!("Skills installed in .opencode/skills/");
    println!("Agent installed in .opencode/agents/");
    println!("Tool installed in .opencode/tools/");
    println!();

    Ok(())
}

/// Migrate from old singular directory names (plugin/, command/) to plural (plugins/, commands/)
fn migrate_opencode_dirs(project_root: &Path) -> Result<(), String> {
    let opencode_dir = project_root.join(".opencode");

    let migrations = [("plugin", "plugins"), ("command", "commands")];

    for (old_name, new_name) in migrations {
        let old_dir = opencode_dir.join(old_name);
        let new_dir = opencode_dir.join(new_name);

        if old_dir.exists() && !new_dir.exists() {
            // Simple rename
            fs::rename(&old_dir, &new_dir)
                .map_err(|e| format!("Could not migrate {} to {}: {}", old_name, new_name, e))?;
            println!(
                "   {} .opencode/{} -> .opencode/{}",
                "Migrated".green(),
                old_name,
                new_name
            );
        } else if old_dir.exists() && new_dir.exists() {
            // Both exist - copy any files from old to new, then remove old
            if let Ok(entries) = fs::read_dir(&old_dir) {
                for entry in entries.flatten() {
                    let dest = new_dir.join(entry.file_name());
                    if !dest.exists() {
                        fs::copy(entry.path(), &dest).ok();
                    }
                }
            }
            fs::remove_dir_all(&old_dir).ok();
            println!(
                "   {} .opencode/{} (merged into {})",
                "Cleaned".green(),
                old_name,
                new_name
            );
        }
    }

    // Remove old skill files from commands/ if they exist (skills now live in skills/)
    let commands_dir = opencode_dir.join("commands");
    let old_skill_files = ["pulse.md", "narratives.md", "archaeology.md"];
    for file in old_skill_files {
        let old_path = commands_dir.join(file);
        if old_path.exists() {
            fs::remove_file(&old_path).ok();
            println!(
                "   {} .opencode/commands/{} (moved to skills/)",
                "Removed".green(),
                file
            );
        }
    }

    Ok(())
}

/// Update OpenCode integration files to latest version (overwrites existing)
pub fn update_opencode(project_root: &Path) -> Result<(), String> {
    // Migrate from old singular directory names to plural
    migrate_opencode_dirs(project_root)?;

    let opencode_dir = project_root.join(".opencode");
    let plugin_dir = opencode_dir.join("plugins");
    let command_dir = opencode_dir.join("commands");
    let skill_dir = opencode_dir.join("skills");
    let agent_dir = opencode_dir.join("agents");
    let tool_dir = opencode_dir.join("tools");

    // Create directories if needed
    for dir in [
        &opencode_dir,
        &plugin_dir,
        &command_dir,
        &skill_dir,
        &agent_dir,
        &tool_dir,
    ] {
        if !dir.exists() {
            fs::create_dir_all(dir).map_err(|e| format!("Could not create {:?}: {}", dir, e))?;
            println!("   {} {:?}", "Creating".green(), dir);
        }
    }

    // Update plugins (overwrite)
    let plugin_path = plugin_dir.join("require-action-node.ts");
    fs::write(&plugin_path, PLUGIN_REQUIRE_ACTION_NODE)
        .map_err(|e| format!("Could not write plugin: {}", e))?;
    println!(
        "   {} .opencode/plugins/require-action-node.ts",
        "Updated".green()
    );

    let plugin_path = plugin_dir.join("post-commit-reminder.ts");
    fs::write(&plugin_path, PLUGIN_POST_COMMIT_REMINDER)
        .map_err(|e| format!("Could not write plugin: {}", e))?;
    println!(
        "   {} .opencode/plugins/post-commit-reminder.ts",
        "Updated".green()
    );

    // Update commands (overwrite)
    let commands = [
        ("work.md", COMMAND_WORK),
        ("recover.md", COMMAND_RECOVER),
        ("decision.md", COMMAND_DECISION),
        ("build-test.md", COMMAND_BUILD_TEST),
        ("serve-ui.md", COMMAND_SERVE_UI),
        ("sync-graph.md", COMMAND_SYNC_GRAPH),
        ("document.md", COMMAND_DOCUMENT),
        ("sync.md", COMMAND_SYNC),
        ("decision-graph.md", COMMAND_DECISION_GRAPH),
    ];

    for (name, content) in commands {
        let cmd_path = command_dir.join(name);
        fs::write(&cmd_path, content)
            .map_err(|e| format!("Could not write command {}: {}", name, e))?;
        println!("   {} .opencode/commands/{}", "Updated".green(), name);
    }

    // Update skills in proper directory structure
    let skills = [
        ("pulse", SKILL_PULSE_OPENCODE),
        ("narratives", SKILL_NARRATIVES_OPENCODE),
        ("archaeology", SKILL_ARCHAEOLOGY_OPENCODE),
    ];

    for (name, content) in skills {
        let skill_subdir = skill_dir.join(name);
        if !skill_subdir.exists() {
            fs::create_dir_all(&skill_subdir)
                .map_err(|e| format!("Could not create {:?}: {}", skill_subdir, e))?;
        }
        let skill_path = skill_subdir.join("SKILL.md");
        fs::write(&skill_path, content)
            .map_err(|e| format!("Could not write skill {}: {}", name, e))?;
        println!(
            "   {} .opencode/skills/{}/SKILL.md",
            "Updated".green(),
            name
        );
    }

    // Update agent (overwrite)
    let agent_path = agent_dir.join("deciduous.md");
    fs::write(&agent_path, AGENT_DECIDUOUS).map_err(|e| format!("Could not write agent: {}", e))?;
    println!("   {} .opencode/agents/deciduous.md", "Updated".green());

    // Update tool (overwrite)
    let tool_path = tool_dir.join("deciduous.ts");
    fs::write(&tool_path, TOOL_DECIDUOUS).map_err(|e| format!("Could not write tool: {}", e))?;
    println!("   {} .opencode/tools/deciduous.ts", "Updated".green());

    // Note: We don't overwrite opencode.json or AGENTS.md as they may have user customizations
    println!(
        "   {} opencode.json and AGENTS.md (preserved)",
        "Skipping".yellow()
    );

    Ok(())
}

/// Convert CLAUDE.md content to AGENTS.md format
fn convert_claude_to_agents_md(claude_content: &str) -> String {
    // OpenCode recognizes CLAUDE.md as a fallback, but AGENTS.md is preferred
    // The format is similar, so we mainly just need to rename references
    let mut content = claude_content.to_string();

    // Replace Claude-specific references
    content = content.replace("Claude Code", "OpenCode");
    content = content.replace("claude code", "opencode");
    content = content.replace(".claude/", ".opencode/");
    content = content.replace("CLAUDE.md", "AGENTS.md");

    // Add OpenCode-specific header
    let header = r#"# Project Instructions for OpenCode

> This file was converted from CLAUDE.md. OpenCode uses AGENTS.md for project instructions.
> See: https://opencode.ai/docs/rules/

"#;

    format!("{}{}", header, content)
}

/// Get just the Decision Graph Workflow section for appending to existing AGENTS.md
fn get_agents_workflow_section() -> String {
    r#"

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
| `/sync-graph` | Export decision graph to GitHub Pages |
| `/decision-graph` | Build a decision graph from commit history |
| `/sync` | Multi-user sync - pull events, rebuild, push |

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
| Exploring possible approaches | `option` | "Use Redux for state" |
| Choosing between approaches | `decision` | "Choose state management" |
| About to write/edit code | `action` | "Implementing Redux store" |
| Something worked or failed | `outcome` | "Redux integration successful" |
| Notice something interesting | `observation` | "Existing code uses hooks" |

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
deciduous doc open <doc_id>     # Open in default application
deciduous doc detach <doc_id>   # Soft-delete (recoverable)
```

### CRITICAL: Capture VERBATIM User Prompts

**Prompts must be the EXACT user message, not a summary.**

```bash
# Use --prompt-stdin for multi-line prompts
deciduous add goal "Add auth" -c 90 --prompt-stdin << 'EOF'
The full verbatim user request goes here...
EOF

# Or use the prompt command to update existing nodes
deciduous prompt 42 << 'EOF'
The full verbatim user message goes here...
EOF
```

### CRITICAL: Maintain Connections

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
deciduous link FROM TO -r "reason"
deciduous serve   # View live graph
deciduous sync    # Export for static hosting
```

### Node Types

| Type | Purpose |
|------|---------|
| `goal` | High-level objectives |
| `option` | Approaches considered (come from goals) |
| `decision` | Choosing an option (come from options) |
| `action` | What was implemented (come from decisions) |
| `outcome` | What happened (come from actions) |
| `observation` | Technical insights (attach anywhere) |
| `revisit` | Reconsidering a decision |

### Multi-User Sync

Sync decisions with teammates via event logs:

```bash
# Check sync status
deciduous events status

# Apply teammate events (after git pull)
deciduous events rebuild

# Compact old events periodically
deciduous events checkpoint --clear-events
```

Events auto-emit on add/link/status commands. Git merges event files automatically.

### Session Start Checklist

```bash
deciduous check-update    # Update needed? Run 'deciduous update' if yes
deciduous nodes           # What decisions exist?
deciduous edges           # How are they connected?
deciduous doc list        # Any attached documents to review?
git status                # Current state
```
"#
    .to_string()
}

/// Generate basic AGENTS.md content
fn generate_basic_agents_md() -> String {
    r#"# Project Instructions for OpenCode

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
| `/sync-graph` | Export decision graph to GitHub Pages |
| `/decision-graph` | Build a decision graph from commit history |
| `/sync` | Multi-user sync - pull events, rebuild, push |

### Available Skills

| Skill | Purpose |
|-------|---------|
| `/pulse` | Map current design as decisions (Now mode) |
| `/narratives` | Understand how the system evolved (History mode) |
| `/archaeology` | Transform narratives into queryable graph |

### Node Flow Rule: goal -> options -> decision -> actions -> outcomes

- **Goals** lead to **options** (possible approaches)
- **Options** lead to a **decision** (choosing which option)
- **Decisions** lead to **actions** (implementing the choice)
- **Actions** lead to **outcomes** (results)
- Goals do NOT lead directly to decisions -- there must be options first

### The Core Rule

```
BEFORE you do something -> Log what you're ABOUT to do
AFTER it succeeds/fails -> Log the outcome
CONNECT immediately -> Link every node to its parent
```

### Quick Commands

```bash
deciduous add goal "Title" -c 90 -p "User's original request"
deciduous add option "Possible approach" -c 70
deciduous link <goal_id> <option_id> -r "Possible approach"
deciduous add action "Title" -c 85
deciduous link FROM TO -r "reason"
deciduous serve   # View live graph
deciduous sync    # Export for static hosting
```

### Node Types

| Type | Purpose |
|------|---------|
| `goal` | High-level objectives |
| `option` | Approaches considered (come from goals) |
| `decision` | Choosing an option (come from options) |
| `action` | What was implemented (come from decisions) |
| `outcome` | What happened (come from actions) |
| `observation` | Technical insights (attach anywhere) |
| `revisit` | Reconsidering a decision |

### Document Attachments

```bash
deciduous doc attach <node_id> <file_path> -d "Description"
deciduous doc list [node_id]
deciduous doc show <doc_id>
```

### Multi-User Sync

```bash
deciduous events status           # Check sync status
deciduous events rebuild          # Apply teammate events
deciduous events checkpoint --clear-events  # Compact old events
```

### Session Start Checklist

```bash
deciduous check-update    # Update needed? Run 'deciduous update' if yes
deciduous nodes           # What decisions exist?
deciduous edges           # How are they connected?
deciduous doc list        # Any attached documents?
git status                # Current state
```
"#
    .to_string()
}

/// Show OpenCode integration status
pub fn opencode_status() -> Result<(), String> {
    let project_root = Config::find_project_root();

    println!("\n{}", "OpenCode Integration Status".cyan().bold());
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let Some(project_root) = project_root else {
        println!(
            "\n   {} Could not find project root (.deciduous directory)",
            "Error:".red()
        );
        println!("   Run 'deciduous init' to initialize the project.");
        return Ok(());
    };

    let opencode_dir = project_root.join(".opencode");

    // Check .opencode directory
    println!("\n{}", "Directory:".cyan());
    if opencode_dir.exists() {
        println!("   {} .opencode/", "✓".green());
    } else {
        println!("   {} .opencode/ (not found)", "○".yellow());
        println!("   Run 'deciduous opencode install' to create it");
    }

    // Check plugins (support both old singular and new plural dirs)
    println!("\n{}", "Plugins (Hooks):".cyan());
    let plugin_dir = {
        let new_dir = opencode_dir.join("plugins");
        if new_dir.exists() {
            new_dir
        } else {
            opencode_dir.join("plugin")
        }
    };
    if plugin_dir.exists() {
        let entries: Vec<_> = fs::read_dir(&plugin_dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| {
                        e.path()
                            .extension()
                            .is_some_and(|ext| ext == "ts" || ext == "js")
                    })
                    .collect()
            })
            .unwrap_or_default();

        if entries.is_empty() {
            println!("   {} (no plugins installed)", "○".yellow());
        } else {
            for entry in entries {
                let name = entry.file_name().to_string_lossy().to_string();
                println!("   {} {}", "✓".green(), name);
            }
        }
    } else {
        println!("   {} (plugins directory not found)", "○".yellow());
    }

    // Check commands (support both old singular and new plural dirs)
    println!("\n{}", "Commands:".cyan());
    let command_dir = {
        let new_dir = opencode_dir.join("commands");
        if new_dir.exists() {
            new_dir
        } else {
            opencode_dir.join("command")
        }
    };
    if command_dir.exists() {
        let entries: Vec<_> = fs::read_dir(&command_dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
                    .collect()
            })
            .unwrap_or_default();

        if entries.is_empty() {
            println!("   {} (no commands installed)", "○".yellow());
        } else {
            for entry in entries {
                let name = entry
                    .path()
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                println!("   {} /{}", "✓".green(), name);
            }
        }
    } else {
        println!("   {} (commands directory not found)", "○".yellow());
    }

    // Check skills
    println!("\n{}", "Skills:".cyan());
    let skill_dir = opencode_dir.join("skills");
    if skill_dir.exists() {
        let entries: Vec<_> = fs::read_dir(&skill_dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| e.path().is_dir())
                    .collect()
            })
            .unwrap_or_default();

        if entries.is_empty() {
            println!("   {} (no skills installed)", "○".yellow());
        } else {
            for entry in entries {
                let name = entry.file_name().to_string_lossy().to_string();
                let skill_md = entry.path().join("SKILL.md");
                if skill_md.exists() {
                    println!("   {} /{}", "✓".green(), name);
                } else {
                    println!("   {} /{} (SKILL.md missing)", "○".yellow(), name);
                }
            }
        }
    } else {
        println!("   {} (skills directory not found)", "○".yellow());
    }

    // Check agents
    println!("\n{}", "Agents:".cyan());
    let agent_dir = opencode_dir.join("agents");
    if agent_dir.exists() {
        let entries: Vec<_> = fs::read_dir(&agent_dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
                    .collect()
            })
            .unwrap_or_default();

        if entries.is_empty() {
            println!("   {} (no agents defined)", "○".yellow());
        } else {
            for entry in entries {
                let name = entry
                    .path()
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                println!("   {} @{}", "✓".green(), name);
            }
        }
    } else {
        println!("   {} (agents directory not found)", "○".yellow());
    }

    // Check tools
    println!("\n{}", "Tools:".cyan());
    let tool_dir = opencode_dir.join("tools");
    if tool_dir.exists() {
        let entries: Vec<_> = fs::read_dir(&tool_dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| {
                        e.path()
                            .extension()
                            .is_some_and(|ext| ext == "ts" || ext == "js")
                    })
                    .collect()
            })
            .unwrap_or_default();

        if entries.is_empty() {
            println!("   {} (no custom tools)", "○".yellow());
        } else {
            for entry in entries {
                let name = entry.file_name().to_string_lossy().to_string();
                println!("   {} {}", "✓".green(), name);
            }
        }
    } else {
        println!("   {} (tools directory not found)", "○".yellow());
    }

    // Check configuration files
    println!("\n{}", "Configuration:".cyan());

    let opencode_json = project_root.join("opencode.json");
    if opencode_json.exists() {
        println!("   {} opencode.json", "✓".green());
    } else {
        println!("   {} opencode.json (not found)", "○".yellow());
    }

    let agents_md = project_root.join("AGENTS.md");
    if agents_md.exists() {
        println!("   {} AGENTS.md", "✓".green());
    } else {
        println!("   {} AGENTS.md (not found)", "○".yellow());
    }

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!(
        "\nRun {} to install OpenCode integration.",
        "'deciduous opencode install'".cyan()
    );
    println!();

    Ok(())
}

/// Uninstall OpenCode integration
pub fn uninstall_opencode(project_root: &Path) -> Result<(), String> {
    let opencode_dir = project_root.join(".opencode");

    if opencode_dir.exists() {
        fs::remove_dir_all(&opencode_dir)
            .map_err(|e| format!("Could not remove .opencode: {}", e))?;
        println!("   {} .opencode/", "Removed".green());
    }

    // Don't remove opencode.json or AGENTS.md as they might have user customizations
    println!(
        "   {} opencode.json and AGENTS.md (preserved)",
        "Note:".yellow()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_convert_claude_to_agents() {
        let claude = "# Claude Code Instructions\n\nUse .claude/ for config.";
        let agents = convert_claude_to_agents_md(claude);

        assert!(agents.contains("OpenCode"));
        assert!(agents.contains(".opencode/"));
        assert!(agents.contains("AGENTS.md"));
    }

    #[test]
    fn test_install_opencode() {
        let tmp = TempDir::new().unwrap();
        let project_root = tmp.path();

        // Create .deciduous with config
        let deciduous_dir = project_root.join(".deciduous");
        fs::create_dir_all(&deciduous_dir).unwrap();
        fs::write(deciduous_dir.join("config.toml"), "[hooks]\nenabled = true").unwrap();

        // Set current dir for Config::load
        // Use canonicalize to get absolute path - avoids issues with parallel tests
        let original_dir = std::env::current_dir()
            .unwrap()
            .canonicalize()
            .unwrap_or_else(|_| std::env::current_dir().unwrap());
        std::env::set_current_dir(project_root).unwrap();

        let result = install_opencode(project_root);

        // Restore original dir (ignore errors if dir was deleted by parallel test cleanup)
        let _ = std::env::set_current_dir(&original_dir);

        assert!(result.is_ok());

        // Check plugins (now in plugins/ plural)
        assert!(project_root
            .join(".opencode/plugins/require-action-node.ts")
            .exists());
        assert!(project_root
            .join(".opencode/plugins/post-commit-reminder.ts")
            .exists());

        // Check commands (now in commands/ plural)
        assert!(project_root.join(".opencode/commands/work.md").exists());
        assert!(project_root.join(".opencode/commands/recover.md").exists());
        assert!(project_root.join(".opencode/commands/decision.md").exists());
        assert!(project_root
            .join(".opencode/commands/build-test.md")
            .exists());
        assert!(project_root.join(".opencode/commands/serve-ui.md").exists());
        assert!(project_root
            .join(".opencode/commands/sync-graph.md")
            .exists());
        assert!(project_root.join(".opencode/commands/document.md").exists());
        assert!(project_root.join(".opencode/commands/sync.md").exists());
        assert!(project_root
            .join(".opencode/commands/decision-graph.md")
            .exists());

        // Check skills (now in skills/<name>/SKILL.md)
        assert!(project_root
            .join(".opencode/skills/pulse/SKILL.md")
            .exists());
        assert!(project_root
            .join(".opencode/skills/narratives/SKILL.md")
            .exists());
        assert!(project_root
            .join(".opencode/skills/archaeology/SKILL.md")
            .exists());

        // Skills should NOT be in commands/
        assert!(!project_root.join(".opencode/commands/pulse.md").exists());
        assert!(!project_root
            .join(".opencode/commands/narratives.md")
            .exists());
        assert!(!project_root
            .join(".opencode/commands/archaeology.md")
            .exists());

        // Check agent and tool
        assert!(project_root.join(".opencode/agents/deciduous.md").exists());
        assert!(project_root.join(".opencode/tools/deciduous.ts").exists());

        // Check config files
        assert!(project_root.join("opencode.json").exists());
        assert!(project_root.join("AGENTS.md").exists());
    }

    #[test]
    fn test_migrate_opencode_dirs() {
        let tmp = TempDir::new().unwrap();
        let project_root = tmp.path();

        // Create old singular directories with files
        let old_plugin = project_root.join(".opencode/plugin");
        fs::create_dir_all(&old_plugin).unwrap();
        fs::write(old_plugin.join("test.ts"), "content").unwrap();

        let old_command = project_root.join(".opencode/command");
        fs::create_dir_all(&old_command).unwrap();
        fs::write(old_command.join("work.md"), "content").unwrap();
        fs::write(old_command.join("pulse.md"), "skill content").unwrap();

        migrate_opencode_dirs(project_root).unwrap();

        // Old dirs should be gone
        assert!(!old_plugin.exists());
        assert!(!old_command.exists());

        // New dirs should have the files
        assert!(project_root.join(".opencode/plugins/test.ts").exists());
        assert!(project_root.join(".opencode/commands/work.md").exists());

        // Skill files should be cleaned from commands
        assert!(!project_root.join(".opencode/commands/pulse.md").exists());
    }

    #[test]
    fn test_skill_frontmatter_format() {
        // Verify OpenCode skill format has required fields
        assert!(SKILL_PULSE_OPENCODE.contains("name: pulse"));
        assert!(SKILL_PULSE_OPENCODE.contains("description:"));
        assert!(SKILL_PULSE_OPENCODE.contains("compatibility: opencode"));

        assert!(SKILL_NARRATIVES_OPENCODE.contains("name: narratives"));
        assert!(SKILL_NARRATIVES_OPENCODE.contains("compatibility: opencode"));

        assert!(SKILL_ARCHAEOLOGY_OPENCODE.contains("name: archaeology"));
        assert!(SKILL_ARCHAEOLOGY_OPENCODE.contains("compatibility: opencode"));
    }

    #[test]
    fn test_agents_workflow_section_has_all_commands() {
        let section = get_agents_workflow_section();

        // All 9 commands should be listed
        assert!(section.contains("/decision"));
        assert!(section.contains("/recover"));
        assert!(section.contains("/work"));
        assert!(section.contains("/document"));
        assert!(section.contains("/build-test"));
        assert!(section.contains("/serve-ui"));
        assert!(section.contains("/sync-graph"));
        assert!(section.contains("/decision-graph"));
        assert!(section.contains("/sync"));

        // All 3 skills should be listed
        assert!(section.contains("/pulse"));
        assert!(section.contains("/narratives"));
        assert!(section.contains("/archaeology"));

        // Key sections should be present
        assert!(section.contains("Node Flow Rule"));
        assert!(section.contains("Document Attachments"));
        assert!(section.contains("Multi-User Sync"));
        assert!(section.contains("VERBATIM User Prompts"));
    }

    #[test]
    fn test_basic_agents_md_has_all_commands() {
        let content = generate_basic_agents_md();

        // All commands and skills should be listed
        assert!(content.contains("/decision"));
        assert!(content.contains("/recover"));
        assert!(content.contains("/work"));
        assert!(content.contains("/document"));
        assert!(content.contains("/sync"));
        assert!(content.contains("/pulse"));
        assert!(content.contains("/narratives"));
        assert!(content.contains("/archaeology"));

        // Key features present
        assert!(content.contains("Document Attachments"));
        assert!(content.contains("Multi-User Sync"));
        assert!(content.contains("Node Flow Rule"));
    }
}
