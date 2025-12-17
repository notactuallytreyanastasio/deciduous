//! Project initialization for deciduous
//!
//! `deciduous init` creates all the files needed for decision graph tracking
//! Supports multiple editors: Claude Code (--claude) and Windsurf (--windsurf)

use colored::Colorize;
use std::fs;
use std::path::Path;

/// Editor environment for initialization
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Editor {
    Claude,
    Windsurf,
    Opencode,
    Codex,
}

/// Static HTML viewer for GitHub Pages (embedded at compile time)
const PAGES_VIEWER_HTML: &str = include_str!("pages_viewer.html");

/// Default configuration file content
const DEFAULT_CONFIG: &str = r#"# Deciduous Configuration
# This file controls branch detection and grouping behavior

[branch]
# Branches considered "main" - nodes on these branches won't trigger feature-branch grouping
# When working on feature branches, nodes are automatically tagged with the branch name
main_branches = ["main", "master"]

# Automatically detect and store git branch when creating nodes
# Set to false to disable branch tracking entirely
auto_detect = true
"#;

/// GitHub Pages deploy workflow (deploys to gh-pages branch, safe for project repos)
const DEPLOY_PAGES_WORKFLOW: &str = r#"name: Deploy Decision Graph to Pages

on:
  push:
    branches: [main]
    paths:
      - 'docs/**'
  workflow_dispatch:

permissions:
  contents: write

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Deploy to gh-pages branch
        uses: peaceiris/actions-gh-pages@v4
        with:
          github_token: ${{ secrets.GITHUB_TOKEN }}
          publish_dir: ./docs
          publish_branch: gh-pages
          force_orphan: true
"#;

/// Templates embedded at compile time
/// NOTE: These templates should match the actual files in .claude/commands/
/// The source of truth is the actual files - update these when those change
const DECISION_MD: &str = r#"---
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
- `add obs <title>` -> `deciduous add observation "<title>" -c 80`
- `add outcome <title>` -> `deciduous add outcome "<title>" -c 90`

### Optional Flags for Nodes
- `-c, --confidence <0-100>` - Confidence level
- `-p, --prompt "..."` - Store the user prompt that triggered this node
- `-f, --files "src/main.py,lib/utils.js"` - Associate files with this node
- `-b, --branch <name>` - Git branch (auto-detected by default)
- `--no-branch` - Skip branch auto-detection
- `--commit <hash|HEAD>` - Link to a git commit (use HEAD for current commit)

### ⚠️ CRITICAL: Link Commits to Actions/Outcomes

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

Prompts are viewable in the TUI detail panel (`deciduous tui`) and web viewer.

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

### Sync Graph
- `sync` -> `deciduous sync`

### Multi-User Sync (Diff/Patch)
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

### Connection Rules
| Node Type | MUST connect to | Example |
|-----------|----------------|---------|
| `outcome` | The action/goal it resolves | "JWT working" → links FROM "Implementing JWT" |
| `action` | The decision/goal that spawned it | "Implementing JWT" → links FROM "Add auth" |
| `option` | Its parent decision | "Use JWT" → links FROM "Choose auth method" |
| `observation` | Related goal/action/decision | "Found existing code" → links TO relevant node |
| `decision` | Parent goal (if any) | "Choose auth" → links FROM "Add auth feature" |
| `goal` | Can be a root (no parent needed) | Root goals are valid orphans |

### Audit Checklist
Ask yourself after creating nodes:
1. Does every **outcome** link back to what caused it?
2. Does every **action** link to why you did it?
3. Does every **option** link to its decision?
4. Are there **dangling outcomes** with no parent action/goal?

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

## Multi-User Sync

**Problem**: Multiple users work on the same codebase, each with a local `.deciduous/deciduous.db` (gitignored). How to share decisions?

**Solution**: jj-inspired dual-ID model. Each node has:
- `id` (integer): Local database primary key, different per machine
- `change_id` (UUID): Globally unique, stable across all databases

### Export Workflow
```bash
# Export nodes from your branch as a patch file
deciduous diff export --branch feature-x -o .deciduous/patches/alice-feature.json

# Or export specific node IDs
deciduous diff export --nodes 172-188 -o .deciduous/patches/alice-feature.json --author alice
```

### Apply Workflow
```bash
# Apply patches from teammates (idempotent - safe to re-apply)
deciduous diff apply .deciduous/patches/*.json

# Preview what would change
deciduous diff apply --dry-run .deciduous/patches/bob-refactor.json
```

### PR Workflow
1. Create nodes locally while working
2. Export: `deciduous diff export --branch my-feature -o .deciduous/patches/my-feature.json`
3. Commit the patch file (NOT the database)
4. Open PR with patch file included
5. Teammates pull and apply: `deciduous diff apply .deciduous/patches/my-feature.json`
6. **Idempotent**: Same patch applied twice = no duplicates

### Patch Format (JSON)
```json
{
  "version": "1.0",
  "author": "alice",
  "branch": "feature/auth",
  "nodes": [{ "change_id": "uuid...", "title": "...", ... }],
  "edges": [{ "from_change_id": "uuid1", "to_change_id": "uuid2", ... }]
}
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

const RECOVER_MD: &str = r#"---
description: Recover context from decision graph and recent activity - USE THIS ON SESSION START
allowed-tools: Bash(deciduous:*, git:*, cat:*, tail:*)
argument-hint: [focus-area]
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

**Review each flagged node:**
- Root `goal` nodes are VALID without parents
- `outcome` nodes MUST link back to their action/goal
- `action` nodes MUST link to their parent goal/decision
- `option` nodes MUST link to their parent decision

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
6. **Suggested next steps**

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
EVERY USER REQUEST → Log goal/decision first
BEFORE CODE CHANGES → Log action
AFTER CHANGES → Log outcome, link nodes
BEFORE GIT PUSH → deciduous sync
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

If $ARGUMENTS specifies a focus, prioritize context for:

- **auth**: Authentication-related decisions
- **ui** / **graph**: UI and graph viewer state
- **cli**: Command-line interface changes
- **api**: API endpoints and data structures

---

## The Memory Loop

```
SESSION START
    ↓
Run /recover → See past decisions
    ↓
AUDIT → Fix any orphan nodes first!
    ↓
DO WORK → Log BEFORE each action
    ↓
CONNECT → Link new nodes immediately
    ↓
AFTER CHANGES → Log outcomes, observations
    ↓
AUDIT AGAIN → Any new orphans?
    ↓
BEFORE PUSH → deciduous sync
    ↓
PUSH → Live graph updates
    ↓
SESSION END → Final audit
    ↓
(repeat)
```

**Live graph**: https://notactuallytreyanastasio.github.io/deciduous/

---

## Multi-User Sync

If working in a team, check for and apply patches from teammates:

```bash
# Check for unapplied patches
deciduous diff status

