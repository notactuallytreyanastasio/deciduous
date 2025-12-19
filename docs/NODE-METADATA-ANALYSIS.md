# Node Metadata: Deep Analysis of Two Approaches

## Context

This repository's decision graph currently contains **783 nodes** and **688 edges**. The breakdown:

| Node Type | Count | Typical Metadata Needs |
|-----------|-------|------------------------|
| action | 261 | Commit links, implementation notes |
| outcome | 157 | Test results, PR bodies, retrospectives |
| observation | 123 | Context, links to docs, discussion summaries |
| option | 91 | Rationale, trade-off analysis |
| decision | 78 | Options considered, chosen path reasoning |
| goal | 73 | Original user prompts, issue bodies, acceptance criteria |

At the current growth rate, this graph will reach **5,000+ nodes** within months of active use. The metadata system we choose now will be the foundation for years of accumulated context.

---

## The Core Question

How do we attach rich context (PR descriptions, issue bodies, notes, links) to nodes in a way that:
1. Scales to thousands of nodes
2. Remains queryable and navigable
3. Supports real workflows (AI assistants logging decisions, humans reviewing)
4. Integrates with TUI, web viewer, and exports

---

## Approach 1: Key-Value Metadata

**Implementation:** PR #165, nodes 732-748

### Schema
```sql
CREATE TABLE node_metadata (
    id INTEGER PRIMARY KEY,
    node_id INTEGER NOT NULL,
    meta_key TEXT NOT NULL,      -- 'pr_body', 'issue_body', 'notes'
    meta_value TEXT NOT NULL,    -- The actual content
    content_type TEXT NOT NULL,  -- 'markdown', 'json', 'text'
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(node_id, meta_key)    -- ONE value per key per node
);
```

### The Key Insight (Node 738, 85% confidence)
> "Upsert semantics: one value per key prevents duplicates, simpler mental model"

This is **dictionary semantics**. Each node has named slots. You fill them or overwrite them.

### Workflow in Practice

```bash
# Session 1: AI logs a goal with the user's prompt
deciduous add goal "Implement auth" -c 90
deciduous meta set 1 issue_body < issue-42.md

# Session 2: Feature complete, add PR body
deciduous meta set 1 pr_body < pr-105.md

# Session 3: Retrospective
deciduous meta set 1 notes "Took longer than expected due to OAuth complexity"

# Query: What nodes have PR bodies?
deciduous meta list --key pr_body
```

### Long-Term at 5,000+ Nodes

**Strengths:**
- **Predictable schema**: Every node with a `pr_body` has exactly one. No surprises.
- **Fast queries**: `SELECT * FROM node_metadata WHERE meta_key = 'pr_body'` returns one row per node max.
- **Idempotent updates**: Running the same script twice doesn't create duplicates.
- **Standard keys**: Team can agree on `pr_body`, `issue_body`, `notes`, `summary` - everyone knows what to expect.

**Weaknesses:**
- **Single notes problem** (Node 776, 90% confidence): You literally cannot have multiple notes. Today's note overwrites yesterday's.
- **Workarounds are ugly**: `custom:note_2024_01`, `custom:note_2024_02` - loses the ability to query "all notes".
- **No provenance**: When did this PR body get added? Who added it? (Only `updated_at`, not history)

### The Critical Trade-off

Key-Value is **optimized for structured, singular fields**. A node has ONE PR body, ONE issue description, ONE summary. This matches how we think about canonical documentation.

But it **fails for accumulating content**. Session notes, discussion threads, related links - these grow over time. Key-Value forces you to either:
1. Append to a single blob (losing individual timestamps)
2. Use numbered keys (losing queryability)

---

## Approach 2: Document-Oriented Attachments

**Implementation:** PR #167, nodes 749-765

### Schema
```sql
CREATE TABLE node_attachments (
    id INTEGER PRIMARY KEY,        -- Attachment ID for direct reference
    node_id INTEGER NOT NULL,
    attachment_type TEXT NOT NULL, -- 'pr_body', 'note', 'link'
    title TEXT,                    -- Human-readable: "PR #105: Auth Feature"
    content TEXT NOT NULL,
    mime_type TEXT NOT NULL,       -- 'text/markdown', 'application/json'
    source_url TEXT,               -- Link to GitHub, external docs
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
    -- NO unique constraint on (node_id, attachment_type)
);
```

### The Key Insight (Node 754, 90% confidence)
> "Attachments allow multiple items of same type (e.g., multiple notes on one node)"

This is **document store semantics**. Each node has a collection of attachments. You add to the collection.

