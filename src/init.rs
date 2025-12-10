//! Project initialization for deciduous
//!
//! `deciduous init` creates all the files needed for decision graph tracking
//! Supports multiple editors: Claude Code (--claude) and Windsurf (--windsurf)

use colored::Colorize;
use std::fs;
use std::path::Path;

/// Editor environment for initialization
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Editor {
    Claude,
    Windsurf,
}

/// Static HTML viewer for GitHub Pages (embedded at compile time)
const PAGES_VIEWER_HTML: &str = include_str!("pages_viewer.html");

/// GitHub Pages deploy workflow (deploys to gh-pages branch, safe for project repos)
const DEPLOY_PAGES_WORKFLOW: &str = r#"name: Deploy Decision Graph to Pages

on:
  push:
    branches: [main]
    paths:
      - 'docs/**'
  workflow_dispatch:

permissions:
  contents: write

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Deploy to gh-pages branch
        uses: peaceiris/actions-gh-pages@v4
        with:
          github_token: ${{ secrets.GITHUB_TOKEN }}
          publish_dir: ./docs
          publish_branch: gh-pages
          force_orphan: true
"#;

/// Templates embedded at compile time
const DECISION_MD: &str = r#"---
description: Manage decision graph - track algorithm choices and reasoning
allowed-tools: Bash(deciduous:*)
argument-hint: <action> [args...]
---

# Decision Graph Management

**Log decisions IN REAL-TIME as you work, not retroactively.**

## When to Use This

| You're doing this... | Log this type | Command |
|---------------------|---------------|---------|
| Starting a new feature | `goal` | `/decision add goal "Add user auth"` |
| Choosing between approaches | `decision` | `/decision add decision "Choose auth method"` |
| Considering an option | `option` | `/decision add option "JWT tokens"` |
| About to write code | `action` | `/decision add action "Implementing JWT"` |
| Noticing something | `observation` | `/decision add obs "Found existing auth code"` |
| Finished something | `outcome` | `/decision add outcome "JWT working"` |

## Quick Commands

Based on $ARGUMENTS:

### View Commands
- `nodes` or `list` -> `deciduous nodes`
- `edges` -> `deciduous edges`
- `graph` -> `deciduous graph`
- `commands` -> `deciduous commands`

### Create Nodes (with optional confidence)
- `add goal <title>` -> `deciduous add goal "<title>" -c 90`
- `add decision <title>` -> `deciduous add decision "<title>" -c 75`
- `add option <title>` -> `deciduous add option "<title>" -c 70`
- `add action <title>` -> `deciduous add action "<title>" -c 85`
- `add obs <title>` -> `deciduous add observation "<title>" -c 80`
- `add outcome <title>` -> `deciduous add outcome "<title>" -c 90`

### Create Edges
- `link <from> <to> [reason]` -> `deciduous link <from> <to> -r "<reason>"`

### Sync Graph
- `sync` -> `deciduous sync`

### Export & Visualization
- `dot` -> `deciduous dot` (output DOT to stdout)
- `dot --png` -> `deciduous dot --png -o graph.dot` (generate PNG)
- `dot --nodes 1-11` -> `deciduous dot --nodes 1-11` (filter nodes)
- `writeup` -> `deciduous writeup` (generate PR writeup)
- `writeup -t "Title" --nodes 1-11` -> filtered writeup

## Node Types

| Type | Purpose | Example |
|------|---------|---------|
| `goal` | High-level objective | "Add user authentication" |
| `decision` | Choice point with options | "Choose auth method" |
| `option` | Possible approach | "Use JWT tokens" |
| `action` | Something implemented | "Added JWT middleware" |
| `outcome` | Result of action | "JWT auth working" |
| `observation` | Finding or data point | "Existing code uses sessions" |

## Edge Types

| Type | Meaning |
|------|---------|
| `leads_to` | Natural progression |
| `chosen` | Selected option |
| `rejected` | Not selected (include reason!) |
| `requires` | Dependency |
| `blocks` | Preventing progress |
| `enables` | Makes something possible |

## The Rule

```
LOG BEFORE YOU CODE, NOT AFTER.
SYNC BEFORE YOU PUSH.
```
"#;

