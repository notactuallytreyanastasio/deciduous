---
trigger: always_on
description: Decision Graph Workflow - Log decisions in real-time using deciduous
---

# Decision Graph Workflow

**THIS IS MANDATORY. Log decisions IN REAL-TIME, not retroactively.**

## The Core Rule

```
BEFORE you do something -> Log what you're ABOUT to do
AFTER it succeeds/fails -> Log the outcome
CONNECT immediately -> Link every node to its parent
AUDIT regularly -> Check for missing connections
```

## Behavioral Triggers - MUST LOG WHEN:

| Trigger | Log Type | Example |
|---------|----------|---------|
| User asks for a new feature | `goal` **with -p** | "Add dark mode" |
| Choosing between approaches | `decision` | "Choose state management" |
| About to write/edit code | `action` | "Implementing Redux store" |
| Something worked or failed | `outcome` | "Redux integration successful" |
| Notice something interesting | `observation` | "Existing code uses hooks" |

## Node Flow Rule: goal -> options -> decision -> actions -> outcomes

- **Goals** lead to **options** (possible approaches)
- **Options** lead to a **decision** (choosing which option)
- **Decisions** lead to **actions** (implementing the choice)
- **Actions** lead to **outcomes** (results)

## Quick Commands

```bash
deciduous add goal "Title" -c 90 -p "User's original request"
deciduous add action "Title" -c 85
deciduous link FROM TO -r "reason"  # DO THIS IMMEDIATELY!
deciduous serve   # View live
deciduous sync    # Export for static hosting
```

## CRITICAL: Link Commits to Actions/Outcomes

```bash
git commit -m "feat: add auth"
deciduous add action "Implemented auth" -c 90 --commit HEAD
deciduous link <goal_id> <action_id> -r "Implementation"
```

## Session Start Checklist

```bash
deciduous nodes           # What decisions exist?
deciduous edges           # How are they connected?
deciduous doc list        # Any attached documents?
git status                # Current state
```
