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
- `-f, --files "file1.rs,file2.rs"` - Associate files with this node
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

Prompts are viewable in the TUI detail panel (`deciduous tui`) and flow through the graph via connections.

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

const CONTEXT_MD: &str = r#"---
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
Run /context → See past decisions
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

### CRITICAL: Capture User Prompts When Semantically Meaningful

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

Prompts are viewable in the TUI detail panel (`deciduous tui`) and flow through the graph via connections.

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

## CRITICAL: Capture User Prompts When Semantically Meaningful

<prompt_capture>
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

Prompts are viewable in the TUI detail panel (`deciduous tui`) and flow through the graph via connections.
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
deciduous add action "Implemented auth" -c 90 --commit HEAD   # Auto-detects current commit
deciduous link <goal_id> <action_id> -r "Implementation"

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

/// Windsurf context rule - placed in .windsurf/rules/context.md
/// Model-triggered rule for session recovery
const WINDSURF_CONTEXT_RULE: &str = r#"---
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

### CRITICAL: Capture User Prompts When Semantically Meaningful

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

Prompts are viewable in the TUI detail panel (`deciduous tui`) and flow through the graph via connections.

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

# Branch filtering
deciduous nodes --branch main
deciduous nodes -b feature-auth
```

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
"#;

/// Initialize deciduous in the current directory
pub fn init_project(editor: Editor) -> Result<(), String> {
    let cwd = std::env::current_dir()
        .map_err(|e| format!("Could not get current directory: {}", e))?;

    let editor_name = match editor {
        Editor::Claude => "Claude Code",
        Editor::Windsurf => "Windsurf",
    };

    println!("\n{}", format!("Initializing Deciduous for {}...", editor_name).cyan().bold());
    println!("   Directory: {}\n", cwd.display());

    // 1. Create .deciduous directory (shared between all editors)
    let deciduous_dir = cwd.join(".deciduous");
    create_dir_if_missing(&deciduous_dir)?;

    // 1b. Create default config.toml if it doesn't exist
    let config_path = deciduous_dir.join("config.toml");
    write_file_if_missing(&config_path, DEFAULT_CONFIG, ".deciduous/config.toml")?;

    // 2. Initialize database by opening it (creates tables)
    let db_path = deciduous_dir.join("deciduous.db");
    if db_path.exists() {
        println!("   {} .deciduous/deciduous.db (already exists, preserving data)", "Skipping".yellow());
    } else {
        println!("   {} {}", "Creating".green(), ".deciduous/deciduous.db");
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

            // Write decision.md slash command
            let decision_path = claude_dir.join("decision.md");
            write_file_if_missing(&decision_path, DECISION_MD, ".claude/commands/decision.md")?;

            // Write context.md slash command
            let context_path = claude_dir.join("context.md");
            write_file_if_missing(&context_path, CONTEXT_MD, ".claude/commands/context.md")?;

            // Append to or create CLAUDE.md
            let claude_md_path = cwd.join("CLAUDE.md");
            append_config_md(&claude_md_path, CLAUDE_MD_SECTION, "CLAUDE.md")?;
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
            write_file_if_missing(&deciduous_rule_path, WINDSURF_DECIDUOUS_RULE, ".windsurf/rules/deciduous.md")?;

            // Write context.md rule (Model-triggered - for session recovery)
            let context_path = windsurf_rules.join("context.md");
            write_file_if_missing(&context_path, WINDSURF_CONTEXT_RULE, ".windsurf/rules/context.md")?;

            // Write memories.md (project-level memories Cascade auto-retrieves)
            let memories_path = windsurf_base.join("memories.md");
            write_file_if_missing(&memories_path, WINDSURF_MEMORIES, ".windsurf/memories.md")?;

            // Append to or create AGENTS.md
            let agents_md_path = cwd.join("AGENTS.md");
            append_config_md(&agents_md_path, AGENTS_MD_SECTION, "AGENTS.md")?;
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
        write_file_if_missing(&cleanup_path, CLEANUP_WORKFLOW, ".github/workflows/cleanup-decision-graphs.yml")?;

        // Deploy workflow for GitHub Pages
        let deploy_path = workflows_dir.join("deploy-pages.yml");
        write_file_if_missing(&deploy_path, DEPLOY_PAGES_WORKFLOW, ".github/workflows/deploy-pages.yml")?;
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
        fs::write(&nojekyll_path, "")
            .map_err(|e| format!("Could not write .nojekyll: {}", e))?;
        println!("   {} docs/.nojekyll", "Creating".green());
    }

    println!("\n{}", format!("Deciduous initialized for {}!", editor_name).green().bold());
    println!("\nNext steps:");
    println!("  1. Run {} to start the local graph viewer", "deciduous serve".cyan());
    println!("  2. Run {} to export graph for GitHub Pages", "deciduous sync".cyan());

    match editor {
        Editor::Claude => {
            println!("  3. Use {} or {} slash commands", "/decision".cyan(), "/context".cyan());
        }
        Editor::Windsurf => {
            println!("  3. Rules created in {}", ".windsurf/rules/".cyan());
            println!("     - {} (set to Always On)", "deciduous.md".cyan());
            println!("     - {} (model-triggered)", "context.md".cyan());
            println!("     - {} (auto-retrieved)", ".windsurf/memories.md".cyan());
            println!();
            println!("{}", "  ⚠️  IMPORTANT: Verify rule activation in Windsurf:".yellow().bold());
            println!("     Open Windsurf → Cascade → Customizations (gear icon)");
            println!("     Ensure {} is set to {}", "deciduous.md".cyan(), "\"Always On\"".green());
        }
    }

    println!();
    println!("  4. Commit and push: {}", "git add docs/ .github/ && git push".cyan());
    println!("  5. Enable GitHub Pages (Settings → Pages → Source: Deploy from branch, gh-pages)");
    println!();
    println!("Your graph will be live at: {}", "https://<user>.github.io/<repo>/".cyan());
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
        println!("   {} {} (already exists)", "Skipping".yellow(), display_name);
    } else {
        fs::write(path, content)
            .map_err(|e| format!("Could not write {}: {}", display_name, e))?;
        println!("   {} {}", "Creating".green(), display_name);
    }
    Ok(())
}

fn write_file_overwrite(path: &Path, content: &str, display_name: &str) -> Result<(), String> {
    fs::write(path, content)
        .map_err(|e| format!("Could not write {}: {}", display_name, e))?;
    println!("   {} {}", "Updated".green(), display_name);
    Ok(())
}

fn replace_config_md_section(path: &Path, section_content: &str, file_name: &str) -> Result<(), String> {
    // Look for either variant of our section header
    let markers = [
        "## Decision Graph Workflow",
        "## ⚠️ MANDATORY: Decision Graph Workflow",
    ];
    // Our section ends when we hit another ## heading or end of file
    let section_end_pattern = "\n## ";

    if path.exists() {
        let existing = fs::read_to_string(path)
            .map_err(|e| format!("Could not read {}: {}", file_name, e))?;

        // Find the start of our section (try each marker)
        let start_idx = markers.iter()
            .filter_map(|m| existing.find(m))
            .min();

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
                format!("{}{}\n{}", before, section_content.trim(), after.trim_start())
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
    let cwd = std::env::current_dir()
        .map_err(|e| format!("Could not get current directory: {}", e))?;

    let editor_name = match editor {
        Editor::Claude => "Claude Code",
        Editor::Windsurf => "Windsurf",
    };

    println!("\n{}", format!("Updating Deciduous tooling for {}...", editor_name).cyan().bold());
    println!("   Directory: {}\n", cwd.display());

    // Update config.toml (only if .deciduous exists)
    let deciduous_dir = cwd.join(".deciduous");
    if deciduous_dir.exists() {
        let config_path = deciduous_dir.join("config.toml");
        write_file_overwrite(&config_path, DEFAULT_CONFIG, ".deciduous/config.toml")?;
    } else {
        println!("   {} .deciduous/ not found - run 'deciduous init' first", "Warning:".yellow());
    }

    match editor {
        Editor::Claude => {
            // Create .claude/commands directory if needed
            let claude_dir = cwd.join(".claude").join("commands");
            create_dir_if_missing(&claude_dir)?;

            // Overwrite decision.md slash command
            let decision_path = claude_dir.join("decision.md");
            write_file_overwrite(&decision_path, DECISION_MD, ".claude/commands/decision.md")?;

            // Overwrite context.md slash command
            let context_path = claude_dir.join("context.md");
            write_file_overwrite(&context_path, CONTEXT_MD, ".claude/commands/context.md")?;

            // Update CLAUDE.md section
            let claude_md_path = cwd.join("CLAUDE.md");
            replace_config_md_section(&claude_md_path, CLAUDE_MD_SECTION, "CLAUDE.md")?;
        }
        Editor::Windsurf => {
            // Create .windsurf directories if needed
            let windsurf_base = cwd.join(".windsurf");
            create_dir_if_missing(&windsurf_base)?;
            let windsurf_rules = windsurf_base.join("rules");
            create_dir_if_missing(&windsurf_rules)?;

            // Overwrite deciduous.md rule
            let deciduous_rule_path = windsurf_rules.join("deciduous.md");
            write_file_overwrite(&deciduous_rule_path, WINDSURF_DECIDUOUS_RULE, ".windsurf/rules/deciduous.md")?;

            // Overwrite context.md rule
            let context_path = windsurf_rules.join("context.md");
            write_file_overwrite(&context_path, WINDSURF_CONTEXT_RULE, ".windsurf/rules/context.md")?;

            // Overwrite memories.md
            let memories_path = windsurf_base.join("memories.md");
            write_file_overwrite(&memories_path, WINDSURF_MEMORIES, ".windsurf/memories.md")?;

            // Update AGENTS.md section
            let agents_md_path = cwd.join("AGENTS.md");
            replace_config_md_section(&agents_md_path, AGENTS_MD_SECTION, "AGENTS.md")?;
        }
    }

    println!("\n{}", format!("Tooling updated for {}!", editor_name).green().bold());
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
        let existing = fs::read_to_string(path)
            .map_err(|e| format!("Could not read {}: {}", file_name, e))?;

        if existing.contains(marker) {
            println!("   {} {} (workflow section already present)", "Skipping".yellow(), file_name);
            return Ok(());
        }

        // Append the section
        let new_content = format!("{}\n{}", existing.trim_end(), section_content);
        fs::write(path, new_content)
            .map_err(|e| format!("Could not update {}: {}", file_name, e))?;
        println!("   {} {} (added workflow section)", "Updated".green(), file_name);
    } else {
        // Create new file
        let content = format!("# Project Instructions\n{}", section_content);
        fs::write(path, content)
            .map_err(|e| format!("Could not create {}: {}", file_name, e))?;
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

        if existing.lines().any(|line| line.trim() == entry || line.trim() == ".deciduous") {
            // Already in gitignore
            return Ok(());
        }

        // Append
        let new_content = format!("{}\n\n# Deciduous database (local)\n{}\n", existing.trim_end(), entry);
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