const CONTEXT_MD: &str = r#"---
description: Recover context from decision graph and recent activity - USE THIS ON SESSION START
allowed-tools: Bash(deciduous:*, git:*)
argument-hint: [focus-area]
---

# Context Recovery

**RUN THIS AT SESSION START.** The decision graph is your persistent memory.

## Step 1: Query the Graph

```bash
# See all decisions (look for recent ones and pending status)
deciduous nodes

# See how decisions connect
deciduous edges

# What commands were recently run?
deciduous commands
```

## Step 2: Check Git State

```bash
git status
git log --oneline -10
git diff --stat
```

## After Gathering Context, Report:

1. **Current branch** and pending changes
2. **Recent decisions** (especially pending/active ones)
3. **Last actions** from git log and command log
4. **Open questions** or unresolved observations
5. **Suggested next steps**

---

## REMEMBER: Real-Time Logging Required

After recovering context, you MUST follow the logging workflow:

```
EVERY USER REQUEST -> Log goal/decision first
BEFORE CODE CHANGES -> Log action
AFTER CHANGES -> Log outcome, link nodes
BEFORE GIT PUSH -> deciduous sync
```

### Quick Logging Commands

```bash
deciduous add goal "What we're trying to do" -c 90
deciduous add action "What I'm about to implement" -c 85
deciduous add outcome "What happened" -c 95
deciduous link FROM TO -r "Connection reason"
deciduous sync  # Do this frequently!
```

---

## The Memory Loop

```
SESSION START
    |
Run /context -> See past decisions
    |
DO WORK -> Log BEFORE each action
    |
AFTER CHANGES -> Log outcomes, observations
    |
BEFORE PUSH -> deciduous sync
    |
PUSH -> Graph persists
    |
SESSION END -> Graph survives
    |
(repeat)
```

---

## Why This Matters

- Context loss during compaction loses your reasoning
- The graph survives - query it early, query it often
- Retroactive logging misses details - log in the moment
"#;

const CLEANUP_WORKFLOW: &str = r#"name: Cleanup Decision Graph PNGs

on:
  pull_request:
    types: [closed]

jobs:
  cleanup:
    # Only run if PR was merged (not just closed)
    if: github.event.pull_request.merged == true
    runs-on: ubuntu-latest

    steps:
      - name: Checkout
        uses: actions/checkout@v4
        with:
          fetch-depth: 0
          token: ${{ secrets.GITHUB_TOKEN }}

      - name: Find and remove decision graph PNGs
        id: find-pngs
        run: |
          # Find decision graph PNGs (in docs/ or root)
          PNGS=$(find . -name "decision-graph*.png" -o -name "deciduous-graph*.png" 2>/dev/null | grep -v node_modules || true)

          if [ -z "$PNGS" ]; then
            echo "No decision graph PNGs found"
            echo "found=false" >> $GITHUB_OUTPUT
          else
            echo "Found PNGs to clean up:"
            echo "$PNGS"
            echo "found=true" >> $GITHUB_OUTPUT

            # Remove the files
            echo "$PNGS" | xargs rm -f

            # Also remove corresponding .dot files
            for png in $PNGS; do
              dot_file="${png%.png}.dot"
              if [ -f "$dot_file" ]; then
                rm -f "$dot_file"
                echo "Also removed: $dot_file"
              fi
            done
          fi

      - name: Create cleanup PR
        if: steps.find-pngs.outputs.found == 'true'
        run: |
          # Check if there are changes to commit
          if git diff --quiet && git diff --staged --quiet; then
            echo "No changes to commit"
            exit 0
          fi

          # Configure git
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"

          # Create branch and commit
          BRANCH="cleanup/decision-graphs-pr-${{ github.event.pull_request.number }}"
          git checkout -b "$BRANCH"
          git add -A
          git commit -m "chore: cleanup decision graph assets from PR #${{ github.event.pull_request.number }}"
          git push origin "$BRANCH"

          # Create and auto-merge PR
          gh pr create \
            --title "chore: cleanup decision graph assets from PR #${{ github.event.pull_request.number }}" \
            --body "Automated cleanup of decision graph PNG/DOT files that were used in PR #${{ github.event.pull_request.number }}.

          These files served their purpose for PR review and are no longer needed." \
            --head "$BRANCH" \
            --base main

          # Auto-merge (requires auto-merge enabled on repo)
          gh pr merge "$BRANCH" --auto --squash --delete-branch || echo "Auto-merge not enabled, PR created for manual merge"
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
"#;

