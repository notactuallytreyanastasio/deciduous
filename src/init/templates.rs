//! Template constants for deciduous init
//!
//! All templates embedded at compile time for project initialization.

/// Static HTML viewer for GitHub Pages (embedded at compile time)
pub const PAGES_VIEWER_HTML: &str = include_str!("../pages_viewer.html");

/// Default configuration file content
pub const DEFAULT_CONFIG: &str = r#"# Deciduous Configuration
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
pub const DEPLOY_PAGES_WORKFLOW: &str = r#"name: Deploy Decision Graph to Pages

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

/// Cleanup workflow for PR graph assets
pub const CLEANUP_WORKFLOW: &str = r#"name: Cleanup Decision Graph PNGs

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

/// Claude Code decision.md slash command template
pub const DECISION_MD: &str = r#"---
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

### Connection Rules
| Node Type | MUST connect to | Example |
|-----------|----------------|---------|
| `outcome` | The action/goal it resolves | "JWT working" -> links FROM "Implementing JWT" |
| `action` | The decision/goal that spawned it | "Implementing JWT" -> links FROM "Add auth" |
| `option` | Its parent decision | "Use JWT" -> links FROM "Choose auth method" |
| `observation` | Related goal/action/decision | "Found existing code" -> links TO relevant node |
| `decision` | Parent goal (if any) | "Choose auth" -> links FROM "Add auth feature" |
| `revisit` | The decision/outcome being reconsidered | "Reconsidering auth" -> links FROM original decision |
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

/// Claude Code recover.md slash command template
pub const RECOVER_MD: &str = r#"---
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

/// CLAUDE.md section to append for decision graph workflow
pub const CLAUDE_MD_SECTION: &str = r#"
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

Prompts are viewable in the web viewer.

### CRITICAL: Maintain Connections

**The graph's value is in its CONNECTIONS, not just nodes.**

| When you create... | IMMEDIATELY link to... |
|-------------------|------------------------|
| `outcome` | The action/goal it resolves |
| `action` | The goal/decision that spawned it |
| `option` | Its parent decision |
| `observation` | Related goal/action |
| `revisit` | The decision/outcome being reconsidered |

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
deciduous check-update    # Update needed? Run 'deciduous update' if yes
deciduous nodes           # What decisions exist?
deciduous edges           # How are they connected? Any gaps?
git status                # Current state
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

PR workflow: Export patch -> commit patch file -> PR -> teammates apply.
"#;

/// Claude Code work.md slash command template (transaction model)
pub const WORK_MD: &str = r#"---
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

## Step 4: After Completion

When the work is done:

```bash
# After committing
deciduous add outcome "What was accomplished" -c 95 --commit HEAD
deciduous link <action_id> <outcome_id> -r "Implementation complete"

# Sync the graph
deciduous sync
```

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

