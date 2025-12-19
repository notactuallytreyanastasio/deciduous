# Node Metadata: Approach Analysis

This document analyzes two independent implementations for adding metadata to decision graph nodes, helping you choose which approach to adopt.

## The Problem

Decision graph nodes need rich context beyond their title and description:
- PR descriptions and writeups
- Issue bodies and commentary
- Notes accumulated over time
- Links to external resources
- Test plans, design docs, etc.

## Two Approaches Implemented

| Approach | PR | Branch | Design Philosophy |
|----------|-----|--------|-------------------|
| **Key-Value** | [#165](https://github.com/notactuallytreyanastasio/deciduous/pull/165) | `feature/node-metadata-keyval` | One value per key, dictionary-like |
| **Attachments** | [#167](https://github.com/notactuallytreyanastasio/deciduous/pull/167) | `feature/node-metadata-attachments` | Multiple documents per type |

---

## Decision Graph

![Analysis Decision Graph](https://raw.githubusercontent.com/notactuallytreyanastasio/deciduous/analysis/node-metadata-comparison/docs/decision-graph-metadata-analysis.png)

---

## Approach 1: Key-Value Metadata

### CLI Example
```bash
deciduous meta set 42 pr_body "## Summary\n\nThis PR adds..."
deciduous meta get 42 pr_body
deciduous meta list 42
deciduous meta delete 42 pr_body
```

### Strengths
| Observation | Confidence |
|-------------|------------|
| Simpler mental model - one value per key, like a dictionary | 85% |
| Standard keys enforce consistency (pr_body, issue_body, notes) | 80% |
| Upsert prevents duplicates - 'set pr_body' always replaces | 85% |
| **USE CASE FIT**: PR/issue body is typically ONE per node | 85% |

### Weaknesses
| Observation | Confidence |
|-------------|------------|
| Cannot have multiple notes - each key is unique | 90% |

### Best For
- Single structured fields per node (pr_body, issue_body)
- When you want exactly one value per type
- Simpler workflows with predictable keys

---

## Approach 2: Document-Oriented Attachments

### CLI Example
```bash
deciduous attach add 42 -t note --title "Session 1 notes" "Content here"
deciduous attach add 42 -t note --title "Session 2 notes" "More content"
deciduous attach add 42 -t link --title "Related PR" --url "https://..."
deciduous attach list 42
deciduous attach view 1
```

### Strengths
| Observation | Confidence |
|-------------|------------|
| Multiple items per type - can have 5 notes on one node | 90% |
| Titles provide context - 'PR #105: URL State' vs just 'pr_body' | 85% |
| Source URLs link to external resources for traceability | 80% |
| **USE CASE FIT**: Notes/comments accumulate over time | 85% |

### Weaknesses
| Observation | Confidence |
|-------------|------------|
| Need attachment IDs to reference/update specific items | 75% |

### Best For
- Accumulating notes over multiple sessions
- When titles matter for organization
- Linking to external resources with URLs
- Collections of related items

---

## Option 3: Hybrid Approach

### Concept
Implement both systems with clear separation:
- `meta` command for single-value fields (pr_body, issue_body, summary)
- `attach` command for collections (notes, links, comments)

### Analysis
| Observation | Confidence |
|-------------|------------|
| Could have both - meta for single values, attach for collections | 70% |
| **RISK**: Two systems to learn, maintain, and keep in sync | 75% |

---

## Comparison Matrix

| Feature | Key-Value | Attachments |
|---------|-----------|-------------|
| Multiple per type | No | Yes |
| Titles | No | Yes |
| Source URLs | No | Yes |
| Lookup by | Key name | Attachment ID |
| Mental model | Dictionary | Document store |
| CLI complexity | Simpler | More options |
| Best for | Structured fields | Accumulating content |

---

## Use Case Analysis

### Scenario: Adding PR context to a goal node

**Key-Value:**
```bash
deciduous meta set 603 pr_body < pr-105-body.md
# Later, to update:
deciduous meta set 603 pr_body < pr-105-body-v2.md  # Replaces
```

**Attachments:**
```bash
deciduous attach add 603 -t pr_body --title "PR #105" --url "https://..." < pr-105-body.md
# Later, to update:
deciduous attach update 3 < pr-105-body-v2.md  # Need to know ID
```

### Scenario: Adding multiple notes over time

**Key-Value:**
```bash
deciduous meta set 603 notes "Note 1"
deciduous meta set 603 notes "Note 2"  # PROBLEM: Overwrites Note 1!
# Workaround: custom:note1, custom:note2 - but loses queryability
```

**Attachments:**
```bash
deciduous attach add 603 -t note --title "Session 1" "Note 1"
deciduous attach add 603 -t note --title "Session 2" "Note 2"  # Works!
deciduous attach list 603 -t note  # Shows both
```

---

## Recommendation Framework

Choose **Key-Value** if:
- You primarily store one value per type (one PR body, one issue body)
- You want the simplest possible workflow
- You query by key name frequently
- Duplicates would be a bug, not a feature

Choose **Attachments** if:
- You accumulate multiple items over time (notes, links)
- Titles and source URLs are important for context
- You need to track provenance (where did this come from?)
- Collections are the norm, not the exception

Choose **Hybrid** if:
- You have clear use cases for both patterns
- You're willing to maintain two systems
- Complexity is acceptable for flexibility

---

## Decision Graph Nodes

This analysis corresponds to nodes **768-784** in the decision graph.

### Full Node List
| ID | Type | Title |
|----|------|-------|
| 768 | goal | Analyze and choose between metadata approaches |
| 769 | decision | Which metadata approach to adopt? |
| 770 | option | Key-Value approach (PR #165) |
| 771 | option | Attachments approach (PR #167) |
| 772 | option | Hybrid: implement both with shared foundation |
| 773-776 | observation | Key-Value pros/cons |
| 777-780 | observation | Attachments pros/cons |
| 781-782 | observation | Use case fits |
| 783-784 | observation | Hybrid pros/cons |

### Links to Implementation Graphs
- **Key-Value graph**: Nodes 732-748 ([PR #165](https://github.com/notactuallytreyanastasio/deciduous/pull/165))
- **Attachments graph**: Nodes 749-765 ([PR #167](https://github.com/notactuallytreyanastasio/deciduous/pull/167))

---

## Your Decision

After reviewing the decision graphs in the web viewer:

1. Open `deciduous serve` and navigate to node 768
2. Explore the analysis tree (768-784)
3. Compare with implementation trees (732-748 and 749-765)
4. Consider your primary use cases

**Which approach best fits your workflow?**