const CLAUDE_MD_SECTION: &str = r#"
## Decision Graph Workflow

**THIS IS MANDATORY. Log decisions IN REAL-TIME, not retroactively.**

### The Core Rule

```
BEFORE you do something -> Log what you're ABOUT to do
AFTER it succeeds/fails -> Log the outcome
ALWAYS -> Sync frequently so the graph updates
```

### Behavioral Triggers - MUST LOG WHEN:

| Trigger | Log Type | Example |
|---------|----------|---------|
| User asks for a new feature | `goal` | "Add dark mode" |
| Choosing between approaches | `decision` | "Choose state management" |
| About to write/edit code | `action` | "Implementing Redux store" |
| Something worked or failed | `outcome` | "Redux integration successful" |
| Notice something interesting | `observation` | "Existing code uses hooks" |

### Quick Commands

```bash
deciduous add goal "Title" -c 90
deciduous add decision "Title" -c 75
deciduous add action "Title" -c 85
deciduous link FROM TO -r "reason"
deciduous serve   # View live
deciduous sync    # Export for static hosting
deciduous dot --png -o graph.dot  # Generate visualization
deciduous writeup -t "PR Title"   # Generate PR writeup
```

### Session Start Checklist

Every new session, run `/context` or:

```bash
deciduous nodes    # What decisions exist?
deciduous edges    # How are they connected?
git status         # Current state
git log -10        # Recent commits
```
"#;

// ============================================================================
// WINDSURF-SPECIFIC TEMPLATES
// ============================================================================

/// Windsurf main rule - placed in .windsurf/rules/deciduous.md
/// "Always On" trigger ensures Cascade enforces decision graph workflow on every interaction
const WINDSURF_DECIDUOUS_RULE: &str = r#"---
trigger: always
---

<decision_graph_workflow>

# Decision Graph - ALWAYS ON

This project uses Deciduous for persistent decision tracking. You MUST log decisions in real-time.

## MANDATORY: Log These Events

<logging_triggers>
- **New feature request** → `deciduous add goal "Feature name" -c 90`
- **Choosing between approaches** → `deciduous add decision "What to decide" -c 75`
- **Considering an option** → `deciduous add option "Option name" -c 70`
- **About to write code** → `deciduous add action "What you're implementing" -c 85`
- **Noticed something** → `deciduous add observation "What you found" -c 80`
- **Something completed** → `deciduous add outcome "Result" -c 95`
</logging_triggers>

## MANDATORY: The Feedback Loop

<workflow>
1. USER REQUEST → Log goal/decision FIRST (before any code)
2. BEFORE coding → Log action
3. AFTER changes → Log outcome + link nodes
4. BEFORE git push → Run `deciduous sync`
</workflow>

## Commands

<commands>
```bash
# Add nodes (with confidence 0-100)
deciduous add goal "Title" -c 90
deciduous add decision "Title" -c 75
deciduous add action "Title" -c 85
deciduous add outcome "Title" -c 95
deciduous add observation "Title" -c 80

# Link nodes together
deciduous link <from> <to> -r "reason"
deciduous link 1 2 --edge-type chosen -r "Selected this approach"

# View current graph
deciduous nodes
deciduous edges

# Sync before pushing
deciduous sync
```
</commands>

## Edge Types

<edge_types>
- `leads_to` - Natural progression (default)
- `chosen` - Selected this option
- `rejected` - Did not select (include why!)
- `requires` - Dependency
- `blocks` - Preventing progress
- `enables` - Makes possible
</edge_types>

</decision_graph_workflow>
"#;

/// Windsurf context rule - placed in .windsurf/rules/context.md
/// Model-triggered rule that fires when Cascade detects session start or context questions
const WINDSURF_CONTEXT_RULE: &str = r#"---
trigger: model
description: Use when starting a new session, recovering context, or when the user asks about previous work
---

<context_recovery>

# Context Recovery

When starting a session or recovering context, query the decision graph:

<session_start>
```bash
# 1. See what decisions exist (look for recent/pending)
deciduous nodes

# 2. See how they connect
deciduous edges

# 3. Check git state
git status
git log --oneline -10
```
</session_start>

