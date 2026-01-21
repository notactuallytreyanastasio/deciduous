//! OpenCode integration
//!
//! Generates and installs OpenCode configuration files for decision graph integration.
//! OpenCode uses TypeScript/JavaScript plugins instead of shell scripts for hooks.
//!
//! Directory structure:
//! - `.opencode/plugin/` - TypeScript plugins (hooks)
//! - `.opencode/command/` - Custom commands (markdown)
//! - `opencode.json` - Configuration file
//! - `AGENTS.md` - Project instructions (equivalent to CLAUDE.md)

use crate::config::Config;
use colored::Colorize;
use serde_json::json;
use std::fs;
use std::path::Path;

/// OpenCode plugin for requiring action nodes before edits
pub const PLUGIN_REQUIRE_ACTION_NODE: &str = r#"// OpenCode Plugin: Require Action Node
// Checks for recent action/goal nodes before file edits
// This enforces the decision graph workflow: log BEFORE you code

import type { Plugin } from "@opencode-ai/plugin"

export const RequireActionNode: Plugin = async ({ $ }) => {
  return {
    "tool.execute.before": async (input, output) => {
      // Only check on edit and write tools
      if (input.tool !== "edit" && input.tool !== "write") {
        return
      }

      try {
        // Get recent nodes from deciduous
        const result = await $`deciduous nodes 2>/dev/null | head -20`.quiet()
        const stdout = result.stdout.toString()

        const lines = stdout.trim().split("\n").filter((l: string) => l.trim())

        // Check for any goal or action node
        let hasRecentNode = false
        for (const line of lines) {
          if (line.match(/goal|action/i)) {
            hasRecentNode = true
            break
          }
        }

        if (!hasRecentNode && lines.length > 2) {
          // Show a toast reminder but don't block
          console.log(`
╔═══════════════════════════════════════════════════════════════════╗
║  DECIDUOUS: Consider adding a goal/action node!                   ║
╠═══════════════════════════════════════════════════════════════════╣
║  Before editing files, log what you're about to do:               ║
║    deciduous add action "Your action description" -c 85           ║
╚═══════════════════════════════════════════════════════════════════╝
`)
        }
      } catch (error) {
        // If deciduous isn't available, continue silently
      }
    }
  }
}
"#;

/// OpenCode plugin for post-commit reminders
pub const PLUGIN_POST_COMMIT_REMINDER: &str = r#"// OpenCode Plugin: Post-Commit Reminder
// Reminds to link commits to the decision graph after git commit
// This ensures commits are connected to the reasoning that led to them

import type { Plugin } from "@opencode-ai/plugin"

export const PostCommitReminder: Plugin = async ({ $ }) => {
  return {
    "tool.execute.after": async (input) => {
      // Only check bash tool
      if (input.tool !== "bash") {
        return
      }

      // Check if this was a git commit command
      const command = input.args?.command || ""
      if (!command.includes("git commit")) {
        return
      }

      try {
        // Get the latest commit info
        const hashResult = await $`git rev-parse --short HEAD 2>/dev/null`.quiet()
        const msgResult = await $`git log -1 --format=%s 2>/dev/null`.quiet()

        const commitHash = hashResult.stdout.toString().trim()
        const commitMsg = msgResult.stdout.toString().trim().slice(0, 50)

        // Show reminder
        console.log(`
╔═══════════════════════════════════════════════════════════════════╗
║  DECIDUOUS: Link this commit to the decision graph!               ║
╠═══════════════════════════════════════════════════════════════════╣
║  Commit: ${commitHash} "${commitMsg}"
║                                                                   ║
║  Run NOW:                                                         ║
║    deciduous add outcome "What was accomplished" -c 95 --commit HEAD
║    deciduous link <action_id> <outcome_id> -r "Implementation"    ║
╚═══════════════════════════════════════════════════════════════════╝
`)
      } catch (error) {
        // If git commands fail, skip the reminder
      }
    }
  }
}
"#;

/// OpenCode command template: /work
pub const COMMAND_WORK: &str = r#"---
description: Start a work transaction with decision graph logging
arguments:
  - name: GOAL
    description: The goal you're working towards
    required: true
---

# Work Transaction

You are starting a work transaction for: **$GOAL**

## Required Steps

