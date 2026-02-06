---
description: Log a decision to the decision graph
arguments:
  - name: TYPE
    description: "Node type: goal, decision, option, action, outcome, observation, revisit"
    required: true
  - name: TITLE
    description: Title of the node
    required: true
---

# Decision Graph Entry

Create a **$TYPE** node: "$TITLE"

## Command

```bash
deciduous add $TYPE "$TITLE" -c <confidence 0-100>
```

## After creating the node

**IMMEDIATELY** link it to related nodes:

| Node Type | Link To |
|-----------|---------|
| outcome | The action/goal it resolves |
| action | The goal/decision that spawned it |
| option | Its parent decision |
| observation | Related goal/action |
| revisit | The decision being reconsidered |

```bash
deciduous link <from_id> <to_id> -r "reason for connection"
```

## Flags

- `-c, --confidence 0-100` - Confidence level
- `-p, --prompt "..."` - Store verbatim user prompt
- `-f, --files "a.rs,b.rs"` - Associate files
- `--commit HEAD` - Link to current git commit
- `--date "YYYY-MM-DD"` - Backdate for archaeology