# Apply all patches (idempotent - safe to run multiple times)
deciduous diff apply .deciduous/patches/*.json

# Preview before applying
deciduous diff apply --dry-run .deciduous/patches/teammate-feature.json
```

Before pushing your branch, export your decisions for teammates:

```bash
# Export your branch's decisions as a patch
deciduous diff export --branch $(git rev-parse --abbrev-ref HEAD) \
  -o .deciduous/patches/$(whoami)-$(git rev-parse --abbrev-ref HEAD).json

# Commit the patch file
git add .deciduous/patches/
```

## Why This Matters

- Context loss during compaction loses your reasoning
- The graph survives - query it early, query it often
- Retroactive logging misses details - log in the moment
- The user sees the graph live - show your work
- Patches share reasoning with teammates
"#;

/// Decision Logger Skill - autonomously triggers Claude to log decisions
const DECIDUOUS_SKILL: &str = r#"---
name: deciduous
description: Plan, implement, track, and reflect on your work goals and decisions.
---

# Planning & Decision Graph Logging

Track every goal, decision, and outcome in the decision graph. This creates persistent memory that survives context loss.

- ALWAYS LOG BEFORE YOU CODE, NOT AFTER.
- Log at the granularity of TODOs or task items.
- When drafting a plan create the GOAL node.
- User Decisions should be tracked

## When to Log (Automatic Triggers)

| Situation | Node Type | Example |
|-----------|-----------|---------|
| In plan mode  | `goal` | "Add user authentication" |
| TODO / Task Item | `action` | "Implementing JWT auth middleware" |
| User requests new feature | `goal` | "Add user authentication" |
| Choosing between approaches | `decision` | "Choose between JWT vs sessions" |
| Considering an option | `option` | "Use JWT with refresh tokens" |
| About to write/edit code | `action` | "Implementing JWT auth middleware" |
| Work completed or failed | `outcome` | "JWT auth working" or "JWT approach failed" |
| Important observation | `observation` | "Existing code uses cookie-based sessions" |

## Commands

```bash
# Create nodes (always include confidence -c)
deciduous add goal "Title" -c 90 -p "User's exact request"
deciduous add decision "Title" -c 75
deciduous add action "Title" -c 85
deciduous add outcome "Title" -c 90
deciduous add observation "Title" -c 80

# CRITICAL: Link nodes immediately after creation
deciduous link <parent_id> <child_id> -r "Reason for connection"

# After git commits, link to the graph
deciduous add action "Committed feature X" -c 90 --commit HEAD

# View the graph
deciduous nodes
deciduous edges
```

## Rules

1. **Log BEFORE acting** - Create the action node before writing code
2. **Link IMMEDIATELY** - Every node except root goals must have a parent
3. **Capture verbatim prompts** - Use `-p` with the user's exact words for goals
4. **Include confidence** - Always use `-c` flag (0-100)
5. **Log outcomes** - Both successes AND failures get logged

## Confidence Guidelines

- 90-100: Certain, verified, tested
- 75-89: High confidence, likely correct
- 50-74: Moderate confidence, some uncertainty
- Below 50: Experimental, speculative

## The Memory Loop

```
User Request → Log goal with -p
    ↓
Choose Approach → Log decision + options
    ↓
Start Coding → Log action FIRST
    ↓
Complete Work → Log outcome, link to parent
    ↓
Git Commit → Log with --commit HEAD
```

**Remember**: The decision graph is your persistent memory. Log as you work, not after.
"#;

const CLEANUP_WORKFLOW: &str = r#"name: Cleanup Decision Graph PNGs

on:
  pull_request:
    types: [closed]

jobs:
  cleanup:
    # Only run if PR was merged (not just closed)
    if: github.event.pull_request.merged == true
    runs-on: ubuntu-latest

    steps:
      - name: Checkout
        uses: actions/checkout@v4
        with:
          fetch-depth: 0
          token: ${{ secrets.GITHUB_TOKEN }}

      - name: Find and remove decision graph PNGs
        id: find-pngs
        run: |
          # Find decision graph PNGs (in docs/ or root)
          PNGS=$(find . -name "decision-graph*.png" -o -name "deciduous-graph*.png" 2>/dev/null | grep -v node_modules || true)

          if [ -z "$PNGS" ]; then
            echo "No decision graph PNGs found"
            echo "found=false" >> $GITHUB_OUTPUT
          else
            echo "Found PNGs to clean up:"
            echo "$PNGS"
            echo "found=true" >> $GITHUB_OUTPUT

            # Remove the files
            echo "$PNGS" | xargs rm -f

            # Also remove corresponding .dot files
            for png in $PNGS; do
              dot_file="${png%.png}.dot"
              if [ -f "$dot_file" ]; then
                rm -f "$dot_file"
                echo "Also removed: $dot_file"
              fi
            done
          fi

      - name: Create cleanup PR
        if: steps.find-pngs.outputs.found == 'true'
        run: |
          # Check if there are changes to commit
          if git diff --quiet && git diff --staged --quiet; then
            echo "No changes to commit"
            exit 0
          fi

          # Configure git
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"

          # Create branch and commit
          BRANCH="cleanup/decision-graphs-pr-${{ github.event.pull_request.number }}"
          git checkout -b "$BRANCH"
          git add -A
          git commit -m "chore: cleanup decision graph assets from PR #${{ github.event.pull_request.number }}"
          git push origin "$BRANCH"

          # Create and auto-merge PR
          gh pr create \
            --title "chore: cleanup decision graph assets from PR #${{ github.event.pull_request.number }}" \
            --body "Automated cleanup of decision graph PNG/DOT files that were used in PR #${{ github.event.pull_request.number }}.

          These files served their purpose for PR review and are no longer needed." \
            --head "$BRANCH" \
            --base main

          # Auto-merge (requires auto-merge enabled on repo)
          gh pr merge "$BRANCH" --auto --squash --delete-branch || echo "Auto-merge not enabled, PR created for manual merge"
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
"#;

const CLAUDE_MD_SECTION: &str = r#"
## Decision Graph Workflow

**THIS IS MANDATORY. Log decisions IN REAL-TIME, not retroactively.**

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
| Choosing between approaches | `decision` | "Choose state management" |
| About to write/edit code | `action` | "Implementing Redux store" |
| Something worked or failed | `outcome` | "Redux integration successful" |
| Notice something interesting | `observation` | "Existing code uses hooks" |

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

Prompts are viewable in the TUI detail panel (`deciduous tui`) and web viewer.

### ⚠️ CRITICAL: Maintain Connections

**The graph's value is in its CONNECTIONS, not just nodes.**

| When you create... | IMMEDIATELY link to... |
|-------------------|------------------------|
| `outcome` | The action/goal it resolves |
| `action` | The goal/decision that spawned it |
| `option` | Its parent decision |
| `observation` | Related goal/action |

**Root `goal` nodes are the ONLY valid orphans.**

### Quick Commands

```bash
deciduous add goal "Title" -c 90 -p "User's original request"
deciduous add action "Title" -c 85
deciduous link FROM TO -r "reason"  # DO THIS IMMEDIATELY!
deciduous serve   # View live (auto-refreshes every 30s)
deciduous sync    # Export for static hosting

# Metadata flags
# -c, --confidence 0-100   Confidence level
# -p, --prompt "..."       Store the user prompt (use when semantically meaningful)
# -f, --files "a.rs,b.rs"  Associate files
# -b, --branch <name>      Git branch (auto-detected)
# --commit <hash|HEAD>     Link to git commit (use HEAD for current commit)

# Branch filtering
deciduous nodes --branch main
deciduous nodes -b feature-auth
```

### ⚠️ CRITICAL: Link Commits to Actions/Outcomes

**After every git commit, link it to the decision graph!**

```bash
git commit -m "feat: add auth"
deciduous add action "Implemented auth" -c 90 --commit HEAD
deciduous link <goal_id> <action_id> -r "Implementation"
```

The `--commit HEAD` flag captures the commit hash and links it to the node. The web viewer will show commit messages, authors, and dates.

### Git History & Deployment

```bash
# Export graph AND git history for web viewer
deciduous sync

# This creates:
# - docs/graph-data.json (decision graph)
# - docs/git-history.json (commit info for linked nodes)
```

To deploy to GitHub Pages:
1. `deciduous sync` to export
2. Push to GitHub
3. Settings > Pages > Deploy from branch > /docs folder

Your graph will be live at `https://<user>.github.io/<repo>/`

### Branch-Based Grouping

Nodes are auto-tagged with the current git branch. Configure in `.deciduous/config.toml`:
```toml
[branch]
main_branches = ["main", "master"]
auto_detect = true
```

### Audit Checklist (Before Every Sync)

1. Does every **outcome** link back to what caused it?
2. Does every **action** link to why you did it?
3. Any **dangling outcomes** without parents?

### Session Start Checklist

```bash
deciduous nodes    # What decisions exist?
deciduous edges    # How are they connected? Any gaps?
git status         # Current state
```

### Multi-User Sync

Share decisions across teammates:

```bash
# Export your branch's decisions
deciduous diff export --branch feature-x -o .deciduous/patches/my-feature.json

# Apply patches from teammates (idempotent)
deciduous diff apply .deciduous/patches/*.json

# Preview before applying
deciduous diff apply --dry-run .deciduous/patches/teammate.json
```

PR workflow: Export patch → commit patch file → PR → teammates apply.

### API Trace Capture

Capture Claude API traffic to correlate decisions with actual API work:

```bash
# Run Claude through the deciduous proxy
deciduous proxy -- claude

# View traces in TUI (press 't' for Trace view)
deciduous tui

# View traces in web viewer (click "Traces" tab)
deciduous serve
```

**Auto-linking**: When running through `deciduous proxy`, any `deciduous add` commands automatically link to the active API span. You'll see output like:

```
Created action #42 "Implementing auth" [traced: span #7]
```

This lets you see exactly which API calls produced which decisions - perfect for "vibe coding" visibility.

**Trace commands:**
```bash
deciduous trace sessions              # List all sessions
deciduous trace spans <session_id>    # List spans in a session
deciduous trace link <session_id> <node_id>   # Manual linking
deciduous trace prune --days 30       # Cleanup old traces
```
"#;

/// Claude Code agents.toml - defines domain-specific subagents for the project
/// NOTE: This is a template for NEW projects. The deciduous repo itself has a more
/// detailed version at .claude/agents.toml that should be kept in sync.
const CLAUDE_AGENTS_TOML: &str = r#"# Project Subagents Configuration
# Domain-specific agents for working on different parts of the codebase.
#
# When working on a specific domain, spawn a Task with subagent_type="Explore" or
# "general-purpose" and include the relevant agent's context in the prompt.
#
# Customize this file for YOUR project's structure. The domains below are examples.

# Example: Backend/Core agent
# [agents.backend]
# name = "Backend Agent"
# description = "API routes, database models, business logic"
# file_patterns = [
#     "src/**/*.rs",
#     "src/**/*.py",
#     "app/**/*.py"
# ]
# focus_areas = [
#     "Database operations",
#     "API endpoints",
#     "Business logic"
# ]
# instructions = """
# When working on backend:
# - Run tests before and after changes
# - Follow existing patterns for new endpoints
# - Maintain backwards compatibility
# """

# Example: Frontend agent
# [agents.frontend]
# name = "Frontend Agent"
# description = "UI components, state management, styling"
# file_patterns = [
#     "web/src/**/*.ts",
#     "web/src/**/*.tsx",
#     "src/components/**"
# ]
# focus_areas = [
#     "React components",
#     "State management",
#     "Styling and layout"
# ]
# instructions = """
# When working on frontend:
# - Test in browser after changes
# - Follow component patterns
# - Keep accessibility in mind
# """

# Example: Infrastructure agent
# [agents.infra]
# name = "Infrastructure Agent"
# description = "CI/CD, deployment, configuration"
# file_patterns = [
#     ".github/workflows/**",
#     "Dockerfile",
#     "docker-compose.yml",
#     "scripts/**"
# ]
# focus_areas = [
#     "GitHub Actions",
#     "Docker configuration",
#     "Deployment scripts"
# ]
# instructions = """
# When working on infrastructure:
# - Test workflows locally when possible
# - Keep builds fast with caching
# - Document any manual steps
# """
"#;

// ============================================================================
// WINDSURF-SPECIFIC TEMPLATES
// ============================================================================

/// Windsurf main rule - placed in .windsurf/rules/deciduous.md
/// NOTE: After running `deciduous init --windsurf`, open Windsurf's Customizations panel
/// and set this rule's activation mode to "Always On" for continuous enforcement.
const WINDSURF_DECIDUOUS_RULE: &str = r#"---
description: Decision graph workflow - log all goals, decisions, actions, and outcomes in real-time using deciduous CLI
globs:
alwaysApply: true
---

<decision_graph_workflow>

# Decision Graph Workflow

This project uses Deciduous for persistent decision tracking. You MUST log decisions in real-time.

## MANDATORY: Log These Events

<logging_triggers>
- **New feature request** → `deciduous add goal "Feature name" -c 90 -p "user's request"`
- **Choosing between approaches** → `deciduous add decision "What to decide" -c 75 -p "user asked"`
- **Considering an option** → `deciduous add option "Option name" -c 70`
- **About to write code** → `deciduous add action "What you're implementing" -c 85`
- **Noticed something** → `deciduous add observation "What you found" -c 80`
- **Something completed** → `deciduous add outcome "Result" -c 95`
</logging_triggers>

## CRITICAL: Capture VERBATIM User Prompts

<prompt_capture>
**Prompts must be the EXACT user message, not a summary.** Capture full messages word-for-word.

**BAD - summaries are useless for context recovery:**
```bash
deciduous add goal "Add auth" -p "User asked: add login to the app"  # DON'T DO THIS
```

**GOOD - verbatim prompts enable full context recovery:**
```bash
# Use --prompt-stdin for multi-line prompts
deciduous add goal "Add auth" -c 90 --prompt-stdin << 'EOF'
I need to add user authentication to the app. Users should be able to sign up
with email/password, and we need OAuth support for Google and GitHub.
EOF

# Update prompts on existing nodes
deciduous prompt <node_id> << 'EOF'
The full verbatim user message goes here...
EOF
```

**When to capture prompts:**
- Root `goal` nodes: YES - the FULL original request
- Major direction changes: YES - when user redirects the work
- Routine downstream nodes: NO - they inherit context via edges
</prompt_capture>

## MANDATORY: The Feedback Loop

<workflow>
1. USER REQUEST → Log goal/decision FIRST (before any code)
2. BEFORE coding → Log action
3. AFTER changes → Log outcome + link nodes
4. BEFORE git push → Run `deciduous sync`
</workflow>

## Commands

<commands>
```bash
# Add nodes (with confidence 0-100)
deciduous add goal "Title" -c 90
deciduous add decision "Title" -c 75
deciduous add action "Title" -c 85
deciduous add outcome "Title" -c 95
deciduous add observation "Title" -c 80

# Optional metadata flags for nodes
# -p, --prompt "..."   Store the user prompt that triggered this
# -f, --files "a.rs,b.rs"   Associate files with this node
# -b, --branch <name>   Git branch (auto-detected)
# --commit <hash|HEAD>   Link to a git commit (use HEAD for current commit)

# Example with prompt and files
deciduous add goal "Add auth" -c 90 -p "User asked: add login feature" -f "src/auth.rs,src/routes.rs"

# CRITICAL: After git commits, link them to the graph!
git commit -m "feat: add auth"
deciduous add action "Implemented auth" -c 90 --commit HEAD
deciduous link <goal_id> <action_id> -r "Implementation"

# Export graph AND git history for web viewer
deciduous sync  # Creates docs/graph-data.json and docs/git-history.json

# Filter by branch
deciduous nodes --branch main
deciduous nodes --branch feature-x

# Link nodes together
deciduous link <from> <to> -r "reason"
deciduous link 1 2 --edge-type chosen -r "Selected this approach"

# View current graph
deciduous nodes
deciduous edges

# Sync before pushing
deciduous sync
```
</commands>

## Branch-Based Grouping

<branch_grouping>
**Nodes are automatically tagged with the current git branch.**

Configure in `.deciduous/config.toml`:
```toml
[branch]
main_branches = ["main", "master"]
auto_detect = true
```

### CLI Commands
```bash
deciduous nodes --branch main       # Filter by branch
deciduous add goal "X" -b feature-x # Override branch
deciduous add goal "X" --no-branch  # No branch tag
```

### Web UI
Branch dropdown filter in stats bar filters all views.
</branch_grouping>

## Edge Types

<edge_types>
- `leads_to` - Natural progression (default)
- `chosen` - Selected this option
- `rejected` - Did not select (include why!)
- `requires` - Dependency
- `blocks` - Preventing progress
- `enables` - Makes possible
</edge_types>

