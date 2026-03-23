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

[updates]
# Version checking is always-on (once per 24h, non-blocking)
# Patch updates show a quiet one-liner; minor/major updates show a prominent banner
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

/// CLAUDE.md section to append for decision graph workflow
pub const CLAUDE_MD_SECTION: &str = r#"
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
- Decision nodes should only be created when an option is actually chosen, not prematurely

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
                          # (auto-checked every 24h if auto-update is on)
deciduous nodes           # What decisions exist?
deciduous edges           # How are they connected? Any gaps?
deciduous doc list        # Any attached documents to review?
git status                # Current state
```

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
<!-- deciduous:end -->
"#;

/// Claude Code document.md slash command template
pub const DOCUMENT_MD: &str = r#"---
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

**Now document: $ARGUMENTS**
"#;

/// Claude Code build-test.md slash command template
pub const BUILD_TEST_MD: &str = r#"# Build and Test

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
   cargo test <pattern>
   ```

$ARGUMENTS
"#;

/// Claude Code serve-ui.md slash command template
pub const SERVE_UI_MD: &str = r#"# Start Decision Graph Viewer

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

/// Claude Code sync-graph.md slash command template
pub const SYNC_GRAPH_MD: &str = r#"# Sync Decision Graph to GitHub Pages

Export the current decision graph to docs/graph-data.json so it's deployed to GitHub Pages.

## Steps

1. Run `deciduous sync` to export the graph
2. Show the user how many nodes/edges were exported
3. If there are changes, stage them: `git add docs/graph-data.json`

This should be run before any push to main to ensure the live site has the latest decisions.
"#;

/// Claude Code decision-graph.md slash command template
pub const DECISION_GRAPH_MD: &str = r#"---
description: Build a deciduous decision graph capturing design evolution from commit history
allowed-tools: *
argument-hint: [repo-path]
---

# Decision Graph Construction

You are building a **deciduous decision graph** - a DAG that captures the evolution of design decisions in a codebase.

**Target repository:** $ARGUMENTS (if provided), otherwise the current directory.

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

/// Claude Code sync.md slash command template
pub const SYNC_MD: &str = r#"---
description: Sync decision graph with teammates - pull events, rebuild, push
allowed-tools: Bash(deciduous:*, git:*)
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
/// Advisory only (exit 0) — does not block or error
pub const HOOK_POST_COMMIT_REMINDER: &str = r#"#!/bin/bash
# post-commit-reminder.sh
# Runs after git commit to nudge Claude to link the commit to deciduous.
# Advisory only (exit 0) — does not block or error.

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

# Soft reminder on stdout — exit 0 so it's advisory, not blocking
echo "deciduous: commit $commit_hash — remember to link with --commit HEAD"

exit 0
"#;

/// PreToolUse hook script - checks for new deciduous versions (always-on, rate-limited)
pub const HOOK_VERSION_CHECK: &str = r#"#!/bin/bash
# version-check.sh
# Checks for new deciduous versions via crates.io (always-on, once per 24h)
# Non-blocking: exit 0 always, informational only
# Patch updates get a quiet one-liner; minor/major updates get a prominent banner

# Check if deciduous is initialized
if [ ! -d ".deciduous" ]; then
    exit 0
fi

# Semver comparison: returns "patch" or "major" based on version difference
classify_update() {
    local cur="$1" lat="$2"
    local cur_major cur_minor lat_major lat_minor
    cur_major=$(echo "$cur" | cut -d. -f1)
    cur_minor=$(echo "$cur" | cut -d. -f2)
    lat_major=$(echo "$lat" | cut -d. -f1)
    lat_minor=$(echo "$lat" | cut -d. -f2)
    if [ "$cur_major" = "$lat_major" ] && [ "$cur_minor" = "$lat_minor" ]; then
        echo "patch"
    else
        echo "major"
    fi
}

# Display update message based on severity
notify_update() {
    local current="$1" latest="$2"
    local kind
    kind=$(classify_update "$current" "$latest")
    if [ "$kind" = "patch" ]; then
        echo "DECIDUOUS: Patch update available (v${current} -> v${latest}). Run 'cargo install deciduous' to upgrade." >&2
    else
        cat >&2 << BANNER
╔══════════════════════════════════════════════════════════════╗
║  DECIDUOUS: New major update available! v${current} -> v${latest}
╚══════════════════════════════════════════════════════════════╝
Run 'deciduous check-update' for details, then 'cargo install deciduous' to upgrade.
After upgrading, run 'deciduous update' to refresh integration files.
Please let the user know a significant new version is available and encourage them to upgrade.
BANNER
    fi
}

