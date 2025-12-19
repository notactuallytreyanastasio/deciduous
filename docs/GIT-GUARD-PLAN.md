# Git Guard Implementation Plan

## Overview

Git Guard provides **runtime-enforced** git safety rules that work across all AI coding tools (Claude Code, Windsurf, OpenCode, Codex). Unlike LLM-enforced rules in CLAUDE.md that can be forgotten, Git Guard uses actual hooks and command interception to guarantee enforcement.

**Key Design Decisions:**
- **No Python dependency** - All logic lives in Rust (`deciduous git-guard` subcommand)
- **Thin shell wrappers** - Git hooks and AI tool hooks just call `deciduous git-guard check`
- **Rebase is blocked** - Users are guided through manual rebase with safety steps
- **Codex uses git native hooks** - No AI tool hooks available, rely on git hooks only

---

## Architecture: Three-Layer Defense

```
┌─────────────────────────────────────────────────────────────┐
│                      User runs git command                   │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  Layer 1: AI Tool Hooks (intercepts BEFORE shell)           │
│  ─────────────────────────────────────────────────────────── │
│  • Claude Code: .claude/settings.json PreToolUse            │
│  • Windsurf: .windsurf/hooks.json pre_run_command           │
│  • OpenCode: --excludedTools or plugin                      │
│  • Codex: N/A (no hooks, relies on Layer 2 only)            │
│                                                              │
│  Shell wrapper calls: deciduous git-guard check <command>    │
│  Rust binary parses command, checks rules, returns verdict  │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  Layer 2: Git Native Hooks (.git/hooks/*)                    │
│  ─────────────────────────────────────────────────────────── │
│  • pre-commit: Block sensitive files, validate message       │
│  • pre-push: Block force push to protected branches          │
│  • pre-rebase: BLOCK ALL REBASES, guide user                 │
│                                                              │
│  Works for ALL tools including Codex.                        │
│  Shell wrapper calls: deciduous git-guard hook <hook-type>   │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  Layer 3: Logging                                            │
│  ─────────────────────────────────────────────────────────── │
│  All git operations logged to git.log                        │
│  Includes: timestamp, status (ALLOWED/BLOCKED), command     │
└─────────────────────────────────────────────────────────────┘
```

---

## Implementation: Rust Subcommand

All git-guard logic lives in the `deciduous` binary. No external dependencies.

### New CLI Subcommands

```bash
# Check a command before execution (used by AI tool hooks)
deciduous git-guard check "git push --force origin main"
# Exit code 0 = allow, exit code 2 = block
# Stdout contains JSON for Claude/Windsurf hooks

# Handle a git hook event (used by git native hooks)
deciduous git-guard hook pre-commit
deciduous git-guard hook pre-push
deciduous git-guard hook pre-rebase

# Interactive setup
deciduous git-guard init

# Show status and recent activity
deciduous git-guard status

# Temporarily disable (requires confirmation)
deciduous git-guard disable
```

### Rust Module Structure

```
src/
├── git_guard/
│   ├── mod.rs           # Public exports
│   ├── config.rs        # TOML config parsing
│   ├── check.rs         # Command checking logic
│   ├── hooks.rs         # Git hook handlers
│   ├── rules.rs         # Rule definitions and matching
│   ├── logging.rs       # git.log file operations
│   └── init.rs          # Interactive setup
└── main.rs              # Add git-guard subcommand
```

---

## File Structure

```
.deciduous/
├── git-guard.toml          # Configuration (TOML, Rust-native)
└── deciduous.db            # (existing)

.git/hooks/
├── pre-commit              # Thin shell wrapper
├── pre-push                # Thin shell wrapper
└── pre-rebase              # Thin shell wrapper (BLOCKS rebase)

.claude/
└── settings.json           # PreToolUse hook config (updated)

.windsurf/
└── hooks.json              # Cascade hook config

.opencode/
└── opencode.json           # Tool exclusions (or plugin if needed)

.codex/
└── (no hooks - relies on git native hooks only)
```

---

## Shell Wrappers

### Git Hook: `.git/hooks/pre-commit`

```bash
#!/bin/sh
# Git Guard pre-commit hook
# Calls deciduous binary for all logic

exec deciduous git-guard hook pre-commit
```

### Git Hook: `.git/hooks/pre-push`

```bash
#!/bin/sh
# Git Guard pre-push hook

# Read push info from stdin and pass to deciduous
exec deciduous git-guard hook pre-push
```

### Git Hook: `.git/hooks/pre-rebase`

```bash
#!/bin/sh
# Git Guard pre-rebase hook
# BLOCKS ALL REBASES - guides user through manual process

exec deciduous git-guard hook pre-rebase "$@"
```