## ⚠️ CRITICAL: Maintain Connections

<connection_rules>
**The graph's value is in its CONNECTIONS, not just nodes.**

| When you create... | IMMEDIATELY link to... |
|-------------------|------------------------|
| `outcome` | The action/goal it resolves |
| `action` | The goal/decision that spawned it |
| `option` | Its parent decision |
| `observation` | Related goal/action |

**Root `goal` nodes are the ONLY valid orphans.**

### Audit Before Every Sync
1. Does every **outcome** link to what caused it?
2. Does every **action** link to why you did it?
3. Any **dangling outcomes** without parents?
</connection_rules>

## Multi-User Sync

<multi_user_sync>
Share decisions across teammates:

```bash
# Export your branch's decisions
deciduous diff export --branch feature-x -o .deciduous/patches/my-feature.json

# Apply patches from teammates (idempotent)
deciduous diff apply .deciduous/patches/*.json

# Preview before applying
deciduous diff apply --dry-run .deciduous/patches/teammate.json
```

PR workflow: Export patch → commit patch file → PR → teammates apply.
</multi_user_sync>

</decision_graph_workflow>
"#;

/// Windsurf context rule - placed in .windsurf/rules/recover.md
/// Model-triggered rule for session recovery
const WINDSURF_RECOVER_RULE: &str = r#"---
description: Context recovery - query decision graph at session start or when recovering from context loss
globs:
alwaysApply: false
---

<context_recovery>

# Context Recovery

When starting a session or recovering context, query the decision graph:

<session_start>
```bash
# 1. See what decisions exist (look for recent/pending)
deciduous nodes

# 2. See how they connect
deciduous edges

# 3. Check git state
git status
git log --oneline -10
```
</session_start>

## After Querying, Report:

1. Current branch and pending changes
2. Recent decisions (especially pending/active ones)
3. Last actions from the command log
4. Suggested next steps

<important>
The decision graph survives context loss. Query it whenever you need to understand
what was decided previously. Then continue logging per the deciduous.md rule.
</important>

</context_recovery>
"#;

/// Windsurf memories file - placed in .windsurf/memories.md
/// Project-level memories that Cascade auto-retrieves when relevant
const WINDSURF_MEMORIES: &str = r#"# Project Memories

## Decision Graph System

This project uses **Deciduous** for persistent decision tracking.

<memory>
**What is Deciduous?**
A decision graph tool that tracks goals, decisions, actions, and outcomes.
The graph survives context loss - when sessions end or context compacts,
the reasoning persists in the database.
</memory>

<memory>
**Why log decisions?**
- Context compaction loses your reasoning
- The graph survives and is queryable by future sessions
- Retroactive logging misses details - log in the moment
- The user may be watching the graph live
</memory>

<memory>
**Key Commands**
- `deciduous nodes` - see all decisions
- `deciduous edges` - see connections
- `deciduous add <type> "title" -c <confidence>` - add node
- `deciduous link <from> <to> -r "reason"` - connect nodes
- `deciduous sync` - export to docs/graph-data.json
</memory>

<memory>
**Node Types**
- goal: High-level objective
- decision: Choice point
- option: Possible approach
- action: Implementation step
- outcome: Result
- observation: Finding/insight
</memory>

<memory>
**The Rule**
LOG BEFORE YOU CODE, NOT AFTER.
SYNC BEFORE YOU PUSH.
</memory>
"#;

/// AGENTS.md section for Windsurf (equivalent to CLAUDE.md section)
const AGENTS_MD_SECTION: &str = r#"
## Decision Graph Workflow

**THIS IS MANDATORY. Log decisions IN REAL-TIME, not retroactively.**

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
| Choosing between approaches | `decision` | "Choose state management" |
| About to write/edit code | `action` | "Implementing Redux store" |
| Something worked or failed | `outcome` | "Redux integration successful" |
| Notice something interesting | `observation` | "Existing code uses hooks" |

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

Prompts are viewable in the TUI detail panel (`deciduous tui`) and web viewer.

### ⚠️ CRITICAL: Maintain Connections

**The graph's value is in its CONNECTIONS, not just nodes.**

| When you create... | IMMEDIATELY link to... |
|-------------------|------------------------|
| `outcome` | The action/goal it resolves |
| `action` | The goal/decision that spawned it |
| `option` | Its parent decision |
| `observation` | Related goal/action |

**Root `goal` nodes are the ONLY valid orphans.**

### Quick Commands

```bash
deciduous add goal "Title" -c 90
deciduous add action "Title" -c 85
deciduous link FROM TO -r "reason"  # DO THIS IMMEDIATELY!
deciduous serve   # View live (auto-refreshes every 30s)
deciduous sync    # Export for static hosting

# Optional metadata
# -p, --prompt "..."   Store the user prompt
# -f, --files "a.rs,b.rs"   Associate files
# -b, --branch <name>   Git branch (auto-detected)
# --commit HEAD   Link to current git commit

# Branch filtering
deciduous nodes --branch main
deciduous nodes -b feature-auth
```

### ⚠️ CRITICAL: Link Commits to Actions/Outcomes

**After every git commit, link it to the decision graph!**

```bash
git commit -m "feat: add auth"
deciduous add action "Implemented auth" -c 90 --commit HEAD
deciduous link <goal_id> <action_id> -r "Implementation"
```

The `--commit HEAD` flag captures the commit hash. The web viewer shows commit messages alongside nodes.

### Git History & Deployment

```bash
# Export graph AND git history for web viewer
deciduous sync

# Creates docs/graph-data.json and docs/git-history.json
```

Deploy to GitHub Pages: `deciduous sync` > push > Settings > Pages > /docs folder

### Branch-Based Grouping

Nodes are auto-tagged with the current git branch. Configure in `.deciduous/config.toml`:
```toml
[branch]
main_branches = ["main", "master"]
auto_detect = true
```

### Audit Checklist (Before Every Sync)

1. Does every **outcome** link to what caused it?
2. Does every **action** link to why you did it?
3. Any **dangling outcomes** without parents?

### Session Start Checklist

Every new session, run:

```bash
deciduous nodes    # What decisions exist?
deciduous edges    # How are they connected?
git status         # Current state
git log -10        # Recent commits
```

### Multi-User Sync

Share decisions across teammates:

```bash
# Export your branch's decisions
deciduous diff export --branch feature-x -o .deciduous/patches/my-feature.json

# Apply patches from teammates (idempotent)
deciduous diff apply .deciduous/patches/*.json

# Preview before applying
deciduous diff apply --dry-run .deciduous/patches/teammate.json
```

PR workflow: Export patch → commit patch file → PR → teammates apply.

### API Trace Capture

Capture Claude API traffic to correlate decisions with actual work:

```bash
deciduous proxy -- claude   # Run with tracing
deciduous tui               # Press 't' for Trace view
deciduous serve             # Click "Traces" tab
```

**Auto-linking**: `deciduous add` commands through proxy automatically link to active spans:
```
Created action #42 "Implementing auth" [traced: span #7]
```
"#;

// ============================================================================
// OPENCODE-SPECIFIC TEMPLATES
// ============================================================================

/// OpenCode decision command - placed in .opencode/command/decision.md
/// Note: OpenCode uses simpler frontmatter than Claude (just description)
const OPENCODE_DECISION_CMD: &str = r#"---
description: "Manage decision graph - track choices and reasoning. Usage: /decision <action> [args...]"
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
- `add obs <title>` -> `deciduous add observation "<title>" -c 80`
- `add outcome <title>` -> `deciduous add outcome "<title>" -c 90`

### Optional Flags for Nodes
- `-c, --confidence <0-100>` - Confidence level
- `-p, --prompt "..."` - Store the user prompt that triggered this node
- `-f, --files "src/main.py,lib/utils.js"` - Associate files with this node
- `-b, --branch <name>` - Git branch (auto-detected by default)
- `--no-branch` - Skip branch auto-detection
- `--commit <hash|HEAD>` - Link to a git commit (use HEAD for current commit)

### CRITICAL: Link Commits to Actions/Outcomes

**After every git commit, link it to the decision graph!**

```bash
git commit -m "feat: add auth"
deciduous add action "Implemented auth" -c 90 --commit HEAD
deciduous link <goal_id> <action_id> -r "Implementation"
```

## CRITICAL: Capture VERBATIM User Prompts

**Prompts must be the EXACT user message, not a summary.** Capture full messages word-for-word.

**BAD - summaries are useless:**
```bash
deciduous add goal "Add auth" -p "User asked: add login"  # DON'T DO THIS
```

**GOOD - verbatim prompts enable context recovery:**
```bash
# Use --prompt-stdin for multi-line prompts
deciduous add goal "Add auth" -c 90 --prompt-stdin << 'EOF'
I need to add user authentication to the app. Users should be able to sign up
with email/password, and we need OAuth support for Google and GitHub.
EOF

# Update prompts on existing nodes
deciduous prompt <node_id> << 'EOF'
The full verbatim user message here...
EOF
```

**When to capture prompts:**
- Root `goal` nodes: YES - the FULL original request
- Major direction changes: YES - when user redirects

### Create Edges
- `link <from> <to> [reason]` -> `deciduous link <from> <to> -r "<reason>"`

### Sync Graph
- `sync` -> `deciduous sync`

### Multi-User Sync (Diff/Patch)
- `diff export -o <file>` -> `deciduous diff export -o <file>`
- `diff export --nodes 1-10 -o <file>` -> export specific nodes
- `diff export --branch feature-x -o <file>` -> export nodes from branch
- `diff apply <file>` -> `deciduous diff apply <file>` (idempotent)
- `diff apply --dry-run <file>` -> preview without applying
- `diff status` -> `deciduous diff status`

### Export & Visualization
- `dot` -> `deciduous dot`
- `dot --png` -> `deciduous dot --png -o graph.dot`
- `writeup` -> `deciduous writeup`
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

## Graph Integrity - CRITICAL

**Every node MUST be logically connected.** Floating nodes break the graph's value.

### Connection Rules
| Node Type | MUST connect to |
|-----------|----------------|
| `outcome` | The action/goal it resolves |
| `action` | The decision/goal that spawned it |
| `option` | Its parent decision |
| `observation` | Related goal/action/decision |
| `decision` | Parent goal (if any) |
| `goal` | Can be a root (no parent needed) |

## API Trace Capture

When running through `deciduous proxy`, nodes auto-link to API spans:
```bash
deciduous proxy -- claude   # Run with tracing
```

## The Rule

```
LOG BEFORE YOU CODE, NOT AFTER.
CONNECT EVERY NODE TO ITS PARENT.
AUDIT FOR ORPHANS REGULARLY.
SYNC BEFORE YOU PUSH.
```
"#;

/// OpenCode context command - placed in .opencode/command/recover.md
const OPENCODE_RECOVER_CMD: &str = r#"---
description: "Recover context from decision graph - USE THIS ON SESSION START. Usage: /recover [focus-area]"
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
```

**Branch-scoped context**: If working on a feature branch, filter nodes to see only decisions relevant to this branch.

## Step 1.5: Audit Graph Integrity

**CRITICAL: Check that all nodes are logically connected.**

```bash
# Find nodes with no incoming edges (potential missing connections)
deciduous edges | cut -d'>' -f2 | cut -d' ' -f2 | sort -u > /tmp/has_parent.txt
deciduous nodes | tail -n+3 | awk '{print $1}' | while read id; do
  grep -q "^$id$" /tmp/has_parent.txt || echo "CHECK: $id"
