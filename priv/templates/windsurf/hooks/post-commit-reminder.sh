#!/bin/bash
# post-commit-reminder.sh
# Runs after git commit to remind Cascade to link the commit to deciduous
# Works with: Windsurf (Cascade)
# Uses exit code 2 to ensure Cascade sees the message and acts on it

# Check if deciduous is initialized
if [ ! -d ".deciduous" ]; then
    exit 0
fi

# Read the input JSON from Windsurf
input=$(cat)

# Windsurf format: {"tool_info": {"command_line": "...", "cwd": "..."}}
command=$(echo "$input" | grep -o '"command_line"[[:space:]]*:[[:space:]]*"[^"]*"' | head -1 | sed 's/"command_line"[[:space:]]*:[[:space:]]*"//;s/"$//')

# Fallback: try jq if available
if [ -z "$command" ] && command -v jq &>/dev/null; then
    command=$(echo "$input" | jq -r '.tool_info.command_line // empty' 2>/dev/null)
fi

# Only trigger on git commit commands
if ! echo "$command" | grep -qE '^git commit'; then
    exit 0
fi

# Get the commit info
commit_hash=$(git rev-parse --short HEAD 2>/dev/null)
commit_msg=$(git log -1 --format=%s 2>/dev/null)

# Output reminder (exit 2 ensures Cascade processes this)
cat >&2 << EOF
+===================================================================+
|  DECIDUOUS: Link this commit to the decision graph!               |
+===================================================================+
|  Commit: $commit_hash "$commit_msg"
|                                                                   |
|  Run NOW:                                                         |
|    deciduous add outcome "What was accomplished" -c 95 --commit HEAD
|    deciduous link <action_id> <outcome_id> -r "Implementation complete"
|                                                                   |
|  Or if this was an action (not outcome):                          |
|    deciduous add action "What was done" -c 90 --commit HEAD       |
+===================================================================+
EOF

exit 2
