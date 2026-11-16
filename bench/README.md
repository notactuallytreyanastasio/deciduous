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

## The blind runs: 60 unrelated decisions, nothing in the code to go on

Three of the four scenarios above give the answer away in the code: a memo on
the request context, a clock module, a sessions module. Only `order` does
not, and it is where every miss was. So three more were written like it
(`retry`, `delete`, `limit`): the approach was tried, went wrong and was
backed out, the task asks for the same work again, and nothing in the
repository says what was settled on. Each repository also records 58 to 60
unrelated decisions.

    python3 bench/run.py --scenarios blind --distractors 60 --trials 3 --jobs 6 --model <model>

Sessions that followed the earlier decision, of 12 per arm:

| | none | adr | graph |
|---|---|---|---|
| claude-sonnet-5-5 | 1 | 12 | 12 |
| claude-sonnet-5-5, first prompt wording | 0 | 10, and 2 declined | 12 |
| claude-fable-5-2, first prompt wording | 6 | 12 | 12 |
| cost of the 12 sessions, Sonnet / Fable | $0.68 / $3.05 | $1.02 / $3.95 | $1.19 / $5.78 |

- Now the record matters. Without one Sonnet repeated the rejected approach
  in 23 of 24 sessions. With one, no session of 72 did.
- The graph still does not beat ADR files. Both arms followed the decision
  every time a session made the change, on both models. The graph arm costs
  1.2 times the ADR arm on Sonnet and 1.5 times on Fable.
- Fable needed the record less. With none it followed `retry` and `limit`
  in 3 of 3: it read the revert in `git log`, worked out why a retried charge
  or a per-key limit would have been backed out, and wrote the other thing,
  down to a queue it named `payment-reconcile`. It repeated `order` and
  `delete` in 3 of 3, where the reason is a fact about the project (which
  register the auditors use, what the ledger export joins) and not something
  to be worked out.
- "Declined" is two Sonnet sessions with ADR files that read the decision
  against deleting a customer, changed nothing, and asked. Arguably the best
  answer of all; it is counted apart.
- Sixty ADR files did not bury anything. All 12 Sonnet sessions ran
  `cat docs/decisions/*` and read every one. In the graph arm 11 of 12 ran
  `deciduous` before editing, 3 of them `deciduous why`; the rest ran
  `deciduous nodes | tail`, which finds the decision only because the
  benchmark records it last. The ADR that matters is the highest-numbered
  for the same reason. Neither arm was made to search.

Two mistakes of the benchmark's own, both in these runs:

- The sentence added to stop sessions bundling an edit with a `python3`
  check first read "Nothing can be run in this session", which rules out
  `deciduous` too. It now says there is no Python interpreter. The Fable run
  and the first Sonnet run used the first wording
  (`prompt_suffix` in each result file).
- "Looked before editing" matched `deciduous` and `git log` only at the start
  of a command, and sessions write `git log --stat -3; deciduous nodes`. It
  reported 2 of 12 for a graph arm where the truth was nearer 11. It is fixed,
  and recounted for the last Sonnet run from `before_edit`, which every row
  now carries. For the two runs before it the three "looked" columns are
  undercounts and should be ignored.

## What is wrong with it

- In every repository the decision that matters is the newest one: the
  highest-numbered ADR, the last nodes in the graph. A session that reads
  only the tail finds it.
- An ADR directory of 60 files is 6 kB and is read whole in one command. The
  arm has not been tested at a size where that stops being possible, and
  neither has the graph.

- 10 of the 48 sessions in the hard run, and 1 in the first, ended with the
  target file unchanged while reporting a change: the session wrote the file
  in the same Bash call as a check it was not allowed to run (sessions may
  not run `python3`). They are left out of the table, which shrinks cells to
  1 or 2 sessions. One more died on a connection error.
- Three trials per cell. The numbers support "no difference was seen", not
  "there is none".
- 4 and 24 unrelated decisions. A real graph has hundreds of goals and a real
  ADR directory has dozens of files; neither was approached.
- The first two runs are one model. The blind runs are two.
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
for the first two runs, about $4 each on Sonnet; 36 for a blind run, $2.90 on
Sonnet and $12.78 on Fable. Sessions run in a directory under the system
temp directory with no MCP servers and project settings only, and may read,
edit, and run `deciduous`, `git log/show/diff/status/blame` and `ls`.
`results/*.json` holds every session's final report and target file, so a
number in the table can be traced to what the session wrote.