done
```

**Review each flagged node:**
- Root `goal` nodes are VALID without parents
- `outcome` nodes MUST link back to their action/goal
- `action` nodes MUST link to their parent goal/decision
- `option` nodes MUST link to their parent decision

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

## After Gathering Context, Report:

1. **Current branch** and pending changes
2. **Branch-specific decisions** (filter by branch if on feature branch)
3. **Recent decisions** (especially pending/active ones)
4. **Last actions** from git log and command log
5. **Open questions** or unresolved observations
6. **Suggested next steps**

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

deciduous sync  # Do this frequently!
```

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

If working in a team, check for and apply patches from teammates:

```bash
# Check for unapplied patches
deciduous diff status

# Apply all patches (idempotent - safe to run multiple times)
deciduous diff apply .deciduous/patches/*.json

# Preview before applying
deciduous diff apply --dry-run .deciduous/patches/teammate-feature.json
```

Before pushing your branch, export your decisions for teammates:

```bash
# Export your branch's decisions as a patch
deciduous diff export --branch $(git rev-parse --abbrev-ref HEAD) \
  -o .deciduous/patches/$(whoami)-$(git rev-parse --abbrev-ref HEAD).json

# Commit the patch file
git add .deciduous/patches/
```

## API Trace Capture

When running through `deciduous proxy`, decisions auto-link to API spans:

```bash
deciduous proxy -- claude   # Run with tracing
deciduous tui               # Press 't' for Trace view
```

## Why This Matters

- Context loss during compaction loses your reasoning
- The graph survives - query it early, query it often
- Retroactive logging misses details - log in the moment
- The user sees the graph live - show your work
"#;

/// OpenCode build-test command
const OPENCODE_BUILD_TEST_CMD: &str = r#"---
description: "Build the project and run the test suite"
---

# Build and Test

Build the project and run the test suite.

## Instructions

1. Detect the project type and run appropriate build/test commands:

   **Rust (Cargo.toml exists):**
   ```bash
   cargo build && cargo test
   ```

   **Node.js (package.json exists):**
   ```bash
   npm install && npm test
   ```

   **Python (pyproject.toml or setup.py exists):**
   ```bash
   pip install -e . && pytest
   ```

   **Go (go.mod exists):**
   ```bash
   go build ./... && go test ./...
   ```

2. If tests fail, analyze the failures and explain:
   - Which test failed
   - What it was testing
   - Likely cause of failure
   - Suggested fix

3. If all tests pass, report success and any warnings from the build.

4. If $ARGUMENTS specifies a test pattern, run only those tests.

$ARGUMENTS
"#;

/// OpenCode serve-ui command
const OPENCODE_SERVE_UI_CMD: &str = r#"---
description: "Launch the deciduous web server for viewing the decision graph"
---

# Start Decision Graph Viewer

Launch the deciduous web server for viewing and navigating the decision graph.

## Instructions

1. Start the server:
   ```bash
   deciduous serve --port 3000
   ```

2. Inform the user:
   - The server is running at http://localhost:3000
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

## Alternative: Static Hosting

For GitHub Pages or other static hosting:
```bash
deciduous sync  # Exports to docs/graph-data.json
```

Then push to GitHub - the graph is viewable at your GitHub Pages URL.

$ARGUMENTS
"#;

/// OpenCode sync-graph command
const OPENCODE_SYNC_GRAPH_CMD: &str = r#"---
description: "Export the decision graph to docs/ for GitHub Pages"
---

# Sync Decision Graph to GitHub Pages

Export the current decision graph to docs/graph-data.json so it's deployed to GitHub Pages.

## Steps

1. Run `deciduous sync` to export the graph
2. Show the user how many nodes/edges were exported
3. If there are changes, stage them: `git add docs/graph-data.json`

This should be run before any push to main to ensure the live site has the latest decisions.
"#;

// ============================================================================
// CODEX-SPECIFIC TEMPLATES
// ============================================================================

/// Codex decision prompt - placed in .codex/prompts/decision.md
/// Note: Codex uses top-level prompts/ directory only (no subdirs)
/// Invoked as: /prompts:decision ACTION="add goal" TITLE="Feature name"
const CODEX_DECISION_PROMPT: &str = r#"---
description: Manage decision graph - track algorithm choices and reasoning
argument-hint: [ACTION=<action>] [TITLE="<title>"]
---

# Decision Graph Management

**Log decisions IN REAL-TIME as you work, not retroactively.**

## When to Use This

| You're doing this... | Log this type | Command |
|---------------------|---------------|---------|
| Starting a new feature | `goal` **with -p** | `/prompts:decision ACTION="add goal" TITLE="Add user auth"` |
| Choosing between approaches | `decision` | `/prompts:decision ACTION="add decision" TITLE="Choose auth method"` |
| Considering an option | `option` | `/prompts:decision ACTION="add option" TITLE="JWT tokens"` |
| About to write code | `action` | `/prompts:decision ACTION="add action" TITLE="Implementing JWT"` |
| Noticing something | `observation` | `/prompts:decision ACTION="add obs" TITLE="Found existing code"` |
| Finished something | `outcome` | `/prompts:decision ACTION="add outcome" TITLE="JWT working"` |

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
- `add obs <title>` -> `deciduous add observation "<title>" -c 80`
- `add outcome <title>` -> `deciduous add outcome "<title>" -c 90`

### Optional Flags for Nodes
- `-c, --confidence <0-100>` - Confidence level
- `-p, --prompt "..."` - Store the user prompt that triggered this node
- `-f, --files "src/main.py,lib/utils.js"` - Associate files with this node
- `-b, --branch <name>` - Git branch (auto-detected by default)
- `--no-branch` - Skip branch auto-detection
- `--commit <hash|HEAD>` - Link to a git commit (use HEAD for current commit)

### CRITICAL: Link Commits to Actions/Outcomes

**After every git commit, link it to the decision graph!**

```bash
git commit -m "feat: add auth"
deciduous add action "Implemented auth" -c 90 --commit HEAD
deciduous link <goal_id> <action_id> -r "Implementation"
```

## CRITICAL: Capture User Prompts When Semantically Meaningful

**Use `-p` / `--prompt` when a user request triggers new work or changes direction.** Don't add prompts to every node - only when a prompt is the actual catalyst.

```bash
# New feature request - capture the prompt on the goal
deciduous add goal "Add auth" -c 90 -p "User asked: add login to the app"

# Downstream work links back - no prompt needed (it flows via edges)
deciduous add decision "Choose auth method" -c 75
deciduous link <goal_id> <decision_id> -r "Deciding approach"

# BUT if the user gives new direction mid-stream, capture that too
deciduous add action "Switch to OAuth" -c 85 -p "User said: use OAuth instead"
```

**When to capture prompts:**
- Root `goal` nodes: YES - the original request
- Major direction changes: YES - when user redirects the work
- Routine downstream nodes: NO - they inherit context via edges

### Create Edges
- `link <from> <to> [reason]` -> `deciduous link <from> <to> -r "<reason>"`

### Sync Graph
- `sync` -> `deciduous sync`

### Multi-User Sync (Diff/Patch)
- `diff export -o <file>` -> `deciduous diff export -o <file>`
- `diff export --nodes 1-10 -o <file>` -> export specific nodes
- `diff export --branch feature-x -o <file>` -> export nodes from branch
- `diff apply <file>` -> `deciduous diff apply <file>` (idempotent)
- `diff apply --dry-run <file>` -> preview without applying
- `diff status` -> `deciduous diff status`

### Export & Visualization
- `dot` -> `deciduous dot`
- `dot --png` -> `deciduous dot --png -o graph.dot`
- `writeup` -> `deciduous writeup`
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

## Graph Integrity - CRITICAL

**Every node MUST be logically connected.** Floating nodes break the graph's value.

### Connection Rules
| Node Type | MUST connect to |
|-----------|----------------|
| `outcome` | The action/goal it resolves |
| `action` | The decision/goal that spawned it |
| `option` | Its parent decision |
| `observation` | Related goal/action/decision |
| `decision` | Parent goal (if any) |
| `goal` | Can be a root (no parent needed) |

## API Trace Capture

When running through `deciduous proxy`, nodes auto-link to API spans:
```bash
deciduous proxy -- claude   # Run with tracing
```

## The Rule

```
LOG BEFORE YOU CODE, NOT AFTER.
CONNECT EVERY NODE TO ITS PARENT.
AUDIT FOR ORPHANS REGULARLY.
SYNC BEFORE YOU PUSH.
```
"#;

/// Codex recover prompt - placed in .codex/prompts/recover.md
const CODEX_RECOVER_PROMPT: &str = r#"---
description: Recover context from decision graph - USE THIS ON SESSION START
argument-hint: [FOCUS="<area>"]
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
```

**Branch-scoped context**: If working on a feature branch, filter nodes to see only decisions relevant to this branch.

## Step 1.5: Audit Graph Integrity

**CRITICAL: Check that all nodes are logically connected.**

```bash
# Find nodes with no incoming edges (potential missing connections)
deciduous edges | cut -d'>' -f2 | cut -d' ' -f2 | sort -u > /tmp/has_parent.txt
deciduous nodes | tail -n+3 | awk '{print $1}' | while read id; do
  grep -q "^$id$" /tmp/has_parent.txt || echo "CHECK: $id"
done
```

**Review each flagged node:**
- Root `goal` nodes are VALID without parents
- `outcome` nodes MUST link back to their action/goal
- `action` nodes MUST link to their parent goal/decision
- `option` nodes MUST link to their parent decision

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

## After Gathering Context, Report:

1. **Current branch** and pending changes
2. **Branch-specific decisions** (filter by branch if on feature branch)
3. **Recent decisions** (especially pending/active ones)
4. **Last actions** from git log and command log
5. **Open questions** or unresolved observations
6. **Suggested next steps**

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

deciduous sync  # Do this frequently!
```

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
Run /prompts:recover -> See past decisions
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

If working in a team, check for and apply patches from teammates:

```bash
# Check for unapplied patches
deciduous diff status

# Apply all patches (idempotent - safe to run multiple times)
deciduous diff apply .deciduous/patches/*.json

# Preview before applying
deciduous diff apply --dry-run .deciduous/patches/teammate-feature.json
```

Before pushing your branch, export your decisions for teammates:

```bash
# Export your branch's decisions as a patch
deciduous diff export --branch $(git rev-parse --abbrev-ref HEAD) \
  -o .deciduous/patches/$(whoami)-$(git rev-parse --abbrev-ref HEAD).json

# Commit the patch file
git add .deciduous/patches/
```

## API Trace Capture

When running through `deciduous proxy`, decisions auto-link to API spans:

```bash
deciduous proxy -- claude   # Run with tracing
deciduous tui               # Press 't' for Trace view
```

## Why This Matters

- Context loss during compaction loses your reasoning
- The graph survives - query it early, query it often
- Retroactive logging misses details - log in the moment
- The user sees the graph live - show your work
"#;

/// Codex build-test prompt
const CODEX_BUILD_TEST_PROMPT: &str = r#"---
description: Build the project and run the test suite
argument-hint: [PATTERN="<test-pattern>"]
---

# Build and Test

Build the project and run the test suite.

## Instructions

1. Detect the project type and run appropriate build/test commands:

   **Rust (Cargo.toml exists):**
   ```bash
   cargo build && cargo test
   ```

   **Node.js (package.json exists):**
   ```bash
   npm install && npm test
   ```

   **Python (pyproject.toml or setup.py exists):**
   ```bash
   pip install -e . && pytest
   ```

   **Go (go.mod exists):**
   ```bash
   go build ./... && go test ./...
   ```

2. If tests fail, analyze the failures and explain:
   - Which test failed
   - What it was testing
   - Likely cause of failure
   - Suggested fix

3. If all tests pass, report success and any warnings from the build.

4. If $PATTERN specifies a test pattern, filter tests accordingly.

$ARGUMENTS
"#;

