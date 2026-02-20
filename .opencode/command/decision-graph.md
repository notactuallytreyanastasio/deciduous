---
description: Build a deciduous decision graph capturing design evolution from commit history
arguments:
  - name: REPO
    description: Path to the repository to analyze (default current directory)
    required: false
---

# Decision Graph Construction

You are building a **deciduous decision graph** - a DAG that captures the evolution of design decisions in a codebase.

**Target repository:** $REPO (if provided), otherwise the current directory.

Use the `deciduous` CLI to build the graph. Run deciduous commands in the current directory (not inside the source repo).

For git commands to explore commit history, use `git -C <repo-path>` to target the source repo.

**CRITICAL: Only use information from the repository itself (commits, code, comments, tests). Do not use your prior knowledge about the project. Everything must be grounded in what you find in the repo.**

## Commit Exploration

Use a layered strategy to find all relevant commits:

**Layer 1: See all commits.** Start with the full list when building narratives.

```bash
git log --oneline --after="..." --before="..." -- path/
```

**Layer 2: Keyword expansion.** Once you have narratives, search for spelling variations and related terms.

**Layer 3: Follow authors.** If a narrative has a key author, check their commits +/-1 month from known commits.

## Finding the Story

Not every commit matters. Look for commits that change **the model** - how the system conceptualizes the problem:

- Existing tests modified (contract changing)
- Data structures replaced or reworked
- Heuristics changed significantly
- New abstractions introduced
- API behavior shifts

## Narrative Tracking

**Don't build the graph as you explore.** First, collect commits into narratives.

Maintain `narratives.md` as you explore:

1. For each significant commit, read `narratives.md`
2. Ask: "Does this commit evolve an existing narrative?"
3. If yes: append the commit to that narrative's section
4. If no: add a new narrative section

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

# Add nodes with descriptions
deciduous add action "Title" -d "Explanation of what happened and why."

# Set status on options
deciduous status <id> rejected    # option that wasn't chosen
deciduous status <id> completed   # option that was chosen

# Connect nodes
deciduous link <from-id> <to-id>
deciduous link <from-id> <to-id> -r "Why this led to that"

# View/restructure
deciduous nodes           # list all
deciduous edges           # list connections
deciduous unlink <from> <to>   # remove edge
deciduous delete <id>          # remove node and edges
```

## Temporal Rule

**Time flows forward. Past influences future, never reverse.**

Options under a decision are alternatives considered _at the same time_. If an approach was tried, failed, and a new approach was designed later - that's a **new decision node**, connected by observations about why the old approach failed.

## Link Patterns (goal -> options -> decision -> actions -> outcomes)

- `goal -> option` - Goal leads to possible approaches
- `option -> decision` - Options lead to choosing
- `decision -> action` - Chosen option leads to implementation
- `action -> outcome` - Action produces result
- `outcome -> observation` - Result reveals new insight
- `observation -> revisit` - Insight forces reconsideration

When a design approach is abandoned and replaced:

```bash
deciduous add observation "JWT too large for mobile"
deciduous add revisit "Reconsidering token strategy"
deciduous link <observation> <revisit> -r "forced rethinking"
deciduous status <old_decision> superseded
```

## Grounding Requirements

1. **Actions must cite commits**: Every action node must reference a real commit SHA in its description
2. **Observations from evidence**: Observations should come from commit messages, code comments, or test descriptions
3. **No speculation**: If you can't find evidence, don't include it
4. **Quote sources**: When possible, quote the actual commit message that supports a node

## Output

When done, run `deciduous graph > graph.json` to export.
