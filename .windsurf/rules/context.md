---
description: Recover context from decision graph on session start - run deciduous nodes and edges to see past decisions
globs:
alwaysApply: false
---

<context_recovery>

# Context Recovery

**Use this at session start to recover from context loss.**

## Quick Context Commands

<commands>
```bash
# See all decisions
deciduous nodes

# See connections
deciduous edges

# Recent command history
deciduous commands

# Git state
git status
git log --oneline -10
```
</commands>

## CRITICAL: Audit Graph Integrity

<integrity_audit>
**Before doing ANY other work, check that nodes are logically connected:**

```bash
# Find nodes with no incoming edges (potential missing connections)
deciduous edges | cut -d'>' -f2 | cut -d' ' -f2 | sort -u > /tmp/has_parent.txt
deciduous nodes | tail -n+3 | awk '{print $1}' | while read id; do
  grep -q "^$id$" /tmp/has_parent.txt || echo "CHECK: $id"
done
```

**Review each flagged node:**
- Root `goal` nodes are VALID without parents
- `outcome` nodes MUST link back to their action/goal
- `action` nodes MUST link to their parent goal/decision
- `option` nodes MUST link to their parent decision

**Fix missing connections:**
```bash
deciduous link <parent_id> <child_id> -r "Retroactive connection - <reason>"
```
</integrity_audit>

## After Recovery

<post_recovery>
1. **Audit graph integrity** - ensure logical connections exist
2. Identify pending/active decisions
3. Note any unresolved observations
4. Check for incomplete action→outcome chains
5. Resume work on the most relevant goal
</post_recovery>

## Remember

<reminder>
The graph survives context compaction. Query it early, query it often.
Log decisions IN REAL-TIME as you work, not retroactively.
CONNECT EVERY NODE LOGICALLY - dangling outcomes break the graph's value.
</reminder>

</context_recovery>
