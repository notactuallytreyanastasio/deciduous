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
- **New feature request** → `deciduous add goal "Feature name" -c 90`
- **Choosing between approaches** → `deciduous add decision "What to decide" -c 75`
- **Considering an option** → `deciduous add option "Option name" -c 70`
- **About to write code** → `deciduous add action "What you're implementing" -c 85`
- **Noticed something** → `deciduous add observation "What you found" -c 80`
- **Something completed** → `deciduous add outcome "Result" -c 95`
</logging_triggers>

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
# -b, --branch <name>   Git branch (auto-detected by default)
# --no-branch   Skip branch auto-detection
# --commit <hash>   Link to a git commit

# Example with prompt and files
deciduous add goal "Add auth" -c 90 -p "User asked: add login feature" -f "src/auth.rs,src/routes.rs"

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

## The Rule

<core_rule>
LOG BEFORE YOU CODE, NOT AFTER.
CONNECT EVERY NODE TO ITS PARENT.
AUDIT FOR ORPHANS REGULARLY.
SYNC BEFORE YOU PUSH.
</core_rule>

</decision_graph_workflow>

**Live graph**: https://notactuallytreyanastasio.github.io/deciduous/