# Always sync
deciduous sync
```

**Now create the goal node for: $ARGUMENTS**
"#;

/// PreToolUse hook script - blocks Edit/Write without recent action node
pub const HOOK_REQUIRE_ACTION_NODE: &str = r#"#!/bin/bash
# require-action-node.sh
# Blocks Edit/Write tools if no recent action/goal node exists in deciduous
# Exit code 2 = block the tool and show error to Claude

# Check if deciduous is initialized
if [ ! -d ".deciduous" ]; then
    # No deciduous in this project, allow all edits
    exit 0
fi

# Check for any action or goal node created in the last 15 minutes
# We check both because starting new work creates a goal first
recent_node=$(deciduous nodes 2>/dev/null | grep -E '\[(goal|action)\]' | tail -5)

if [ -z "$recent_node" ]; then
    # No nodes at all - this is a fresh project, allow edits
    exit 0
fi

# Check if any node was created recently (within last 15 min)
# Parse the timestamps from nodes output
now=$(date +%s)
fifteen_min_ago=$((now - 900))

# Get the most recent node's timestamp
# deciduous nodes format: ID [type] Title [confidence%] [timestamp]
latest_timestamp=$(deciduous nodes 2>/dev/null | tail -1 | grep -oE '[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}:[0-9]{2}' | tail -1)

if [ -n "$latest_timestamp" ]; then
    # Convert to epoch
    if [[ "$OSTYPE" == "darwin"* ]]; then
        node_epoch=$(date -j -f "%Y-%m-%d %H:%M:%S" "$latest_timestamp" +%s 2>/dev/null || echo "0")
    else
        node_epoch=$(date -d "$latest_timestamp" +%s 2>/dev/null || echo "0")
    fi

    if [ "$node_epoch" -gt "$fifteen_min_ago" ]; then
        # Recent node exists, allow the edit
        exit 0
    fi
fi

# No recent node - block and provide guidance
cat >&2 << 'EOF'
+===================================================================+
|  DECIDUOUS: No recent action/goal node found                      |
+===================================================================+
|  Before editing files, log what you're about to do:               |
|                                                                   |
|  For new work:                                                    |
|    deciduous add goal "What you're trying to achieve" -c 90       |
|                                                                   |
|  For implementation:                                              |
|    deciduous add action "What you're about to implement" -c 85    |
|                                                                   |
|  Then link to parent:                                             |
|    deciduous link <parent_id> <new_id> -r "reason"                |
+===================================================================+
EOF

exit 2
"#;

/// PostToolUse hook script - reminds to link commits after git commit
pub const HOOK_POST_COMMIT_REMINDER: &str = r#"#!/bin/bash
# post-commit-reminder.sh
# Runs after git commit to remind Claude to link the commit to deciduous
# Uses exit code 2 to ensure Claude sees the message and acts on it

# Check if deciduous is initialized
if [ ! -d ".deciduous" ]; then
    exit 0
fi

# Read the input JSON to check if this was a git commit
input=$(cat)
command=$(echo "$input" | grep -o '"command":"[^"]*"' | head -1 | sed 's/"command":"//;s/"$//')

# Only trigger on git commit commands
if ! echo "$command" | grep -qE '^git commit'; then
    exit 0
fi

# Get the commit hash that was just created
commit_hash=$(git rev-parse --short HEAD 2>/dev/null)
commit_msg=$(git log -1 --format=%s 2>/dev/null)

# Output reminder to stderr (exit 2 ensures Claude sees and processes this)
cat >&2 << EOF
+===================================================================+
|  DECIDUOUS: Link this commit to the decision graph!               |
+===================================================================+
|  Commit: $commit_hash "$commit_msg"
|                                                                   |
|  Run NOW:                                                         |
|    deciduous add outcome "What was accomplished" -c 95 --commit HEAD
|    deciduous link <action_id> <outcome_id> -r "Implementation complete"
|                                                                   |
|  Or if this was an action (not outcome):                          |
|    deciduous add action "What was done" -c 90 --commit HEAD       |
+===================================================================+
EOF

# Exit 2 to ensure Claude processes this as important feedback
exit 2
"#;

/// Claude Code settings.json with hooks configuration
pub const CLAUDE_SETTINGS_JSON: &str = r#"{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Edit|Write",
        "hooks": [
          {
            "type": "command",
            "command": "\"$CLAUDE_PROJECT_DIR/.claude/hooks/require-action-node.sh\""
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "\"$CLAUDE_PROJECT_DIR/.claude/hooks/post-commit-reminder.sh\""
          }
        ]
      }
    ]
  }
}
"#;

/// Claude Code agents.toml template for new projects
pub const CLAUDE_AGENTS_TOML: &str = r#"# Project Subagents Configuration
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

/// Pulse skill - Map the current model as decisions
pub const SKILL_PULSE: &str = r#"# Pulse

**Map the current model as decisions. No history, just now.**

---

## What This Is

Pulse captures the current heartbeat of a system - what decisions define how it works TODAY.

Not how it evolved. Not what was tried before. Just: *"What are the design decisions that make this system work the way it does?"*

---

## When to Use

- Understanding an unfamiliar codebase
- Documenting the current architecture
- Before making changes (know what decisions you might affect)
- Explaining a system to someone new
- When you don't care about history, just current state

---

## Process

### 1. Pick a scope

What part of the system are you taking the pulse of?

- A feature ("Suspense fallback behavior")
- A subsystem ("Authentication")
- A boundary ("API request lifecycle")

### 2. Ask: "What decisions define this?"

Read the code. For the thing you're scoping, ask:

> "What design questions had to be answered for this to work?"

Not implementation questions ("which library?") - model questions ("what's the behavior?")

**Examples:**
- "When should the fallback show?"
- "How should nested components interact?"
- "What happens on timeout?"
- "How are errors handled?"

### 3. Create the goal node

```bash
deciduous add goal "<Scope>: <Core question>" -c 90
```

Example:
```bash
deciduous add goal "Determine when and whether to show Suspense fallback" -c 90
```

### 4. Map the decisions

For each design question you identified:

```bash
deciduous add decision "<Design question>" -c <confidence>
deciduous link <parent> <decision> -r "leads_to"
```

Decisions can spawn other decisions:
```bash
# Root goal
deciduous add goal "Suspense fallback behavior" -c 90
# → 1

