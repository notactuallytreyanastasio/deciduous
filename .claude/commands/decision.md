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
| Starting a new feature | `goal` | `/decision add goal "Add user auth"` |
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
- `--commit <hash>` - Link to a git commit

### Branch Filtering
- `deciduous nodes --branch main` - Show only nodes from main branch
- `deciduous nodes --branch feature-x` - Show only nodes from feature-x branch
- Web UI has branch dropdown filter in stats bar

### Create Edges
- `link <from> <to> [reason]` -> `deciduous link <from> <to> -r "<reason>"`

### Sync Graph
- `sync` -> `deciduous sync`

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

## The Rule

```
LOG BEFORE YOU CODE, NOT AFTER.
CONNECT EVERY NODE TO ITS PARENT.
AUDIT FOR ORPHANS REGULARLY.
SYNC BEFORE YOU PUSH.
```

**Live graph**: https://notactuallytreyanastasio.github.io/deciduous/
