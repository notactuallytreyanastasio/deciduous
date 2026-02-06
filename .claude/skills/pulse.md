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

### 4. Map the options

For each design question you identified, create options (possible approaches):

```bash
deciduous add option "<Possible approach>" -c <confidence>
deciduous link <goal> <option> -r "possible_approach"
```

Options come from goals, and decisions come from choosing options:
```bash
# Root goal
deciduous add goal "Suspense fallback behavior" -c 90
# → 1

# Top-level options (possible approaches)
deciduous add option "Timeout-based thresholds" -c 85
deciduous link 1 2 -r "possible_approach"

deciduous add option "Error boundary propagation" -c 85
deciduous link 1 3 -r "possible_approach"

deciduous add option "Nested Suspense coordination" -c 85
deciduous link 1 4 -r "possible_approach"

# When an option is chosen, create a decision node
deciduous add decision "Chose timeout-based approach" -c 90
deciduous link 2 5 -r "chosen"
```

### 5. Add chosen decisions

When an option is chosen in the current system, create a decision to record it:

```bash
deciduous add decision "Chose <approach>" -c 90
deciduous link <option> <decision> -r "chosen"
```

If a question is still open or unclear, leave it as option nodes without a decision.

---

## The Output

A decision tree showing the current model (goal -> options -> decisions):

```
[GOAL: Suspense fallback behavior]
    │
    ├── [OPTION: Timeout-based thresholds]
    │       └── [DECISION: Chose timeout approach] (chosen)
    │               └── [ACTION: Implement timeouts]
    │                       └── [OUTCOME: Timeouts working]
    │
    ├── [OPTION: Error boundary propagation]
    │       └── [DECISION: Chose error boundary] (chosen)
    │
    └── [OPTION: Nested Suspense coordination]
            ├── (not yet decided)
            └── ...
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

**Option vs Decision?**
- Option = a possible approach explored from a goal ("Timeout-based thresholds")
- Decision = choosing which option to pursue ("Chose timeout approach")

---

## Example: API Rate Limiting Pulse

```bash
# Goal
deciduous add goal "API rate limiting behavior" -c 90
# → 1

# Options (possible approaches to explore)
deciduous add option "Identify users by auth token" -c 85
deciduous link 1 2 -r "possible_approach"

deciduous add option "Use IP-based rate limiting" -c 85
deciduous link 1 3 -r "possible_approach"

deciduous add option "Return 429 with Retry-After header on exceed" -c 85
deciduous link 1 4 -r "possible_approach"

# Decision: chose auth-based identification
deciduous add decision "Chose user ID when authenticated, IP when not" -c 90
deciduous link 2 5 -r "chosen"

# Decision: chose 429 response
deciduous add decision "Chose 429 with Retry-After" -c 90
deciduous link 4 6 -r "chosen"

# Sub-options for rate limit thresholds
deciduous add option "Different limits per endpoint" -c 80
deciduous link 1 7 -r "possible_approach"

deciduous add option "Different limits per user tier" -c 80
deciduous link 1 8 -r "possible_approach"
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

# Add options (possible approaches) -- options come from goals
deciduous add option "<Possible approach>" -c 85
deciduous link <goal> <option> -r "possible_approach"

# When an option is chosen, create a decision
deciduous add decision "Chose <approach>" -c 90
deciduous link <option> <decision> -r "chosen"

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
