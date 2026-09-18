# Decision Graph Concepts

Deciduous captures design decisions as a queryable graph. This document explains the conceptual model and the three skills for working with it.

---

## The Two Modes

Every system has two stories:

| Mode | Question | What it captures |
|------|----------|------------------|
| **Now** | "How does this work?" | Current design decisions |
| **History** | "How did we get here?" | Evolution, pivots, dead ends |

Both are valuable:
- **Now** → Explain to a new team member
- **History** → Prevent repeating mistakes

---

## The Three Skills

| Skill | Mode | Purpose |
|-------|------|---------|
| `/pulse` | Now | Map current design as decisions |
| `/narratives` | History | Understand how things evolved |
| `/archaeology` | Structure | Transform narratives into queryable graph |

### How They Connect

```
/pulse                              /narratives
"What decisions define             "How did we get to
 how this works?"                   these decisions?"
        │                                  │
        ▼                                  ▼
[Current model as                   [Evolution stories
 decision tree]                      with pivots]
        │                                  │
        └────────────┬─────────────────────┘
                     │
                     ▼
              /archaeology
              "Make it queryable"
                     │
                     ▼
              [Connected graph]
              Now ←── revisit ←── History
```

---

## /pulse - The Now Mode

**Purpose:** Map the current system as a tree of design decisions.

**When to use:**
- Understanding an unfamiliar codebase
- Documenting current architecture
- Before making changes

**How it works:**

1. Pick a scope (a feature, subsystem, or behavior)
2. Ask: "What decisions define how this works?"
3. Build a decision tree

**Example output:**

```
[GOAL: Suspense fallback behavior]
    │
    ├── [DECISION: How should timeout work?]
    │       ├── [DECISION: Configurable per-component?]
    │       │       └── [OPTION: Yes, via prop] (chosen)
    │       └── [DECISION: Default timeout value?]
    │               └── [OPTION: 1000ms] (chosen)
    │
    ├── [DECISION: What happens on fetch failure?]
    │       └── [OPTION: Propagate to error boundary] (chosen)
    │
    └── [DECISION: How do nested Suspense interact?]
            └── [DECISION: Independent or coordinated?]
                    └── [OPTION: Coordinated with parent] (chosen)
```

**Key insight:** This is about the MODEL, not the code. Not "which library" but "what's the behavior."

**Commands:**
```bash
deciduous add goal "Suspense fallback behavior" -c 90
deciduous add decision "How should timeout work?" -c 85
deciduous link 1 2 -r "leads_to"
deciduous add option "1000ms default" -c 90
deciduous link 2 3 -r "resolved_by"
deciduous status 3 chosen
```

---

## /narratives - The History Mode

**Purpose:** Understand how the system evolved to its current state.

**When to use:**
- Investigating why something works a certain way
- Before proposing changes (to avoid repeating past mistakes)
- Building institutional memory

**The core insight:** Narratives are conceptual, not tied to commits.

**How it works:**

1. Look at the current system and ask "how did this get this way?"
2. Infer narratives from the design (hybrid systems suggest pivots)
3. Find evidence (commits, PRs, docs) to support the narrative
4. Identify pivots - where the model changed

**Example output:**

```markdown
## Authentication
> How users prove their identity to the system.

**Current state:** Hybrid - JWT for API, sessions for web.

**Evolution:**
1. Started with JWT everywhere - stateless, simple
2. **PIVOT:** Mobile hit 4KB cookie limits with JWT payloads
3. Added sessions for web, kept JWT for API

**Why the pivot:** JWT tokens contained permissions, grew to 3KB+.
Mobile Safari's cookie limit caused silent auth failures.

**Connects to:** "Rate Limiting" (auth method affects rate limit keys)
```

**Key insight:** Commits are evidence, not the source of truth. The narrative exists at the conceptual level.

---

## /archaeology - Structuring for Query

**Purpose:** Transform narratives into a queryable decision graph.

**When to use:**
- After `/pulse` and/or `/narratives` have captured the understanding
- When you want to query the graph programmatically
- When you want to visualize connections

**How it works:**

1. Read the narratives
2. Map narrative elements to graph nodes
3. Connect with edges
4. Set status (active/superseded/abandoned)

**Mapping rules:**

| Narrative Element | Graph Node |
|-------------------|------------|
| Narrative title | `goal` |
| Design question | `decision` |
| Answer/choice | `option` |
| What was learned | `observation` |
| **PIVOT** | `revisit` |
| Pre-pivot work | Status: `superseded` |
| Cross-narrative link | Edge |

**The revisit pattern:**

Every pivot becomes:
```
[Previous approach]
        │
        ▼
[Observation: what was learned]
        │
        ▼
[Revisit: reconsidering X]
        │
        ▼
[New approach]
```

---

## Core Concepts

### Narratives (not Chains)

Old deciduous used "chains" tied to git branches. This is replaced by **narratives** - semantic groupings that tell coherent stories.