# Top-level decisions
deciduous add decision "How should timeout thresholds work?" -c 85
deciduous link 1 2 -r "leads_to"

deciduous add decision "What happens when fetch fails?" -c 85
deciduous link 1 3 -r "leads_to"

deciduous add decision "How should nested Suspense interact?" -c 85
deciduous link 1 4 -r "leads_to"

# Sub-decisions (questions that arise from parent decisions)
deciduous add decision "Should timeout be configurable per-component?" -c 80
deciduous link 2 5 -r "leads_to"

deciduous add decision "What's the default timeout value?" -c 80
deciduous link 2 6 -r "leads_to"
```

### 5. Add answers where known

If a decision has a clear answer in the current system:

```bash
deciduous add option "<The answer/choice>" -c 90
deciduous link <decision> <option> -r "resolved_by"
deciduous status <option> chosen
```

If a decision is still open or unclear, leave it as just the decision node.

---

## The Output

A decision tree showing the current model:

```
[GOAL: Suspense fallback behavior]
    │
    ├── [DECISION: How should timeout work?]
    │       ├── [DECISION: Configurable per-component?]
    │       └── [DECISION: Default timeout value?]
    │               └── [OPTION: 1000ms] (chosen)
    │
    ├── [DECISION: What happens on fetch failure?]
    │       └── [OPTION: Propagate to error boundary] (chosen)
    │
    └── [DECISION: How do nested Suspense interact?]
            ├── [DECISION: Should parent wait for children?]
            └── [DECISION: Independent or coordinated?]
```

---

## Decision Criteria

**Is this a decision worth capturing?**
- Does it define BEHAVIOR (not implementation)? → Yes
- Would changing it change how users experience the system? → Yes
- Is it a choice that could have gone differently? → Yes
- Is it just "how the code is organized"? → No

**How deep to go?**
- Stop when decisions become implementation details
- Stop when the answer is obvious/forced (no real choice)
- Stop when you've captured what someone needs to understand the model

**Decision vs Option?**
- Decision = the question ("How should timeout work?")
- Option = an answer ("Use 1000ms default")

---

## Example: API Rate Limiting Pulse

```bash
# Goal
deciduous add goal "API rate limiting behavior" -c 90
# → 1

# Core decisions
deciduous add decision "What identifies a user for rate limiting?" -c 85
deciduous link 1 2 -r "leads_to"

deciduous add decision "What are the rate limit thresholds?" -c 85
deciduous link 1 3 -r "leads_to"

deciduous add decision "What happens when limit is exceeded?" -c 85
deciduous link 1 4 -r "leads_to"

# Answers for decision 2
deciduous add option "User ID when authenticated, IP when not" -c 90
deciduous link 2 5 -r "resolved_by"
deciduous status 5 chosen

# Sub-decisions for decision 3
deciduous add decision "Different limits for different endpoints?" -c 80
deciduous link 3 6 -r "leads_to"

deciduous add decision "Different limits for different user tiers?" -c 80
deciduous link 3 7 -r "leads_to"

