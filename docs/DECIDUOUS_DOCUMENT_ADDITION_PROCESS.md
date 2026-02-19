# Document Attachments: The Full Process

How to use `deciduous doc` in the real flow of working with an AI assistant. Not just the commands — the thinking, the triggers, and the habits that make documents actually useful for context recovery.

---

## Why Attach Documents?

The decision graph captures *reasoning*. But reasoning often depends on things that aren't text:

- **The screenshot** the user showed you of a bug
- **The architecture diagram** you discussed before choosing an approach
- **The PDF spec** that defined the requirements
- **The Figma export** that drove the UI decisions
- **The error output** you saved before debugging

Without these, a future session sees "chose approach B over approach A" but not *the diagram that made it obvious why*. Documents fill that gap.

---

## The Commands

```bash
# Attach a file to a node
deciduous doc attach <node_id> <file_path>
deciduous doc attach <node_id> <file_path> -d "What this document is"
deciduous doc attach <node_id> <file_path> --ai-describe   # Claude writes the description

# See what's attached
deciduous doc list                    # Everything
deciduous doc list <node_id>          # Documents for one node
deciduous doc show <doc_id>           # Full detail on one document

# Update a description
deciduous doc describe <doc_id> "Better description"
deciduous doc describe <doc_id> --ai  # Let Claude describe it

# Open in your default app
deciduous doc open <doc_id>

# Remove (soft-delete — recoverable)
deciduous doc detach <doc_id>

# Clean up orphaned files on disk
deciduous doc gc
deciduous doc gc --dry-run            # Preview first
```

Files are stored in `.deciduous/documents/` with content-hash names. Same file attached twice = stored once.

---

## When to Attach Documents

Not everything needs a document. Here's the filter:

| Situation | Attach? | Why |
|-----------|---------|-----|
| User shares a screenshot of a bug | **Yes** | The visual context is irreplaceable |
| User drops in a PDF spec | **Yes** | The spec drove the goal — link it |
| User pastes an error message | No | Capture in the node description instead |
| You generate an architecture diagram | **Yes** | Visual evidence of the design at this point |
| User references a Figma file | **Yes** if they export it | External URLs aren't recoverable |
| Code file already in the repo | No | Use `-f "src/auth.rs"` instead |
| Meeting notes about a decision | **Yes** | Context that doesn't live in code |

**The rule**: Attach it if a future session would need to *see* it to understand why a decision was made, and it's not already in the repo.

---

## How the AI Should Use This

The CLAUDE.md behavioral triggers tell the AI when to suggest documents:

```
| User shares an image or screenshot | Ask: "Want me to attach this to the current goal/action node?" |
| User references an external document | Ask: "Should I attach a copy to the decision graph?" |
| Architecture diagram is discussed    | Suggest attaching it to the relevant goal node |
| Files not in the project are dropped in | Attach to the most relevant active node |
```

The AI should **not** prompt on every message. Only when files are directly relevant to a decision node that's in play.

### What "intelligent" use looks like