/// Codex serve-ui prompt
const CODEX_SERVE_UI_PROMPT: &str = r#"---
description: Launch the deciduous web server for viewing the decision graph
argument-hint: [PORT="<port>"]
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

## Alternative: Static Hosting

For GitHub Pages or other static hosting:
```bash
deciduous sync  # Exports to docs/graph-data.json
```

Then push to GitHub - the graph is viewable at your GitHub Pages URL.

$ARGUMENTS
"#;

/// Codex sync-graph prompt
const CODEX_SYNC_GRAPH_PROMPT: &str = r#"---
description: Export the decision graph to docs/ for GitHub Pages
argument-hint:
---

# Sync Decision Graph to GitHub Pages

Export the current decision graph to docs/graph-data.json so it's deployed to GitHub Pages.

## Steps

1. Run `deciduous sync` to export the graph
2. Show the user how many nodes/edges were exported
3. If there are changes, stage them: `git add docs/graph-data.json`

This should be run before any push to main to ensure the live site has the latest decisions.
"#;

/// Initialize deciduous in the current directory
pub fn init_project(editor: Editor, force: bool) -> Result<(), String> {
    let cwd =
        std::env::current_dir().map_err(|e| format!("Could not get current directory: {}", e))?;

    let editor_name = match editor {
        Editor::Claude => "Claude Code",
        Editor::Windsurf => "Windsurf",
        Editor::Opencode => "OpenCode",
        Editor::Codex => "Codex",
    };

    println!(
        "\n{}",
        format!("Initializing Deciduous for {}...", editor_name)
            .cyan()
            .bold()
    );
    println!("   Directory: {}", cwd.display());
    if force {
        println!(
            "   Mode: {} (overwriting existing files)\n",
            "force".yellow()
        );
    } else {
        println!();
    }

    // 1. Create .deciduous directory (shared between all editors)
    let deciduous_dir = cwd.join(".deciduous");
    create_dir_if_missing(&deciduous_dir)?;

    // 1b. Create default config.toml if it doesn't exist (or overwrite with force)
    let config_path = deciduous_dir.join("config.toml");
    if force {
        write_file_overwrite(&config_path, DEFAULT_CONFIG, ".deciduous/config.toml")?;
    } else {
        write_file_if_missing(&config_path, DEFAULT_CONFIG, ".deciduous/config.toml")?;
    }

    // 2. Initialize database by opening it (creates tables)
    let db_path = deciduous_dir.join("deciduous.db");
    if db_path.exists() {
        println!(
            "   {} .deciduous/deciduous.db (already exists, preserving data)",
            "Skipping".yellow()
        );
    } else {
        println!("   {} .deciduous/deciduous.db", "Creating".green());
    }

    // Set the env var so Database::open() uses this path
    // Database::open() uses CREATE TABLE IF NOT EXISTS - safe for existing DBs
    std::env::set_var("DECIDUOUS_DB_PATH", &db_path);

    // 3. Create editor-specific configuration
    match editor {
        Editor::Claude => {
            // Create .claude/commands directory
            let claude_dir = cwd.join(".claude").join("commands");
            create_dir_if_missing(&claude_dir)?;

            // Write deciduous.decision.md slash command
            let decision_path = claude_dir.join("deciduous.decision.md");
            if force {
                write_file_overwrite(
                    &decision_path,
                    DECISION_MD,
                    ".claude/commands/deciduous.decision.md",
                )?;
            } else {
                write_file_if_missing(
                    &decision_path,
                    DECISION_MD,
                    ".claude/commands/deciduous.decision.md",
                )?;
            }

            // Write deciduous.recover.md slash command (context recovery)
            let recover_path = claude_dir.join("deciduous.recover.md");
            if force {
                write_file_overwrite(
                    &recover_path,
                    RECOVER_MD,
                    ".claude/commands/deciduous.recover.md",
                )?;
            } else {
                write_file_if_missing(
                    &recover_path,
                    RECOVER_MD,
                    ".claude/commands/deciduous.recover.md",
                )?;
            }

            // Write agents.toml for subagent configuration
            let claude_base = cwd.join(".claude");
            let agents_path = claude_base.join("agents.toml");
            if force {
                write_file_overwrite(&agents_path, CLAUDE_AGENTS_TOML, ".claude/agents.toml")?;
            } else {
                write_file_if_missing(&agents_path, CLAUDE_AGENTS_TOML, ".claude/agents.toml")?;
            }

            // Handle CLAUDE.md - append if missing, replace section if force
            let claude_md_path = cwd.join("CLAUDE.md");
            if force {
                write_file_overwrite(&claude_md_path, CLAUDE_MD_SECTION, "CLAUDE.md")?;
            } else {
                append_config_md(&claude_md_path, CLAUDE_MD_SECTION, "CLAUDE.md")?;
            }

            // Create .claude/skills/deciduous directory and SKILL.md
            let skills_dir = cwd.join(".claude").join("skills").join("deciduous");
            create_dir_if_missing(&skills_dir)?;

            let skill_path = skills_dir.join("SKILL.md");
            if force {
                write_file_overwrite(
                    &skill_path,
                    DECIDUOUS_SKILL,
                    ".claude/skills/deciduous/SKILL.md",
                )?;
            } else {
                write_file_if_missing(
                    &skill_path,
                    DECIDUOUS_SKILL,
                    ".claude/skills/deciduous/SKILL.md",
                )?;
            }
        }
        Editor::Windsurf => {
            // Create .windsurf directory
            let windsurf_base = cwd.join(".windsurf");
            create_dir_if_missing(&windsurf_base)?;

            // Create .windsurf/rules directory
            let windsurf_rules = windsurf_base.join("rules");
            create_dir_if_missing(&windsurf_rules)?;

            // Write deciduous.md rule (Always On - main workflow)
            let deciduous_rule_path = windsurf_rules.join("deciduous.md");
            if force {
                write_file_overwrite(
                    &deciduous_rule_path,
                    WINDSURF_DECIDUOUS_RULE,
                    ".windsurf/rules/deciduous.md",
                )?;
            } else {
                write_file_if_missing(
                    &deciduous_rule_path,
                    WINDSURF_DECIDUOUS_RULE,
                    ".windsurf/rules/deciduous.md",
                )?;
            }

            // Write recover.md rule (Model-triggered - for session recovery)
            let recover_path = windsurf_rules.join("recover.md");
            if force {
                write_file_overwrite(
                    &recover_path,
                    WINDSURF_RECOVER_RULE,
                    ".windsurf/rules/recover.md",
                )?;
            } else {
                write_file_if_missing(
                    &recover_path,
                    WINDSURF_RECOVER_RULE,
                    ".windsurf/rules/recover.md",
                )?;
            }

            // Write memories.md (project-level memories Cascade auto-retrieves)
            let memories_path = windsurf_base.join("memories.md");
            if force {
                write_file_overwrite(&memories_path, WINDSURF_MEMORIES, ".windsurf/memories.md")?;
            } else {
                write_file_if_missing(&memories_path, WINDSURF_MEMORIES, ".windsurf/memories.md")?;
            }

            // Handle AGENTS.md - append if missing, overwrite if force
            let agents_md_path = cwd.join("AGENTS.md");
            if force {
                write_file_overwrite(&agents_md_path, AGENTS_MD_SECTION, "AGENTS.md")?;
            } else {
                append_config_md(&agents_md_path, AGENTS_MD_SECTION, "AGENTS.md")?;
            }
        }
        Editor::Opencode => {
            // Create .opencode/command directory (note: singular "command", not "commands")
            let opencode_cmd_dir = cwd.join(".opencode").join("command");
            create_dir_if_missing(&opencode_cmd_dir)?;

            // Helper to write file with force support
            let write_cmd = |path: &Path, content: &str, name: &str| -> Result<(), String> {
                if force {
                    write_file_overwrite(path, content, name)
                } else {
                    write_file_if_missing(path, content, name)
                }
            };

            // Write decision.md command
            let decision_path = opencode_cmd_dir.join("decision.md");
            write_cmd(
                &decision_path,
                OPENCODE_DECISION_CMD,
                ".opencode/command/decision.md",
            )?;

            // Write recover.md command (context recovery)
            let recover_path = opencode_cmd_dir.join("recover.md");
            write_cmd(
                &recover_path,
                OPENCODE_RECOVER_CMD,
                ".opencode/command/recover.md",
            )?;

            // Write build-test.md command
            let build_test_path = opencode_cmd_dir.join("build-test.md");
            write_cmd(
                &build_test_path,
                OPENCODE_BUILD_TEST_CMD,
                ".opencode/command/build-test.md",
            )?;

            // Write serve-ui.md command
            let serve_ui_path = opencode_cmd_dir.join("serve-ui.md");
            write_cmd(
                &serve_ui_path,
                OPENCODE_SERVE_UI_CMD,
                ".opencode/command/serve-ui.md",
            )?;

            // Write sync-graph.md command
            let sync_graph_path = opencode_cmd_dir.join("sync-graph.md");
            write_cmd(
                &sync_graph_path,
                OPENCODE_SYNC_GRAPH_CMD,
                ".opencode/command/sync-graph.md",
            )?;

            // Handle AGENTS.md - append if missing, overwrite if force
            let agents_md_path = cwd.join("AGENTS.md");
            if force {
                write_file_overwrite(&agents_md_path, AGENTS_MD_SECTION, "AGENTS.md")?;
            } else {
                append_config_md(&agents_md_path, AGENTS_MD_SECTION, "AGENTS.md")?;
            }
        }
        Editor::Codex => {
            // Create .codex/prompts directory (top-level only, Codex doesn't read subdirs)
            let codex_prompts_dir = cwd.join(".codex").join("prompts");
            create_dir_if_missing(&codex_prompts_dir)?;

            // Helper to write file with force support
            let write_prompt = |path: &Path, content: &str, name: &str| -> Result<(), String> {
                if force {
                    write_file_overwrite(path, content, name)
                } else {
                    write_file_if_missing(path, content, name)
                }
            };

            // Write deciduous.decision.md prompt
            let decision_path = codex_prompts_dir.join("deciduous.decision.md");
            write_prompt(
                &decision_path,
                CODEX_DECISION_PROMPT,
                ".codex/prompts/deciduous.decision.md",
            )?;

            // Write deciduous.recover.md prompt (context recovery)
            let recover_path = codex_prompts_dir.join("deciduous.recover.md");
            write_prompt(
                &recover_path,
                CODEX_RECOVER_PROMPT,
                ".codex/prompts/deciduous.recover.md",
            )?;

            // Write deciduous.build-test.md prompt
            let build_test_path = codex_prompts_dir.join("deciduous.build-test.md");
            write_prompt(
                &build_test_path,
                CODEX_BUILD_TEST_PROMPT,
                ".codex/prompts/deciduous.build-test.md",
            )?;

            // Write deciduous.serve-ui.md prompt
            let serve_ui_path = codex_prompts_dir.join("deciduous.serve-ui.md");
            write_prompt(
                &serve_ui_path,
                CODEX_SERVE_UI_PROMPT,
                ".codex/prompts/deciduous.serve-ui.md",
            )?;

            // Write deciduous.sync-graph.md prompt
            let sync_graph_path = codex_prompts_dir.join("deciduous.sync-graph.md");
            write_prompt(
                &sync_graph_path,
                CODEX_SYNC_GRAPH_PROMPT,
                ".codex/prompts/deciduous.sync-graph.md",
            )?;

            // Add Codex-specific entries to .gitignore (selective ignoring)
            add_codex_to_gitignore(&cwd)?;

            // Handle AGENTS.md - append if missing, overwrite if force
            let agents_md_path = cwd.join("AGENTS.md");
            if force {
                write_file_overwrite(&agents_md_path, AGENTS_MD_SECTION, "AGENTS.md")?;
            } else {
                append_config_md(&agents_md_path, AGENTS_MD_SECTION, "AGENTS.md")?;
            }
        }
    }

    // 4. Add .deciduous to .gitignore if not already there
    add_to_gitignore(&cwd)?;

    // 5. Create GitHub workflows (if .git exists)
    let git_dir = cwd.join(".git");
    if git_dir.exists() {
        let workflows_dir = cwd.join(".github").join("workflows");
        create_dir_if_missing(&workflows_dir)?;

        // Cleanup workflow for PR graph assets
        let cleanup_path = workflows_dir.join("cleanup-decision-graphs.yml");
        write_file_if_missing(
            &cleanup_path,
            CLEANUP_WORKFLOW,
            ".github/workflows/cleanup-decision-graphs.yml",
        )?;

        // Deploy workflow for GitHub Pages
        let deploy_path = workflows_dir.join("deploy-pages.yml");
        write_file_if_missing(
            &deploy_path,
            DEPLOY_PAGES_WORKFLOW,
            ".github/workflows/deploy-pages.yml",
        )?;
    }

    // 6. Create docs/ directory for GitHub Pages
    let docs_dir = cwd.join("docs");
    create_dir_if_missing(&docs_dir)?;

    // 7. Write static viewer HTML to docs/index.html
    let viewer_path = docs_dir.join("index.html");
    write_file_if_missing(&viewer_path, PAGES_VIEWER_HTML, "docs/index.html")?;

    // 8. Create empty graph-data.json (will be populated by sync)
    let graph_data_path = docs_dir.join("graph-data.json");
    if !graph_data_path.exists() {
        let empty_graph = r#"{"nodes":[],"edges":[]}"#;
        fs::write(&graph_data_path, empty_graph)
            .map_err(|e| format!("Could not write graph-data.json: {}", e))?;
        println!("   {} docs/graph-data.json", "Creating".green());
    }

    // 9. Create .nojekyll for GitHub Pages (prevents Jekyll processing)
    let nojekyll_path = docs_dir.join(".nojekyll");
    if !nojekyll_path.exists() {
        fs::write(&nojekyll_path, "").map_err(|e| format!("Could not write .nojekyll: {}", e))?;
        println!("   {} docs/.nojekyll", "Creating".green());
    }

    println!(
        "\n{}",
        format!("Deciduous initialized for {}!", editor_name)
            .green()
            .bold()
    );
    println!("\nNext steps:");
    println!(
        "  1. Run {} to start the local graph viewer",
        "deciduous serve".cyan()
    );
    println!(
        "  2. Run {} to export graph for GitHub Pages",
        "deciduous sync".cyan()
    );

    match editor {
        Editor::Claude => {
            println!(
                "  3. Use {} or {} slash commands",
                "/decision".cyan(),
                "/recover".cyan()
            );
        }
        Editor::Windsurf => {
            println!("  3. Rules created in {}", ".windsurf/rules/".cyan());
            println!("     - {} (set to Always On)", "deciduous.md".cyan());
            println!("     - {} (model-triggered)", "recover.md".cyan());
            println!("     - {} (auto-retrieved)", ".windsurf/memories.md".cyan());
            println!();
            println!(
                "{}",
                "  ⚠️  IMPORTANT: Verify rule activation in Windsurf:"
                    .yellow()
                    .bold()
            );
            println!("     Open Windsurf → Cascade → Customizations (gear icon)");
            println!(
                "     Ensure {} is set to {}",
                "deciduous.md".cyan(),
                "\"Always On\"".green()
            );
        }
        Editor::Opencode => {
            println!("  3. Commands created in {}", ".opencode/command/".cyan());
            println!("     - {} (decision tracking)", "/decision".cyan());
            println!("     - {} (context recovery)", "/recover".cyan());
            println!("     - {} (build & test)", "/build-test".cyan());
            println!("     - {} (graph viewer)", "/serve-ui".cyan());
            println!("     - {} (export graph)", "/sync-graph".cyan());
            println!("  4. Instructions added to {}", "AGENTS.md".cyan());
        }
        Editor::Codex => {
            println!("  3. Prompts created in {}", ".codex/prompts/".cyan());
            println!("     - {} (decision tracking)", "/prompts:decision".cyan());
            println!("     - {} (context recovery)", "/prompts:recover".cyan());
            println!("     - {} (build & test)", "/prompts:build-test".cyan());
            println!("     - {} (graph viewer)", "/prompts:serve-ui".cyan());
            println!("     - {} (export graph)", "/prompts:sync-graph".cyan());
            println!("  4. Instructions added to {}", "AGENTS.md".cyan());
            println!();
            println!(
                "{}",
                "  Note: Set CODEX_HOME to use project-local prompts:"
                    .yellow()
                    .bold()
            );
            println!("     {}", "export CODEX_HOME=.codex".cyan());
        }
    }

    println!();
    println!(
        "  4. Commit and push: {}",
        "git add docs/ .github/ && git push".cyan()
    );
    println!("  5. Enable GitHub Pages (Settings → Pages → Source: Deploy from branch, gh-pages)");
    println!();
    println!(
        "Your graph will be live at: {}",
        "https://<user>.github.io/<repo>/".cyan()
    );
    println!();

    Ok(())
}

