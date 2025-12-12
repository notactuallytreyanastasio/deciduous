In a subagent, analyze the Claude Code session transcript at: $ARGUMENTS

Read the JSONL file and extract all implicit decisions, goals, actions, and outcomes. For each one:

1. Identify the node type (goal, decision, action, outcome, observation)
2. Determine an appropriate title
3. Estimate a confidence score (0-100)
4. Identify relationships between nodes

Then execute the appropriate deciduous commands:
- `deciduous add <type> "title" -c <confidence>`
- `deciduous link <from> <to> -r "reason"`

Start with root goals (user requests), then work through decisions and actions that flowed from them.

Focus on substantive decisions, not routine operations. A "decision" is a choice between alternatives. An "action" is implementation work. An "outcome" is a result.
