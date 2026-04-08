# Deciduous Concepts

This document explains the mental models behind deciduous - why it exists and how to think about it.

---

## The Context Problem

AI assistants are powerful but forgetful. Every session:

1. **Starts fresh** - no memory of past work
2. **Gets compacted** - long conversations get summarized, losing detail
3. **Loses nuance** - "we tried X but it didn't work" becomes just "we did Y"

This creates problems:

- **Repeated mistakes**: "Let's try JWT" ... "Actually we tried JWT last week and it had issues"
- **Lost reasoning**: The code uses approach A, but WHY? The decision is gone.
- **Pivot confusion**: We changed direction, but which code is old vs new?

---

## The Solution: Externalize Reasoning

Deciduous captures decisions **as they happen**, creating a persistent graph that survives:

- Session boundaries
- Context compaction
- Memory limits

The graph becomes **external memory** that any future session can query.

---

## The Decision Graph

Think of it as a DAG (Directed Acyclic Graph) where:

- **Nodes** = Things that happened (goals, decisions, actions, outcomes)
- **Edges** = Relationships between them (leads_to, requires, chosen)

### Node Types

| Type | When to Use | Example |
|------|-------------|---------|
| **goal** | User wants something | "Add user authentication" |
| **decision** | A choice must be made | "How to store sessions?" |
| **option** | One possible choice | "Use Redis" or "Use JWT" |
| **action** | Work is being done | "Implementing session middleware" |
| **outcome** | Work is complete | "Sessions working in dev" |
| **observation** | Something was noticed (title + description) | "Redis adds infrastructure cost" + -d "Running Redis requires a managed instance or self-hosted server, adding $50/mo minimum and ops burden" |
| **revisit** | Reconsidering past work | "Rethinking session approach" |

### Edge Types

| Type | Meaning | Example |
|------|---------|---------|
| **leads_to** | One thing causes another | goal → option |
| **chosen** | Option was selected | option → decision |
| **rejected** | Option was not selected | option (marked rejected) |
| **requires** | Dependency | action → another action |
| **blocks** | Prevents progress | observation → action |
| **enables** | Makes something possible | action → outcome |

---

## Real-Time Logging

The key insight: **Log BEFORE you do, not after.**

```
BAD (retroactive):
  1. Write code
  2. Commit
  3. "Oh I should log that"  ← Often forgotten, context lost

GOOD (real-time):
  1. deciduous add action "Implementing X"  ← What you're ABOUT to do
  2. Write code
  3. deciduous add outcome "X complete"  ← What happened
  4. deciduous link <action> <outcome>  ← Connect them
```

Real-time logging works because:
- The AI knows what it's about to do
- The full context exists right now
- Hooks can enforce it

---

## The Revisit Pattern

When you change direction, capture the **pivot**:

```
Old Approach                          New Approach
────────────                          ────────────
[Decision: Use JWT]                   [Decision: Use sessions]
      │                                     ▲
      ▼                                     │
[Option: JWT chosen]                  [Option: Sessions chosen]
      │                                     │
      ▼                                     │
[Action: Implement JWT]               ┌─────┘
      │                               │
      ▼                               │
[Outcome: Working but...]             │
      │                               │
      ▼                               │
[Observation: JWT too large]──────────┤
      │                               │
      └─────►[REVISIT]────────────────┘
             "Reconsidering token strategy"
```

The **revisit** node:
1. Links to observations that caused the rethink
2. Links to the old decision being superseded
3. Leads to the new decision

This captures **WHY** we changed, not just **WHAT** changed.

---

## Node Status

Nodes have status to show their state:

| Status | Meaning |
|--------|---------|
| `active` | Current truth - this is how things work now |
| `superseded` | Replaced by newer approach |
| `abandoned` | Tried and rejected, not replaced |
| `pending` | Not yet started |
| `completed` | Finished successfully |

When querying, you can filter:

```bash
# Current state of the system
deciduous nodes --status active

# Everything including history
deciduous nodes

# Find pivot points
deciduous nodes --type revisit
```

---

## Sessions

Nodes are grouped into **sessions** based on time proximity.

- Gap threshold: 4 hours
- Nodes within 4 hours = same session
- Gap > 4 hours = new session

Sessions help answer "what did I do yesterday?" vs "what did I do last week?"

---

## Chains

A **chain** is a connected component of the graph.

Starting from root nodes (goals, or nodes with no incoming edges), BFS traverses all connected nodes. This groups related decisions together.

Chains answer "show me everything related to authentication."

---

## The Prompt Field

The most important field for context recovery.

When a user asks for something, capture their **exact words**:

```bash
# BAD - summary loses context
deciduous add goal "Add auth" -p "User wants login"

# GOOD - verbatim prompt enables full recovery
deciduous add goal "Add auth" --prompt-stdin << 'EOF'
I need to add user authentication to the app. Users should be able to sign up
with email/password, and we need OAuth support for Google and GitHub. The auth
should use JWT tokens with refresh token rotation.
EOF
```

The prompt field stores the **verbatim user message**. Future sessions can read this and understand exactly what was requested.

---

## Document Attachments

Decision nodes can have **files attached** — architecture diagrams, screenshots, specs, PDFs.

```bash
deciduous doc attach <node_id> <file> -d "Description"
deciduous doc list <node_id>
deciduous doc open <doc_id>
```

Why attach documents?

- **Visual context**: Architecture diagrams explain decisions better than text
- **Evidence**: Screenshots capture the state at a specific point in time
- **Reference**: Specs and PDFs linked to the goal that consumed them
- **Recovery**: Future sessions can view attached documents to rebuild context

Files are stored in `.deciduous/documents/` with content-hash naming. Duplicate files are deduplicated automatically. Soft-delete with `detach`; garbage-collect with `gc`.

The web viewer displays attached documents in the node detail panel with filename, size, MIME type, and description. AI-generated descriptions are marked with an **(AI)** badge.

---

## Multi-User Sync

The database (`.deciduous/deciduous.db`) is local and gitignored. How do teams share?

### The Dual-ID Model

Every node has:
- `id` (integer): Local primary key, different on each machine
- `change_id` (UUID): Globally unique, stable everywhere

### Patch Files

```bash
# Export decisions as a patch
deciduous diff export --branch feature-x -o .deciduous/patches/my-feature.json

# Apply patches from teammates (idempotent)
deciduous diff apply .deciduous/patches/*.json
```

Patches reference nodes by `change_id`, so they work across different local databases.

### Workflow

1. Work locally, creating nodes
2. Export patch file
3. Commit patch file (NOT the database) to git
4. Teammates apply patches after pulling

---

## Hook Enforcement

Hooks make logging **mandatory**, not optional.

```toml
# .deciduous/config.toml
[[hooks.pre_tool_use]]
name = "require-action-node"
matcher = "Edit|Write"
enabled = true
```

When the AI tries to edit code:

1. Claude Code runs the pre-tool-use hook
2. Hook checks: Is there an action node in the last 15 minutes?
3. No? **Block the edit** with a message

This forces the AI to log first, then edit.

---

## Three Browsing Modes

### Now Mode: How does this work?

Query the current state of the system:

```bash
deciduous nodes --status active
```

View: DAG, Chains, Graph

### History Mode: How did we get here?

Query evolution and pivots:

```bash
deciduous nodes --type revisit
deciduous show <revisit_id>  # See what caused the pivot
```

View: Archaeology, Timeline, Narratives

### Roadmap Mode: What's next?

Query planned work:

```bash
deciduous roadmap list
deciduous roadmap sync  # Sync with GitHub Issues
```

View: Roadmap

---

## Best Practices

### 1. Log Before You Do

```bash
deciduous add action "Implementing X" -c 85
# ... do the work ...
deciduous add outcome "X complete" -c 95
deciduous link <action> <outcome>
```

### 2. Link Immediately

Don't create orphan nodes:

```bash
deciduous add decision "How to do X?"
deciduous link <goal_id> <new_decision_id> -r "X is part of goal"
```

### 3. Capture Verbatim Prompts

For goals that come from user requests:

```bash
deciduous add goal "Title" --prompt-stdin << 'EOF'
The full user message here...
EOF
```

### 4. Use Observations

When you notice something that might matter later:

```bash
deciduous add observation "Redis requires additional infrastructure" -c 70 -d "Running Redis requires a managed instance or self-hosted server. Adds operational complexity and ~$50/mo minimum cost."
deciduous link <action> <observation> -r "Discovered during implementation"
```

### 5. Create Revisit Nodes for Pivots

When changing direction:

```bash
deciduous add revisit "Reconsidering X approach"
deciduous link <observation_that_caused_it> <revisit>
deciduous link <revisit> <new_decision>
deciduous status <old_decision> superseded
```

### 6. Audit Before Sync

```bash
# Check for orphans
deciduous edges  # Any nodes missing connections?

# Then sync
deciduous sync
```

---

## Summary

Deciduous externalizes AI reasoning into a queryable graph.

**Core ideas:**
- Log in real-time, not retroactively
- Connect everything with edges
- Capture pivots with revisit nodes
- Store verbatim prompts for recovery
- Enforce with hooks

**Result:** AI-assisted development that survives context loss.