fn create_dir_if_missing(path: &Path) -> Result<(), String> {
    if !path.exists() {
        fs::create_dir_all(path)
            .map_err(|e| format!("Could not create {}: {}", path.display(), e))?;
        println!("   {} {}", "Creating".green(), path.display());
    }
    Ok(())
}

fn write_file_if_missing(path: &Path, content: &str, display_name: &str) -> Result<(), String> {
    if path.exists() {
        println!(
            "   {} {} (already exists)",
            "Skipping".yellow(),
            display_name
        );
    } else {
        fs::write(path, content).map_err(|e| format!("Could not write {}: {}", display_name, e))?;
        println!("   {} {}", "Creating".green(), display_name);
    }
    Ok(())
}

fn write_file_overwrite(path: &Path, content: &str, display_name: &str) -> Result<(), String> {
    fs::write(path, content).map_err(|e| format!("Could not write {}: {}", display_name, e))?;
    println!("   {} {}", "Updated".green(), display_name);
    Ok(())
}

fn replace_config_md_section(
    path: &Path,
    section_content: &str,
    file_name: &str,
) -> Result<(), String> {
    // Look for either variant of our section header
    let markers = [
        "## Decision Graph Workflow",
        "## ⚠️ MANDATORY: Decision Graph Workflow",
    ];
    // Our section ends when we hit another ## heading or end of file
    let section_end_pattern = "\n## ";

    if path.exists() {
        let existing =
            fs::read_to_string(path).map_err(|e| format!("Could not read {}: {}", file_name, e))?;

        // Find the start of our section (try each marker)
        let start_idx = markers.iter().filter_map(|m| existing.find(m)).min();

        if let Some(start) = start_idx {
            // Find the end of our section (next ## heading after our section starts)
            // Need to skip past the marker properly - find the newline after it
            let after_marker = existing[start..]
                .find('\n')
                .map(|i| start + i)
                .unwrap_or(start + 10);
            let end_idx = existing[after_marker..]
                .find(section_end_pattern)
                .map(|i| after_marker + i + 1) // +1 to keep the newline before next section
                .unwrap_or(existing.len()); // If no next section, replace to end

            // Rebuild the file: before our section + new section + after our section
            let before = &existing[..start];
            let after = &existing[end_idx..];

            let new_content = if after.is_empty() {
                format!("{}{}", before, section_content.trim_start())
            } else {
                format!(
                    "{}{}\n{}",
                    before,
                    section_content.trim(),
                    after.trim_start()
                )
            };

            fs::write(path, new_content)
                .map_err(|e| format!("Could not write {}: {}", file_name, e))?;
            println!("   {} {} (section replaced)", "Updated".green(), file_name);
        } else {
            // No existing section, append
            let mut file = fs::OpenOptions::new()
                .append(true)
                .open(path)
                .map_err(|e| format!("Could not open {} for append: {}", file_name, e))?;
            use std::io::Write;
            writeln!(file, "\n{}", section_content.trim())
                .map_err(|e| format!("Could not append to {}: {}", file_name, e))?;
            println!("   {} {} (section added)", "Updated".green(), file_name);
        }
    } else {
        // File doesn't exist, create it
        fs::write(path, section_content.trim())
            .map_err(|e| format!("Could not create {}: {}", file_name, e))?;
        println!("   {} {}", "Creating".green(), file_name);
    }
    Ok(())
}

