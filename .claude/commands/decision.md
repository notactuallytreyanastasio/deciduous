---
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