### Claude Code Hook: Shell wrapper for PreToolUse

The Claude Code hook receives JSON on stdin. We need a wrapper that:
1. Reads the JSON
2. Extracts the command
3. Calls `deciduous git-guard check`
4. Formats response as JSON

```bash
#!/bin/sh
# .deciduous/hooks/claude-git-guard.sh
# Wrapper for Claude Code PreToolUse

# Pass stdin directly to deciduous, it handles JSON parsing
exec deciduous git-guard check --claude-mode
```

### Windsurf Hook: Shell wrapper for pre_run_command

```bash
#!/bin/sh
# .deciduous/hooks/windsurf-git-guard.sh
# Wrapper for Windsurf Cascade hooks

exec deciduous git-guard check --windsurf-mode
```

---

## Configuration: `.deciduous/git-guard.toml`

```toml
# Git Guard Configuration
# Runtime-enforced git safety rules

[general]
enabled = true
log_file = "git.log"

# ─────────────────────────────────────────────────────────────
# BANNED COMMANDS - Always blocked, no exceptions
# ─────────────────────────────────────────────────────────────
[banned]
commands = [
    # Destructive operations
    "git reset --hard*",
    "git clean -fd*",
    "git clean -f*",
    "rm -rf .git",
    "rm -rf .git/",

    # Force push to protected branches
    "git push --force origin main",
    "git push --force origin master",
    "git push -f origin main",
    "git push -f origin master",
    "git push origin +main",
    "git push origin +master",

    # Hard branch deletion
    "git branch -D *",

    # ALL rebase operations (guided manually instead)
    "git rebase*",
]

block_message = """
🛑 BLOCKED by Git Guard

This command is on the banned list.

For rebase operations, see the guidance below.
For other operations, ask the user to run manually with appropriate safeguards.
"""

# ─────────────────────────────────────────────────────────────
# REBASE GUIDANCE - Shown when rebase is blocked
# ─────────────────────────────────────────────────────────────
[rebase]
# Rebase is destructive - it rewrites commit history
# Block all rebase and guide user through safe manual process
blocked = true

guidance_message = """
🔄 REBASE BLOCKED - Manual Steps Required

Rebase rewrites commit history and can cause data loss.
Follow these steps manually:

1. CREATE BACKUP BRANCH:
   git branch backup-$(date +%Y%m%d-%H%M%S)

2. VERIFY YOU'RE ON THE RIGHT BRANCH:
   git branch --show-current
   git log --oneline -5

3. RUN REBASE MANUALLY:
   git rebase <target-branch>

4. IF CONFLICTS OCCUR:
   - Resolve conflicts
   - git add <resolved-files>
   - git rebase --continue
   - OR: git rebase --abort to cancel

5. VERIFY RESULT:
   git log --oneline -10
   git diff backup-<timestamp>..HEAD

6. IF PUSHING TO SHARED BRANCH:
   ⚠️  Force push required: git push --force-with-lease origin <branch>
   This will overwrite remote history!

The backup branch preserves your original state if anything goes wrong.
"""

# ─────────────────────────────────────────────────────────────
# SENSITIVE FILES - Never allow committing these
# ─────────────────────────────────────────────────────────────
[sensitive_files]
patterns = [
    ".env",
    ".env.*",
    "*.pem",
    "*.key",
    "*.p12",
    "*.pfx",
    "credentials.*",
    "secrets.*",
    "*_rsa",
    "*_ecdsa",
    "*_ed25519",
    "id_rsa*",
    "*.secret",
    "token.txt",
    "api_key*",
]

block_message = """
🔐 BLOCKED: Sensitive file detected

The following files appear to contain secrets:
{files}

Add them to .gitignore instead.
"""

# ─────────────────────────────────────────────────────────────
# COMMIT MESSAGE RULES
# ─────────────────────────────────────────────────────────────
[commit_message]
block_ai_attribution = true

ai_attribution_patterns = [
    "Generated with Claude",
    "Co-Authored-By: Claude",
    "Generated by AI",
    "🤖",
    "Claude Code",
    "Anthropic",
]

min_length = 10

# ─────────────────────────────────────────────────────────────
# PROTECTED BRANCHES
# ─────────────────────────────────────────────────────────────
[protected_branches]
names = ["main", "master", "production", "release/*"]
on_push = "confirm"  # "confirm", "block", or "allow"
allow_force_push = false

# ─────────────────────────────────────────────────────────────
# LOGGING
# ─────────────────────────────────────────────────────────────
[logging]
log_all_commands = true
log_file = "git.log"
format = "{timestamp} | {status} | {command}"
```

---

## Rust Implementation

### `src/git_guard/mod.rs`

