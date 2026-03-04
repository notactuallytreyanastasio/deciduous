---
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

## Step 2: Check Git State

```bash
git status
git log --oneline -10
git diff --stat
```

## After Gathering Context, Report:

1. **Current branch** and pending changes
2. **Branch-specific decisions** (filter by branch if on feature branch)
3. **Recent decisions** (especially pending/active ones)
4. **Last actions** from git log and command log
5. **Open questions** or unresolved observations
6. **Attached documents** - diagrams, specs, or screenshots on key nodes
7. **Suggested next steps**

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

## Why This Matters

- Context loss during compaction loses your reasoning
- The graph survives - query it early, query it often
- Retroactive logging misses details - log in the moment
- The user sees the graph live - show your work
