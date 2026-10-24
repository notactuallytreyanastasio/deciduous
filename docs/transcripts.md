# The transcript is the log

An agent that logs its own decisions as it works is an agent that can forget
to. The transcript cannot forget. Claude Code writes every session to a JSONL
file under `~/.claude/projects/<slug>/`: every prompt you typed, every file the
agent edited, every command it ran and what came back. `deciduous transcript
import` reads that file and derives the graph from it.

```
$ deciduous transcript list
/Users/bg/.claude/projects/-Users-bg-code-deciduous
new      b30c9d4b    1 prompts  2026-10-24  Deciduous file-first memory feedback
imported cd5135a8    1 prompts  2026-10-17  Postgres port 5432 docs
new      328f8485   68 prompts  2026-09-21  Remote postgres code upstream

$ deciduous transcript import 328f8485
Imported Remote postgres code upstream (328f8485, 68 prompts)
  goals 68 · actions 119 · outcomes 67 proven, 41 inferred · observations 7 · edges 301
  50 files hashed · 52 deciduous MCP writes skipped (already in the graph)
  root goal #13: deciduous show 13
```

That session was three weeks of work on the shared server. The agent logged
nine nodes through MCP while it ran. The transcript held 119 edits and
commits, 67 build and test results, and 68 verbatim prompts.

## What a node is derived from

Every derived node says in `metadata.evidence` what it is backed by:

| node | derived from | evidence |
|------|--------------|----------|
| `goal` | a prompt you typed, kept verbatim in `metadata.prompt` | proven |
| `action` | a run of `Edit` / `Write` / `MultiEdit` calls, or a shell command Claude Code saw change files | proven |
| `action` | a `git commit`, with the hash from `[branch sha]` or the `git log` that followed | proven |
| `outcome` | a build, test or lint run (`cargo test`, `npm run build`, `pytest`, `mix compile`, `make`...) and its exit status | proven |
| `outcome` | `gh pr create` printing the PR's URL | proven |
| `outcome` | the assistant's closing text for a turn that did things | inferred |
| `observation` | the assistant's closing text for a turn that only answered | inferred |

**Proven** means the transcript recorded the thing happening: a tool call and
its result, or your own words. **Inferred** means the assistant said so. The
two are kept side by side on purpose. A turn ends with the assistant writing
"all tests pass"; the `cargo test` outcome next to it says `12 passed, 1
failed`. The second is the one to believe, and the graph says which is which.

```
$ deciduous show 42
Node #42 outcome
Title: cargo test passed
Description: 212 passed, 0 failed
  Evidence: proven transcript, session 328f8485, 1 tool call(s), exit ok

$ deciduous show 67
Node #67 outcome
Title: Both built, tested against the live server, and shipped.
  Evidence: inferred transcript, session 328f8485
```

What the assistant said *before* an edit run becomes that action's
description: the stated intent sits next to the proof of what was done.

## The evidence trail without a filesystem watcher

The question "what did the agent actually read before it changed this?" does
not need `inotify`. The transcript already records every `Read` call. Each
action lists the files read since the previous action in `metadata.consulted`:

```
Node #18 action
Title: Edit src/db.rs, src/main.rs
  Files: src/db.rs (git:adbe7bd4...), src/main.rs (git:5a0b2d8d...)
  Consulted: src/records.rs, src/schema.rs, tests/cli_integration.rs
  Evidence: proven transcript, session cd5135a8, 3 tool call(s)
```

Files the action touched are hashed at import time into
`metadata.file_hashes`. The hash is the blob id `git hash-object` gives, so
`git log --find-object=<id>` finds the commit that holds that exact content.
Without git it is `sha256:<hex>`. `hashed_at` says when. Run the import from a
session-end hook and the hashes are the state the agent left; run it later and
they are the state now, which is also an answer.

## Idempotent, and the same on every machine

Change ids are derived from the session id and the entry's uuid, so importing
a transcript twice creates nothing the second time, and two people importing
the same transcript get the same nodes, which `deciduous sync` then merges as
one.

```
$ deciduous transcript import 328f8485
Imported Remote postgres code upstream (328f8485, 68 prompts)
  goals 0 · actions 0 · outcomes 0 proven, 0 inferred · observations 0 · edges 0
  301 already present · 50 files hashed
```

## Structure

One session is one tree. The first prompt is the root goal, titled with the
name Claude Code gave the session. Every later prompt is a goal under it
(`prompt N of the session`). Under each goal: its actions, and under each
action the command outcomes that followed it. The closing report hangs off
the goal, beside the proven outcomes, not under one of them.

A check that ran eight times in a row with the same result is one node with
`metadata.runs = 8` and the last run's output, not eight nodes.

## What is not derived

**Decisions and options.** Whether the assistant chose one approach over
another is a judgement about prose, not a fact in a tool result. The material
for that judgement is in the graph (each action's description is what the
assistant said it was about to do); the pass that turns it into `option` and
`decision` nodes belongs to an agent reading the imported graph, and
`capture_conversation_turn` is the tool it writes with.

**Subagent work.** An `Agent` call is counted and skipped. Its transcript is a
separate file under `<session>/subagents/`, and importing it as its own tree
is not built yet.

**Reads, searches and ordinary commands.** `ls`, `grep`, `git status` leave no
node; the `Read` calls are kept only as the `consulted` list of the next
action.

**Writes the agent already made to the graph.** An `mcp__deciduous__*` call,
or `deciduous add` run in the shell, is skipped: those nodes are already
there.

## Running it at the end of every session

A Claude Code `SessionEnd` hook receives the transcript path on stdin. Put this
in `.claude/settings.json` and the agent never has to log anything for the
session to be in the graph:

```json
{
  "hooks": {
    "SessionEnd": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "jq -r .transcript_path | xargs deciduous transcript import --no-hash >/dev/null 2>&1 || true"
          }
        ]
      }
    ]
  }
}
```

`--no-hash` because the session may have ended mid-edit and a hash of a
half-written file is not worth keeping. Hash on a later import, or leave it
on and accept that.

## Reference

```
deciduous transcript import <PATH|SESSION_ID>... [--dry-run] [--no-hash] [--json]
deciduous transcript list [--dir DIR] [--json]
```

A session id is any prefix of the file's name under this project's transcript
directory (`~/.claude/projects/<slug>/`, the slug being the project's absolute
path with every character outside `[A-Za-z0-9]` replaced by `-`). `--dry-run`
prints the counts and writes nothing. `--json` prints the report as JSON, one
object per transcript.

Only Claude Code's format is read. A transcript from another assistant is not
refused, it is simply a file with no user prompt in it, and the import says so
by name.