1. **Create a goal node** (if this is new work):
   ```bash
   deciduous add goal "$GOAL" -c 90 --prompt-stdin << 'EOF'
   <paste the user's original request here>
   EOF
   ```

2. **Before any code changes**, create an action node:
   ```bash
   deciduous add action "What you're about to do" -c 85
   deciduous link <goal_id> <action_id> -r "Implementation step"
   ```

3. **After successful changes**, create an outcome:
   ```bash
   deciduous add outcome "What was accomplished" -c 95 --commit HEAD
   deciduous link <action_id> <outcome_id> -r "Completed"
   ```

## Rules
- NEVER edit files without an action node
- ALWAYS link commits to the graph
- Capture verbatim user prompts on goal nodes
"#;

/// OpenCode command template: /recover
pub const COMMAND_RECOVER: &str = r#"---
description: Recover context from decision graph and recent activity
arguments:
  - name: FOCUS
    description: Optional focus area to filter by
    required: false
---

# Context Recovery

Recovering context from the decision graph.

## Steps

1. **Check recent nodes**:
   ```bash
   deciduous nodes --branch $(git branch --show-current 2>/dev/null || echo main) | head -30
   ```

2. **Check graph connections**:
   ```bash
   deciduous edges | tail -20
   ```

3. **Check recent commands**:
   ```bash
   deciduous commands --limit 10
   ```

4. **Check git status**:
   ```bash
   git status
   git log --oneline -10
   ```

5. **Audit for orphan nodes** (nodes without connections):
   - Every outcome should link to an action
   - Every action should link to a goal
   - Only root goals should be orphans

Report what you find and any gaps that need attention.
"#;

/// OpenCode command template: /decision
pub const COMMAND_DECISION: &str = r#"---
description: Log a decision to the decision graph
arguments:
  - name: TYPE
    description: "Node type: goal, decision, option, action, outcome, observation, revisit"
    required: true
  - name: TITLE
    description: Title of the node
    required: true
---

# Decision Graph Entry

Create a **$TYPE** node: "$TITLE"

## Command

```bash
deciduous add $TYPE "$TITLE" -c <confidence 0-100>
```

## After creating the node

**IMMEDIATELY** link it to related nodes:

| Node Type | Link To |
|-----------|---------|
| outcome | The action/goal it resolves |
| action | The goal/decision that spawned it |
| option | Its parent decision |
| observation | Related goal/action |
| revisit | The decision being reconsidered |

```bash
deciduous link <from_id> <to_id> -r "reason for connection"
```

## Flags

- `-c, --confidence 0-100` - Confidence level
- `-p, --prompt "..."` - Store verbatim user prompt
- `-f, --files "a.rs,b.rs"` - Associate files
- `--commit HEAD` - Link to current git commit
- `--date "YYYY-MM-DD"` - Backdate for archaeology
"#;

/// OpenCode command template: /build-test
pub const COMMAND_BUILD_TEST: &str = r#"---
description: Build the project and run the test suite
arguments:
  - name: PATTERN
    description: Optional test pattern to filter tests
    required: false
---

# Build and Test

Build the project and run the test suite.

## Instructions

1. Run the full build and test cycle:
   ```bash
   cargo build --release && cargo test
   ```

2. If tests fail, analyze the failures and explain:
   - Which test failed
   - What it was testing
   - Likely cause of failure
   - Suggested fix

3. If all tests pass, report success and any warnings from the build.

4. If the user specifies a specific test pattern, run only those tests:
   ```bash
   cargo test $PATTERN
   ```

## Test categories in this project
- `test_public_exports` - API verification
- `test_filter_graph` - Graph filtering
- `test_extract_commit` - Commit extraction from metadata
- `test_extract_confidence` - Confidence extraction from metadata
- `test_graph_to_dot` - DOT export
- `test_generate_writeup` - PR writeup generation
"#;

/// OpenCode command template: /serve-ui
pub const COMMAND_SERVE_UI: &str = r#"---
description: Start the decision graph web viewer
arguments:
  - name: PORT
    description: Port to run the server on (default 3000)
    required: false
---

# Start Decision Graph Viewer

Launch the deciduous web server for viewing and navigating the decision graph.

## Instructions

