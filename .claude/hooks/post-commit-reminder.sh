#!/bin/bash
# post-commit-reminder.sh
# Runs after git commit to remind Claude to link the commit to deciduous
# Uses exit code 2 to ensure Claude sees the message and acts on it

# Check if deciduous is initialized
if [ ! -d ".deciduous" ]; then
    exit 0
fi

# Read the input JSON to check if this was a git commit
input=$(cat)
command=$(echo "$input" | grep -o '"command":"[^"]*"' | head -1 | sed 's/"command":"//;s/"$//')

# Only trigger on git commit commands
if ! echo "$command" | grep -qE '^git commit'; then
    exit 0
fi

# Get the commit hash that was just created
commit_hash=$(git rev-parse --short HEAD 2>/dev/null)
commit_msg=$(git log -1 --format=%s 2>/dev/null)

# Output reminder to stderr (exit 2 ensures Claude sees and processes this)
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

# Exit 2 to ensure Claude processes this as important feedback
exit 2
