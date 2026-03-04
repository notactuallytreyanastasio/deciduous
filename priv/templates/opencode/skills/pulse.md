# Pulse

**Map the current model as decisions. No history, just now.**

## Step 1: Get current state

```bash
deciduous pulse
```

Review the report: active goals, coverage gaps, orphan nodes.

## Step 2: Pick a scope

What part of the system are you taking the pulse of?

- A feature ("Suspense fallback behavior")
- A subsystem ("Authentication")
- A boundary ("API request lifecycle")

## Step 3: Ask "What decisions define this?"

Read the code. For the thing you're scoping, ask:

> "What design questions had to be answered for this to work?"

Not implementation questions ("which library?") - model questions ("what's the behavior?")

## Step 4: Build the goal -> options -> decisions

```bash
# Create the root goal
deciduous add goal "<Scope>: <Core question>" -c 90

# Add options (possible approaches from the goal)
deciduous add option "<Possible approach>" -c 85
deciduous link <goal> <option> -r "possible_approach"

# When an option is chosen, create a decision
deciduous add decision "Chose <approach>" -c 90
deciduous link <option> <decision> -r "chosen"
```

If a question is still open, leave it as option nodes without a decision.

## Step 5: Review

```bash
# Check the pulse again to see what's mapped
deciduous pulse

# View visually
deciduous serve
```

## Decision Criteria

- **Worth capturing?** Does it define BEHAVIOR, not implementation?
- **How deep?** Stop when decisions become implementation details
- **Option vs Decision?** Option = possible approach. Decision = choosing which option.

## Connecting to History

Pulse gives you the "Now". For history, run `/narratives` then `/archaeology`.