## After Querying, Report:

1. Current branch and pending changes
2. Recent decisions (especially pending/active ones)
3. Last actions from the command log
4. Suggested next steps

<important>
The decision graph survives context loss. Query it whenever you need to understand
what was decided previously. Then continue logging per the deciduous.md rule.
</important>

</context_recovery>
"#;

/// Windsurf memories file - placed in .windsurf/memories.md
/// Project-level memories that Cascade auto-retrieves when relevant
const WINDSURF_MEMORIES: &str = r#"# Project Memories

## Decision Graph System

This project uses **Deciduous** for persistent decision tracking.

<memory>
**What is Deciduous?**
A decision graph tool that tracks goals, decisions, actions, and outcomes.
The graph survives context loss - when sessions end or context compacts,
the reasoning persists in the database.
</memory>

<memory>
**Why log decisions?**
- Context compaction loses your reasoning
- The graph survives and is queryable by future sessions
- Retroactive logging misses details - log in the moment
- The user may be watching the graph live
</memory>

<memory>
**Key Commands**
- `deciduous nodes` - see all decisions
- `deciduous edges` - see connections
- `deciduous add <type> "title" -c <confidence>` - add node
- `deciduous link <from> <to> -r "reason"` - connect nodes
- `deciduous sync` - export to docs/graph-data.json
</memory>

<memory>
**Node Types**
- goal: High-level objective
- decision: Choice point
- option: Possible approach
- action: Implementation step
- outcome: Result
- observation: Finding/insight
</memory>

<memory>
**The Rule**
LOG BEFORE YOU CODE, NOT AFTER.
SYNC BEFORE YOU PUSH.
</memory>
"#;

/// AGENTS.md section for Windsurf (equivalent to CLAUDE.md section)
const AGENTS_MD_SECTION: &str = r#"
## Decision Graph Workflow

**THIS IS MANDATORY. Log decisions IN REAL-TIME, not retroactively.**

### The Core Rule

```
BEFORE you do something -> Log what you're ABOUT to do
AFTER it succeeds/fails -> Log the outcome
ALWAYS -> Sync frequently so the graph updates
```

### Behavioral Triggers - MUST LOG WHEN:

| Trigger | Log Type | Example |
|---------|----------|---------|
| User asks for a new feature | `goal` | "Add dark mode" |
| Choosing between approaches | `decision` | "Choose state management" |
| About to write/edit code | `action` | "Implementing Redux store" |
| Something worked or failed | `outcome` | "Redux integration successful" |
| Notice something interesting | `observation` | "Existing code uses hooks" |

### Quick Commands

```bash
deciduous add goal "Title" -c 90
deciduous add decision "Title" -c 75
deciduous add action "Title" -c 85
deciduous link FROM TO -r "reason"
deciduous serve   # View live
deciduous sync    # Export for static hosting
deciduous dot --png -o graph.dot  # Generate visualization
deciduous writeup -t "PR Title"   # Generate PR writeup
```

### Session Start Checklist

Every new session, run:

```bash
deciduous nodes    # What decisions exist?
deciduous edges    # How are they connected?
git status         # Current state
git log -10        # Recent commits
```
"#;