1. Start the server:
   ```bash
   deciduous serve --port ${PORT:-3000}
   ```

2. Inform the user:
   - The server is running at http://localhost:${PORT:-3000}
   - The graph auto-refreshes every 30 seconds
   - They can browse decisions, chains, and timeline views
   - Changes made via CLI will appear automatically

3. The server will run in the foreground. Remind user to stop it when done (Ctrl+C).

## UI Features
- **Chains View**: See decision chains grouped by goals
- **Timeline View**: Chronological view of all decisions
- **Graph View**: Interactive force-directed graph
- **DAG View**: Directed acyclic graph visualization
- **Detail Panel**: Click any node to see full details including:
  - Node metadata (confidence, commit, prompt, files)
  - Connected nodes (incoming/outgoing edges)
  - Timestamps and status

## Alternative: Static Hosting

For GitHub Pages or other static hosting:
```bash
deciduous sync  # Exports to docs/graph-data.json
```

Then push to GitHub - the graph is viewable at your GitHub Pages URL.
"#;

/// OpenCode command template: /sync-graph
pub const COMMAND_SYNC_GRAPH: &str = r#"---
description: Sync the decision graph to GitHub Pages
arguments: []
---

# Sync Decision Graph to GitHub Pages

Export the current decision graph to docs/graph-data.json so it's deployed to GitHub Pages.

## Steps

1. Run `deciduous sync` to export the graph
2. Show the user how many nodes/edges were exported
3. If there are changes, stage them: `git add docs/graph-data.json`

This should be run before any push to main to ensure the live site has the latest decisions.
"#;

/// OpenCode skill template: /pulse
pub const SKILL_PULSE: &str = r#"---
description: Map the current model as decisions - no history, just now
arguments:
  - name: SCOPE
    description: "What part of the system to map (e.g., 'Authentication', 'API rate limiting')"
    required: true
---

# Pulse

**Map the current model as decisions. No history, just now.**

## What This Is

Pulse captures the current heartbeat of a system - what decisions define how it works TODAY.

Not how it evolved. Not what was tried before. Just: *"What are the design decisions that make this system work the way it does?"*

## Scope

Taking the pulse of: **$SCOPE**

## Process

### 1. Ask: "What decisions define this?"

Read the code. For the thing you're scoping, ask:

> "What design questions had to be answered for this to work?"

Not implementation questions ("which library?") - model questions ("what's the behavior?")

**Examples:**
- "When should the fallback show?"
- "How should nested components interact?"
- "What happens on timeout?"
- "How are errors handled?"

### 2. Create the goal node

```bash
deciduous add goal "$SCOPE: <Core question>" -c 90
```

### 3. Map the decisions

For each design question you identified:

```bash
deciduous add decision "<Design question>" -c <confidence>
deciduous link <parent> <decision> -r "leads_to"
```

### 4. Add answers where known

If a decision has a clear answer in the current system:

```bash
deciduous add option "<The answer/choice>" -c 90
deciduous link <decision> <option> -r "resolved_by"
deciduous status <option> chosen
```

If a decision is still open or unclear, leave it as just the decision node.

## Decision Criteria

**Is this a decision worth capturing?**
- Does it define BEHAVIOR (not implementation)? → Yes
- Would changing it change how users experience the system? → Yes
- Is it a choice that could have gone differently? → Yes
- Is it just "how the code is organized"? → No

**How deep to go?**
- Stop when decisions become implementation details
- Stop when the answer is obvious/forced (no real choice)
- Stop when you've captured what someone needs to understand the model

## View the result

```bash
deciduous serve
```
"#;

/// OpenCode skill template: /narratives
pub const SKILL_NARRATIVES: &str = r#"---
description: Understand how a system evolved - narratives are the source of truth
arguments:
  - name: FOCUS
    description: "What part of the system to trace (e.g., 'auth', 'caching', 'API')"
    required: true
---

# Narrative Tracking

**Narratives are the source of truth. Commits are just evidence.**

## The Core Insight

Don't start with commits. Start with understanding.

A narrative is: *"The story of how one piece of the system's design evolved."*

Focus area: **$FOCUS**

## Process

### 1. Understand the system first

Before looking at git:

```bash
# Read the code
cat README.md
ls src/
```

Ask: **What are the major pieces of this system?**

