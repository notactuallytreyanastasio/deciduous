---
description: Start a work transaction with decision graph logging
arguments:
  - name: GOAL
    description: The goal you're working towards
    required: true
---

# Work Transaction

You are starting a work transaction for: **$GOAL**

## Required Steps

1. **Create a goal node** (if this is new work):
   ```bash
   deciduous add goal "$GOAL" -c 90 --prompt-stdin << 'EOF'
   <paste the user's original request here>
   EOF
   ```

2. **Before any code changes**, create an action node:
   ```bash
   deciduous add action "What you're about to do" -c 85
   deciduous link <goal_id> <action_id> -r "Implementation step"
   ```

3. **After successful changes**, create an outcome:
   ```bash
   deciduous add outcome "What was accomplished" -c 95 --commit HEAD
   deciduous link <action_id> <outcome_id> -r "Completed"
   ```

## Rules
- NEVER edit files without an action node
- ALWAYS link commits to the graph
- Capture verbatim user prompts on goal nodes