| Old (Chains) | New (Narratives) |
|--------------|------------------|
| Grouped by git branch | Grouped by meaning |
| Automatic, implicit | Explicit, intentional |
| One per branch | Multiple anywhere |
| No cross-links | Connected via revisits |

### The Revisit Node

When a design approach is abandoned and replaced, create a **revisit** node:

```
[Old Decision]              [Observation: why it failed]
        │                              │
        └──────────┬───────────────────┘
                   │
                   ▼
            ╔════════════════════╗
            ║      REVISIT       ║
            ║ "Reconsidering X"  ║
            ╚════════════════════╝
                   │
                   ▼
            [New Decision]
```

The revisit captures:
- WHAT is being reconsidered
- WHY (via linked observations)
- The pivot point between old and new

### Node Status

| Status | Meaning |
|--------|---------|
| `active` | Current truth |
| `superseded` | Replaced by newer approach |
| `abandoned` | Tried and rejected |

```bash
deciduous status <node_id> superseded
```

### Now vs History Queries

```bash
# Now mode - only active nodes
deciduous nodes --status active

# History mode - everything
deciduous nodes

# Just the pivots
deciduous nodes --type revisit

# What was abandoned
deciduous nodes --status superseded
```

---

## Workflows

### Prospective: Building as You Work

Use deciduous in real-time during development:

```bash
# Map the current design question (pulse)
deciduous add goal "API authentication" -c 90
deciduous add decision "Token strategy" -c 85
deciduous link 1 2 -r "leads_to"

# Choose an approach
deciduous add option "JWT tokens" -c 85
deciduous link 2 3 -r "resolved_by"
deciduous status 3 chosen

# Later, when problems emerge
deciduous add observation "JWT too large for mobile"
deciduous link 3 4 -r "discovered"

# Pivot
deciduous add revisit "Reconsidering token strategy"
deciduous link 4 5 -r "forced rethinking"
deciduous status 3 superseded

# New approach
deciduous add decision "Session-based auth"
deciduous link 5 6 -r "new direction"
```

### Retrospective: Understanding Existing Systems

For codebases without deciduous history:

1. **Run /pulse** - Map current design as decisions
2. **Run /narratives** - Understand evolution from code + evidence
3. **Run /archaeology** - Structure into queryable graph

```bash
# Pulse: What decisions define auth today?
deciduous add goal "Auth system" -c 90
deciduous add decision "Token type for API?" -c 85
# ... map current design

# Narratives: How did it evolve?
# (Captured in .deciduous/narratives.md)

# Archaeology: Structure the history
deciduous add observation "Mobile cookie limits discovered"
deciduous add revisit "JWT → Sessions pivot"
deciduous link <pulse_decision> <revisit> -r "evolved from"
```

---

## The "Person in the Room"

The goal is to create **the equivalent of "the person who was in the room when these decisions got made"** - someone who can say:

- **"Wait, this won't work"** - We tried it, here's why it failed (superseded nodes)
- **"We couldn't do X before because Y"** - Constraint captured in observation
- **"But Y doesn't apply anymore because Z"** - New observation invalidates old constraint

### Two types of queries:

**"How does it work?"** (Now)
- `/pulse` → Current design as decision tree
- `deciduous nodes --status active`

**"How did we get here?"** (History)
- `/narratives` → Evolution stories
- `deciduous nodes` → Full graph with superseded paths

---

## Quick Reference

### Skills

```
/pulse       - Map current model as decisions (Now)
/narratives  - Understand evolution (History)
/archaeology - Transform to queryable graph
```

### Node Types

```
goal        - What we're trying to achieve
decision    - A design question
option      - An answer to a decision
action      - Work that was done
outcome     - Result of an action
observation - Something learned
revisit     - Pivot point (connects old → new)
```

### Status

```
active      - Current truth
superseded  - Replaced
abandoned   - Dead end
```

### Commands

```bash
# Add nodes
deciduous add <type> "title" -c <confidence>

# Link nodes
deciduous link <from> <to> -r "reason"

# Set status
deciduous status <id> active|superseded|abandoned

# Query
deciduous nodes --status active    # Now mode
deciduous nodes --type revisit     # Pivot points
deciduous nodes                    # Everything

# Visualize
deciduous serve                    # Web viewer
```

---

## Example: Full Graph

```
[GOAL: API Rate Limiting]
        │
        ▼
[DECISION: How to identify users?]
        │
        ├── [OPTION: IP address] ←── chosen, then superseded
        │           │
        │           ▼
        │   [OBSERVATION: Shared IPs blocking legitimate users]
        │           │
        │           ▼
        │   [REVISIT: IP-based too coarse]
        │           │
        │           └──────────────────────┐
        │                                  │
        └── [OPTION: User ID when auth'd]  │
                    │◄─────────────────────┘
                    │
                    ▼
            [DECISION: What about unauthenticated?]
                    │
                    └── [OPTION: Fall back to IP] (chosen)
```

This graph captures:
- **Now**: User ID when auth'd, IP when not (active options)
- **History**: Started with IP-only, pivoted when it caused problems
- **Why**: Observation explains the pivot
- **Connections**: Revisit links old approach to new