### 2. Identify narratives from the design

Look at the current system and ask:

- "How did this get this way?"
- "Why is this done like this?"
- "What's the story behind this design?"

**Write down the narratives you can INFER from the code.** You don't need commits yet.

### 3. Find evidence (optional)

Now, IF you want supporting evidence, look at git:

```bash
git log --oneline --all -- src/$FOCUS/
git log --oneline --grep="$FOCUS"
```

### 4. Look for pivots

The most valuable thing in a narrative is: **when did the model change?**

Signs of a pivot:
- Two approaches coexisting (migration in progress)
- Comments explaining "we used to do X"
- Config for old + new system
- Deprecation warnings

### 5. Find the "why" for pivots

Sources:
- PR descriptions
- Commit messages around the change
- Issue discussions
- Architecture decision records

## Output Format

Write to `.deciduous/narratives.md`:

```markdown
# Narratives

## <Name>
> <One sentence: what this piece of the system does>

**Current state:** <How it works today>

**Evolution:**
1. <First approach> - <why>
2. **PIVOT:** <what changed> - <why it changed>
3. <Current approach> - <why this is better>

**Evidence:** <Optional: PRs, commits, docs>
**Connects to:** <Other narratives this influenced>
**Status:** active | superseded | abandoned
```

After collecting narratives, run `/archaeology` to transform them into a queryable graph.
"#;

/// OpenCode skill template: /archaeology
pub const SKILL_ARCHAEOLOGY: &str = r#"---
description: Transform narratives into a queryable decision graph
arguments: []
---

# Archaeology

**Transform narratives into a queryable decision graph.**

Run `/narratives` first. This skill takes conceptual narratives and structures them for querying.

## The Relationship

```
Narratives (conceptual)     →    Decision Graph (structural)
"How auth evolved"          →    Nodes + edges you can query
Human-readable stories      →    Machine-traversable graph
```

## Process

### 1. Read the narratives

```bash
cat .deciduous/narratives.md
```

### 2. Map narrative → graph

Each narrative becomes a connected subgraph:

| Narrative Element | Graph Element |
|-------------------|---------------|
| Narrative title | `goal` node (the root) |
| Evolution step | `action` or `decision` node |
| **PIVOT** | `revisit` node |
| Pivot "why" | `observation` node (links INTO revisit) |
| Pre-pivot state | Nodes marked `superseded` |
| **Connects to** | Cross-narrative edge |

### 3. Build the subgraph

For a narrative with pivots:

```bash
# Root (backdate to when project started)
deciduous add goal "Authentication" -c 90 --date "2023-01-15"
# → id: 1

# First approach
deciduous add decision "JWT for all auth" -c 85 --date "2023-01-20"
deciduous link 1 2 -r "Initial design"

# What was learned (leads to pivot)
deciduous add observation "Mobile Safari 4KB cookie limit breaking JWT auth"
deciduous link 2 3 -r "Discovered in production"

# The pivot
deciduous add revisit "Reconsidering auth token strategy"
deciduous link 3 4 -r "Cookie limits forced rethink"

# Mark pre-pivot as superseded
deciduous status 2 superseded

# New approach
deciduous add decision "Hybrid: JWT for API, sessions for web"
deciduous link 4 5 -r "New approach"
```

## The Revisit Pattern

Every **PIVOT** in a narrative becomes this structure:

```
[Previous approach]
        │
        ▼
[Observation: what was learned]
        │
        ▼
[Revisit: reconsidering X]
        │
        ▼
[New approach]
```

## Querying the Graph

After building, you can ask:

```bash
# What's the current state?
deciduous nodes --status active

# What was tried and abandoned?
deciduous nodes --status superseded

# What led to a specific decision?
deciduous edges --to <node_id>

# What are the pivot points?
deciduous nodes --type revisit

# Visual exploration
deciduous serve
```

## The Goal

Build a graph that can answer:

- **"Why does it work this way?"** → Trace from current state back through revisits
- **"What did we try before?"** → Look at superseded nodes
- **"Can we change X?"** → Check what depends on X via edges
- **"We should do Y"** → "We tried that, here's why it failed"
"#;

