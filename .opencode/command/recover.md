---
description: Recover context from decision graph and recent activity
arguments:
  - name: FOCUS
    description: Optional focus area to filter by
    required: false
---

# Context Recovery

Recovering context from the decision graph.

## Steps

1. **Check recent nodes**:
   ```bash
   deciduous nodes --branch $(git branch --show-current 2>/dev/null || echo main) | head -30
   ```

2. **Check graph connections**:
   ```bash
   deciduous edges | tail -20
   ```

3. **Check recent commands**:
   ```bash
   deciduous commands --limit 10
   ```

4. **Check git status**:
   ```bash
   git status
   git log --oneline -10
   ```

5. **Audit for orphan nodes** (nodes without connections):
   - Every outcome should link to an action
   - Every action should link to a goal
   - Only root goals should be orphans

Report what you find and any gaps that need attention.
