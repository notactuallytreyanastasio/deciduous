---
description: Document a file or directory comprehensively - shaking the tree to truly understand it
allowed-tools: Read, Glob, Grep, Bash(ls:*, find:*, wc:*, git log:*, git blame:*, deciduous:*), Write, Edit
argument-hint: <file-or-directory>
---

# Document

**Comprehensive documentation that shakes the tree to understand everything.**

This skill generates in-depth documentation for a file or directory, focusing on:
- Human readability while covering ALL surface area
- Linking to tests as working examples
- Integration with the deciduous decision graph

---

## Step 1: Create Documentation Goal Node

Before documenting, log what you're about to do:

```bash
deciduous add goal "Document $ARGUMENTS" -c 90
```

Store the goal ID for linking later.

---

## Step 2: Understand the Target

### For a File

1. **Read the file completely**
   - Understand every function, class, type, and export
   - Note all imports and dependencies
   - Identify the file's role in the larger system

2. **Find tests for this file**
   - Look for test files with similar names
   - Search for imports/references in test directories
   - These will become working examples in the docs

3. **Trace callers/callees**
   - Who calls this file?
   - What does this file call?
   - Map the dependency graph

### For a Directory

1. **Map the structure**
   - List all files and their purposes
   - Identify the public API (index/mod files)
   - Find the entry point

2. **Understand relationships**
   - How do files in this directory interact?
   - What's the data flow?

3. **Find related tests**
   - Test directories that cover this code
   - Integration tests that exercise the whole module

---

## Step 3: Document Each Component

For each file/component, document:

### 3.1 Purpose
- One sentence: what does this do?
- Why does it exist? (The "why" is more important than the "what")

### 3.2 API Surface
For every public function/method/class, include:
- Function signature
- Parameters with descriptions
- Return value meaning
- Possible errors

### 3.3 Internal Architecture
- How does it work internally?
- What are the key data structures?
- What are the invariants?

### 3.4 Dependencies
- What does this depend on?
- What depends on this?

### 3.5 Tests as Examples
- Show tests as working examples
- Explain what each test demonstrates

---

## Step 4: Link to Decision Graph

After documentation is complete:

```bash
# Create documentation action
deciduous add action "Documented <target>" -c 95 -f "<files-created>"

# Link to goal
deciduous link <goal_id> <action_id> -r "Documentation complete"

# Create outcome
deciduous add outcome "Documentation complete for <target>" -c 95
deciduous link <action_id> <outcome_id> -r "Successfully documented"

# Sync
deciduous sync
```

---

## Step 5: Verify Coverage

**Checklist before completing:**

- [ ] Every public function documented
- [ ] Every parameter explained
- [ ] Every return value explained
- [ ] Every error case documented
- [ ] At least one example per function (from tests)
- [ ] Architecture overview included
- [ ] Dependencies mapped
- [ ] Links to tests included

---

**Now document: $ARGUMENTS**
