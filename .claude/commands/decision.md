---
description: Manage decision graph - track algorithm choices and reasoning
allowed-tools: Bash(losselot:*, make:*)
argument-hint: <action> [args...]
---

# Decision Graph Management

**Log decisions IN REAL-TIME as you work, not retroactively.**

## ⚠️ When to Use This

| You're doing this... | Log this type | Command |
|---------------------|---------------|---------|
| Starting a new feature | `goal` | `/decision goal Add user auth` |
| Choosing between approaches | `decision` | `/decision decision Choose auth method` |
| Considering an option | `option` | `/decision option JWT tokens` |
| About to write code | `action` | `/decision action Implementing JWT` |
| Noticing something | `observation` | `/decision obs Found existing auth code` |
| Finished something | `outcome` | `/decision outcome JWT working` |

## Quick Commands

Based on $ARGUMENTS:

### View Commands
- `nodes` or `list` → `./target/release/losselot db nodes`
- `edges` → `./target/release/losselot db edges`
- `graph` → `./target/release/losselot db graph`
- `commands` → `./target/release/losselot db commands`

### Create Nodes (with optional confidence)
- `goal <title>` → `./target/release/losselot db add-node -t goal "<title>" --confidence 90`
- `decision <title>` → `./target/release/losselot db add-node -t decision "<title>" --confidence 75`
- `option <title>` → `./target/release/losselot db add-node -t option "<title>" --confidence 70`
- `action <title>` → `./target/release/losselot db add-node -t action "<title>" --confidence 85`
- `obs <title>` → `./target/release/losselot db add-node -t observation "<title>" --confidence 80`
- `outcome <title>` → `./target/release/losselot db add-node -t outcome "<title>" --confidence 90`

### Create Edges
- `link <from> <to> [reason]` → `./target/release/losselot db add-edge <from> <to> -r "<reason>"`

### Sync to Live Site
- `sync` → `make sync-graph`

## Node Types

| Type | Purpose | Example |
|------|---------|---------|
| `goal` | High-level objective | "Improve lo-fi detection" |
| `decision` | Choice point with options | "Choose detection algorithm" |
| `option` | Possible approach | "Use spectral slope" |
| `action` | Something implemented | "Added CFCC analysis" |
| `outcome` | Result of action | "CFCC working, 95% accuracy" |
| `observation` | Finding or data point | "Tape has gradual rolloff" |

## Edge Types

| Type | Meaning |
|------|---------|
| `leads_to` | Natural progression |
| `chosen` | Selected option |
| `rejected` | Not selected (include reason!) |
| `requires` | Dependency |
| `blocks` | Preventing progress |
| `enables` | Makes something possible |

## Example Workflow

```bash
# 1. User asks for a feature - log the goal
./target/release/losselot db add-node -t goal "Add dark mode" --confidence 90
# Created node 50

# 2. You identify options - log each one
./target/release/losselot db add-node -t decision "Choose theme approach" --confidence 80
./target/release/losselot db add-node -t option "CSS variables" --confidence 85
./target/release/losselot db add-node -t option "Styled components" --confidence 70
# Created nodes 51, 52, 53

# 3. Link them
./target/release/losselot db add-edge 50 51 -r "Goal leads to decision"
./target/release/losselot db add-edge 51 52 -r "Option A"
./target/release/losselot db add-edge 51 53 -r "Option B"

# 4. Make a choice - log it
./target/release/losselot db add-edge 51 52 -t chosen -r "Simpler, no build changes"
./target/release/losselot db add-edge 51 53 -t rejected -r "Would require restructure"

# 5. Implement - log BEFORE coding
./target/release/losselot db add-node -t action "Implementing CSS variables theme" --confidence 90
# Created node 54

# 6. Done - log outcome
./target/release/losselot db add-node -t outcome "Dark mode working" --confidence 95
./target/release/losselot db add-edge 54 55 -r "Action completed"

# 7. SYNC before pushing
make sync-graph
git add docs/demo/graph-data.json
```

## The Rule

```
LOG BEFORE YOU CODE, NOT AFTER.
SYNC BEFORE YOU PUSH.
THE USER IS WATCHING LIVE.
```

**Live graph**: https://notactuallytreyanastasio.github.io/losselot/demo/