/// Initialize deciduous in the current directory
pub fn init_project(editor: Editor) -> Result<(), String> {
    let cwd = std::env::current_dir()
        .map_err(|e| format!("Could not get current directory: {}", e))?;

    let editor_name = match editor {
        Editor::Claude => "Claude Code",
        Editor::Windsurf => "Windsurf",
    };

    println!("\n{}", format!("Initializing Deciduous for {}...", editor_name).cyan().bold());
    println!("   Directory: {}\n", cwd.display());

    // 1. Create .deciduous directory (shared between all editors)
    let deciduous_dir = cwd.join(".deciduous");
    create_dir_if_missing(&deciduous_dir)?;

    // 2. Initialize database by opening it (creates tables)
    let db_path = deciduous_dir.join("deciduous.db");
    if db_path.exists() {
        println!("   {} .deciduous/deciduous.db (already exists, preserving data)", "Skipping".yellow());
    } else {
        println!("   {} {}", "Creating".green(), ".deciduous/deciduous.db");
    }

    // Set the env var so Database::open() uses this path
    // Database::open() uses CREATE TABLE IF NOT EXISTS - safe for existing DBs
    std::env::set_var("DECIDUOUS_DB_PATH", &db_path);

    // 3. Create editor-specific configuration
    match editor {
        Editor::Claude => {
            // Create .claude/commands directory
            let claude_dir = cwd.join(".claude").join("commands");
            create_dir_if_missing(&claude_dir)?;

            // Write decision.md slash command
            let decision_path = claude_dir.join("decision.md");
            write_file_if_missing(&decision_path, DECISION_MD, ".claude/commands/decision.md")?;

            // Write context.md slash command
            let context_path = claude_dir.join("context.md");
            write_file_if_missing(&context_path, CONTEXT_MD, ".claude/commands/context.md")?;

            // Append to or create CLAUDE.md
            let claude_md_path = cwd.join("CLAUDE.md");
            append_config_md(&claude_md_path, CLAUDE_MD_SECTION, "CLAUDE.md")?;
        }
        Editor::Windsurf => {
            // Create .windsurf directory
            let windsurf_base = cwd.join(".windsurf");
            create_dir_if_missing(&windsurf_base)?;

            // Create .windsurf/rules directory
            let windsurf_rules = windsurf_base.join("rules");
            create_dir_if_missing(&windsurf_rules)?;

            // Write deciduous.md rule (Always On - main workflow)
            let deciduous_rule_path = windsurf_rules.join("deciduous.md");
            write_file_if_missing(&deciduous_rule_path, WINDSURF_DECIDUOUS_RULE, ".windsurf/rules/deciduous.md")?;

            // Write context.md rule (Model-triggered - for session recovery)
            let context_path = windsurf_rules.join("context.md");
            write_file_if_missing(&context_path, WINDSURF_CONTEXT_RULE, ".windsurf/rules/context.md")?;

            // Write memories.md (project-level memories Cascade auto-retrieves)
            let memories_path = windsurf_base.join("memories.md");
            write_file_if_missing(&memories_path, WINDSURF_MEMORIES, ".windsurf/memories.md")?;

            // Append to or create AGENTS.md
            let agents_md_path = cwd.join("AGENTS.md");
            append_config_md(&agents_md_path, AGENTS_MD_SECTION, "AGENTS.md")?;
        }
    }

    // 4. Add .deciduous to .gitignore if not already there
    add_to_gitignore(&cwd)?;

    // 5. Create GitHub workflows (if .git exists)
    let git_dir = cwd.join(".git");
    if git_dir.exists() {
        let workflows_dir = cwd.join(".github").join("workflows");
        create_dir_if_missing(&workflows_dir)?;

        // Cleanup workflow for PR graph assets
        let cleanup_path = workflows_dir.join("cleanup-decision-graphs.yml");
        write_file_if_missing(&cleanup_path, CLEANUP_WORKFLOW, ".github/workflows/cleanup-decision-graphs.yml")?;

        // Deploy workflow for GitHub Pages
        let deploy_path = workflows_dir.join("deploy-pages.yml");
        write_file_if_missing(&deploy_path, DEPLOY_PAGES_WORKFLOW, ".github/workflows/deploy-pages.yml")?;
    }

    // 6. Create docs/ directory for GitHub Pages
    let docs_dir = cwd.join("docs");
    create_dir_if_missing(&docs_dir)?;

    // 7. Write static viewer HTML to docs/index.html
    let viewer_path = docs_dir.join("index.html");
    write_file_if_missing(&viewer_path, PAGES_VIEWER_HTML, "docs/index.html")?;

    // 8. Create empty graph-data.json (will be populated by sync)
    let graph_data_path = docs_dir.join("graph-data.json");
    if !graph_data_path.exists() {
        let empty_graph = r#"{"nodes":[],"edges":[]}"#;
        fs::write(&graph_data_path, empty_graph)
            .map_err(|e| format!("Could not write graph-data.json: {}", e))?;
        println!("   {} docs/graph-data.json", "Creating".green());
    }

    // 9. Create .nojekyll for GitHub Pages (prevents Jekyll processing)
    let nojekyll_path = docs_dir.join(".nojekyll");
    if !nojekyll_path.exists() {
        fs::write(&nojekyll_path, "")
            .map_err(|e| format!("Could not write .nojekyll: {}", e))?;
        println!("   {} docs/.nojekyll", "Creating".green());
    }

    println!("\n{}", format!("Deciduous initialized for {}!", editor_name).green().bold());
    println!("\nNext steps:");
    println!("  1. Run {} to start the local graph viewer", "deciduous serve".cyan());
    println!("  2. Run {} to export graph for GitHub Pages", "deciduous sync".cyan());

    match editor {
        Editor::Claude => {
            println!("  3. Use {} or {} slash commands", "/decision".cyan(), "/context".cyan());
        }
        Editor::Windsurf => {
            println!("  3. Cascade will auto-enforce rules from {}", ".windsurf/rules/".cyan());
            println!("     - {} (Always On)", "deciduous.md".cyan());
            println!("     - {} (Model-triggered)", "context.md".cyan());
            println!("     - {} (Auto-retrieved memories)", ".windsurf/memories.md".cyan());
        }
    }

    println!("  4. Commit and push: {}", "git add docs/ .github/ && git push".cyan());
    println!("  5. Enable GitHub Pages (Settings → Pages → Source: Deploy from branch, gh-pages)");
    println!();
    println!("Your graph will be live at: {}", "https://<user>.github.io/<repo>/".cyan());
    println!();

    Ok(())
}