# Rate limit: only check once per 24 hours
check_file=".deciduous/.last_version_check"
if [ -f "$check_file" ]; then
    last_check=$(cat "$check_file" 2>/dev/null || echo "0")
    now=$(date +%s)
    elapsed=$((now - last_check))
    # 86400 seconds = 24 hours
    if [ "$elapsed" -lt 86400 ]; then
        # Already checked recently - but still report cached result if newer
        cached_file=".deciduous/.latest_version"
        if [ -f "$cached_file" ]; then
            latest=$(cat "$cached_file" 2>/dev/null)
            current=$(deciduous --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)
            if [ -n "$latest" ] && [ -n "$current" ] && [ "$latest" != "$current" ]; then
                notify_update "$current" "$latest"
            fi
        fi
        exit 0
    fi
fi

# Fetch latest version from crates.io (3 second timeout)
latest=$(curl -s --max-time 3 "https://crates.io/api/v1/crates/deciduous" 2>/dev/null | grep -oE '"max_version":"[^"]*"' | head -1 | sed 's/"max_version":"//;s/"//')

if [ -z "$latest" ]; then
    # Network error or timeout - skip silently
    exit 0
fi

# Cache the result
echo "$latest" > ".deciduous/.latest_version"
date +%s > "$check_file"

# Compare with current version
current=$(deciduous --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)

if [ -n "$current" ] && [ "$latest" != "$current" ]; then
    notify_update "$current" "$latest"
fi

exit 0
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
      },
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "\"$CLAUDE_PROJECT_DIR/.claude/hooks/version-check.sh\""
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

## Step 4: Build the goal → options → decisions

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

/// Narratives skill - Track evolution stories
pub const SKILL_NARRATIVES: &str = r#"# Narrative Tracking

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
      },
      {
        "command": "./.windsurf/hooks/version-check.sh",
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
/// Advisory only (exit 0) — does not block or error
pub const WINDSURF_HOOK_POST_COMMIT_REMINDER: &str = r#"#!/bin/bash
# post-commit-reminder.sh
# Runs after git commit to nudge Cascade to link the commit to deciduous.
# Works with: Windsurf (Cascade)
# Advisory only (exit 0) — does not block or error.

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

# Soft reminder on stdout — exit 0 so it's advisory, not blocking
echo "deciduous: commit $commit_hash — remember to link with --commit HEAD"

exit 0
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

## What NOT to Log

**The decision graph records the USER'S project decisions, not your internal process.** Do NOT create nodes for reading code, planning your approach, running tests to check status, or exploring the codebase. Only log things the user would recognize as project milestones or decisions.

## CRITICAL: Capture VERBATIM User Prompts

**Prompts must be the EXACT user message, not a summary.**

```bash
# GOOD - verbatim prompts enable full context recovery:
deciduous add goal "Add auth" -c 90 --prompt-stdin << 'EOF'
I need to add user authentication to the app. Users should be able to sign up
with email/password, and we need OAuth support for Google and GitHub.
EOF
```

## Node Flow Rule: goal -> options -> decision -> actions -> outcomes

- **Goals** lead to **options** (possible approaches)
- **Options** lead to a **decision** (choosing which option)
- **Decisions** lead to **actions** (implementing the choice)
- **Actions** lead to **outcomes** (results)
- Goals do NOT lead directly to decisions -- there must be options first

## CRITICAL: Maintain Connections

| When you create... | IMMEDIATELY link to... |
|-------------------|------------------------|
| `outcome` | The action that produced it |
| `action` | The decision that spawned it |
| `decision` | The option(s) it chose between |
| `option` | Its parent goal |
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

## Document Attachments

Attach files (images, PDFs, diagrams, specs) to decision nodes:

```bash
deciduous doc attach <node_id> <file> -d "description"
deciduous doc list [node_id]
deciduous doc show <id>
deciduous doc detach <id>    # Soft-delete
deciduous doc gc             # Clean orphaned files
```

When the user shares images or references external files, suggest attaching them to the relevant node.

## Session Start Checklist

```bash
deciduous nodes           # What decisions exist?
deciduous edges           # How are they connected?
deciduous doc list        # Any attached documents?
git status                # Current state
```
"#;

// =============================================================================
// SKILLS
// =============================================================================

/// Archaeology skill - Transform narratives into decision graph
pub const SKILL_ARCHAEOLOGY: &str = r#"# Archaeology

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
- **Don't over-structure.** Simple narratives might just be: goal → option → decision.
"#;
