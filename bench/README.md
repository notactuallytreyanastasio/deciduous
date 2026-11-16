# Does the record of a decision change what an agent does?

Four small repositories, each with a history: something was tried, it went
wrong for a reason the code does not show, and it was taken back out. A real
Claude Code session is then asked for work where the approach that went wrong
is the obvious one. Each repository is built three ways:

| arm | what the session finds |
|---|---|
| `none` | the code and its git history |
| `adr` | plus `docs/decisions/*.md` and a two-line CLAUDE.md that says to read them |
| `graph` | plus `deciduous init` and the decision in the graph |

The runs below had a fourth arm, `hook`: the graph plus a `PreToolUse` hook
that ran `deciduous why` on the first edit of each file. It measured the same
as `graph`, and the hook has been removed from deciduous; the arm cannot be
built any more and its rows stay in the result files.

The ADR arm is the point. That a record beats no record is not news; the
question is whether the graph beats the markdown file a team would otherwise
write.

## What it found (2026-11-14, Claude Code 2.1.325, claude-sonnet-5-5, 3 trials)

It does not. Sessions that followed the earlier decision, of those whose edit
landed:

| run | none | adr | graph | hook |
|---|---|---|---|---|
| first: 4 unrelated decisions, the right approach in use in a neighbouring file | 8/11 | 12/12 | 12/12 | 12/12 |
| `--hard --distractors 24`: the right approach used nowhere yet | 5/8 | 8/8 | 12/12 | 10/10 |
| cost of the arm's 12 sessions, first / hard | $0.68 / $0.67 | $0.68 / $0.81 | $1.25 / $1.31 | $1.18 / $1.29 |

- With any record, every session whose edit landed followed the decision:
  20 of 20 with ADR files, 24 of 24 with the graph, 22 of 22 with the hook.
- Without one, 13 of 19, and all six misses are one scenario (`order`), the
  only one where neither a neighbouring file nor the helper's name gives the
  answer away. Three scenarios out of four do not discriminate at all.
- The graph arms cost 1.6 to 1.9 times the ADR arm and take about four more
  turns: a larger CLAUDE.md, and sessions that ran `deciduous` commands
  before editing in 10 or 11 of 12 cases, hook or no hook.
- The hook changed nothing measurable. Sessions in the `graph` arm asked the
  graph anyway.

So on these tasks a directory of ADRs is as effective as the graph and
cheaper. What this benchmark does not test is the premise of the ADR arm:
that somebody wrote the ADR. Every arm was handed its record ready-made.

## What is wrong with it

- 10 of the 48 sessions in the hard run, and 1 in the first, ended with the
  target file unchanged while reporting a change: the session wrote the file
  in the same Bash call as a check it was not allowed to run (sessions may
  not run `python3`). They are left out of the table, which shrinks cells to
  1 or 2 sessions. One more died on a connection error.
- Three trials per cell. The numbers support "no difference was seen", not
  "there is none".
- 4 and 24 unrelated decisions. A real graph has hundreds of goals and a real
  ADR directory has dozens of files; neither was approached.
- One model. The pilot ran four sessions on claude-fable-5-2 (`order`, one
  per arm): `none` sorted by id, the other three followed the decision.
- The scorers are regular expressions over one file. The first pilot scored
  a sort by id as a sort by name, and the hard run scored a helper on the
  request context as "other"; both are in `test_checks.py` now. A scorer case
  nobody has written yet is a wrong number nobody has noticed yet.
- "Mentioned the decision" is a word match on the final report and fires on
  sessions that had no record to mention. Ignore it for the `none` arm.

## Running it

    cargo build --release
    python3 bench/test_checks.py
    python3 bench/run.py --trials 3 --jobs 6 --model sonnet
    python3 bench/run.py --hard --distractors 24 --trials 3 --jobs 6 --model sonnet
    python3 bench/run.py --rescore bench/results/<file>.json

Each trial is one `claude -p` session on the logged-in account: 48 sessions
per run, about $4 on Sonnet. Sessions run in a directory under the system
temp directory with no MCP servers and project settings only, and may read,
edit, and run `deciduous`, `git log/show/diff/status/blame` and `ls`.
`results/*.json` holds every session's final report and target file, so a
number in the table can be traced to what the session wrote.
