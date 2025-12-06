---
layout: default
title: Claude Tooling
---

# Claude Tooling: Persistent AI Assistance

One of the challenges with AI-assisted development is **context loss**. Sessions end, context windows fill up, and the AI forgets what it learned.

This project includes tooling designed to solve that problem.

---

## The Problem

Traditional AI chat:
1. Start session → AI knows nothing
2. Work together → AI learns your codebase
3. Session ends or context fills → AI forgets everything
4. Start new session → Back to square one

This is frustrating for long-running projects where continuity matters.

---

## The Solution: Decision Graph + Context Recovery

### 1. Persistent Memory

The decision graph isn't just documentation - it's **Claude's external memory**. When a session ends, the graph persists. When a new session starts, Claude can query it:

```bash
# What have we been working on?
./losselot db nodes

# What decisions are pending?
./losselot db nodes | grep pending

# What was recently added?
./losselot db commands
```

### 2. Context Recovery Slash Command

The `/context` command is designed to be run at the start of every session:

```markdown
# .claude/commands/context.md

## Automatic Context Gathering

Execute these commands to recover project state:

1. Query decision graph nodes
2. Check git status and recent commits
3. Read git.log for session history
```

When Claude runs `/context`, it immediately understands:
- What decisions are pending
- What was recently worked on
- What observations haven't been addressed

### 3. CLAUDE.md Instructions

The project's `CLAUDE.md` file includes explicit instructions:

```markdown
## Context Recovery (CRITICAL)

**On every session start or after context compaction, IMMEDIATELY run:**

./losselot db nodes
./losselot db commands
git status && git log --oneline -5
tail -20 git.log
```

This ensures continuity even when Claude "wakes up" in a new session.

---

## Available Slash Commands

| Command | Purpose |
|---------|---------|
| `/context` | Recover state from decision graph and git |
| `/decision <action>` | Manage decision graph (add nodes, edges) |
| `/analyze` | Run audio analysis workflow |
| `/build-test` | Build and test the project |
| `/serve-ui` | Start the web UI |

### Example: `/decision`

```bash
# Add a new goal
/decision goal "Implement mixed-source detection"

# Add an observation
/decision obs "Spectrograms show pillars of HF content in mixed tracks"

# Link nodes
/decision link 13 17
```

The slash commands make complex operations simple and ensure consistency.

---

## Session Logging

Every git operation is logged to `git.log` with timestamps:

```
Fri Dec  5 17:34:27 EST 2025: git status
Fri Dec  5 17:34:41 EST 2025: git branch backup-before-decision-graph-commit
Fri Dec  5 17:34:51 EST 2025: git add [specific files]
Fri Dec  5 17:35:02 EST 2025: git commit
Fri Dec  5 17:35:46 EST 2025: git push -u origin feature/sqlite-diesel-graph
```

This creates an audit trail that survives session boundaries. When Claude starts a new session, reading `git.log` reveals:
- What operations were performed
- In what order
- How recently

---

## Git Rules

To prevent destructive operations, `CLAUDE.md` includes strict rules:

1. **NO DELETING ANYTHING UNSTAGED, EVER**
2. **ALWAYS MAKE A BACKUP BRANCH** before risky operations
3. **ALWAYS LOG TO git.log**
4. **NO MAJORLY DESTRUCTIVE ACTIONS WITHOUT CONFIRMATION**
5. **NO COMMITTING AS CLAUDE** - use the user's config
6. **BE DELIBERATE** - explicit file staging, no `.` or `-u`

These rules ensure Claude can't accidentally destroy work, even if it misunderstands context.

---

## The Workflow

Here's how a typical session flows:

### Session Start
```
Claude: *reads CLAUDE.md*
Claude: *runs context recovery*
Claude: "I see we were working on the GitHub Pages site.
        Node 13 (goal) is creating the living museum.
        Node 14 (decision) is about site structure.
        Recent commits show we added docs/index.md."
```

### During Work
```
User: "Let's add a section about CFCC detection"

Claude: *does the work*
Claude: *adds observation to decision graph*
Claude: "Added node 17 documenting the CFCC section addition"
```

### Before Session Ends
```
Claude: *updates decision graph with current state*
Claude: *commits work with descriptive message*
Claude: "Graph updated. Ready for next session."
```

### Next Session Starts
```
Claude: *queries decision graph*
Claude: "Resuming from where we left off.
        Last action: Added CFCC section.
        Pending: Complete audio-analysis page."
```

---

## Why This Works

The key insight is that **state should be external, not internal**.

Instead of relying on Claude's context window to remember everything:
- **Decisions** → stored in SQLite graph
- **Actions** → stored in git commits
- **Session history** → stored in git.log
- **Instructions** → stored in CLAUDE.md

When context is cleared, these external stores remain. Claude can reconstruct its understanding by querying them.

---

## Try It Yourself

If you're using Claude Code with this project:

```bash
# Clone the repo
git clone https://github.com/notactuallytreyanastasio/losselot.git
cd losselot

# Build
cargo build --release

# Start a Claude session and run:
/context

# Claude will query the decision graph and understand the project state
```

The decision graph, git.log, and CLAUDE.md work together to give Claude persistent memory.

---

[← Decision Graph](decision-graph) | [Next: Development Story →](story)
