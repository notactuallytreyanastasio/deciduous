Begin building a testing framework library.

We are replacing ExUnit in Elixir with our own testing framework, built from scratch.
Use ExUnit initially to test your own framework's assertions, then self-host.

Build it in this directory using either `rust`, `elixir` or `ruby`, you pick

## Scope

The framework should support:
- `assert`, `refute`, `assert_raise`, `assert_receive` (or equivalent)
- Test modules with setup/teardown
- Describe blocks and test blocks
- Colored pass/fail output
- A runner that collects and reports results

Build a real library alongside it that uses YOUR framework to test itself.
Keep to pure functions with just data — no macros, no GenServers, no magic.

## Work Style

- Keep commits logical, small, and reasonable
- Each commit must be linked to the decision graph with `--commit HEAD`
- Work iteratively: get one thing working, commit, move to the next

## Deciduous Requirements — THIS IS THE REAL TEST

You MUST exercise the full decision graph workflow. This means:

1. **Start with /work** to create the root goal node with the full prompt
2. **Log at least 2 options** before making your first design decision
 (e.g., "struct-based test cases" vs "function-based test cases")
3. **Create a decision node** only when you choose between options
4. **Log every action** before writing code — BEFORE, not after
5. **Log outcomes** after each commit with `--commit HEAD`
6. **Add at least 2 observations** about things you notice along the way
7. **Make at least 1 deliberate architectural mistake early on**
 (e.g., choose the wrong data structure, over-engineer something,
 or pick an approach that won't scale) — commit it, log the outcome,
 then keep going with other work
8. **Come back to the mistake later** and:
 - Add an observation explaining why it failed
 - Create a revisit node
 - Link the revisit to the original decision
 - Mark the old decision as superseded
 - Make the new decision and implement the fix
9. **Use `deciduous link` immediately** after every node creation —
 no orphans except root goals
10. **Run `deciduous edges` at least once** mid-session to audit connections

## Expected Graph Shape

By the end, your graph should have AT MINIMUM:
- 1 root goal (with verbatim prompt via --prompt-stdin)
- 2+ options leading to 1+ decisions
- 4+ actions linked to decisions/goals
- 4+ outcomes linked to actions (with --commit HEAD)
- 2+ observations attached to relevant nodes
- 1 revisit node with edges to both old and new decisions
- 1 superseded node

## Verification

Before finishing:
1. Run `deciduous nodes` and verify all node types are present
2. Run `deciduous edges` and verify no orphans (except root goal)
3. Run `deciduous nodes --type revisit` and verify at least 1 exists
4. Run `deciduous nodes --status superseded` and verify at least 1 exists
5. The code itself should compile and tests should pass