/// Install OpenCode configuration and plugins
pub fn install_opencode(project_root: &Path) -> Result<(), String> {
    let config = Config::load();

    println!("\n{}", "Installing OpenCode integration...".cyan().bold());

    let opencode_dir = project_root.join(".opencode");
    let plugin_dir = opencode_dir.join("plugin");
    let command_dir = opencode_dir.join("command");

    // Create directories
    for dir in [&opencode_dir, &plugin_dir, &command_dir] {
        if !dir.exists() {
            fs::create_dir_all(dir).map_err(|e| format!("Could not create {:?}: {}", dir, e))?;
            println!("   {} {:?}", "Creating".green(), dir);
        }
    }

    // Install plugins (hooks)
    if config.hooks.enabled {
        for hook in &config.hooks.pre_tool_use {
            if hook.enabled && hook.name == "require-action-node" {
                let plugin_path = plugin_dir.join("require-action-node.ts");
                fs::write(&plugin_path, PLUGIN_REQUIRE_ACTION_NODE)
                    .map_err(|e| format!("Could not write plugin: {}", e))?;
                println!(
                    "   {} .opencode/plugin/require-action-node.ts",
                    "Installed".green()
                );
            }
        }

        for hook in &config.hooks.post_tool_use {
            if hook.enabled && hook.name == "post-commit-reminder" {
                let plugin_path = plugin_dir.join("post-commit-reminder.ts");
                fs::write(&plugin_path, PLUGIN_POST_COMMIT_REMINDER)
                    .map_err(|e| format!("Could not write plugin: {}", e))?;
                println!(
                    "   {} .opencode/plugin/post-commit-reminder.ts",
                    "Installed".green()
                );
            }
        }
    }

    // Install commands
    let commands = [
        ("work.md", COMMAND_WORK),
        ("recover.md", COMMAND_RECOVER),
        ("decision.md", COMMAND_DECISION),
        ("build-test.md", COMMAND_BUILD_TEST),
        ("serve-ui.md", COMMAND_SERVE_UI),
        ("sync-graph.md", COMMAND_SYNC_GRAPH),
    ];

    for (name, content) in commands {
        let cmd_path = command_dir.join(name);
        fs::write(&cmd_path, content)
            .map_err(|e| format!("Could not write command {}: {}", name, e))?;
        println!("   {} .opencode/command/{}", "Installed".green(), name);
    }

    // Install skills (in OpenCode, skills are just more complex commands)
    let skills = [
        ("pulse.md", SKILL_PULSE),
        ("narratives.md", SKILL_NARRATIVES),
        ("archaeology.md", SKILL_ARCHAEOLOGY),
    ];

    for (name, content) in skills {
        let skill_path = command_dir.join(name);
        fs::write(&skill_path, content)
            .map_err(|e| format!("Could not write skill {}: {}", name, e))?;
        println!(
            "   {} .opencode/command/{} (skill)",
            "Installed".green(),
            name
        );
    }

    // Generate opencode.json config
    // Note: Plugins are auto-loaded from .opencode/plugin/, not configured in JSON
    // Instructions/rules go in AGENTS.md, referenced via the 'instructions' key
    let opencode_config = json!({
        "$schema": "https://opencode.ai/config.json",
        "instructions": ["AGENTS.md"]
    });

    let config_path = project_root.join("opencode.json");
    if !config_path.exists() {
        fs::write(
            &config_path,
            serde_json::to_string_pretty(&opencode_config).unwrap(),
        )
        .map_err(|e| format!("Could not write opencode.json: {}", e))?;
        println!("   {} opencode.json", "Created".green());
    } else {
        println!("   {} opencode.json (already exists)", "Skipped".yellow());
    }

    // Generate or update AGENTS.md
    let agents_md_path = project_root.join("AGENTS.md");
    if !agents_md_path.exists() {
        // Check if CLAUDE.md exists to convert from
        let claude_md_path = project_root.join("CLAUDE.md");
        if claude_md_path.exists() {
            // Read CLAUDE.md and adapt for OpenCode
            let claude_content = fs::read_to_string(&claude_md_path)
                .map_err(|e| format!("Could not read CLAUDE.md: {}", e))?;

            let agents_content = convert_claude_to_agents_md(&claude_content);
            fs::write(&agents_md_path, agents_content)
                .map_err(|e| format!("Could not write AGENTS.md: {}", e))?;
            println!(
                "   {} AGENTS.md (converted from CLAUDE.md)",
                "Created".green()
            );
        } else {
            // Create basic AGENTS.md
            let agents_content = generate_basic_agents_md();
            fs::write(&agents_md_path, agents_content)
                .map_err(|e| format!("Could not write AGENTS.md: {}", e))?;
            println!("   {} AGENTS.md", "Created".green());
        }
    } else {
        // AGENTS.md exists - append workflow section if not present
        let existing_content = fs::read_to_string(&agents_md_path)
            .map_err(|e| format!("Could not read AGENTS.md: {}", e))?;

        if existing_content.contains("## Decision Graph Workflow") {
            println!(
                "   {} AGENTS.md (workflow section already present)",
                "Skipped".yellow()
            );
        } else {
            // Append the workflow section
            let workflow_section = get_agents_workflow_section();
            let new_content = format!("{}\n{}", existing_content.trim_end(), workflow_section);
            fs::write(&agents_md_path, new_content)
                .map_err(|e| format!("Could not update AGENTS.md: {}", e))?;
            println!(
                "   {} AGENTS.md (appended workflow section)",
                "Updated".green()
            );
        }
    }

    println!("\n{}", "OpenCode integration installed!".green().bold());
    println!();
    println!("Plugins installed in .opencode/plugin/");
    println!("Commands installed in .opencode/command/");
    println!();

    Ok(())
}

