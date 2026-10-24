# Document Attachments: The Full Process

A decision often rests on something that is not text in the repository: the
screenshot of the bug, the diagram drawn before the approach was chosen, the
PDF spec, the cost breakdown that forced the pivot. `deciduous doc` attaches
such files to nodes so a later session can see what the decision was made
from, not only that it was made. This page is the commands, verified against
1.1.0, and the habits that make attachments worth having.

---

## The commands

Every subcommand and flag below is in `deciduous doc --help`.

```bash
deciduous doc attach <node> <file>                  # node: local id, change_id prefix, or server id
deciduous doc attach <node> <file> -d "What this is"
deciduous doc attach <node> <file> --ai-describe     # the claude CLI writes the description

deciduous doc list                                   # every attached document
deciduous doc list <node>                            # one node's
deciduous doc list --include-detached [--json]       # detached ones too
deciduous doc show <doc_id> [--json]

deciduous doc describe <doc_id> "Better description"
deciduous doc describe <doc_id> --ai
deciduous doc describe <doc_id> < description.txt    # text on stdin when omitted

deciduous doc open <doc_id>                          # default application
deciduous doc detach <doc_id>                        # soft delete, recoverable
deciduous doc gc --dry-run                           # files no active record references
deciduous doc gc
```

From a scratch project:

```
$ deciduous doc attach 1 spec.md -d "Rate limit spec from kickoff"
Attached document 1 to node 1 (spec.md)
  Description: Rate limit spec from kickoff

$ deciduous doc show 1
Document Details
  ID:          1
  Node:        1
  Filename:    spec.md
  MIME type:   text/markdown
  Size:        5B
  Hash:        3b92cc255009c1a8541990fd00bcde181bc803a80b234f086647210c078bb7e4
  Storage:     .deciduous/documents/spec.md.3b92cc25
  Attached:    2026-10-24T12:47:21.048730-04:00
  Description: Rate limit spec from kickoff (manual)

$ deciduous doc detach 1
Detached document 1

$ deciduous doc gc --dry-run
1 orphaned files:
  .deciduous/documents/spec.md.3b92cc25
(dry run - no files deleted)
```

## Storage

| | |
|---|---|
| Location | `.deciduous/documents/`, next to the database; not in git (`.deciduous/*` is ignored) |
| File name | `<original filename>.<first 8 hex of the SHA-256>`: `spec.md.3b92cc25`, not a bare hash |
| Deduplication | the same bytes attached to two nodes are one file on disk and two records |
| Record | `node_documents` in the local database: original filename, MIME type, size, full hash, description and whether it was `manual` or `ai`, who attached it and when, `detached_at` |
| Detach | sets `detached_at`; the record stays and `list --include-detached` shows it; the file stays until `gc` |
| `gc` | deletes files no active record references; `--dry-run` lists them first |
| Limits | the local MCP server's `attach_document` reads regular files inside the project up to 25 MiB |

## Sharing

Documents are not in `.deciduous/graph.json`, so they do not travel through
git with the rest of the graph; a teammate who pulls sees the node and not the
file. With a `[remote]`, `deciduous remote push --seed` sends documents the
server lacks, bytes included, and the server stores them once per SHA-256
across workspaces. Attaching does not send; seeding does. For a git-model
project, a file that must be shared belongs in the repository and in the
node's `-f` list instead.

## When to attach

| Situation | Attach? | Why |
|---|---|---|
| The user shares a screenshot of a bug | yes | the visual state is not recoverable any other way |
| The user drops in a PDF spec | yes | it drove the goal |
| The user pastes an error message | no | put it in the node's description |
| You generate an architecture diagram | yes | evidence of the design at that moment |
| The user references a Figma file | yes, if exported | a URL is not a copy |
| A code file already in the repository | no | `-f "src/auth.rs"` on the node |
| Meeting notes behind a decision | yes | context that lives nowhere in the code |

The rule: attach it if a later session would need to see it to understand why
a decision was made, and it is not already in the repository.

## How the agent uses it

The `CLAUDE.md` section `init` writes tells the agent when to offer:

```
| User shares an image or screenshot        | Ask: "Want me to attach this to the current goal/action node?" |
| User references an external document      | Ask: "Should I attach a copy to the decision graph?"           |
| Architecture diagram is discussed         | Suggest attaching it to the relevant goal node                 |
| Files not in the project are dropped in   | Attach to the most relevant active node                        |
```

and not to ask on every message. In practice:

1. A screenshot arrives: ask once, attach on yes, move on.
2. A spec arrives with the request: attach it to the goal without asking.
3. "The architecture doc" is mentioned: `deciduous doc list` first; it may be there.
4. At `/recover`: `deciduous doc list` and name the documents on active nodes.
5. At `/work`: after the goal node, check whether reference material came with it.
6. In `/narratives`: attached diagrams and specs are evidence for a narrative.

## Walkthrough: a notification service

