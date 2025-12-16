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

# Filter by current branch (useful for feature work)
deciduous nodes --branch $(git rev-parse --abbrev-ref HEAD)

# See connections
deciduous edges

# Recent command history
deciduous commands

# Git state
git status
git log --oneline -10
```
</commands>

## Branch Configuration

<branch_config>
Check `.deciduous/config.toml` for branch settings:
```toml
[branch]
main_branches = ["main", "master"]  # Which branches are "main"
auto_detect = true                    # Auto-detect branch on node creation
```

**Branch-scoped context**: When on feature branches, use `--branch` filter to see only relevant decisions.
</branch_config>

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

## Multi-User Sync

<multi_user_sync>
If working in a team, check for and apply patches from teammates:

```bash
# Check for unapplied patches
deciduous diff status

# Apply all patches (idempotent - safe to run multiple times)
deciduous diff apply .deciduous/patches/*.json

# Preview before applying
deciduous diff apply --dry-run .deciduous/patches/teammate-feature.json
```

Before pushing your branch, export your decisions:

```bash
# Export your branch's decisions as a patch
deciduous diff export --branch $(git rev-parse --abbrev-ref HEAD) \
  -o .deciduous/patches/$(whoami)-$(git rev-parse --abbrev-ref HEAD).json

# Commit the patch file
git add .deciduous/patches/
```
</multi_user_sync>

## Remember

<reminder>
The graph survives context compaction. Query it early, query it often.
Log decisions IN REAL-TIME as you work, not retroactively.
CONNECT EVERY NODE LOGICALLY - dangling outcomes break the graph's value.
Share your decisions via patches for teammates.
</reminder>

</context_recovery>