/// Update OpenCode integration files to latest version (overwrites existing)
pub fn update_opencode(project_root: &Path) -> Result<(), String> {
    let opencode_dir = project_root.join(".opencode");
    let plugin_dir = opencode_dir.join("plugin");
    let command_dir = opencode_dir.join("command");

    // Create directories if needed
    for dir in [&opencode_dir, &plugin_dir, &command_dir] {
        if !dir.exists() {
            fs::create_dir_all(dir).map_err(|e| format!("Could not create {:?}: {}", dir, e))?;
            println!("   {} {:?}", "Creating".green(), dir);
        }
    }

    // Update plugins (overwrite)
    let plugin_path = plugin_dir.join("require-action-node.ts");
    fs::write(&plugin_path, PLUGIN_REQUIRE_ACTION_NODE)
        .map_err(|e| format!("Could not write plugin: {}", e))?;
    println!(
        "   {} .opencode/plugin/require-action-node.ts",
        "Updated".green()
    );

    let plugin_path = plugin_dir.join("post-commit-reminder.ts");
    fs::write(&plugin_path, PLUGIN_POST_COMMIT_REMINDER)
        .map_err(|e| format!("Could not write plugin: {}", e))?;
    println!(
        "   {} .opencode/plugin/post-commit-reminder.ts",
        "Updated".green()
    );

    // Update commands (overwrite)
    let commands = [
        ("work.md", COMMAND_WORK),
        ("recover.md", COMMAND_RECOVER),
        ("decision.md", COMMAND_DECISION),
        ("build-test.md", COMMAND_BUILD_TEST),
        ("serve-ui.md", COMMAND_SERVE_UI),
        ("sync-graph.md", COMMAND_SYNC_GRAPH),
    ];

    for (name, content) in commands {
        let cmd_path = command_dir.join(name);
        fs::write(&cmd_path, content)
            .map_err(|e| format!("Could not write command {}: {}", name, e))?;
        println!("   {} .opencode/command/{}", "Updated".green(), name);
    }

    // Update skills (overwrite)
    let skills = [
        ("pulse.md", SKILL_PULSE),
        ("narratives.md", SKILL_NARRATIVES),
        ("archaeology.md", SKILL_ARCHAEOLOGY),
    ];

    for (name, content) in skills {
        let skill_path = command_dir.join(name);
        fs::write(&skill_path, content)
            .map_err(|e| format!("Could not write skill {}: {}", name, e))?;
        println!(
            "   {} .opencode/command/{} (skill)",
            "Updated".green(),
            name
        );
    }

    // Note: We don't overwrite opencode.json or AGENTS.md as they may have user customizations
    println!(
        "   {} opencode.json and AGENTS.md (preserved)",
        "Skipping".yellow()
    );

    Ok(())
}

