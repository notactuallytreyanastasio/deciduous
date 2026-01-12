# Pulse

**Map the current model as decisions. No history, just now.**

---

## What This Is

Pulse captures the current heartbeat of a system - what decisions define how it works TODAY.

Not how it evolved. Not what was tried before. Just: *"What are the design decisions that make this system work the way it does?"*

---

## When to Use

- Understanding an unfamiliar codebase
- Documenting the current architecture
- Before making changes (know what decisions you might affect)
- Explaining a system to someone new
- When you don't care about history, just current state

---

## Process

### 1. Pick a scope

What part of the system are you taking the pulse of?

- A feature ("Suspense fallback behavior")
- A subsystem ("Authentication")
- A boundary ("API request lifecycle")

### 2. Ask: "What decisions define this?"

Read the code. For the thing you're scoping, ask:

> "What design questions had to be answered for this to work?"

Not implementation questions ("which library?") - model questions ("what's the behavior?")

**Examples:**
- "When should the fallback show?"
- "How should nested components interact?"
- "What happens on timeout?"
- "How are errors handled?"

### 3. Create the goal node

```bash
deciduous add goal "<Scope>: <Core question>" -c 90
```

Example:
```bash
deciduous add goal "Determine when and whether to show Suspense fallback" -c 90
```

### 4. Map the decisions

For each design question you identified:

```bash
deciduous add decision "<Design question>" -c <confidence>
deciduous link <parent> <decision> -r "leads_to"
```

Decisions can spawn other decisions:
```bash
# Root goal
deciduous add goal "Suspense fallback behavior" -c 90
# → 1

# Top-level decisions
deciduous add decision "How should timeout thresholds work?" -c 85
deciduous link 1 2 -r "leads_to"

deciduous add decision "What happens when fetch fails?" -c 85
deciduous link 1 3 -r "leads_to"

deciduous add decision "How should nested Suspense interact?" -c 85
deciduous link 1 4 -r "leads_to"

# Sub-decisions (questions that arise from parent decisions)
deciduous add decision "Should timeout be configurable per-component?" -c 80
deciduous link 2 5 -r "leads_to"

deciduous add decision "What's the default timeout value?" -c 80
deciduous link 2 6 -r "leads_to"
```

### 5. Add answers where known

If a decision has a clear answer in the current system:

```bash
deciduous add option "<The answer/choice>" -c 90
deciduous link <decision> <option> -r "resolved_by"
deciduous status <option> chosen
```

If a decision is still open or unclear, leave it as just the decision node.

---

## The Output

A decision tree showing the current model:

```
[GOAL: Suspense fallback behavior]
    │
    ├── [DECISION: How should timeout work?]
    │       ├── [DECISION: Configurable per-component?]
    │       └── [DECISION: Default timeout value?]
    │               └── [OPTION: 1000ms] (chosen)
    │
    ├── [DECISION: What happens on fetch failure?]
    │       └── [OPTION: Propagate to error boundary] (chosen)
    │
    └── [DECISION: How do nested Suspense interact?]
            ├── [DECISION: Should parent wait for children?]
            └── [DECISION: Independent or coordinated?]
```

---

## Decision Criteria

**Is this a decision worth capturing?**
- Does it define BEHAVIOR (not implementation)? → Yes
- Would changing it change how users experience the system? → Yes
- Is it a choice that could have gone differently? → Yes
- Is it just "how the code is organized"? → No

**How deep to go?**
- Stop when decisions become implementation details
- Stop when the answer is obvious/forced (no real choice)
- Stop when you've captured what someone needs to understand the model

**Decision vs Option?**
- Decision = the question ("How should timeout work?")
- Option = an answer ("Use 1000ms default")

---

## Example: API Rate Limiting Pulse

```bash
# Goal
deciduous add goal "API rate limiting behavior" -c 90
# → 1

# Core decisions
deciduous add decision "What identifies a user for rate limiting?" -c 85
deciduous link 1 2 -r "leads_to"

deciduous add decision "What are the rate limit thresholds?" -c 85
deciduous link 1 3 -r "leads_to"

deciduous add decision "What happens when limit is exceeded?" -c 85
deciduous link 1 4 -r "leads_to"

# Answers for decision 2
deciduous add option "User ID when authenticated, IP when not" -c 90
deciduous link 2 5 -r "resolved_by"
deciduous status 5 chosen

# Sub-decisions for decision 3
deciduous add decision "Different limits for different endpoints?" -c 80
deciduous link 3 6 -r "leads_to"

deciduous add decision "Different limits for different user tiers?" -c 80
deciduous link 3 7 -r "leads_to"

# Answer for decision 4
deciduous add option "Return 429 with Retry-After header" -c 90
deciduous link 4 8 -r "resolved_by"
deciduous status 8 chosen
```

---

## Connecting to History Later

Pulse gives you the "Now". If you later want to add "How we got here":

1. Run `/narratives` to understand the evolution
2. Create `revisit` nodes that connect old decisions to current ones
3. Mark superseded approaches

The pulse becomes the destination that history leads to.

```
[Old decision] → [Observation] → [Revisit] → [Current decision from pulse]
     (history)      (history)     (pivot)         (now)
```

---

## Quick Reference

```bash
# Start with a goal
deciduous add goal "<What aspect of the system?>" -c 90

# Add decisions (the questions)
deciduous add decision "<Design question?>" -c 85
deciduous link <parent> <decision> -r "leads_to"

# Add answers where known
deciduous add option "<The answer>" -c 90
deciduous link <decision> <option> -r "resolved_by"
deciduous status <option> chosen

# View the pulse
deciduous serve
```

---

## The Mindset

You're a doctor taking the pulse of a system.

- What's the heartbeat? (core behavior)
- What decisions keep it alive? (design choices)
- What would happen if you changed X? (dependencies)

Don't worry about how it got this way. Just understand what it IS.
