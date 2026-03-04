---
description: Build a deciduous decision graph capturing design evolution from commit history
allowed-tools: *
argument-hint: [repo-path]
---

# Decision Graph Construction

You are building a **deciduous decision graph** - a DAG that captures the evolution of design decisions in a codebase.

**Target repository:** $ARGUMENTS (if provided), otherwise the current directory.

Use the `deciduous` CLI to build the graph.

**CRITICAL: Only use information from the repository itself (commits, code, comments, tests). Do not use your prior knowledge about the project. Everything must be grounded in what you find in the repo.**

## Commit Exploration

Use a layered strategy to find all relevant commits:

**Layer 1: See all commits.** Start with the full list when building narratives.

```bash
git log --oneline --after="..." --before="..." -- path/
```

**Layer 2: Keyword expansion.** Search for spelling variations and related terms.

**Layer 3: Follow authors.** Check key author's commits +/-1 month from known commits.

**Layer 4: Pull request context.** Use `gh` CLI to find PRs associated with key commits.

## Finding the Story

Look for commits that change **the model** - how the system conceptualizes the problem:

- Existing tests modified (contract changing)
- Data structures replaced or reworked
- Heuristics changed significantly
- New abstractions introduced
- API behavior shifts

## Node Types

| Type | Purpose |
|------|---------|
| **goal** | High-level objective being pursued |
| **decision** | A choice point with multiple possible paths |
| **option** | A possible approach to a decision |
| **observation** | Learning, insight, or new information discovered |
| **action** | Something that was done (must reference a commit) |
| **outcome** | Result or consequence of an action |
| **revisit** | Pivot point where a previous approach is reconsidered |

## CLI Commands

```bash
# Add nodes (returns node ID)
deciduous add goal "Title of the goal"
deciduous add decision "The question or choice point"
deciduous add option "One possible approach"
deciduous add observation "Something learned or discovered"
deciduous add action "Descriptive title of what was done"
deciduous add outcome "What resulted from the action"
deciduous add revisit "Reconsidering previous approach"

# Connect nodes (from -> to means "from leads_to to")
deciduous link <from-id> <to-id>
deciduous link <from-id> <to-id> -r "Why this led to that"

# View/restructure
deciduous nodes           # list all
deciduous edges           # list connections
deciduous unlink <from> <to>   # remove edge
deciduous delete <id>          # remove node and edges
```

## Link Patterns (goal -> options -> decision -> actions -> outcomes)

- `goal -> option` - Goal leads to possible approaches
- `option -> decision` - Options lead to choosing
- `decision -> action` - Chosen option leads to implementation
- `action -> outcome` - Action produces result
- `outcome -> observation` - Result reveals new insight
- `observation -> option` - Insight suggests new approach
- `observation -> revisit` - Insight forces reconsideration
- `revisit -> option` - Pivot leads to exploring new options

## Output

When done, run `deciduous graph > graph.json` to export.