Day 1, the user drops in a requirements PDF:

> Build a notification service. Users can subscribe to topics and receive notifications via email, SMS, or push. Here's the product spec.

```bash
deciduous add goal "Build notification service" -c 90 --prompt-stdin << 'EOF'
Build a notification service. Users can subscribe to topics and receive
notifications via email, SMS, or push. Here's the product spec.
EOF
# Created node 1
deciduous doc attach 1 ~/Downloads/notification-service-spec.pdf -d "Product requirements spec from kickoff"

deciduous add option "Fan-out on write (eager delivery)" -c 80
deciduous link 1 2 -r "possible approach"
deciduous add option "Fan-out on read (pull-based)" -c 80
deciduous link 1 3 -r "possible approach"
deciduous add option "Hybrid: write for push/SMS, read for email digests" -c 85
deciduous link 1 4 -r "possible approach"
```

The user sketches the delivery pipeline and drops the image in:

```bash
deciduous add observation "User proposed delivery pipeline architecture" -c 85
deciduous link 1 5 -r "architecture context"
deciduous doc attach 5 ~/Downloads/delivery-pipeline-sketch.png --ai-describe
```

`--ai-describe` runs the `claude` CLI over the file and stores its description
marked `ai`, so the diagram is findable from `doc list` without opening it.
Without the CLI installed the attach still succeeds and the description is
empty; add one with `describe`.

Day 2, the choice:

```bash
deciduous add decision "Hybrid fan-out over eager and pull-based" -c 90
deciduous link 4 6 -t chosen
deciduous link 2 6 -t rejected -r "too expensive at scale for email"
deciduous link 3 6 -t rejected -r "latency too high for push and SMS"
deciduous add action "Implementing message broker with topic subscriptions" -c 85 -f "src/broker.rs,src/subscription.rs"
deciduous link 6 7 -r "implementation"
```

No document: the decision is in the graph itself.

Day 3, a bug with a screenshot of the Firebase console:

```bash
deciduous add goal "Fix iOS push notification delivery failure" -c 90 --prompt-stdin << 'EOF'
Push notifications aren't reaching iOS devices. Here's what I'm seeing
in the Firebase console.
EOF
# Created node 8
deciduous doc attach 8 ~/Desktop/firebase-console-error.png -d "Firebase console: 0% delivery on iOS, 98% on Android"
deciduous add observation "APNs certificate expired 3 days ago" -c 95
deciduous link 8 9 -r "root cause"
deciduous add action "Renewing APNs certificate and updating FCM config" -c 90 -f "config/firebase.json"
deciduous link 8 10 -r "fix"
```

When the certificate expires again, the screenshot is the pattern to
recognise.

Day 5, a cost report forces a pivot:

```bash
deciduous add observation "SES costs $400/mo in dev" -c 95 -d "SES is 60% of the dev AWS bill"
deciduous doc attach 11 ~/Downloads/aws-cost-breakdown.csv -d "AWS billing CSV, SES line items"
deciduous archaeology pivot 6 "SES costs $400/mo in dev; unsustainable" "Postfix relay for email delivery" -c 85 -r "cost"
```

`archaeology pivot` writes the observation, the revisit and the new decision,
links them, and marks decision 6 superseded. The CSV on observation 11 is the
evidence; without it a later session sees "switched from SES to Postfix" and
not the number.

Day 7, a new session runs `/recover`, which runs `deciduous doc list` among
other things and gets four rows: the spec on goal 1, the sketch on
observation 5, the console screenshot on goal 8, the billing CSV on
observation 11, each with its description. The requirements, the
architecture, the bug's symptom and the pivot's evidence are one command away,
and nobody has to ask "what were the requirements?"

## In the viewer

`deciduous serve` shows a node's documents in its detail panel: filename,
size, MIME type and description, with `(AI)` after descriptions the CLI
wrote. It lists them and stops there: the page has no link to the file, even
though the server answers `GET /api/documents/file/<id>`. To see the file,
`deciduous doc open <id>`.

## Anti-patterns

- **Attaching repository files.** Use `-f` on the node. Documents are for what
  version control does not hold.
- **Attaching every screenshot.** A debugging screenshot that will not matter
  in a week is noise. Attach what explains a decision.
- **No description.** `IMG_4392.png` with no description is useless in three
  months. `--ai-describe` beats nothing.
- **Documents instead of structure.** A PDF does not replace a goal, its
  options and the decision. The graph carries reasoning; documents carry
  evidence.

## Quick reference card

```bash
deciduous doc attach <node> <file> -d "what this is"     # or --ai-describe
deciduous doc list [node] [--include-detached] [--json]
deciduous doc show <id> [--json]
deciduous doc describe <id> "text" | --ai
deciduous doc open <id>
deciduous doc detach <id>
deciduous doc gc --dry-run && deciduous doc gc
deciduous remote push --seed                            # send documents to the server ([remote] projects)
```
