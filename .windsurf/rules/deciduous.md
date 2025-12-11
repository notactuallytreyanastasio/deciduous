---
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
- **Choosing between approaches** → `deciduous add decision "What to decide" -c 75`
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
# Add nodes (with confidence 0-100, -p when semantically meaningful)
deciduous add goal "Title" -c 90 -p "User's original request"
deciduous add decision "Title" -c 75
deciduous add action "Title" -c 85
deciduous add outcome "Title" -c 95
deciduous add observation "Title" -c 80

# Optional metadata flags for nodes
# -p, --prompt "..."   Store the user prompt (use when semantically meaningful)
# -f, --files "a.rs,b.rs"   Associate files with this node
# -b, --branch <name>   Git branch (auto-detected by default)
# --no-branch   Skip branch auto-detection
# --commit <hash|HEAD>   Link to a git commit (use HEAD for current commit)

# Example with prompt and files on root goal
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
**Nodes are automatically tagged with the current git branch.** This enables filtering by feature/PR.

### How It Works
- When you create a node, the current git branch is stored automatically
- Configure which branches are "main" in `.deciduous/config.toml`:
  ```toml
  [branch]
  main_branches = ["main", "master"]  # Branches not treated as feature branches
  auto_detect = true                    # Auto-detect branch on node creation
  ```
- Nodes on feature branches can be filtered and grouped

### CLI Commands
```bash
# Filter nodes by branch
deciduous nodes --branch main
deciduous nodes --branch feature-auth
deciduous nodes -b my-feature

# Override auto-detection
deciduous add goal "Feature work" -b feature-x  # Force specific branch
deciduous add goal "Universal note" --no-branch  # No branch tag
```

### Web UI
The graph viewer has a branch dropdown filter in the stats bar.

### When to Use
- **Feature work**: Nodes auto-grouped by branch
- **PR context**: Filter to see decisions for specific PR
- **Cross-cutting**: Use `--no-branch` for universal notes
</branch_grouping>

## Edge Types

<edge_types>
- `leads_to` - Natural progression (default)
- `chosen` - Selected this option
- `rejected` - Did not select (always include reason!)
- `requires` - Dependency relationship
- `blocks` - Preventing progress
- `enables` - Makes something possible
</edge_types>

## Graph Integrity - CRITICAL

<integrity_rules>
**Every node MUST be logically connected.** Floating nodes break the graph's value.

### Connection Rules
| Node Type | MUST connect to | Valid orphan? |
|-----------|----------------|---------------|
| `outcome` | The action/goal it resolves | NO - always needs parent |
| `action` | The decision/goal that spawned it | NO - always needs parent |
| `option` | Its parent decision | NO - always needs parent |
| `observation` | Related goal/action/decision | Usually no |
| `decision` | Parent goal (if any) | Sometimes |
| `goal` | Can be a root | YES - root goals are valid |

### Audit Checklist
After creating nodes, ask:
1. Does every **outcome** link back to what caused it?
2. Does every **action** link to why you did it?
3. Does every **option** link to its decision?
4. Are there **dangling outcomes** with no parent?

### Find Disconnected Nodes
```bash
# List nodes with no incoming edges
deciduous edges | cut -d'>' -f2 | cut -d' ' -f2 | sort -u > /tmp/has_parent.txt
deciduous nodes | tail -n+3 | awk '{print $1}' | while read id; do
  grep -q "^$id$" /tmp/has_parent.txt || echo "CHECK: $id"
done
```

### Fix Missing Connections
```bash
deciduous link <parent_id> <child_id> -r "Retroactive connection - <reason>"
```

### When to Audit
- At session start
- Before every `deciduous sync`
- After creating multiple nodes quickly
- When graph looks disconnected in web UI
</integrity_rules>

## Multi-User Sync

<multi_user_sync>
**Problem**: Multiple users work on the same codebase, each with a local `.deciduous/deciduous.db` (gitignored). How to share decisions?

**Solution**: jj-inspired dual-ID model. Each node has:
- `id` (integer): Local database primary key, different per machine
- `change_id` (UUID): Globally unique, stable across all databases

### Commands
```bash
# Export your branch's decisions as a patch
deciduous diff export --branch feature-x -o .deciduous/patches/alice-feature.json

# Export specific nodes
deciduous diff export --nodes 172-188 -o .deciduous/patches/feature.json --author alice

# Apply patches from teammates (idempotent)
deciduous diff apply .deciduous/patches/*.json

# Preview without applying
deciduous diff apply --dry-run .deciduous/patches/bob.json

# Check patch status
deciduous diff status
```

### PR Workflow
1. Create nodes while working
2. Export: `deciduous diff export --branch my-feature -o .deciduous/patches/my-feature.json`
3. Commit the patch file (NOT the database)
4. Open PR with patch file
5. Teammates pull and apply: `deciduous diff apply .deciduous/patches/my-feature.json`
6. **Idempotent**: Same patch applied twice = no duplicates
</multi_user_sync>

## The Rule

<core_rule>
LOG BEFORE YOU CODE, NOT AFTER.
CONNECT EVERY NODE TO ITS PARENT.
AUDIT FOR ORPHANS REGULARLY.
SYNC BEFORE YOU PUSH.
EXPORT PATCHES FOR YOUR TEAMMATES.
</core_rule>

</decision_graph_workflow>

**Live graph**: https://notactuallytreyanastasio.github.io/deciduous/