### Workflow in Practice

```bash
# Session 1: AI logs a goal
deciduous add goal "Implement auth" -c 90
deciduous attach add 1 -t issue_body --title "Issue #42: Add auth" \
  --url "https://github.com/org/repo/issues/42" < issue-42.md

# Session 2: First implementation attempt
deciduous attach add 1 -t note --title "Session 2: OAuth research" \
  "Explored OAuth2 vs JWT. OAuth2 seems better for third-party integrations."

# Session 3: Second session
deciduous attach add 1 -t note --title "Session 3: Implementation" \
  "Implemented OAuth2 flow. Hit CORS issues with callback."

# Session 4: PR merged
deciduous attach add 1 -t pr_body --title "PR #105: Auth Feature" \
  --url "https://github.com/org/repo/pull/105" < pr-105.md

# Query: All notes for this goal
deciduous attach list 1 -t note
# Returns:
#   #3 [note] Session 3: Implementation - Implemented OAuth2 flow...
#   #2 [note] Session 2: OAuth research - Explored OAuth2 vs JWT...
```

### Long-Term at 5,000+ Nodes

**Strengths:**
- **Accumulating content natural**: Each session adds a note. History preserved.
- **Titles provide context**: "Session 3: Implementation" vs just "notes"
- **Source URLs for traceability**: Link back to GitHub issues, PRs, external docs.
- **Temporal ordering**: Attachments have `created_at`, you see the timeline.

**Weaknesses:**
- **ID-based updates** (Node 780, 75% confidence): To update attachment #47, you need to know it's #47.
- **Query complexity**: "Get the PR body for node 603" requires filtering, might return multiple if someone added two.
- **No upsert**: If an automation runs twice, you get duplicate attachments.
- **Schema is looser**: Nothing prevents 5 `pr_body` attachments on one node.

### The Critical Trade-off

Attachments are **optimized for accumulating, timestamped content**. Notes, links, discussion summaries - things that grow over time.

But they **add complexity for canonical fields**. A node should have ONE PR body. Attachments don't enforce this. You need application logic to handle "which one is current?"

---

## Head-to-Head: Real Scenarios

### Scenario 1: Storing PR Context After Merge

**Key-Value:**
```bash
deciduous meta set 603 pr_body < pr-105.md
# Done. One command. Idempotent.
```

**Attachments:**
```bash
deciduous attach add 603 -t pr_body --title "PR #105" --url "https://..." < pr-105.md
# Done. But if you run it again, you get a duplicate.
```

**Winner: Key-Value** - Simpler, idempotent.

---

### Scenario 2: AI Session Notes Over Multiple Days

**Key-Value:**
```bash
# Day 1
deciduous meta set 42 notes "Started implementation. Chose React over Vue."

# Day 2
deciduous meta set 42 notes "Fixed auth flow. Added tests."
# PROBLEM: Day 1 notes are gone!

# Workaround
deciduous meta get 42 notes > /tmp/old.txt
echo "Day 2: Fixed auth flow." >> /tmp/old.txt
deciduous meta set 42 notes < /tmp/old.txt
# Ugly. Loses timestamps. No structure.
```

**Attachments:**
```bash
# Day 1
deciduous attach add 42 -t note --title "Day 1: Setup" \
  "Started implementation. Chose React over Vue."

# Day 2
deciduous attach add 42 -t note --title "Day 2: Auth" \
  "Fixed auth flow. Added tests."

# Both preserved. Query by type.
deciduous attach list 42 -t note
```

**Winner: Attachments** - Natural accumulation, no data loss.

---

### Scenario 3: Linking to Related Resources

**Key-Value:**
```bash
# Need to store multiple links
deciduous meta set 42 links '["https://docs.example.com/auth", "https://github.com/..."]'
# It's JSON. Queryability lost. Adding a link means parse/append/write.
```

**Attachments:**
```bash
deciduous attach add 42 -t link --title "Auth Docs" --url "https://docs.example.com/auth"
deciduous attach add 42 -t link --title "Reference Impl" --url "https://github.com/..."

# Query all links
deciduous attach list 42 -t link
# Each has title, URL, timestamp.
```

**Winner: Attachments** - Links are first-class, not JSON blobs.

---

### Scenario 4: CI/Automation Adding Metadata

**Key-Value:**
```bash
# In CI, after PR merge:
deciduous meta set $NODE_ID pr_body < pr-body.md
# Safe to run multiple times. Idempotent.
```