```rust
//! Git Guard - Runtime-enforced git safety rules

mod config;
mod check;
mod hooks;
mod rules;
mod logging;
mod init;

pub use config::GitGuardConfig;
pub use check::check_command;
pub use hooks::handle_hook;
pub use init::interactive_init;
```

### `src/git_guard/check.rs`

```rust
//! Command checking logic

use crate::git_guard::{GitGuardConfig, logging, rules};
use std::io::{self, Read};

/// Check mode for different AI tools
pub enum CheckMode {
    /// Raw command string passed as argument
    Direct(String),
    /// Claude Code: JSON on stdin with tool_input.command
    Claude,
    /// Windsurf: JSON on stdin with command_line
    Windsurf,
}

/// Result of checking a command
pub enum CheckResult {
    Allow,
    Block { reason: String, guidance: Option<String> },
}

pub fn check_command(mode: CheckMode, config: &GitGuardConfig) -> CheckResult {
    let command = match mode {
        CheckMode::Direct(cmd) => cmd,
        CheckMode::Claude => parse_claude_stdin(),
        CheckMode::Windsurf => parse_windsurf_stdin(),
    };

    // Not a git command? Allow.
    if !command.starts_with("git ") {
        return CheckResult::Allow;
    }

    // Check banned commands
    if let Some(pattern) = rules::matches_banned(&command, config) {
        logging::log_command(&command, "BLOCKED", config);

        // Special handling for rebase
        if command.contains("rebase") {
            return CheckResult::Block {
                reason: format!("Matches banned pattern: {}", pattern),
                guidance: Some(config.rebase.guidance_message.clone()),
            };
        }

        return CheckResult::Block {
            reason: format!("Matches banned pattern: {}", pattern),
            guidance: None,
        };
    }

    // Log and allow
    logging::log_command(&command, "ALLOWED", config);
    CheckResult::Allow
}

fn parse_claude_stdin() -> String {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap_or_default();

    // Parse JSON: {"tool_input": {"command": "..."}}
    let parsed: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();
    parsed["tool_input"]["command"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

fn parse_windsurf_stdin() -> String {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap_or_default();

    // Parse JSON: {"command_line": "..."}
    let parsed: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();
    parsed["command_line"]
        .as_str()
        .unwrap_or("")
        .to_string()
}
```

### `src/git_guard/hooks.rs`

```rust
//! Git native hook handlers

use crate::git_guard::{GitGuardConfig, logging, rules};
use std::process::Command;

pub enum HookType {
    PreCommit,
    PrePush,
    PreRebase,
}

pub fn handle_hook(hook_type: HookType, config: &GitGuardConfig) -> Result<(), String> {
    match hook_type {
        HookType::PreCommit => handle_pre_commit(config),
        HookType::PrePush => handle_pre_push(config),
        HookType::PreRebase => handle_pre_rebase(config),
    }
}

fn handle_pre_commit(config: &GitGuardConfig) -> Result<(), String> {
    // Get staged files
    let output = Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .output()
        .map_err(|e| e.to_string())?;

    let files: Vec<&str> = std::str::from_utf8(&output.stdout)
        .unwrap_or("")
        .lines()
        .collect();

    // Check for sensitive files
    let sensitive = rules::find_sensitive_files(&files, config);
    if !sensitive.is_empty() {
        let msg = config.sensitive_files.block_message
            .replace("{files}", &sensitive.join("\n  - "));
        return Err(msg);
    }

    // Check commit message (if available via prepare-commit-msg)
    // This is handled separately

    Ok(())
}

fn handle_pre_push(config: &GitGuardConfig) -> Result<(), String> {
    // Read push info from stdin
    // Format: <local ref> <local sha> <remote ref> <remote sha>

    // Check for force push to protected branches
    // This is complex - simplified version:

    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 {
        let remote = &args[1];
        let _url = &args[2];

        // Check if pushing to protected branch with force
        // Full implementation would parse stdin for ref info
        logging::log_command(&format!("git push {}", remote), "CHECKED", config);
    }

    Ok(())
}

fn handle_pre_rebase(config: &GitGuardConfig) -> Result<(), String> {
    // ALWAYS block rebase and show guidance
    logging::log_command("git rebase", "BLOCKED", config);

    Err(config.rebase.guidance_message.clone())
}
```

---

## AI Tool Hook Configurations