/// Update tooling files to the latest version (overwrites existing)
pub fn update_tooling(editor: Editor) -> Result<(), String> {
    let cwd =
        std::env::current_dir().map_err(|e| format!("Could not get current directory: {}", e))?;

    let editor_name = match editor {
        Editor::Claude => "Claude Code",
        Editor::Windsurf => "Windsurf",
        Editor::Opencode => "OpenCode",
        Editor::Codex => "Codex",
    };

    println!(
        "\n{}",
        format!("Updating Deciduous tooling for {}...", editor_name)
            .cyan()
            .bold()
    );
    println!("   Directory: {}\n", cwd.display());

    // Update config.toml (only if .deciduous exists)
    let deciduous_dir = cwd.join(".deciduous");
    if deciduous_dir.exists() {
        let config_path = deciduous_dir.join("config.toml");
        write_file_overwrite(&config_path, DEFAULT_CONFIG, ".deciduous/config.toml")?;
    } else {
        println!(
            "   {} .deciduous/ not found - run 'deciduous init' first",
            "Warning:".yellow()
        );
    }

    match editor {
        Editor::Claude => {
            // Create .claude/commands directory if needed
            let claude_dir = cwd.join(".claude").join("commands");
            create_dir_if_missing(&claude_dir)?;

            // Overwrite deciduous.decision.md slash command
            let decision_path = claude_dir.join("deciduous.decision.md");
            write_file_overwrite(
                &decision_path,
                DECISION_MD,
                ".claude/commands/deciduous.decision.md",
            )?;

            // Overwrite deciduous.recover.md slash command
            let recover_path = claude_dir.join("deciduous.recover.md");
            write_file_overwrite(
                &recover_path,
                RECOVER_MD,
                ".claude/commands/deciduous.recover.md",
            )?;

            // Update CLAUDE.md section
            let claude_md_path = cwd.join("CLAUDE.md");
            replace_config_md_section(&claude_md_path, CLAUDE_MD_SECTION, "CLAUDE.md")?;

            // Create/update deciduous skill
            let skills_dir = cwd.join(".claude").join("skills").join("deciduous");
            create_dir_if_missing(&skills_dir)?;
            let skill_path = skills_dir.join("SKILL.md");
            write_file_overwrite(
                &skill_path,
                DECIDUOUS_SKILL,
                ".claude/skills/deciduous/SKILL.md",
            )?;
        }
        Editor::Windsurf => {
            // Create .windsurf directories if needed
            let windsurf_base = cwd.join(".windsurf");
            create_dir_if_missing(&windsurf_base)?;
            let windsurf_rules = windsurf_base.join("rules");
            create_dir_if_missing(&windsurf_rules)?;

            // Overwrite deciduous.md rule
            let deciduous_rule_path = windsurf_rules.join("deciduous.md");
            write_file_overwrite(
                &deciduous_rule_path,
                WINDSURF_DECIDUOUS_RULE,
                ".windsurf/rules/deciduous.md",
            )?;

            // Overwrite context.md rule
            let context_path = windsurf_rules.join("context.md");
            write_file_overwrite(
                &context_path,
                WINDSURF_RECOVER_RULE,
                ".windsurf/rules/recover.md",
            )?;

            // Overwrite memories.md
            let memories_path = windsurf_base.join("memories.md");
            write_file_overwrite(&memories_path, WINDSURF_MEMORIES, ".windsurf/memories.md")?;

            // Update AGENTS.md section
            let agents_md_path = cwd.join("AGENTS.md");
            replace_config_md_section(&agents_md_path, AGENTS_MD_SECTION, "AGENTS.md")?;
        }
        Editor::Opencode => {
            // Create .opencode/command directory if needed
            let opencode_cmd_dir = cwd.join(".opencode").join("command");
            create_dir_if_missing(&opencode_cmd_dir)?;

            // Overwrite decision.md command
            let decision_path = opencode_cmd_dir.join("decision.md");
            write_file_overwrite(
                &decision_path,
                OPENCODE_DECISION_CMD,
                ".opencode/command/decision.md",
            )?;

            // Overwrite context.md command
            let context_path = opencode_cmd_dir.join("context.md");
            write_file_overwrite(
                &context_path,
                OPENCODE_RECOVER_CMD,
                ".opencode/command/recover.md",
            )?;

            // Overwrite build-test.md command
            let build_test_path = opencode_cmd_dir.join("build-test.md");
            write_file_overwrite(
                &build_test_path,
                OPENCODE_BUILD_TEST_CMD,
                ".opencode/command/build-test.md",
            )?;

            // Overwrite serve-ui.md command
            let serve_ui_path = opencode_cmd_dir.join("serve-ui.md");
            write_file_overwrite(
                &serve_ui_path,
                OPENCODE_SERVE_UI_CMD,
                ".opencode/command/serve-ui.md",
            )?;

            // Overwrite sync-graph.md command
            let sync_graph_path = opencode_cmd_dir.join("sync-graph.md");
            write_file_overwrite(
                &sync_graph_path,
                OPENCODE_SYNC_GRAPH_CMD,
                ".opencode/command/sync-graph.md",
            )?;

            // Update AGENTS.md section
            let agents_md_path = cwd.join("AGENTS.md");
            replace_config_md_section(&agents_md_path, AGENTS_MD_SECTION, "AGENTS.md")?;
        }
        Editor::Codex => {
            // Create .codex/prompts directory if needed
            let codex_prompts_dir = cwd.join(".codex").join("prompts");
            create_dir_if_missing(&codex_prompts_dir)?;

            // Overwrite deciduous.decision.md prompt
            let decision_path = codex_prompts_dir.join("deciduous.decision.md");
            write_file_overwrite(
                &decision_path,
                CODEX_DECISION_PROMPT,
                ".codex/prompts/deciduous.decision.md",
            )?;

            // Overwrite deciduous.recover.md prompt
            let recover_path = codex_prompts_dir.join("deciduous.recover.md");
            write_file_overwrite(
                &recover_path,
                CODEX_RECOVER_PROMPT,
                ".codex/prompts/deciduous.recover.md",
            )?;

            // Overwrite deciduous.build-test.md prompt
            let build_test_path = codex_prompts_dir.join("deciduous.build-test.md");
            write_file_overwrite(
                &build_test_path,
                CODEX_BUILD_TEST_PROMPT,
                ".codex/prompts/deciduous.build-test.md",
            )?;

            // Overwrite deciduous.serve-ui.md prompt
            let serve_ui_path = codex_prompts_dir.join("deciduous.serve-ui.md");
            write_file_overwrite(
                &serve_ui_path,
                CODEX_SERVE_UI_PROMPT,
                ".codex/prompts/deciduous.serve-ui.md",
            )?;

            // Overwrite deciduous.sync-graph.md prompt
            let sync_graph_path = codex_prompts_dir.join("deciduous.sync-graph.md");
            write_file_overwrite(
                &sync_graph_path,
                CODEX_SYNC_GRAPH_PROMPT,
                ".codex/prompts/deciduous.sync-graph.md",
            )?;

            // Update AGENTS.md section
            let agents_md_path = cwd.join("AGENTS.md");
            replace_config_md_section(&agents_md_path, AGENTS_MD_SECTION, "AGENTS.md")?;
        }
    }

    println!(
        "\n{}",
        format!("Tooling updated for {}!", editor_name)
            .green()
            .bold()
    );
    println!("\nUpdated files contain the latest:");
    println!("  - Branch-based grouping with config.toml");
    println!("  - Graph integrity auditing workflows");
    println!("  - Improved error messages and documentation");
    println!();

    Ok(())
}

/// Append the Decision Graph Workflow section to a config file (CLAUDE.md or AGENTS.md)
fn append_config_md(path: &Path, section_content: &str, file_name: &str) -> Result<(), String> {
    let marker = "## Decision Graph Workflow";

    if path.exists() {
        let existing =
            fs::read_to_string(path).map_err(|e| format!("Could not read {}: {}", file_name, e))?;

        if existing.contains(marker) {
            println!(
                "   {} {} (workflow section already present)",
                "Skipping".yellow(),
                file_name
            );
            return Ok(());
        }

        // Append the section
        let new_content = format!("{}\n{}", existing.trim_end(), section_content);
        fs::write(path, new_content)
            .map_err(|e| format!("Could not update {}: {}", file_name, e))?;
        println!(
            "   {} {} (added workflow section)",
            "Updated".green(),
            file_name
        );
    } else {
        // Create new file
        let content = format!("# Project Instructions\n{}", section_content);
        fs::write(path, content).map_err(|e| format!("Could not create {}: {}", file_name, e))?;
        println!("   {} {}", "Creating".green(), file_name);
    }

    Ok(())
}

fn add_to_gitignore(cwd: &Path) -> Result<(), String> {
    let gitignore_path = cwd.join(".gitignore");
    let entry = ".deciduous/";

    if gitignore_path.exists() {
        let existing = fs::read_to_string(&gitignore_path)
            .map_err(|e| format!("Could not read .gitignore: {}", e))?;

        if existing
            .lines()
            .any(|line| line.trim() == entry || line.trim() == ".deciduous")
        {
            // Already in gitignore
            return Ok(());
        }

        // Append
        let new_content = format!(
            "{}\n\n# Deciduous database (local)\n{}\n",
            existing.trim_end(),
            entry
        );
        fs::write(&gitignore_path, new_content)
            .map_err(|e| format!("Could not update .gitignore: {}", e))?;
        println!("   {} .gitignore (added .deciduous/)", "Updated".green());
    } else {
        // Create new .gitignore
        let content = format!("# Deciduous database (local)\n{}\n", entry);
        fs::write(&gitignore_path, content)
            .map_err(|e| format!("Could not create .gitignore: {}", e))?;
        println!("   {} .gitignore", "Creating".green());
    }

    Ok(())
}

