---
name: decision-logger
description: |
  Log decisions to the decision graph when working on features, fixing bugs, or making architectural choices.
  USE THIS SKILL when:
  - User asks for a new feature or change (log as goal)
  - Choosing between implementation approaches (log as decision)
  - About to write or modify code (log as action)
  - Completing work or encountering results (log as outcome)
  - Noticing something important about the codebase (log as observation)
---

# Decision Graph Logging

Track every goal, decision, and outcome in the decision graph. This creates persistent memory that survives context loss.

## When to Log (Automatic Triggers)

| Situation | Node Type | Example |
|-----------|-----------|---------|
| User requests new feature | `goal` | "Add user authentication" |
| Choosing between approaches | `decision` | "Choose between JWT vs sessions" |
| Considering an option | `option` | "Use JWT with refresh tokens" |
| About to write/edit code | `action` | "Implementing JWT auth middleware" |
| Work completed or failed | `outcome` | "JWT auth working" or "JWT approach failed" |
| Important observation | `observation` | "Existing code uses cookie-based sessions" |

## Commands

```bash
# Create nodes (always include confidence -c)
deciduous add goal "Title" -c 90 -p "User's exact request"
deciduous add decision "Title" -c 75
deciduous add action "Title" -c 85
deciduous add outcome "Title" -c 90
deciduous add observation "Title" -c 80

# CRITICAL: Link nodes immediately after creation
deciduous link <parent_id> <child_id> -r "Reason for connection"

# After git commits, link to the graph
deciduous add action "Committed feature X" -c 90 --commit HEAD

# View the graph
deciduous nodes
deciduous edges
```

## Rules

1. **Log BEFORE acting** - Create the action node before writing code
2. **Link IMMEDIATELY** - Every node except root goals must have a parent
3. **Capture verbatim prompts** - Use `-p` with the user's exact words for goals
4. **Include confidence** - Always use `-c` flag (0-100)
5. **Log outcomes** - Both successes AND failures get logged

## Confidence Guidelines

- 90-100: Certain, verified, tested
- 75-89: High confidence, likely correct
- 50-74: Moderate confidence, some uncertainty
- Below 50: Experimental, speculative

## The Memory Loop

```
User Request → Log goal with -p
    ↓
Choose Approach → Log decision + options
    ↓
Start Coding → Log action FIRST
    ↓
Complete Work → Log outcome, link to parent
    ↓
Git Commit → Log with --commit HEAD
```

**Remember**: The decision graph is your persistent memory. Log as you work, not after.