### Claude Code: `.claude/settings.json`

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "deciduous git-guard check --claude-mode",
            "timeout": 5
          }
        ]
      }
    ]
  }
}
```

### Windsurf: `.windsurf/hooks.json`

```json
{
  "hooks": {
    "pre_run_command": [
      {
        "command": "deciduous git-guard check --windsurf-mode",
        "show_output": true
      }
    ]
  }
}
```

### OpenCode: `.opencode.json`

```json
{
  "shell": {
    "path": "/bin/bash"
  },
  "hooks": {
    "pre_tool": "deciduous git-guard check --opencode-mode"
  }
}
```

Note: OpenCode plugin system may require different approach.
Alternative: Use `--excludedTools` for dangerous git commands.

### Codex: No hooks available

Codex relies entirely on:
1. Git native hooks (pre-commit, pre-push, pre-rebase)
2. The git hooks call `deciduous git-guard hook <type>`
3. All blocking happens at git level, not AI tool level

---

## Init Integration

### Updated `deciduous init` Flow

```rust
pub fn init_project(editor: Editor) -> Result<(), String> {
    // ... existing init ...

    // Git Guard setup (prompt user)
    println!("\n{}", "Git Guard Setup".cyan().bold());
    println!("Git Guard provides runtime-enforced git safety rules.\n");

    print!("Enable Git Guard? [Y/n] ");
    // ... read input ...

    if enable_git_guard {
        init_git_guard(&cwd, editor)?;
    }

    // ... rest of init ...
}

fn init_git_guard(cwd: &Path, editor: Editor) -> Result<(), String> {
    // 1. Write default git-guard.toml
    let config_path = cwd.join(".deciduous").join("git-guard.toml");
    write_file_if_missing(&config_path, DEFAULT_GIT_GUARD_TOML)?;

    // 2. Install git hooks
    install_git_hooks(cwd)?;

    // 3. Configure AI tool hooks (except Codex)
    match editor {
        Editor::Claude => update_claude_settings_for_git_guard(cwd)?,
        Editor::Windsurf => update_windsurf_hooks_for_git_guard(cwd)?,
        Editor::Opencode => update_opencode_config_for_git_guard(cwd)?,
        Editor::Codex => {
            println!("   {} Codex uses git native hooks only", "Note:".yellow());
        }
    }

    Ok(())
}

fn install_git_hooks(cwd: &Path) -> Result<(), String> {
    let git_hooks_dir = cwd.join(".git").join("hooks");

    for hook in ["pre-commit", "pre-push", "pre-rebase"] {
        let hook_path = git_hooks_dir.join(hook);
        let script = format!(
            "#!/bin/sh\nexec deciduous git-guard hook {} \"$@\"\n",
            hook
        );
        fs::write(&hook_path, script)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o755))?;
        }

        println!("   {} .git/hooks/{}", "Creating".green(), hook);
    }

    Ok(())
}
```

---

## Rollout Plan

### Phase 1: Core Rust Implementation
1. Add `src/git_guard/` module structure
2. Implement TOML config parsing (`config.rs`)
3. Implement command checking logic (`check.rs`)
4. Implement rule matching (`rules.rs`)
5. Implement logging (`logging.rs`)
6. Add `git-guard` subcommand to CLI

### Phase 2: Git Native Hooks
1. Implement `hook` subcommand handlers
2. Generate hook scripts during init
3. Handle pre-commit (sensitive files)
4. Handle pre-push (protected branches)
5. Handle pre-rebase (BLOCK + guidance)

### Phase 3: AI Tool Hooks
1. Add `--claude-mode` JSON parsing
2. Add `--windsurf-mode` JSON parsing
3. Generate `.claude/settings.json` hooks config
4. Generate `.windsurf/hooks.json` config

### Phase 4: Polish
1. `deciduous git-guard status` command
2. `deciduous git-guard disable` command
3. Interactive init prompts
4. Documentation

---

## Decision Log Reference

See decision graph nodes 690-730 for full decision history:

| Node | Decision/Option |
|------|-----------------|
| 690 | Goal: Git Guard runtime enforcement |
| 691 | Decision: Enforcement architecture |
| 695 | Chosen: Hybrid (git hooks + AI hooks + wrappers) |
| 701 | Decision: Scripting language |
| 705 | Chosen: Rust subcommand (no Python) |
| 711 | Decision: Codex approach |
| 714 | Chosen: Git native hooks only |
| 716 | Decision: Rebase handling |
| 719 | Chosen: Block + guide user |
| 724 | Decision: Hook architecture |
| 726 | Chosen: Thin shell wrappers calling deciduous |

---

## References

- [Claude Code Hooks](https://docs.anthropic.com/claude-code/hooks) - PreToolUse event
- [Windsurf Cascade Hooks](https://docs.windsurf.com/windsurf/cascade/hooks) - pre_run_command event
- [OpenCode Extensibility](https://dev.to/einarcesar/does-opencode-support-hooks) - Plugin system
- [Git Hooks](https://git-scm.com/docs/githooks) - pre-commit, pre-push, pre-rebase
