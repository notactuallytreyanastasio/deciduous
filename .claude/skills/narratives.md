# Narrative Tracking

**Narratives are the source of truth. Commits are just evidence.**

---

## The Core Insight

Don't start with commits. Start with understanding.

A narrative is: *"The story of how one piece of the system's design evolved."*

Commits might support a narrative. Or they might not exist. The narrative is the thing - it lives at the conceptual level, not the git level.

---

## When to Use

When you need to understand how a system got to where it is. Before building any decision graph.

---

## Process

### 1. Understand the system first

Before looking at git:

```bash
# Read the code
cat README.md
ls src/

# Understand what exists TODAY
```

Ask: **What are the major pieces of this system?**

Each major piece probably has a narrative behind it.

### 2. Identify narratives from the design

Look at the current system and ask:

- "How did the auth system get this way?"
- "Why is caching done like this?"
- "What's the story behind this API design?"

**Write down the narratives you can INFER from the code.** You don't need commits yet.

```markdown
# Narratives

## Authentication
> There's JWT + sessions. Probably started with one, switched to the other?

## Caching
> Redis everywhere. Was it always Redis? Probably not.

## API Design
> REST with some GraphQL. Hybrid approach suggests evolution.
```

### 3. Find evidence (optional)

Now, IF you want supporting evidence, look at git:

```bash
git log --oneline --all -- src/auth/
git log --oneline --grep="auth"
```

But the commits are just evidence for narratives you already identified. They're not the starting point.

### 4. Look for pivots

The most valuable thing in a narrative is: **when did the model change?**

Not "when did code change" - when did the CONCEPT change?

Signs of a pivot:
- Two approaches coexisting (migration in progress)
- Comments explaining "we used to do X"
- Config for old + new system
- Deprecation warnings

```markdown
## Authentication
> JWT for API clients, sessions for web. Evidence of a pivot.

**PIVOT:** Probably moved from pure JWT to hybrid approach.
**Evidence:** Session middleware exists alongside JWT validation.
**Why?:** (unknown - need to find out)
```

### 5. Find the "why" for pivots

This is the gold. For each pivot, figure out WHY.

Sources:
- PR descriptions
- Commit messages around the change
- Issue discussions
- Architecture decision records
- Ask someone who was there

```markdown
## Authentication
**PIVOT:** JWT → JWT + Sessions
**Why:** Mobile clients couldn't handle large JWT payloads (4KB cookie limit)
**Evidence:** PR #234 "Add session-based auth for mobile"
```

---

## Output Format

`.deciduous/narratives.md`:

```markdown
# Narratives

## <Name>
> <One sentence: what this piece of the system does>

**Current state:** <How it works today>

**Evolution:**
1. <First approach> - <why>
2. **PIVOT:** <what changed> - <why it changed>
3. <Current approach> - <why this is better>

**Evidence:** <Optional: PRs, commits, docs that support this>
**Connects to:** <Other narratives this influenced/was influenced by>
**Status:** active | superseded | abandoned

---
```

---

## Decision Criteria

**What makes something a narrative?**
- It's a coherent story about ONE design aspect
- It explains HOW something works and WHY it evolved
- It would help a new team member understand the system

**What's NOT a narrative?**
- A list of commits
- A feature changelog
- Implementation details that don't affect the model

**When is a commit worth noting?**
- Only if it supports understanding the narrative
- Only if it marks a model change (not implementation)
- Most commits are noise - skip them

**How do I know I've found a pivot?**
- The conceptual model changed, not just the code
- There's a "before" and "after" that work differently
- Someone had to make a decision to change direction

---

## Example

```markdown
# Narratives

## Authentication
> How users prove their identity to the system.

**Current state:** Hybrid - JWT for API clients, sessions for web app.

**Evolution:**
1. Started with JWT everywhere - stateless, simple, standard
2. **PIVOT:** Mobile web hit 4KB cookie limits with JWT payloads
3. Added session-based auth for web, kept JWT for API

**Why the pivot:** JWT tokens contained user permissions, growing to 3KB+.
Mobile Safari's 4KB cookie limit caused silent auth failures. Sessions
store permissions server-side, only send session ID.

**Evidence:**
- PR #234 "Add session auth for mobile web"
- Slack thread 2024-03-15 "mobile auth broken"

**Connects to:** "API Rate Limiting" (auth method affects rate limit keys)
**Status:** active

---

## API Rate Limiting
> Protecting the API from abuse and ensuring fair usage.

**Current state:** Redis-based, per-user limits with auth-aware tiers.

**Evolution:**
1. No rate limiting initially
2. **PIVOT:** Bot abuse caused outages
3. Added basic IP-based throttling
4. **PIVOT:** Legitimate users sharing IPs got blocked
5. Moved to per-user limits (requires auth)
6. **PIVOT:** Auth failures created different abuse vector
7. Added auth-aware tiers (unauth'd gets stricter limits)

**Connects to:** "Authentication" (rate limit strategy depends on auth state)
**Status:** active

---
```

---

## What This Enables

After collecting narratives, you can:

1. **Build the decision graph** (`/archaeology`) - narratives become goal nodes, pivots become revisit nodes

2. **Answer questions** like:
   - "Why does auth work this way?" → Read the Authentication narrative
   - "Can we remove sessions?" → Narrative explains why they exist
   - "What happens if we change rate limiting?" → Connections show dependencies

3. **Avoid repeating mistakes** - Pivots document what DIDN'T work and why

---

## The Mindset

Think like an anthropologist, not a git archaeologist.

You're trying to understand a culture (the system's design) by studying artifacts (code, commits, docs). The artifacts are evidence, but the culture is what matters.

**Bad:** "Let me read through 500 commits and categorize them"
**Good:** "Let me understand how auth works, then find evidence for how it evolved"