fn create_dir_if_missing(path: &Path) -> Result<(), String> {
    if !path.exists() {
        fs::create_dir_all(path)
            .map_err(|e| format!("Could not create {}: {}", path.display(), e))?;
        println!("   {} {}", "Creating".green(), path.display());
    }
    Ok(())
}

fn write_file_if_missing(path: &Path, content: &str, display_name: &str) -> Result<(), String> {
    if path.exists() {
        println!("   {} {} (already exists)", "Skipping".yellow(), display_name);
    } else {
        fs::write(path, content)
            .map_err(|e| format!("Could not write {}: {}", display_name, e))?;
        println!("   {} {}", "Creating".green(), display_name);
    }
    Ok(())
}

/// Append the Decision Graph Workflow section to a config file (CLAUDE.md or AGENTS.md)
fn append_config_md(path: &Path, section_content: &str, file_name: &str) -> Result<(), String> {
    let marker = "## Decision Graph Workflow";

    if path.exists() {
        let existing = fs::read_to_string(path)
            .map_err(|e| format!("Could not read {}: {}", file_name, e))?;

        if existing.contains(marker) {
            println!("   {} {} (workflow section already present)", "Skipping".yellow(), file_name);
            return Ok(());
        }

        // Append the section
        let new_content = format!("{}\n{}", existing.trim_end(), section_content);
        fs::write(path, new_content)
            .map_err(|e| format!("Could not update {}: {}", file_name, e))?;
        println!("   {} {} (added workflow section)", "Updated".green(), file_name);
    } else {
        // Create new file
        let content = format!("# Project Instructions\n{}", section_content);
        fs::write(path, content)
            .map_err(|e| format!("Could not create {}: {}", file_name, e))?;
        println!("   {} {}", "Creating".green(), file_name);
    }

    Ok(())
}

fn add_to_gitignore(cwd: &Path) -> Result<(), String> {
    let gitignore_path = cwd.join(".gitignore");
    let entry = ".deciduous/";

    if gitignore_path.exists() {
        let existing = fs::read_to_string(&gitignore_path)
            .map_err(|e| format!("Could not read .gitignore: {}", e))?;

        if existing.lines().any(|line| line.trim() == entry || line.trim() == ".deciduous") {
            // Already in gitignore
            return Ok(());
        }

        // Append
        let new_content = format!("{}\n\n# Deciduous database (local)\n{}\n", existing.trim_end(), entry);
        fs::write(&gitignore_path, new_content)
            .map_err(|e| format!("Could not update .gitignore: {}", e))?;
        println!("   {} .gitignore (added .deciduous/)", "Updated".green());
    } else {
        // Create new .gitignore
        let content = format!("# Deciduous database (local)\n{}\n", entry);
        fs::write(&gitignore_path, content)
            .map_err(|e| format!("Could not create .gitignore: {}", e))?;
        println!("   {} .gitignore", "Creating".green());
    }

    Ok(())
}
