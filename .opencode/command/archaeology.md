---
description: Transform narratives into a queryable decision graph
arguments: []
---

# Archaeology

**Transform narratives into a queryable decision graph.**

Run `/narratives` first. This skill takes conceptual narratives and structures them for querying.

## The Relationship

```
Narratives (conceptual)     →    Decision Graph (structural)
"How auth evolved"          →    Nodes + edges you can query
Human-readable stories      →    Machine-traversable graph
```

## Process

### 1. Read the narratives

```bash
cat .deciduous/narratives.md
```

### 2. Map narrative → graph

Each narrative becomes a connected subgraph:

| Narrative Element | Graph Element |
|-------------------|---------------|
| Narrative title | `goal` node (the root) |
| Evolution step | `action` or `decision` node |
| **PIVOT** | `revisit` node |
| Pivot "why" | `observation` node (links INTO revisit) |
| Pre-pivot state | Nodes marked `superseded` |
| **Connects to** | Cross-narrative edge |

### 3. Build the subgraph

For a narrative with pivots:

```bash
# Root (backdate to when project started)
deciduous add goal "Authentication" -c 90 --date "2023-01-15"
# → id: 1

# First approach
deciduous add decision "JWT for all auth" -c 85 --date "2023-01-20"
deciduous link 1 2 -r "Initial design"

# What was learned (leads to pivot)
deciduous add observation "Mobile Safari 4KB cookie limit breaking JWT auth"
deciduous link 2 3 -r "Discovered in production"

# The pivot
deciduous add revisit "Reconsidering auth token strategy"
deciduous link 3 4 -r "Cookie limits forced rethink"

# Mark pre-pivot as superseded
deciduous status 2 superseded

# New approach
deciduous add decision "Hybrid: JWT for API, sessions for web"
deciduous link 4 5 -r "New approach"
```

## The Revisit Pattern

Every **PIVOT** in a narrative becomes this structure:

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

## Querying the Graph

After building, you can ask:

```bash
# What's the current state?
deciduous nodes --status active

# What was tried and abandoned?
deciduous nodes --status superseded

# What led to a specific decision?
deciduous edges --to <node_id>

# What are the pivot points?
deciduous nodes --type revisit

# Visual exploration
deciduous serve
```

## The Goal

Build a graph that can answer:

- **"Why does it work this way?"** → Trace from current state back through revisits
- **"What did we try before?"** → Look at superseded nodes
- **"Can we change X?"** → Check what depends on X via edges
- **"We should do Y"** → "We tried that, here's why it failed"