# Answer for decision 4
deciduous add option "Return 429 with Retry-After header" -c 90
deciduous link 4 8 -r "resolved_by"
deciduous status 8 chosen
```

---

## Connecting to History Later

Pulse gives you the "Now". If you later want to add "How we got here":

1. Run `/narratives` to understand the evolution
2. Create `revisit` nodes that connect old decisions to current ones
3. Mark superseded approaches

The pulse becomes the destination that history leads to.

```
[Old decision] → [Observation] → [Revisit] → [Current decision from pulse]
     (history)      (history)     (pivot)         (now)
```

---

## Quick Reference

```bash
# Start with a goal
deciduous add goal "<What aspect of the system?>" -c 90

# Add decisions (the questions)
deciduous add decision "<Design question?>" -c 85
deciduous link <parent> <decision> -r "leads_to"

# Add answers where known
deciduous add option "<The answer>" -c 90
deciduous link <decision> <option> -r "resolved_by"
deciduous status <option> chosen

# View the pulse
deciduous serve
```

---

## The Mindset

You're a doctor taking the pulse of a system.

- What's the heartbeat? (core behavior)
- What decisions keep it alive? (design choices)
- What would happen if you changed X? (dependencies)

Don't worry about how it got this way. Just understand what it IS.
"#;

/// Narratives skill - Track evolution stories
pub const SKILL_NARRATIVES: &str = r#"# Narrative Tracking

**Narratives are the source of truth. Commits are just evidence.**

---

## The Core Insight

Don't start with commits. Start with understanding.

A narrative is: *"The story of how one piece of the system's design evolved."*

Commits might support a narrative. Or they might not exist. The narrative is the thing - it lives at the conceptual level, not the git level.

---

## When to Use

When you need to understand how a system got to where it is. Before building any decision graph.

---

## Process

### 1. Understand the system first

Before looking at git:

```bash
# Read the code
cat README.md
ls src/

# Understand what exists TODAY
```

Ask: **What are the major pieces of this system?**

Each major piece probably has a narrative behind it.

### 2. Identify narratives from the design

Look at the current system and ask:

- "How did the auth system get this way?"
- "Why is caching done like this?"
- "What's the story behind this API design?"

**Write down the narratives you can INFER from the code.** You don't need commits yet.

```markdown
# Narratives

## Authentication
> There's JWT + sessions. Probably started with one, switched to the other?

## Caching
> Redis everywhere. Was it always Redis? Probably not.

## API Design
> REST with some GraphQL. Hybrid approach suggests evolution.
```

### 3. Find evidence (optional)

Now, IF you want supporting evidence, look at git:

```bash
git log --oneline --all -- src/auth/
git log --oneline --grep="auth"
```

But the commits are just evidence for narratives you already identified. They're not the starting point.

### 4. Look for pivots

The most valuable thing in a narrative is: **when did the model change?**

Not "when did code change" - when did the CONCEPT change?

Signs of a pivot:
- Two approaches coexisting (migration in progress)
- Comments explaining "we used to do X"
- Config for old + new system
- Deprecation warnings

```markdown
## Authentication
> JWT for API clients, sessions for web. Evidence of a pivot.

**PIVOT:** Probably moved from pure JWT to hybrid approach.
**Evidence:** Session middleware exists alongside JWT validation.
**Why?:** (unknown - need to find out)
```

### 5. Find the "why" for pivots

This is the gold. For each pivot, figure out WHY.

Sources:
- PR descriptions
- Commit messages around the change
- Issue discussions
- Architecture decision records
- Ask someone who was there

```markdown
## Authentication
**PIVOT:** JWT → JWT + Sessions
**Why:** Mobile clients couldn't handle large JWT payloads (4KB cookie limit)
**Evidence:** PR #234 "Add session-based auth for mobile"
```

---

## Output Format

`.deciduous/narratives.md`:

```markdown
# Narratives

## <Name>
> <One sentence: what this piece of the system does>

**Current state:** <How it works today>

**Evolution:**
1. <First approach> - <why>
2. **PIVOT:** <what changed> - <why it changed>
3. <Current approach> - <why this is better>

**Evidence:** <Optional: PRs, commits, docs that support this>
**Connects to:** <Other narratives this influenced/was influenced by>
**Status:** active | superseded | abandoned

