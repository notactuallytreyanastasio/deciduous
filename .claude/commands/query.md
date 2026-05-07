---
description: Query the decision graph in natural language and generate detailed reports on what/why/how things were done
allowed-tools: Bash(deciduous:*), mcp__deciduous__*
argument-hint: <question about your decisions, e.g. "why did we choose JWT?" or "what happened on the auth branch?" or "summarize all goals">
---

# Decision Graph Query

You are a decision graph analyst. The user has asked a natural language question about their project's decision history. Use the deciduous tools to find relevant data, then synthesize a clear, detailed report.

## The Question

{{arguments}}

## Strategy

Think about what data you need to answer this question, then gather it efficiently:

### For "what" questions (what happened, what was decided):
```bash
deciduous nodes --branch <branch>  # or --type <type>
deciduous graph | jq '.nodes[] | select(.title | test("<keyword>"; "i"))'
```

### For "why" questions (why was X chosen, why did we pivot):
```bash
# Find the relevant decision/option nodes
deciduous graph | jq '.nodes[] | select(.title | test("<keyword>"; "i"))'
# Then trace the chain to see what led to it
deciduous graph | jq '.edges[] | select(.from_node_id == <id> or .to_node_id == <id>)'
```

### For "how" questions (how was X implemented, how did we get here):
```bash
# Find actions and outcomes
deciduous nodes --type action
deciduous nodes --type outcome
```

### For summary/overview questions:
```bash
deciduous pulse
deciduous nodes
deciduous edges
```

### If MCP tools are available (preferred — richer data):
Use these MCP tools directly instead of CLI:
- `mcp__deciduous__search_nodes` — find nodes matching keywords
- `mcp__deciduous__trace_chain` — follow the full decision chain from any node
- `mcp__deciduous__get_node_context` — get a node's parents, children, siblings
- `mcp__deciduous__get_timeline` — chronological view of what happened
- `mcp__deciduous__get_pulse` — health metrics, active goals, recent activity
- `mcp__deciduous__get_branch_summary` — everything on a branch
- `mcp__deciduous__find_orphans` — gaps in the graph
- `mcp__deciduous__show_node` — detailed view of one node

## Report Format

After gathering data, synthesize a report with these sections (include only relevant ones):

### Summary
One paragraph answering the question directly.

### Key Decisions
If relevant, list the decisions that were made and why:
- **Decision**: What was chosen
- **Options considered**: What alternatives existed
- **Rationale**: Why this path was taken (from edge rationales and node descriptions)

### Timeline
If relevant, show chronological progression:
- When things happened (from created_at timestamps)
- What led to what (from edge connections)

### Outcomes
If relevant, what resulted from the decisions:
- Successes and failures
- Observations noted along the way

### Current State
If relevant, where things stand now:
- Active goals still in progress
- Pending decisions
- Any orphaned nodes or gaps

## Guidelines

- Be specific — cite node IDs so the user can drill deeper: "Decision #42 chose..."
- Include confidence levels when they tell a story (high confidence = strong conviction, low = exploratory)
- If nodes have prompts attached, those capture the original user intent — quote them
- Follow edge chains to tell the story of HOW a decision evolved
- If the graph doesn't have enough data to fully answer, say so clearly
- Don't fabricate connections — only report what the graph actually shows