/// Convert CLAUDE.md content to AGENTS.md format
fn convert_claude_to_agents_md(claude_content: &str) -> String {
    // OpenCode recognizes CLAUDE.md as a fallback, but AGENTS.md is preferred
    // The format is similar, so we mainly just need to rename references
    let mut content = claude_content.to_string();

    // Replace Claude-specific references
    content = content.replace("Claude Code", "OpenCode");
    content = content.replace("claude code", "opencode");
    content = content.replace(".claude/", ".opencode/");
    content = content.replace("CLAUDE.md", "AGENTS.md");

    // Add OpenCode-specific header
    let header = r#"# Project Instructions for OpenCode

> This file was converted from CLAUDE.md. OpenCode uses AGENTS.md for project instructions.
> See: https://opencode.ai/docs/rules/

"#;

    format!("{}{}", header, content)
}

/// Get just the Decision Graph Workflow section for appending to existing AGENTS.md
fn get_agents_workflow_section() -> String {
    r#"

## Decision Graph Workflow

**THIS IS MANDATORY. Log decisions IN REAL-TIME, not retroactively.**

### The Core Rule

```
BEFORE you do something -> Log what you're ABOUT to do
AFTER it succeeds/fails -> Log the outcome
CONNECT immediately -> Link every node to its parent
```

### Quick Commands

```bash
deciduous add goal "Title" -c 90 -p "User's original request"
deciduous add action "Title" -c 85
deciduous link FROM TO -r "reason"
deciduous serve   # View live graph
deciduous sync    # Export for static hosting
```

### Node Types

| Type | Purpose |
|------|---------|
| `goal` | High-level objectives |
| `decision` | Choice points |
| `option` | Approaches considered |
| `action` | What was implemented |
| `outcome` | What happened |
| `observation` | Technical insights |
| `revisit` | Reconsidering a decision |

### Session Start Checklist

```bash
deciduous nodes           # What decisions exist?
deciduous edges           # How are they connected?
git status                # Current state
```
"#
    .to_string()
}

/// Generate basic AGENTS.md content
fn generate_basic_agents_md() -> String {
    r#"# Project Instructions for OpenCode

## Decision Graph Workflow

**THIS IS MANDATORY. Log decisions IN REAL-TIME, not retroactively.**

### The Core Rule

```
BEFORE you do something -> Log what you're ABOUT to do
AFTER it succeeds/fails -> Log the outcome
CONNECT immediately -> Link every node to its parent
```

### Quick Commands

```bash
deciduous add goal "Title" -c 90 -p "User's original request"
deciduous add action "Title" -c 85
deciduous link FROM TO -r "reason"
deciduous serve   # View live graph
deciduous sync    # Export for static hosting
```

### Node Types

| Type | Purpose |
|------|---------|
| `goal` | High-level objectives |
| `decision` | Choice points |
| `option` | Approaches considered |
| `action` | What was implemented |
| `outcome` | What happened |
| `observation` | Technical insights |
| `revisit` | Reconsidering a decision |

### Session Start Checklist

```bash
deciduous nodes           # What decisions exist?
deciduous edges           # How are they connected?
git status                # Current state
```
"#
    .to_string()
}