**Attachments:**
```bash
# In CI, after PR merge:
deciduous attach add $NODE_ID -t pr_body --title "PR #$PR_NUM" < pr-body.md
# DANGER: If CI retries, you get duplicates.
# Need: deciduous attach add --upsert-title "PR #$PR_NUM"  (doesn't exist)
```

**Winner: Key-Value** - Automation-friendly idempotency.

---

## Long-Term Implications

### At 5,000 Nodes with Key-Value

```
node_metadata table: ~15,000 rows (avg 3 keys per node)
- Fast lookups by key
- Predictable storage growth
- BUT: "notes" field becomes a massive blob for active nodes
- Lost history of individual note additions
```

### At 5,000 Nodes with Attachments

```
node_attachments table: ~25,000 rows (avg 5 attachments per node)
- Natural timeline of additions
- Queryable by type
- BUT: Need cleanup of duplicates from automation failures
- More storage (titles, URLs for each attachment)
- ID management complexity
```

---

## The Hybrid Option

Both systems share the same database. You could merge both PRs:

```bash
# Use meta for canonical, singular fields
deciduous meta set 42 pr_body < pr.md
deciduous meta set 42 issue_body < issue.md
deciduous meta set 42 summary "One-line summary"

# Use attach for accumulating content
deciduous attach add 42 -t note --title "Session 1" "Notes..."
deciduous attach add 42 -t link --title "Docs" --url "https://..."
```

### Hybrid Risks (Node 784, 75% confidence)
> "Two systems to learn, maintain, and keep in sync"

- Cognitive load: "Do I `meta set` or `attach add`?"
- Documentation burden: Explaining when to use which
- Possible inconsistency: Some teams use meta for notes, others use attach
- Two tables to migrate, index, backup

### Hybrid Benefits (Node 783, 70% confidence)
> "Could have both - meta for single values, attach for collections"

- Right tool for each job
- No workarounds needed
- Clear semantic separation

---

## Recommendation Framework

### Choose Key-Value If:
1. Your primary use is **canonical fields** (PR body, issue body, summary)
2. You value **idempotent automation** over accumulating history
3. **Simplicity** matters more than flexibility
4. You're okay with **append-to-blob** for notes

### Choose Attachments If:
1. You **accumulate notes over time** across sessions
2. **Titles and source URLs** matter for organization
3. You want a **timeline** of additions, not just current state
4. You'll build **UI for managing duplicates** from automation

### Choose Hybrid If:
1. You have **clear use cases for both** patterns
2. You're willing to **document the separation** clearly
3. **Long-term flexibility** outweighs short-term complexity
4. You have **engineering capacity** to maintain both

---

## My Assessment

Looking at the 783 nodes in this repo:

- **73 goals** - Would benefit from issue_body (singular), but also session notes (accumulating)
- **261 actions** - Commit links (singular), but implementation notes (accumulating)
- **157 outcomes** - PR bodies (singular), test results (could be multiple runs)

The reality is **both patterns exist** in real usage:
- PR body → singular, canonical → Key-Value
- Session notes → accumulating → Attachments
- Related links → collection → Attachments
- Summary → singular → Key-Value

**If I had to pick one:** Attachments with discipline (use one `pr_body` per node by convention).

**If flexibility is acceptable:** Hybrid, with clear guidelines.

---

## Decision Graph Reference

### This Analysis
- **Goal (768):** Analyze and choose between metadata approaches
- **Decision (769):** Which metadata approach to adopt?
- **Options (770-772):** Key-Value, Attachments, Hybrid
- **Observations (773-784):** Pros, cons, use cases for each

### Implementation Details
- **Key-Value (732-748):** Full implementation with 5 observations, 4 actions, 3 outcomes
- **Attachments (749-765):** Full implementation with 5 observations, 4 actions, 3 outcomes

To explore interactively:
```bash
deciduous serve
# Navigate to node 768 for analysis
# Navigate to node 732 for Key-Value implementation
# Navigate to node 749 for Attachments implementation
```

---

## Questions to Answer Before Deciding

1. **How often do you add notes across multiple sessions to the same node?**
   - Rarely → Key-Value
   - Frequently → Attachments

2. **Will automation add metadata?**
   - Yes, needs retry safety → Key-Value
   - Manual only → Either

3. **Do you need to link to external resources with context?**
   - Yes, titles matter → Attachments
   - No, just content → Key-Value

4. **How important is temporal history of additions?**
   - Critical → Attachments
   - Current state is enough → Key-Value

5. **Team size and documentation capacity?**
   - Small, less docs → Pick one
   - Larger, can document → Hybrid is viable