/// Add Codex-specific entries to .gitignore (selective, not entire directory)
/// Uses negation pattern to allow prompts/ to be committed while ignoring other files
fn add_codex_to_gitignore(cwd: &Path) -> Result<(), String> {
    let gitignore_path = cwd.join(".gitignore");

    // The pattern we want to add:
    // .codex/*           - Ignore everything in .codex
    // !.codex/prompts/   - Except prompts directory
    // !.codex/prompts/** - And its contents
    let codex_entries = [".codex/*", "!.codex/prompts/", "!.codex/prompts/**"];

    let existing = if gitignore_path.exists() {
        fs::read_to_string(&gitignore_path)
            .map_err(|e| format!("Could not read .gitignore: {}", e))?
    } else {
        String::new()
    };

    // Check if codex entries are already present
    if existing.lines().any(|line| line.trim() == ".codex/*") {
        // Already configured
        return Ok(());
    }

    // Build the new content to append
    let mut new_section = String::new();
    if !existing.trim().is_empty() {
        new_section.push('\n');
    }
    new_section.push_str("\n# Codex files (prompts/ should be committed)\n");
    for entry in &codex_entries {
        new_section.push_str(entry);
        new_section.push('\n');
    }

    let new_content = format!("{}{}", existing.trim_end(), new_section);
    fs::write(&gitignore_path, new_content)
        .map_err(|e| format!("Could not update .gitignore: {}", e))?;

    println!("   {} .gitignore (added Codex entries)", "Updated".green());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // === Editor Enum Tests ===

    #[test]
    fn test_editor_equality() {
        assert_eq!(Editor::Claude, Editor::Claude);
        assert_eq!(Editor::Windsurf, Editor::Windsurf);
        assert_eq!(Editor::Opencode, Editor::Opencode);
        assert_ne!(Editor::Claude, Editor::Windsurf);
        assert_ne!(Editor::Claude, Editor::Opencode);
        assert_ne!(Editor::Windsurf, Editor::Opencode);
    }

    #[test]
    fn test_editor_debug() {
        assert_eq!(format!("{:?}", Editor::Claude), "Claude");
        assert_eq!(format!("{:?}", Editor::Windsurf), "Windsurf");
        assert_eq!(format!("{:?}", Editor::Opencode), "Opencode");
    }

    // === Template Content Tests ===

    #[test]
    fn test_default_config_is_valid_toml() {
        // Ensure the DEFAULT_CONFIG constant is valid TOML
        let result: Result<toml::Value, _> = toml::from_str(DEFAULT_CONFIG);
        assert!(result.is_ok(), "DEFAULT_CONFIG should be valid TOML");
    }

    #[test]
    fn test_decision_md_has_required_frontmatter() {
        assert!(
            DECISION_MD.starts_with("---"),
            "decision.md should start with frontmatter"
        );
        assert!(
            DECISION_MD.contains("description:"),
            "decision.md should have description"
        );
        assert!(
            DECISION_MD.contains("allowed-tools:"),
            "decision.md should have allowed-tools"
        );
    }

    #[test]
    fn test_recover_md_has_required_frontmatter() {
        assert!(
            RECOVER_MD.starts_with("---"),
            "recover.md should start with frontmatter"
        );
        assert!(
            RECOVER_MD.contains("description:"),
            "recover.md should have description"
        );
    }

    #[test]
    fn test_claude_md_section_contains_workflow() {
        assert!(CLAUDE_MD_SECTION.contains("Decision Graph Workflow"));
        assert!(CLAUDE_MD_SECTION.contains("deciduous add"));
        assert!(CLAUDE_MD_SECTION.contains("deciduous link"));
    }

    #[test]
    fn test_windsurf_rule_has_required_frontmatter() {
        assert!(
            WINDSURF_DECIDUOUS_RULE.starts_with("---"),
            "windsurf rule should start with frontmatter"
        );
        assert!(
            WINDSURF_DECIDUOUS_RULE.contains("alwaysApply:"),
            "windsurf rule should have alwaysApply"
        );
    }

    // === OpenCode Template Tests ===

    #[test]
    fn test_opencode_decision_cmd_has_required_frontmatter() {
        assert!(
            OPENCODE_DECISION_CMD.starts_with("---"),
            "opencode decision cmd should start with frontmatter"
        );
        assert!(
            OPENCODE_DECISION_CMD.contains("description:"),
            "opencode decision cmd should have description"
        );
    }

    #[test]
    fn test_opencode_recover_cmd_has_required_frontmatter() {
        assert!(
            OPENCODE_RECOVER_CMD.starts_with("---"),
            "opencode recover cmd should start with frontmatter"
        );
        assert!(
            OPENCODE_RECOVER_CMD.contains("description:"),
            "opencode recover cmd should have description"
        );
    }

    #[test]
    fn test_opencode_decision_cmd_contains_workflow() {
        assert!(OPENCODE_DECISION_CMD.contains("Decision Graph Management"));
        assert!(OPENCODE_DECISION_CMD.contains("deciduous add"));
        assert!(OPENCODE_DECISION_CMD.contains("deciduous link"));
        assert!(OPENCODE_DECISION_CMD.contains("$ARGUMENTS"));
    }

    #[test]
    fn test_opencode_recover_cmd_contains_recovery() {
        assert!(OPENCODE_RECOVER_CMD.contains("Context Recovery"));
        assert!(OPENCODE_RECOVER_CMD.contains("deciduous nodes"));
        assert!(OPENCODE_RECOVER_CMD.contains("deciduous edges"));
        assert!(OPENCODE_RECOVER_CMD.contains("$ARGUMENTS"));
    }

    #[test]
    fn test_opencode_build_test_cmd_has_frontmatter() {
        assert!(OPENCODE_BUILD_TEST_CMD.starts_with("---"));
        assert!(OPENCODE_BUILD_TEST_CMD.contains("description:"));
        assert!(OPENCODE_BUILD_TEST_CMD.contains("Build and Test"));
    }

    #[test]
    fn test_opencode_serve_ui_cmd_has_frontmatter() {
        assert!(OPENCODE_SERVE_UI_CMD.starts_with("---"));
        assert!(OPENCODE_SERVE_UI_CMD.contains("description:"));
        assert!(OPENCODE_SERVE_UI_CMD.contains("deciduous serve"));
    }

    #[test]
    fn test_opencode_sync_graph_cmd_has_frontmatter() {
        assert!(OPENCODE_SYNC_GRAPH_CMD.starts_with("---"));
        assert!(OPENCODE_SYNC_GRAPH_CMD.contains("description:"));
        assert!(OPENCODE_SYNC_GRAPH_CMD.contains("deciduous sync"));
    }

    // === File Helper Tests (with tempdir) ===

    #[test]
    fn test_create_dir_if_missing_creates_new() {
        let temp = TempDir::new().unwrap();
        let new_dir = temp.path().join("new_dir");

        assert!(!new_dir.exists());
        create_dir_if_missing(&new_dir).unwrap();
        assert!(new_dir.exists());
    }

    #[test]
    fn test_create_dir_if_missing_skips_existing() {
        let temp = TempDir::new().unwrap();
        // temp.path() already exists
        let result = create_dir_if_missing(temp.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_write_file_if_missing_creates_new() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.txt");

        assert!(!file_path.exists());
        write_file_if_missing(&file_path, "hello world", "test.txt").unwrap();
        assert!(file_path.exists());
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "hello world");
    }

    #[test]
    fn test_write_file_if_missing_preserves_existing() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.txt");

        // Create file with original content
        fs::write(&file_path, "original").unwrap();

        // Try to write new content - should be skipped
        write_file_if_missing(&file_path, "new content", "test.txt").unwrap();

        // Should still have original content
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "original");
    }

    #[test]
    fn test_write_file_overwrite_replaces_existing() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.txt");

        // Create file with original content
        fs::write(&file_path, "original").unwrap();

        // Overwrite with new content
        write_file_overwrite(&file_path, "new content", "test.txt").unwrap();

        // Should have new content
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "new content");
    }

    #[test]
    fn test_add_to_gitignore_creates_new() {
        let temp = TempDir::new().unwrap();
        let gitignore_path = temp.path().join(".gitignore");

        add_to_gitignore(temp.path()).unwrap();

        assert!(gitignore_path.exists());
        let content = fs::read_to_string(&gitignore_path).unwrap();
        assert!(content.contains(".deciduous/"));
    }

    #[test]
    fn test_add_to_gitignore_appends_to_existing() {
        let temp = TempDir::new().unwrap();
        let gitignore_path = temp.path().join(".gitignore");

        // Create existing gitignore
        fs::write(&gitignore_path, "node_modules/\n*.log").unwrap();

        add_to_gitignore(temp.path()).unwrap();

        let content = fs::read_to_string(&gitignore_path).unwrap();
        assert!(content.contains("node_modules/"));
        assert!(content.contains(".deciduous/"));
    }

    #[test]
    fn test_add_to_gitignore_skips_if_present() {
        let temp = TempDir::new().unwrap();
        let gitignore_path = temp.path().join(".gitignore");

        // Create gitignore with .deciduous/ already present
        let original = "node_modules/\n.deciduous/\n*.log";
        fs::write(&gitignore_path, original).unwrap();

        add_to_gitignore(temp.path()).unwrap();

        // Content should be unchanged
        let content = fs::read_to_string(&gitignore_path).unwrap();
        assert_eq!(content, original);
    }

    #[test]
    fn test_append_config_md_creates_new() {
        let temp = TempDir::new().unwrap();
        let md_path = temp.path().join("CLAUDE.md");

        append_config_md(&md_path, CLAUDE_MD_SECTION, "CLAUDE.md").unwrap();

        assert!(md_path.exists());
        let content = fs::read_to_string(&md_path).unwrap();
        assert!(content.contains("Decision Graph Workflow"));
    }

    #[test]
    fn test_append_config_md_appends_to_existing() {
        let temp = TempDir::new().unwrap();
        let md_path = temp.path().join("CLAUDE.md");

        // Create existing file
        fs::write(&md_path, "# My Project\n\nSome instructions.").unwrap();

        append_config_md(&md_path, CLAUDE_MD_SECTION, "CLAUDE.md").unwrap();

        let content = fs::read_to_string(&md_path).unwrap();
        assert!(content.contains("# My Project"));
        assert!(content.contains("Decision Graph Workflow"));
    }

    #[test]
    fn test_append_config_md_skips_if_present() {
        let temp = TempDir::new().unwrap();
        let md_path = temp.path().join("CLAUDE.md");

        // Create file with workflow section already
        let original = "# My Project\n\n## Decision Graph Workflow\n\nAlready here.";
        fs::write(&md_path, original).unwrap();

        append_config_md(&md_path, CLAUDE_MD_SECTION, "CLAUDE.md").unwrap();

        // Should be unchanged
        let content = fs::read_to_string(&md_path).unwrap();
        assert_eq!(content, original);
    }

    #[test]
    fn test_replace_config_md_section_replaces_existing() {
        let temp = TempDir::new().unwrap();
        let md_path = temp.path().join("CLAUDE.md");

        // Create file with old workflow section
        let original = r#"# My Project

## Decision Graph Workflow

Old workflow content here.
More old content.

## Other Section

This should be preserved."#;
        fs::write(&md_path, original).unwrap();

        let new_section = "\n## Decision Graph Workflow\n\nNew workflow content!\n";
        replace_config_md_section(&md_path, new_section, "CLAUDE.md").unwrap();

        let content = fs::read_to_string(&md_path).unwrap();
        assert!(content.contains("New workflow content!"));
        assert!(!content.contains("Old workflow content"));
        assert!(content.contains("## Other Section"));
        assert!(content.contains("This should be preserved"));
    }

    // === Workflow YAML Tests ===

    #[test]
    fn test_cleanup_workflow_is_valid_yaml() {
        let result: Result<serde_yaml::Value, _> = serde_yaml::from_str(CLEANUP_WORKFLOW);
        assert!(result.is_ok(), "CLEANUP_WORKFLOW should be valid YAML");
    }

    #[test]
    fn test_deploy_workflow_is_valid_yaml() {
        let result: Result<serde_yaml::Value, _> = serde_yaml::from_str(DEPLOY_PAGES_WORKFLOW);
        assert!(result.is_ok(), "DEPLOY_PAGES_WORKFLOW should be valid YAML");
    }

    // === Codex Template Tests ===

    #[test]
    fn test_editor_codex_equality() {
        assert_eq!(Editor::Codex, Editor::Codex);
        assert_ne!(Editor::Claude, Editor::Codex);
        assert_ne!(Editor::Windsurf, Editor::Codex);
        assert_ne!(Editor::Opencode, Editor::Codex);
    }

    #[test]
    fn test_editor_codex_debug() {
        assert_eq!(format!("{:?}", Editor::Codex), "Codex");
    }

    #[test]
    fn test_codex_decision_prompt_has_required_frontmatter() {
        assert!(
            CODEX_DECISION_PROMPT.starts_with("---"),
            "codex decision prompt should start with frontmatter"
        );
        assert!(
            CODEX_DECISION_PROMPT.contains("description:"),
            "codex decision prompt should have description"
        );
        assert!(
            CODEX_DECISION_PROMPT.contains("argument-hint:"),
            "codex decision prompt should have argument-hint"
        );
    }

    #[test]
    fn test_codex_recover_prompt_has_required_frontmatter() {
        assert!(
            CODEX_RECOVER_PROMPT.starts_with("---"),
            "codex recover prompt should start with frontmatter"
        );
        assert!(
            CODEX_RECOVER_PROMPT.contains("description:"),
            "codex recover prompt should have description"
        );
    }

    #[test]
    fn test_codex_decision_prompt_contains_workflow() {
        assert!(CODEX_DECISION_PROMPT.contains("Decision Graph Management"));
        assert!(CODEX_DECISION_PROMPT.contains("deciduous add"));
        assert!(CODEX_DECISION_PROMPT.contains("deciduous link"));
        assert!(CODEX_DECISION_PROMPT.contains("$ARGUMENTS"));
    }

    #[test]
    fn test_codex_recover_prompt_contains_recovery() {
        assert!(CODEX_RECOVER_PROMPT.contains("Context Recovery"));
        assert!(CODEX_RECOVER_PROMPT.contains("deciduous nodes"));
        assert!(CODEX_RECOVER_PROMPT.contains("deciduous edges"));
    }

    #[test]
    fn test_codex_build_test_prompt_has_frontmatter() {
        assert!(CODEX_BUILD_TEST_PROMPT.starts_with("---"));
        assert!(CODEX_BUILD_TEST_PROMPT.contains("description:"));
        assert!(CODEX_BUILD_TEST_PROMPT.contains("Build and Test"));
    }

    #[test]
    fn test_codex_serve_ui_prompt_has_frontmatter() {
        assert!(CODEX_SERVE_UI_PROMPT.starts_with("---"));
        assert!(CODEX_SERVE_UI_PROMPT.contains("description:"));
        assert!(CODEX_SERVE_UI_PROMPT.contains("deciduous serve"));
    }

    #[test]
    fn test_codex_sync_graph_prompt_has_frontmatter() {
        assert!(CODEX_SYNC_GRAPH_PROMPT.starts_with("---"));
        assert!(CODEX_SYNC_GRAPH_PROMPT.contains("description:"));
        assert!(CODEX_SYNC_GRAPH_PROMPT.contains("deciduous sync"));
    }

    // === Codex Gitignore Tests ===

    #[test]
    fn test_add_codex_to_gitignore_creates_new() {
        let temp = TempDir::new().unwrap();

        add_codex_to_gitignore(temp.path()).unwrap();

        let gitignore_path = temp.path().join(".gitignore");
        assert!(gitignore_path.exists());
        let content = fs::read_to_string(&gitignore_path).unwrap();

        // Check all Codex entries are present
        assert!(content.contains(".codex/*"));
        assert!(content.contains("!.codex/prompts/"));
        assert!(content.contains("!.codex/prompts/**"));
    }

    #[test]
    fn test_add_codex_to_gitignore_preserves_existing() {
        let temp = TempDir::new().unwrap();
        let gitignore_path = temp.path().join(".gitignore");

        // Create existing gitignore
        fs::write(&gitignore_path, "node_modules/\n*.log").unwrap();

        add_codex_to_gitignore(temp.path()).unwrap();

        let content = fs::read_to_string(&gitignore_path).unwrap();
        assert!(content.contains("node_modules/"));
        assert!(content.contains(".codex/*"));
    }

    #[test]
    fn test_add_codex_to_gitignore_idempotent() {
        let temp = TempDir::new().unwrap();

        // Run twice
        add_codex_to_gitignore(temp.path()).unwrap();
        add_codex_to_gitignore(temp.path()).unwrap();

        let gitignore_path = temp.path().join(".gitignore");
        let content = fs::read_to_string(&gitignore_path).unwrap();

        // Should only appear once
        let count = content.matches(".codex/*").count();
        assert_eq!(count, 1, "Entry should only appear once");
    }
}
