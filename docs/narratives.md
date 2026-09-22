<!-- Generated from content/history.md by scripts/docs/build.mjs. Edit the source. -->

# Reconstruct earlier decisions

An agent team can add useful history to its shared graph before it starts changing an unfamiliar repository. Use commits, tests, issues, and design notes to explain how the current implementation developed. Store the resulting records in the same workspace the team uses for current work.

This is optional groundwork. You can begin with [the team workflow](content/teams.md) and record new decisions as they happen without reconstructing the entire repository.

## Choose a bounded question

Pick a subsystem or a decision that affects the task: why retries have a fixed cap, why an endpoint has two response shapes, or why a dependency was removed. Assign separate investigations to agents only when the questions are independent. Share the source nodes so they can connect findings that overlap.

Use one workspace and each investigator's actual branch. Read existing decisions first to avoid creating a second account of the same change without noticing the first.

## Read the evidence before writing a story

Inspect the relevant history with Git:

```sh
git log --all --oneline -- src/retry.rs tests/retry.rs
git log --all --format=fuller -- src/retry.rs
git show COMMIT_SHA -- src/retry.rs tests/retry.rs
```

Replace the paths and commit placeholder with evidence from the repository. Start with the relevant commit list before narrowing it by keywords. Search for introductions, replacements, renames, and removals; an abandoned approach often explains the surviving one.

A commit proves what changed. It may not prove why. Distinguish an author's stated rationale from your inference. If no source supports the rationale, record the uncertainty instead of filling the gap with a plausible explanation.

## Put the finding in shared memory

Use HTTP MCP tools to create an observation with a useful title, a description of the source evidence, and the full commit hash. Connect it to the goal or decision under investigation. A teammate should be able to follow the node back to the exact diff or test.

Record a historical design choice as a decision only when the evidence supports that choice. Record alternatives as options only when they were considered, not because they are alternatives you can imagine now.

The shared `add_node` tool does not have a backdating argument. Preserve the historical date in the description and commit reference; its creation timestamp records when your team added the node. The optional local CLI supports `--date`, but a local write does not appear in the shared workspace without a separate, reviewed migration.

## Represent a replacement without erasing the old choice

Connect the old decision to the observation that challenged it, then to a revisit and the new decision:

```text
old decision -> observation -> revisit -> replacement decision
```

Mark the old choice `superseded` if it explains a previous implementation. Keep the reason for replacement specific: a failing test, a deployment limit, or a measured cost. Do not model attempts made years apart as options considered at one meeting.

## Connect investigators' findings

An agent studying the retry loop may discover that another agent already traced its timeout requirement. Read that source node, link the dependency, and use `took_from` when the source informed your own work. The [MCP guide](content/mcp.md#borrow-with-a-source) shows the supported borrowing call.

Do not duplicate the entire other investigation inside your description. Link to it and record the new inference or evidence you contributed. Check activity at milestones so the team can change direction before two agents complete the same history pass.

## Review the result

Use `show_node` to inspect claims and sources, `find_orphans` to find missing incoming links, and graph reads to check connections. A clean orphan check cannot establish factual accuracy. Have a second agent compare important claims with the original commits or tests.

Keep a short description of what you did not investigate. That gives the next session a boundary and prevents a partial history from looking complete.

## Existing local archaeology workflows

Older installations may have `/pulse`, `/narratives`, `/archaeology`, or `/decision-graph` command files and local narrative Markdown. They can help structure an investigation, but their generated instructions may call the SQLite CLI. Review them before giving them to a team using shared Postgres.

Use the same evidence discipline while replacing local writes with the shared HTTP tools. Migrate an existing local graph through [upgrading](content/upgrading.md); do not bulk-push an old copy into an active team workspace without review.
