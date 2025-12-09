---
description: Recover context from decision graph and recent activity - USE THIS ON SESSION START
allowed-tools: Bash(losselot:*, make:*, git:*, cat:*, tail:*)
argument-hint: [focus-area]
---

# Context Recovery

**RUN THIS AT SESSION START.** The decision graph is your persistent memory.

## Step 1: Query the Graph

```bash
# See all decisions (look for recent ones and pending status)
./target/release/losselot db nodes

# See how decisions connect
./target/release/losselot db edges

# What commands were recently run?
./target/release/losselot db commands
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
2. **Recent decisions** (especially pending/active ones)
3. **Last actions** from git log and command log
4. **Open questions** or unresolved observations
5. **Suggested next steps**

---

## ⚠️ REMEMBER: Real-Time Logging Required

After recovering context, you MUST follow the logging workflow:

```
EVERY USER REQUEST → Log goal/decision first
BEFORE CODE CHANGES → Log action
AFTER CHANGES → Log outcome, link nodes
BEFORE GIT PUSH → make sync-graph
```

**The user is watching the graph live.** Log as you go, not after.

### Quick Logging Commands

```bash
./target/release/losselot db add-node -t goal "What we're trying to do" --confidence 90
./target/release/losselot db add-node -t action "What I'm about to implement" --confidence 85
./target/release/losselot db add-node -t outcome "What happened" --confidence 95
./target/release/losselot db add-edge FROM TO -r "Connection reason"
make sync-graph  # Do this frequently!
```

---

## Focus Areas

If $ARGUMENTS specifies a focus, prioritize context for:

- **lofi** / **cfcc**: Lo-fi detection, CFCC algorithm nodes
- **spectral**: Spectral analysis decisions
- **ui** / **graph**: UI and graph viewer state
- **detection**: General detection algorithms

---

## The Memory Loop

```
SESSION START
    ↓
Run /context → See past decisions
    ↓
DO WORK → Log BEFORE each action
    ↓
AFTER CHANGES → Log outcomes, observations
    ↓
BEFORE PUSH → make sync-graph
    ↓
PUSH → Live graph updates
    ↓
SESSION END → Graph persists
    ↓
(repeat)
```

**Live graph**: https://notactuallytreyanastasio.github.io/losselot/demo/

---

## Why This Matters

- Context loss during compaction loses your reasoning
- The graph survives - query it early, query it often
- Retroactive logging misses details - log in the moment
- The user sees the graph live - show your work