---
```

---

## Decision Criteria

**What makes something a narrative?**
- It's a coherent story about ONE design aspect
- It explains HOW something works and WHY it evolved
- It would help a new team member understand the system

**What's NOT a narrative?**
- A list of commits
- A feature changelog
- Implementation details that don't affect the model

**When is a commit worth noting?**
- Only if it supports understanding the narrative
- Only if it marks a model change (not implementation)
- Most commits are noise - skip them

**How do I know I've found a pivot?**
- The conceptual model changed, not just the code
- There's a "before" and "after" that work differently
- Someone had to make a decision to change direction

---

## Example

```markdown
# Narratives

## Authentication
> How users prove their identity to the system.

**Current state:** Hybrid - JWT for API clients, sessions for web app.

**Evolution:**
1. Started with JWT everywhere - stateless, simple, standard
2. **PIVOT:** Mobile web hit 4KB cookie limits with JWT payloads
3. Added session-based auth for web, kept JWT for API

**Why the pivot:** JWT tokens contained user permissions, growing to 3KB+.
Mobile Safari's 4KB cookie limit caused silent auth failures. Sessions
store permissions server-side, only send session ID.

**Evidence:**
- PR #234 "Add session auth for mobile web"
- Slack thread 2024-03-15 "mobile auth broken"

**Connects to:** "API Rate Limiting" (auth method affects rate limit keys)
**Status:** active

---

## API Rate Limiting
> Protecting the API from abuse and ensuring fair usage.

**Current state:** Redis-based, per-user limits with auth-aware tiers.

