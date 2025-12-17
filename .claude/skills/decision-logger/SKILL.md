---
name: decision-logger
description: Log all work to the decision graph with deciduous CLI. ALWAYS use when starting tasks, implementing features, fixing bugs, planning, refactoring, choosing approaches, or making any code changes. Creates persistent memory that survives context loss.
---

# Decision Graph Logger

**MANDATORY: Log decisions in real-time as you work.** The decision graph is your persistent memory.

## Start EVERY Task with a Goal

When the user asks for ANYTHING, IMMEDIATELY create a goal node FIRST:

```bash
deciduous add goal "Brief title" -c 90 --prompt-stdin << 'EOF'
<user's exact request, verbatim>
EOF
```

Do this BEFORE exploring code, BEFORE planning, BEFORE anything else.

## Node Types - Use ALL of Them Granularly

| When This Happens | Log This | Command |
|-------------------|----------|---------|
| User requests anything | `goal` | `deciduous add goal "..." -c 90 --prompt-stdin` |
| Choosing between approaches | `decision` | `deciduous add decision "..." -c 75` |
| Each alternative considered | `option` | `deciduous add option "..." -c 70` |
| About to write/edit code | `action` | `deciduous add action "..." -c 85` |
| Code change completed | `outcome` | `deciduous add outcome "..." -c 90` |
| Something failed | `outcome` | `deciduous add outcome "Failed: ..." -c 90` |
| Noticed something in codebase | `observation` | `deciduous add observation "..." -c 80` |

## Granular Tracking

**Log frequently, not just milestones:**
- Multiple `action` nodes per goal (one per file change or logical step)
- Multiple `outcome` nodes (one per completed step)
- `observation` nodes when you discover anything relevant
- `decision` nodes for ANY choice, not just big architectural ones

## Link IMMEDIATELY After Creating Nodes

```bash
# Every node except root goals MUST link to a parent
deciduous link <parent_id> <child_id> -r "Reason for connection"
```

**Linking rules:**
- `action` → links to `goal` or `decision` that spawned it
- `outcome` → links to `action` that produced it
- `option` → links to `decision` it belongs to
- `observation` → links to relevant `goal` or `action`
- `decision` → links to `goal` or parent `decision`

## The Workflow

```
1. USER REQUEST ARRIVES
   ↓
   deciduous add goal "..." --prompt-stdin    ← ALWAYS FIRST
   ↓
2. EXPLORE/PLAN
   ↓
   deciduous add observation "Found X in codebase" (if relevant)
   deciduous link <goal> <obs> -r "Discovery"
   ↓
3. CHOOSE APPROACH (if multiple options)
   ↓
   deciduous add decision "How to implement X"
   deciduous link <goal> <decision> -r "Design choice"
   deciduous add option "Approach A"
   deciduous add option "Approach B"
   deciduous link <decision> <option> -r "Considered"
   ↓
4. BEFORE EACH CODE CHANGE
   ↓
   deciduous add action "Implement X in file.py"
   deciduous link <goal_or_decision> <action> -r "Implementation step"
   ↓
5. MAKE THE CODE CHANGE
   ↓
6. AFTER CHANGE COMPLETES
   ↓
   deciduous add outcome "X implemented successfully" (or "Failed: reason")
   deciduous link <action> <outcome> -r "Result"
   ↓
7. REPEAT 4-6 FOR EACH STEP
   ↓
8. FINAL OUTCOME
   ↓
   deciduous add outcome "Feature complete" -c 95
   deciduous link <goal> <outcome> -r "Goal achieved"
```

## Confidence Levels

| Level | Meaning | Use For |
|-------|---------|---------|
| 90-100 | Certain, verified | Completed work, user-confirmed goals |
| 75-89 | High confidence | Actions about to take, solid plans |
| 50-74 | Moderate | Experimental approaches, uncertain fixes |
| Below 50 | Speculative | Risky changes, untested ideas |

## After Git Commits

```bash
git commit -m "feat: add auth"
deciduous add action "Committed auth feature" -c 95 --commit HEAD
deciduous link <parent_action> <commit_action> -r "Git commit"
```

## Examples of Granular Logging

**User asks: "Add a login page"**

```bash
# 1. Goal first (with full prompt)
deciduous add goal "Add login page" -c 90 --prompt-stdin << 'EOF'
Add a login page to the app with email/password fields
EOF
# Returns: Created node 42

# 2. Observation while exploring
deciduous add observation "Found existing auth utils in src/auth/" -c 85
deciduous link 42 43 -r "Codebase discovery"

# 3. Decision on approach
deciduous add decision "Choose login form implementation" -c 75
deciduous link 42 44 -r "Design decision"

# 4. Action before creating component
deciduous add action "Create LoginForm component" -c 85
deciduous link 44 45 -r "Implementation"

# 5. Outcome after component works
deciduous add outcome "LoginForm component created with validation" -c 90
deciduous link 45 46 -r "Completed"

# 6. Next action
deciduous add action "Add login API endpoint" -c 85
deciduous link 44 47 -r "Backend implementation"

# 7. And so on...
```

## Remember

- **Goal FIRST** - Before any other work
- **Log BEFORE acting** - Create action, then write code
- **Log AFTER completing** - Outcomes capture results
- **Link IMMEDIATELY** - No orphan nodes
- **Granular > Sparse** - More nodes = better context recovery
- **Failures matter** - Log unsuccessful outcomes too
- **This IS your memory** - The graph survives context loss, you don't