1. User shares a screenshot → AI asks once, attaches if yes, moves on
2. User drops in a PDF → AI attaches it to the active goal without asking (it's obviously relevant)
3. User mentions "the architecture doc" → AI checks `deciduous doc list` to see if it's already attached
4. During `/recover` → AI runs `deciduous doc list` and mentions any documents on active nodes
5. During `/work` → After creating the goal node, AI checks if reference materials should be attached

---

## Walkthrough: Building a Notification Service

Here's a realistic project showing documents woven into the deciduous workflow from start to finish.

### Day 1: The Kickoff

User drops in a requirements PDF and says:

> Build a notification service. Users can subscribe to topics and receive notifications via email, SMS, or push. Here's the product spec.

```bash
# AI creates the goal with the verbatim prompt
deciduous add goal "Build notification service" -c 90 --prompt-stdin << 'EOF'
Build a notification service. Users can subscribe to topics and receive
notifications via email, SMS, or push. Here's the product spec.
EOF
# Created node 1

# AI attaches the spec PDF to the goal
deciduous doc attach 1 /tmp/notification-service-spec.pdf -d "Product requirements spec from kickoff"
# Attached document 1

# AI maps the first design question
deciduous add option "Fan-out on write (eager delivery)" -c 80
deciduous link 1 2 -r "possible_approach"

deciduous add option "Fan-out on read (pull-based)" -c 80
deciduous link 1 3 -r "possible_approach"

deciduous add option "Hybrid: write for push/SMS, read for email digests" -c 85
deciduous link 1 4 -r "possible_approach"
```

**What happened**: The PDF is now linked to the goal. Any future session that opens node 1 can read the spec and understand exactly what was requested.

### Day 1: The Architecture Discussion

User sketches a diagram and drops it in:

> Here's how I'm thinking about the delivery pipeline. What do you think?

```bash
# AI creates an observation about the architecture
deciduous add observation "User proposed delivery pipeline architecture" -c 85
deciduous link 1 5 -r "architecture_context"

# AI attaches the diagram
deciduous doc attach 5 /tmp/delivery-pipeline-sketch.png --ai-describe
# AI generates: "Hand-drawn architecture diagram showing a message broker
# fanning out to three delivery channels (email via SES, SMS via Twilio,
# push via FCM) with a retry queue feeding back into the broker."
```

**What happened**: The observation node captures *that* there was an architecture discussion. The attached diagram captures *what* was discussed. The AI-generated description means the diagram is searchable and recoverable even without opening the image.

### Day 2: Choosing the Approach

```bash
# After discussing trade-offs, user picks the hybrid approach
deciduous add decision "Chose hybrid fan-out: eager for push/SMS, batched for email" -c 90
deciduous link 4 6 -r "chosen"
deciduous link 2 6 -r "rejected: too expensive at scale for email"
deciduous link 3 6 -r "rejected: latency too high for push/SMS"

# AI starts implementing
deciduous add action "Implementing message broker with topic subscriptions" -c 85 -f "src/broker.rs,src/subscription.rs"
deciduous link 6 7 -r "implementation"
```

No documents needed here — the decision is captured in the graph itself.

### Day 3: The Bug Report

User comes back with a screenshot:

> Push notifications aren't reaching iOS devices. Here's what I'm seeing in the Firebase console.

```bash
# AI creates a new goal for the bug
deciduous add goal "Fix iOS push notification delivery failure" -c 90 --prompt-stdin << 'EOF'
Push notifications aren't reaching iOS devices. Here's what I'm seeing
in the Firebase console.
EOF
# Created node 8

# AI attaches the screenshot
deciduous doc attach 8 /tmp/firebase-console-error.png -d "Firebase console showing 0% delivery rate for iOS, 98% for Android"

# AI investigates and finds the issue
deciduous add observation "APNs certificate expired 3 days ago" -c 95
deciduous link 8 9 -r "root_cause"

deciduous add action "Renewing APNs certificate and updating FCM config" -c 90 -f "config/firebase.json"
deciduous link 8 10 -r "fix"
```

**What happened**: The screenshot is attached to the bug goal. When this bug comes up again in 3 months (maybe the cert expires again), a future session can see the exact Firebase console state and immediately recognize the pattern.

### Day 5: The Pivot

User sends a benchmark report:

> Email delivery via SES is costing us $400/month for just the dev environment. We need to switch to a self-hosted SMTP relay. Here's the cost breakdown.

```bash
# AI attaches the cost report to an observation
deciduous add observation "SES costs $400/mo in dev — unsustainable" -c 95
deciduous doc attach 11 /tmp/aws-cost-breakdown.csv -d "AWS billing CSV showing SES as 60% of dev costs"

# AI creates the pivot
deciduous add revisit "Reconsidering email delivery: SES to self-hosted SMTP" -c 85
deciduous link 11 12 -r "forced_rethinking"
deciduous status 6 superseded

deciduous add decision "Switch to Postfix relay for email delivery" -c 85
deciduous link 12 13 -r "new_approach"
```

**What happened**: The cost CSV is the *evidence* for the pivot. Without it, a future session sees "we switched from SES to Postfix" but not *why*. With the document, the reasoning is grounded in data.

### Day 7: Session Recovery

New session starts. AI runs `/recover`:

```bash
deciduous nodes
deciduous edges
deciduous doc list
git status
```

The recovery report includes:

> **Active goals**: "Build notification service" (node 1), "Fix iOS push delivery" (node 8)
>
> **Attached documents** (5 total):
> - Node 1: notification-service-spec.pdf — Product requirements spec
> - Node 5: delivery-pipeline-sketch.png — Architecture diagram
> - Node 8: firebase-console-error.png — Firebase console error screenshot
> - Node 11: aws-cost-breakdown.csv — AWS billing showing SES costs
>
> **Recent pivot**: Switched from SES to Postfix for email (node 12)
>
> **Open work**: Postfix relay implementation in progress

The documents provide instant context. The AI doesn't need to ask "what were the requirements?" or "why did we switch from SES?" — the answers are attached to the graph.

---

## Document Lifecycle

```
File arrives (user shares, AI generates, dropped in)
    |
    v
deciduous doc attach <node_id> <file> -d "description"
    |
    v
File hashed (SHA-256), copied to .deciduous/documents/
Record created in node_documents table
    |
    v
Available in:
├── deciduous doc list          (CLI)
├── deciduous doc show <id>     (CLI detail)
├── Web viewer detail panel     (visual)
├── GET /api/documents          (API)
├── /recover context reports    (AI sessions)
└── Event sync to teammates     (multi-user)
    |
    v
If no longer needed:
    deciduous doc detach <id>   (soft-delete, recoverable)
    |
    v
If orphaned files pile up:
    deciduous doc gc            (removes unreferenced files from disk)
```

---

## Storage Details

- **Location**: `.deciduous/documents/`
- **Naming**: Content hash (SHA-256) — `a1b2c3d4...` not `architecture.png`
- **Deduplication**: Same file attached to two nodes = one file on disk, two records
- **Soft delete**: `detach` sets `detached_at` timestamp but keeps the file
- **Hard delete**: `gc` removes files with no active references
- **Metadata tracked**: original filename, MIME type, file size, description, who attached it, when

---

## Multi-User Sync

Documents participate in the event-based sync system:

```bash
# Events auto-emit when you attach/detach
deciduous doc attach 42 diagram.png -d "Architecture"
# → AttachDocument event written to .deciduous/sync/

# Teammates pull and rebuild
git pull
deciduous events rebuild
# → Document metadata reconstructed in their local DB
```

**Important**: Event sync transfers *metadata* (filename, hash, description), not the file itself. The file stays in `.deciduous/documents/` which is gitignored. For sharing actual files, include them in the repo or use a shared drive.

---

## Integration Points

### In `/work` transactions

```bash
/work "Build the payment integration"

# Step 1: Goal created
# Step 2: AI checks — "Did the user provide any reference materials?"
#         If yes → deciduous doc attach <goal_id> <file>
# Step 3: Action nodes as work proceeds
# Step 4: Outcome with --commit HEAD
# Step 5: Attach any generated artifacts (diagrams, reports)
```

### In `/recover` sessions

```bash
/recover

# AI runs: deciduous doc list
# Reports: "There are 3 documents attached to active nodes:
#   - requirements.pdf on goal 'Build payment integration'
#   - stripe-flow.png on observation 'Stripe webhook lifecycle'
#   - error-log.txt on goal 'Fix webhook timeout'"
```

### In `/pulse` mapping

```bash
/pulse

# When mapping current design, AI checks:
#   deciduous doc list <goal_id>
# If architecture diagrams exist, references them when building the model
```

### In the web viewer

Select any node → detail panel shows:

```
DOCUMENTS (2)
┌─────────────────────────────────────────┐
│ notification-service-spec.pdf           │
│ 2.1 MB  application/pdf                │
│ Product requirements spec from kickoff  │
├─────────────────────────────────────────┤
│ delivery-pipeline-sketch.png            │
│ 145 KB  image/png                       │
│ Hand-drawn architecture diagram...  (AI)│
└─────────────────────────────────────────┘
```

AI-generated descriptions show an **(AI)** badge.

---

## Anti-Patterns

**Don't attach code files that are in the repo.** Use `-f "src/auth.rs"` on the node instead. Documents are for things *not* tracked in version control.

**Don't attach every screenshot.** If it's a quick debug session and the screenshot won't matter in a week, don't bother. Attach evidence that explains *decisions*, not transient debugging.

**Don't skip descriptions.** A file named `IMG_4392.png` with no description is useless in 3 months. Use `--ai-describe` if you're lazy — it's better than nothing.

**Don't use documents as a substitute for graph structure.** The graph captures *reasoning*. Documents capture *evidence*. A PDF doesn't replace a proper goal → option → decision chain.

---

## Quick Reference Card

```bash
# The basics
deciduous doc attach <node> <file> -d "what this is"
deciduous doc list [node]
deciduous doc show <id>
deciduous doc open <id>

# Maintenance
deciduous doc detach <id>        # Soft-delete
deciduous doc gc --dry-run       # Preview cleanup
deciduous doc gc                 # Remove orphans

# AI descriptions
deciduous doc attach <node> <file> --ai-describe
deciduous doc describe <id> --ai

# In the workflow
/work "task"     → attach reference materials to the goal
/recover         → review documents on active nodes
/pulse           → check for architecture docs
```
