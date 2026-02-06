---
description: Map the current model as decisions - no history, just now
arguments:
  - name: SCOPE
    description: "What part of the system to map (e.g., 'Authentication', 'API rate limiting')"
    required: true
---

# Pulse

**Map the current model as decisions. No history, just now.**

## What This Is

Pulse captures the current heartbeat of a system - what decisions define how it works TODAY.

Not how it evolved. Not what was tried before. Just: *"What are the design decisions that make this system work the way it does?"*

## Scope

Taking the pulse of: **$SCOPE**

## Process

### 1. Ask: "What decisions define this?"

Read the code. For the thing you're scoping, ask:

> "What design questions had to be answered for this to work?"

Not implementation questions ("which library?") - model questions ("what's the behavior?")

**Examples:**
- "When should the fallback show?"
- "How should nested components interact?"
- "What happens on timeout?"
- "How are errors handled?"

### 2. Create the goal node

```bash
deciduous add goal "$SCOPE: <Core question>" -c 90
```

### 3. Map the decisions

For each design question you identified:

```bash
deciduous add decision "<Design question>" -c <confidence>
deciduous link <parent> <decision> -r "leads_to"
```

### 4. Add answers where known

If a decision has a clear answer in the current system:

```bash
deciduous add option "<The answer/choice>" -c 90
deciduous link <decision> <option> -r "resolved_by"
deciduous status <option> chosen
```

If a decision is still open or unclear, leave it as just the decision node.

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

## View the result

```bash
deciduous serve
```
