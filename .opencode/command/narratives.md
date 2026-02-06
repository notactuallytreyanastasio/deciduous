---
description: Understand how a system evolved - narratives are the source of truth
arguments:
  - name: FOCUS
    description: "What part of the system to trace (e.g., 'auth', 'caching', 'API')"
    required: true
---

# Narrative Tracking

**Narratives are the source of truth. Commits are just evidence.**

## The Core Insight

Don't start with commits. Start with understanding.

A narrative is: *"The story of how one piece of the system's design evolved."*

Focus area: **$FOCUS**

## Process

### 1. Understand the system first

Before looking at git:

```bash
# Read the code
cat README.md
ls src/
```

Ask: **What are the major pieces of this system?**

### 2. Identify narratives from the design

Look at the current system and ask:

- "How did this get this way?"
- "Why is this done like this?"
- "What's the story behind this design?"

**Write down the narratives you can INFER from the code.** You don't need commits yet.

### 3. Find evidence (optional)

Now, IF you want supporting evidence, look at git:

```bash
git log --oneline --all -- src/$FOCUS/
git log --oneline --grep="$FOCUS"
```

### 4. Look for pivots

The most valuable thing in a narrative is: **when did the model change?**

Signs of a pivot:
- Two approaches coexisting (migration in progress)
- Comments explaining "we used to do X"
- Config for old + new system
- Deprecation warnings

### 5. Find the "why" for pivots

Sources:
- PR descriptions
- Commit messages around the change
- Issue discussions
- Architecture decision records

## Output Format

Write to `.deciduous/narratives.md`:

```markdown
# Narratives

## <Name>
> <One sentence: what this piece of the system does>

**Current state:** <How it works today>

**Evolution:**
1. <First approach> - <why>
2. **PIVOT:** <what changed> - <why it changed>
3. <Current approach> - <why this is better>

**Evidence:** <Optional: PRs, commits, docs>
**Connects to:** <Other narratives this influenced>
**Status:** active | superseded | abandoned
```

After collecting narratives, run `/archaeology` to transform them into a queryable graph.