/// Show OpenCode integration status
pub fn opencode_status() -> Result<(), String> {
    let project_root = Config::find_project_root();

    println!("\n{}", "OpenCode Integration Status".cyan().bold());
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let Some(project_root) = project_root else {
        println!(
            "\n   {} Could not find project root (.deciduous directory)",
            "Error:".red()
        );
        println!("   Run 'deciduous init' to initialize the project.");
        return Ok(());
    };

    let opencode_dir = project_root.join(".opencode");

    // Check .opencode directory
    println!("\n{}", "Directory:".cyan());
    if opencode_dir.exists() {
        println!("   {} .opencode/", "✓".green());
    } else {
        println!("   {} .opencode/ (not found)", "○".yellow());
        println!("   Run 'deciduous opencode install' to create it");
    }

    // Check plugins
    println!("\n{}", "Plugins (Hooks):".cyan());
    let plugin_dir = opencode_dir.join("plugin");
    if plugin_dir.exists() {
        let entries: Vec<_> = fs::read_dir(&plugin_dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| {
                        e.path()
                            .extension()
                            .is_some_and(|ext| ext == "ts" || ext == "js")
                    })
                    .collect()
            })
            .unwrap_or_default();

        if entries.is_empty() {
            println!("   {} (no plugins installed)", "○".yellow());
        } else {
            for entry in entries {
                let name = entry.file_name().to_string_lossy().to_string();
                println!("   {} {}", "✓".green(), name);
            }
        }
    } else {
        println!("   {} (plugin directory not found)", "○".yellow());
    }

    // Check commands
    println!("\n{}", "Commands:".cyan());
    let command_dir = opencode_dir.join("command");
    if command_dir.exists() {
        let entries: Vec<_> = fs::read_dir(&command_dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
                    .collect()
            })
            .unwrap_or_default();

        if entries.is_empty() {
            println!("   {} (no commands installed)", "○".yellow());
        } else {
            for entry in entries {
                let name = entry
                    .path()
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                println!("   {} /{}", "✓".green(), name);
            }
        }
    } else {
        println!("   {} (command directory not found)", "○".yellow());
    }

    // Check configuration files
    println!("\n{}", "Configuration:".cyan());

    let opencode_json = project_root.join("opencode.json");
    if opencode_json.exists() {
        println!("   {} opencode.json", "✓".green());
    } else {
        println!("   {} opencode.json (not found)", "○".yellow());
    }

    let agents_md = project_root.join("AGENTS.md");
    if agents_md.exists() {
        println!("   {} AGENTS.md", "✓".green());
    } else {
        println!("   {} AGENTS.md (not found)", "○".yellow());
    }

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!(
        "\nRun {} to install OpenCode integration.",
        "'deciduous opencode install'".cyan()
    );
    println!();

    Ok(())
}

/// Uninstall OpenCode integration
pub fn uninstall_opencode(project_root: &Path) -> Result<(), String> {
    let opencode_dir = project_root.join(".opencode");

    if opencode_dir.exists() {
        fs::remove_dir_all(&opencode_dir)
            .map_err(|e| format!("Could not remove .opencode: {}", e))?;
        println!("   {} .opencode/", "Removed".green());
    }

    // Don't remove opencode.json or AGENTS.md as they might have user customizations
    println!(
        "   {} opencode.json and AGENTS.md (preserved)",
        "Note:".yellow()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_convert_claude_to_agents() {
        let claude = "# Claude Code Instructions\n\nUse .claude/ for config.";
        let agents = convert_claude_to_agents_md(claude);

        assert!(agents.contains("OpenCode"));
        assert!(agents.contains(".opencode/"));
        assert!(agents.contains("AGENTS.md"));
    }

    #[test]
    fn test_install_opencode() {
        let tmp = TempDir::new().unwrap();
        let project_root = tmp.path();

        // Create .deciduous with config
        let deciduous_dir = project_root.join(".deciduous");
        fs::create_dir_all(&deciduous_dir).unwrap();
        fs::write(deciduous_dir.join("config.toml"), "[hooks]\nenabled = true").unwrap();

        // Set current dir for Config::load
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(project_root).unwrap();

        let result = install_opencode(project_root);

        std::env::set_current_dir(original_dir).unwrap();

        assert!(result.is_ok());

        // Check plugins
        assert!(project_root
            .join(".opencode/plugin/require-action-node.ts")
            .exists());
        assert!(project_root
            .join(".opencode/plugin/post-commit-reminder.ts")
            .exists());

        // Check commands
        assert!(project_root.join(".opencode/command/work.md").exists());
        assert!(project_root.join(".opencode/command/recover.md").exists());
        assert!(project_root.join(".opencode/command/decision.md").exists());
        assert!(project_root
            .join(".opencode/command/build-test.md")
            .exists());
        assert!(project_root.join(".opencode/command/serve-ui.md").exists());
        assert!(project_root
            .join(".opencode/command/sync-graph.md")
            .exists());

        // Check skills (installed as commands)
        assert!(project_root.join(".opencode/command/pulse.md").exists());
        assert!(project_root
            .join(".opencode/command/narratives.md")
            .exists());
        assert!(project_root
            .join(".opencode/command/archaeology.md")
            .exists());

        // Check config files
        assert!(project_root.join("opencode.json").exists());
        assert!(project_root.join("AGENTS.md").exists());
    }
}
