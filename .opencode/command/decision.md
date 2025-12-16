---
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
- `-f, --files "file1.rs,file2.rs"` - Associate files with this node
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

## The Rule

```
LOG BEFORE YOU CODE, NOT AFTER.
CONNECT EVERY NODE TO ITS PARENT.
AUDIT FOR ORPHANS REGULARLY.
SYNC BEFORE YOU PUSH.
```
