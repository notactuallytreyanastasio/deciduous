# Archaeology

**Transform narratives into a queryable decision graph.**

Run `/narratives` first. This skill takes conceptual narratives and structures them for querying.

---

## The Relationship

```
Narratives (conceptual)     →    Decision Graph (structural)
"How auth evolved"          →    Nodes + edges you can query
Human-readable stories      →    Machine-traversable graph
```

The narrative is the truth. The graph is a queryable representation of it.

---

## When to Use

When `.deciduous/narratives.md` exists and you want to:
- Query the evolution ("what led to this?")
- Visualize connections between design decisions
- Build the "person in the room" that can answer questions

---

## Process

### 1. Read the narratives

```bash
cat .deciduous/narratives.md
```

For each narrative, you'll create a subgraph.

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

For a narrative like:

```markdown
## Authentication
**Evolution:**
1. Started with JWT everywhere
2. **PIVOT:** Mobile hit cookie limits
3. Added sessions for web, kept JWT for API
```

Build:

```bash
# Root (backdate to when project started)
deciduous add goal "Authentication" -c 90 --date "2023-01-15"
# → id: 1

# First approach (backdate to when it was made)
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

### 4. Connect narratives

For `**Connects to:** "Rate Limiting"`:

Find a meaningful connection point (usually an observation or decision that influenced the other narrative):

```bash
deciduous link <auth_observation> <ratelimit_decision> \
  -r "Auth failures drove rate limit redesign"
```

---

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

The observation captures WHY. The revisit is the decision point. The new approach is what came after.

```bash
# Pattern
deciduous add observation "<what was learned>"
deciduous link <previous_node> <observation> -r "Discovery"

deciduous add revisit "<what's being reconsidered>"
deciduous link <observation> <revisit> -r "Forced rethinking"

deciduous add decision "<new approach>"
deciduous link <revisit> <decision> -r "New direction"

# Mark old path as superseded
deciduous status <previous_node> superseded
```

---

## What NOT to Do

**Don't create nodes for every commit.**
Commits are evidence. If a narrative mentions a commit as evidence, you might reference it (`--commit <hash>`), but don't enumerate commits.

**Don't create implementation nodes.**
The graph is about the MODEL, not the code. "Implemented JWT" is not interesting. "Chose JWT over sessions" is.

**Don't over-structure.**
If a narrative has a simple evolution with no pivots, it might just be: `goal → decision → current state`. That's fine.

---

## Example: Full Transformation

**Narrative:**
```markdown
## API Rate Limiting
> Protecting the API from abuse.

**Current state:** Redis-based, per-user, auth-aware tiers.

**Evolution:**
1. No rate limiting initially
2. **PIVOT:** Bot abuse caused outages → Added IP-based throttling
3. **PIVOT:** Legitimate users on shared IPs blocked → Per-user limits
4. **PIVOT:** Auth failures as abuse vector → Auth-aware tiers

**Connects to:** "Authentication"
```

**Graph:**
```bash
# Use --date to place nodes at their historical point
deciduous add goal "API Rate Limiting" -c 90 --date "2023-02-01"
# → 1

deciduous add decision "No rate limiting" -c 70 --date "2023-02-01"
deciduous link 1 2 -r "Initial state"

# Pivot 1
deciduous add observation "Bot abuse causing service outages"
deciduous link 2 3 -r "Problem discovered"

deciduous add revisit "Need rate limiting"
deciduous link 3 4 -r "Abuse forced action"

deciduous add decision "IP-based throttling"
deciduous link 4 5 -r "First solution"

deciduous status 2 superseded

# Pivot 2
deciduous add observation "Legitimate users on shared IPs getting blocked"
deciduous link 5 6 -r "Collateral damage"

deciduous add revisit "IP-based approach too broad"
deciduous link 6 7 -r "Rethinking granularity"

deciduous add decision "Per-user rate limits"
deciduous link 7 8 -r "More precise"

deciduous status 5 superseded

# Pivot 3
deciduous add observation "Auth failures used to bypass rate limits"
deciduous link 8 9 -r "New abuse pattern"

deciduous add revisit "Rate limiting needs auth awareness"
deciduous link 9 10 -r "Security gap"

deciduous add decision "Auth-aware tier system"
deciduous link 10 11 -r "Current approach"

deciduous status 8 superseded

# Connect to Auth narrative
# deciduous link <auth_node> 9 -r "Auth design affected rate limiting"
```

---

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

---

## The "Person in the Room"

The goal is to build a graph that can answer:

- **"Why does it work this way?"** → Trace from current state back through revisits
- **"What did we try before?"** → Look at superseded nodes
- **"Can we change X?"** → Check what depends on X via edges
- **"We should do Y"** → "We tried that, here's why it failed" (superseded + observation)

The graph is the institutional memory. The narratives are the source. The commits are just footnotes.