**Evolution:**
1. No rate limiting initially
2. **PIVOT:** Bot abuse caused outages
3. Added basic IP-based throttling
4. **PIVOT:** Legitimate users sharing IPs got blocked
5. Moved to per-user limits (requires auth)
6. **PIVOT:** Auth failures created different abuse vector
7. Added auth-aware tiers (unauth'd gets stricter limits)

**Connects to:** "Authentication" (rate limit strategy depends on auth state)
**Status:** active

---
```

---

## What This Enables

After collecting narratives, you can:

1. **Build the decision graph** (`/archaeology`) - narratives become goal nodes, pivots become revisit nodes

2. **Answer questions** like:
   - "Why does auth work this way?" → Read the Authentication narrative
   - "Can we remove sessions?" → Narrative explains why they exist
   - "What happens if we change rate limiting?" → Connections show dependencies

3. **Avoid repeating mistakes** - Pivots document what DIDN'T work and why

---

## The Mindset

Think like an anthropologist, not a git archaeologist.

You're trying to understand a culture (the system's design) by studying artifacts (code, commits, docs). The artifacts are evidence, but the culture is what matters.

**Bad:** "Let me read through 500 commits and categorize them"
**Good:** "Let me understand how auth works, then find evidence for how it evolved"
"#;

// =============================================================================
// WINDSURF TEMPLATES
// =============================================================================

/// Windsurf hooks.json configuration
pub const WINDSURF_HOOKS_JSON: &str = r#"{
  "hooks": {
    "pre_write_code": [
      {
        "command": "./.windsurf/hooks/require-action-node.sh",
        "show_output": true
      }
    ],
    "post_run_command": [
      {
        "command": "./.windsurf/hooks/post-commit-reminder.sh",
        "show_output": true
      }
    ]
  }
}
"#;

/// Windsurf pre_write_code hook - blocks writes without recent action node
pub const WINDSURF_HOOK_REQUIRE_ACTION_NODE: &str = r#"#!/bin/bash
# require-action-node.sh
# Blocks write operations if no recent action/goal node exists in deciduous
# Works with: Windsurf (Cascade)
# Exit code 2 = block the tool and show error

# Check if deciduous is initialized
if [ ! -d ".deciduous" ]; then
    exit 0
fi

# Check for any action or goal node
recent_node=$(deciduous nodes 2>/dev/null | grep -E '\[(goal|action)\]' | tail -5)

if [ -z "$recent_node" ]; then
    # No nodes at all - fresh project, allow edits
    exit 0
fi

# Check if any node was created recently (within last 15 min)
now=$(date +%s)
fifteen_min_ago=$((now - 900))

# Get the most recent node's timestamp
latest_timestamp=$(deciduous nodes 2>/dev/null | tail -1 | grep -oE '[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}:[0-9]{2}' | tail -1)

if [ -n "$latest_timestamp" ]; then
    if [[ "$OSTYPE" == "darwin"* ]]; then
        node_epoch=$(date -j -f "%Y-%m-%d %H:%M:%S" "$latest_timestamp" +%s 2>/dev/null || echo "0")
    else
        node_epoch=$(date -d "$latest_timestamp" +%s 2>/dev/null || echo "0")
    fi

    if [ "$node_epoch" -gt "$fifteen_min_ago" ]; then
        exit 0
    fi
fi

# No recent node - block and provide guidance
cat >&2 << 'EOF'
+===================================================================+
|  DECIDUOUS: No recent action/goal node found                      |
+===================================================================+
|  Before editing files, log what you're about to do:               |
|                                                                   |
|  For new work:                                                    |
|    deciduous add goal "What you're trying to achieve" -c 90       |
|                                                                   |
|  For implementation:                                              |
|    deciduous add action "What you're about to implement" -c 85    |
|                                                                   |
|  Then link to parent:                                             |
|    deciduous link <parent_id> <new_id> -r "reason"                |
+===================================================================+
EOF

exit 2
"#;

/// Windsurf post_run_command hook - reminds to link commits
pub const WINDSURF_HOOK_POST_COMMIT_REMINDER: &str = r#"#!/bin/bash
# post-commit-reminder.sh
# Runs after git commit to remind Cascade to link the commit to deciduous
# Works with: Windsurf (Cascade)
# Uses exit code 2 to ensure Cascade sees the message and acts on it

# Check if deciduous is initialized
if [ ! -d ".deciduous" ]; then
    exit 0
fi

# Read the input JSON from Windsurf
input=$(cat)

# Windsurf format: {"tool_info": {"command_line": "...", "cwd": "..."}}
command=$(echo "$input" | grep -o '"command_line"[[:space:]]*:[[:space:]]*"[^"]*"' | head -1 | sed 's/"command_line"[[:space:]]*:[[:space:]]*"//;s/"$//')

# Fallback: try jq if available
if [ -z "$command" ] && command -v jq &>/dev/null; then
    command=$(echo "$input" | jq -r '.tool_info.command_line // empty' 2>/dev/null)
fi

# Only trigger on git commit commands
if ! echo "$command" | grep -qE '^git commit'; then
    exit 0
fi

# Get the commit info
commit_hash=$(git rev-parse --short HEAD 2>/dev/null)
commit_msg=$(git log -1 --format=%s 2>/dev/null)

# Output reminder (exit 2 ensures Cascade processes this)
cat >&2 << EOF
+===================================================================+
|  DECIDUOUS: Link this commit to the decision graph!               |
+===================================================================+
|  Commit: $commit_hash "$commit_msg"
|                                                                   |
|  Run NOW:                                                         |
|    deciduous add outcome "What was accomplished" -c 95 --commit HEAD
|    deciduous link <action_id> <outcome_id> -r "Implementation complete"
|                                                                   |
|  Or if this was an action (not outcome):                          |
|    deciduous add action "What was done" -c 90 --commit HEAD       |
+===================================================================+
EOF

exit 2
"#;

/// Windsurf rules file - always-on deciduous workflow
pub const WINDSURF_RULES_DECIDUOUS: &str = r#"---
trigger: always_on
description: Decision Graph Workflow - Log decisions in real-time using deciduous
---

# Decision Graph Workflow

**THIS IS MANDATORY. Log decisions IN REAL-TIME, not retroactively.**

## The Core Rule

```
BEFORE you do something -> Log what you're ABOUT to do
AFTER it succeeds/fails -> Log the outcome
CONNECT immediately -> Link every node to its parent
AUDIT regularly -> Check for missing connections
```

## Behavioral Triggers - MUST LOG WHEN:

| Trigger | Log Type | Example |
|---------|----------|---------|
| User asks for a new feature | `goal` **with -p** | "Add dark mode" |
| Choosing between approaches | `decision` | "Choose state management" |
| About to write/edit code | `action` | "Implementing Redux store" |
| Something worked or failed | `outcome` | "Redux integration successful" |
| Notice something interesting | `observation` | "Existing code uses hooks" |

## CRITICAL: Capture VERBATIM User Prompts

**Prompts must be the EXACT user message, not a summary.**

```bash
# GOOD - verbatim prompts enable full context recovery:
deciduous add goal "Add auth" -c 90 --prompt-stdin << 'EOF'
I need to add user authentication to the app. Users should be able to sign up
with email/password, and we need OAuth support for Google and GitHub.
EOF
```

## CRITICAL: Maintain Connections

| When you create... | IMMEDIATELY link to... |
|-------------------|------------------------|
| `outcome` | The action/goal it resolves |
| `action` | The goal/decision that spawned it |
| `option` | Its parent decision |
| `observation` | Related goal/action |

**Root `goal` nodes are the ONLY valid orphans.**

## Quick Commands

```bash
deciduous add goal "Title" -c 90 -p "User's original request"
deciduous add action "Title" -c 85
deciduous link FROM TO -r "reason"  # DO THIS IMMEDIATELY!
deciduous serve   # View live
deciduous sync    # Export for static hosting

# Metadata flags
# -c, --confidence 0-100   Confidence level
# -p, --prompt "..."       Store the user prompt
# --commit <hash|HEAD>     Link to git commit
```

## CRITICAL: Link Commits to Actions/Outcomes

```bash
git commit -m "feat: add auth"
deciduous add action "Implemented auth" -c 90 --commit HEAD
deciduous link <goal_id> <action_id> -r "Implementation"
```

## Session Start Checklist

```bash
deciduous nodes           # What decisions exist?
deciduous edges           # How are they connected?
git status                # Current state
```
"#;

// =============================================================================
// SKILLS
// =============================================================================

/// Archaeology skill - Transform narratives into decision graph
pub const SKILL_ARCHAEOLOGY: &str = r#"# Archaeology

**Transform narratives into a queryable decision graph.**

Run `/narratives` first. This skill takes conceptual narratives and structures them for querying.

---

## The Relationship

```
Narratives (conceptual)     →    Decision Graph (structural)
"How auth evolved"          →    Nodes + edges you can query
Human-readable stories      →    Machine-traversable graph
```

The narrative is the truth. The graph is a queryable representation of it.

---

## When to Use

When `.deciduous/narratives.md` exists and you want to:
- Query the evolution ("what led to this?")
- Visualize connections between design decisions
- Build the "person in the room" that can answer questions

---

## Process

### 1. Read the narratives

```bash
cat .deciduous/narratives.md
```

For each narrative, you'll create a subgraph.

### 2. Map narrative → graph

Each narrative becomes a connected subgraph:

| Narrative Element | Graph Element |
|-------------------|---------------|
| Narrative title | `goal` node (the root) |
| Evolution step | `action` or `decision` node |
| **PIVOT** | `revisit` node |
| Pivot "why" | `observation` node (links INTO revisit) |
| Pre-pivot state | Nodes marked `superseded` |
| **Connects to** | Cross-narrative edge |

### 3. Build the subgraph

For a narrative like:

```markdown
## Authentication
**Evolution:**
1. Started with JWT everywhere
2. **PIVOT:** Mobile hit cookie limits
3. Added sessions for web, kept JWT for API
```

Build:

```bash
# Root (backdate to when project started)
deciduous add goal "Authentication" -c 90 --date "2023-01-15"
# → id: 1

# First approach (backdate to when it was made)
deciduous add decision "JWT for all auth" -c 85 --date "2023-01-20"
deciduous link 1 2 -r "Initial design"

# What was learned (leads to pivot)
deciduous add observation "Mobile Safari 4KB cookie limit breaking JWT auth"
deciduous link 2 3 -r "Discovered in production"

# The pivot
deciduous add revisit "Reconsidering auth token strategy"
deciduous link 3 4 -r "Cookie limits forced rethink"

# Mark pre-pivot as superseded
deciduous status 2 superseded

# New approach
deciduous add decision "Hybrid: JWT for API, sessions for web"
deciduous link 4 5 -r "New approach"
```

### 4. Connect narratives

For `**Connects to:** "Rate Limiting"`:

Find a meaningful connection point (usually an observation or decision that influenced the other narrative):

```bash
deciduous link <auth_observation> <ratelimit_decision> \
  -r "Auth failures drove rate limit redesign"
```

---

## The Revisit Pattern

Every **PIVOT** in a narrative becomes this structure:

```
[Previous approach]
        │
        ▼
[Observation: what was learned]
        │
        ▼
[Revisit: reconsidering X]
        │
        ▼
[New approach]
```

The observation captures WHY. The revisit is the decision point. The new approach is what came after.

```bash
# Pattern
deciduous add observation "<what was learned>"
deciduous link <previous_node> <observation> -r "Discovery"

deciduous add revisit "<what's being reconsidered>"
deciduous link <observation> <revisit> -r "Forced rethinking"

deciduous add decision "<new approach>"
deciduous link <revisit> <decision> -r "New direction"

# Mark old path as superseded
deciduous status <previous_node> superseded
```

---

## What NOT to Do

**Don't create nodes for every commit.**
Commits are evidence. If a narrative mentions a commit as evidence, you might reference it (`--commit <hash>`), but don't enumerate commits.

**Don't create implementation nodes.**
The graph is about the MODEL, not the code. "Implemented JWT" is not interesting. "Chose JWT over sessions" is.

**Don't over-structure.**
If a narrative has a simple evolution with no pivots, it might just be: `goal → decision → current state`. That's fine.

---

## Example: Full Transformation

**Narrative:**
```markdown
## API Rate Limiting
> Protecting the API from abuse.

**Current state:** Redis-based, per-user, auth-aware tiers.

**Evolution:**
1. No rate limiting initially
2. **PIVOT:** Bot abuse caused outages → Added IP-based throttling
3. **PIVOT:** Legitimate users on shared IPs blocked → Per-user limits
4. **PIVOT:** Auth failures as abuse vector → Auth-aware tiers

**Connects to:** "Authentication"
```

**Graph:**
```bash
# Use --date to place nodes at their historical point
deciduous add goal "API Rate Limiting" -c 90 --date "2023-02-01"
# → 1

deciduous add decision "No rate limiting" -c 70 --date "2023-02-01"
deciduous link 1 2 -r "Initial state"

# Pivot 1
deciduous add observation "Bot abuse causing service outages"
deciduous link 2 3 -r "Problem discovered"

deciduous add revisit "Need rate limiting"
deciduous link 3 4 -r "Abuse forced action"

deciduous add decision "IP-based throttling"
deciduous link 4 5 -r "First solution"

deciduous status 2 superseded

# Pivot 2
deciduous add observation "Legitimate users on shared IPs getting blocked"
deciduous link 5 6 -r "Collateral damage"

deciduous add revisit "IP-based approach too broad"
deciduous link 6 7 -r "Rethinking granularity"

deciduous add decision "Per-user rate limits"
deciduous link 7 8 -r "More precise"

deciduous status 5 superseded

# Pivot 3
deciduous add observation "Auth failures used to bypass rate limits"
deciduous link 8 9 -r "New abuse pattern"

deciduous add revisit "Rate limiting needs auth awareness"
deciduous link 9 10 -r "Security gap"

deciduous add decision "Auth-aware tier system"
deciduous link 10 11 -r "Current approach"

deciduous status 8 superseded

# Connect to Auth narrative
# deciduous link <auth_node> 9 -r "Auth design affected rate limiting"
```

---

## Querying the Graph

After building, you can ask:

```bash
# What's the current state?
deciduous nodes --status active

# What was tried and abandoned?
deciduous nodes --status superseded

# What led to a specific decision?
deciduous edges --to <node_id>

# What are the pivot points?
deciduous nodes --type revisit

# Visual exploration
deciduous serve
```

---

## The "Person in the Room"

The goal is to build a graph that can answer:

- **"Why does it work this way?"** → Trace from current state back through revisits
- **"What did we try before?"** → Look at superseded nodes
- **"Can we change X?"** → Check what depends on X via edges
- **"We should do Y"** → "We tried that, here's why it failed" (superseded + observation)

The graph is the institutional memory. The narratives are the source. The commits are just footnotes.
"#;
