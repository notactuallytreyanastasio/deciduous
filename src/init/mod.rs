//! Project initialization for deciduous
//!
//! `deciduous init` creates all the files needed for decision graph tracking
//! with AI assistant integration (Claude Code and/or OpenCode).

pub mod templates;

use crate::opencode;
use colored::Colorize;
use std::fs;
use std::path::Path;

use templates::{
    CLAUDE_AGENTS_TOML, CLAUDE_MD_SECTION, CLAUDE_SETTINGS_JSON, CLEANUP_WORKFLOW, DECISION_MD,
    DEFAULT_CONFIG, DEPLOY_PAGES_WORKFLOW, HOOK_POST_COMMIT_REMINDER, HOOK_REQUIRE_ACTION_NODE,
    PAGES_VIEWER_HTML, RECOVER_MD, SKILL_ARCHAEOLOGY, SKILL_NARRATIVES, SKILL_PULSE, WORK_MD,
    WINDSURF_HOOKS_JSON, WINDSURF_HOOK_POST_COMMIT_REMINDER, WINDSURF_HOOK_REQUIRE_ACTION_NODE,
    WINDSURF_RULES_DECIDUOUS,
};

/// Initialize a new deciduous project with AI assistant integration
///
/// # Arguments
/// * `setup_claude` - Whether to set up Claude Code integration
/// * `setup_opencode` - Whether to set up OpenCode integration
/// * `setup_windsurf` - Whether to set up Windsurf integration (also auto-detects .windsurf/)
pub fn init_project(setup_claude: bool, setup_opencode: bool, setup_windsurf: bool) -> Result<(), String> {
    let cwd =
        std::env::current_dir().map_err(|e| format!("Could not get current directory: {}", e))?;

    let assistant_name = match (setup_claude, setup_opencode) {
        (true, true) => "Claude Code + OpenCode",
        (true, false) => "Claude Code",
        (false, true) => "OpenCode",
        (false, false) => return Err("At least one assistant must be selected".to_string()),
    };

    println!(
        "\n{}",
        format!("Initializing Deciduous for {}...", assistant_name)
            .cyan()
            .bold()
    );
    println!("   Directory: {}\n", cwd.display());

    // 1. Create .deciduous directory
    let deciduous_dir = cwd.join(".deciduous");
    create_dir_if_missing(&deciduous_dir)?;

    // 1b. Create default config.toml if it doesn't exist
    let config_path = deciduous_dir.join("config.toml");
    write_file_if_missing(&config_path, DEFAULT_CONFIG, ".deciduous/config.toml")?;

    // 1c. Write version file for auto-update detection
    let version_path = deciduous_dir.join(".version");
    let version = env!("CARGO_PKG_VERSION");
    fs::write(&version_path, version)
        .map_err(|e| format!("Could not write version file: {}", e))?;
    println!(
        "   {} .deciduous/.version ({})",
        "Creating".green(),
        version
    );

    // 2. Initialize database by opening it (creates tables)
    let db_path = deciduous_dir.join("deciduous.db");
    if db_path.exists() {
        println!(
            "   {} .deciduous/deciduous.db (already exists, preserving data)",
            "Skipping".yellow()
        );
    } else {
        println!("   {} .deciduous/deciduous.db", "Creating".green());
    }

    // Set the env var so Database::open() uses this path
    std::env::set_var("DECIDUOUS_DB_PATH", &db_path);

    // 3. Create Claude Code configuration (if enabled)
    if setup_claude {
        // Create .claude/commands directory
        let claude_dir = cwd.join(".claude").join("commands");
        create_dir_if_missing(&claude_dir)?;

        // Write decision.md slash command
        let decision_path = claude_dir.join("decision.md");
        write_file_if_missing(&decision_path, DECISION_MD, ".claude/commands/decision.md")?;

        // Write recover.md slash command
        let recover_path = claude_dir.join("recover.md");
        write_file_if_missing(&recover_path, RECOVER_MD, ".claude/commands/recover.md")?;

        // Write work.md slash command (transaction model)
        let work_path = claude_dir.join("work.md");
        write_file_if_missing(&work_path, WORK_MD, ".claude/commands/work.md")?;

        // Write agents.toml for subagent configuration
        let claude_base = cwd.join(".claude");
        let agents_path = claude_base.join("agents.toml");
        write_file_if_missing(&agents_path, CLAUDE_AGENTS_TOML, ".claude/agents.toml")?;

        // Create .claude/hooks directory and write enforcement hooks
        let hooks_dir = claude_base.join("hooks");
        create_dir_if_missing(&hooks_dir)?;

        // Write require-action-node.sh hook
        let require_action_path = hooks_dir.join("require-action-node.sh");
        write_executable_if_missing(
            &require_action_path,
            HOOK_REQUIRE_ACTION_NODE,
            ".claude/hooks/require-action-node.sh",
        )?;

        // Write post-commit-reminder.sh hook
        let post_commit_path = hooks_dir.join("post-commit-reminder.sh");
        write_executable_if_missing(
            &post_commit_path,
            HOOK_POST_COMMIT_REMINDER,
            ".claude/hooks/post-commit-reminder.sh",
        )?;

        // Write settings.json with hooks configuration
        let settings_path = claude_base.join("settings.json");
        write_file_if_missing(
            &settings_path,
            CLAUDE_SETTINGS_JSON,
            ".claude/settings.json",
        )?;

        // Create .claude/skills directory and write skill files
        let skills_dir = claude_base.join("skills");
        create_dir_if_missing(&skills_dir)?;

        let pulse_path = skills_dir.join("pulse.md");
        write_file_if_missing(&pulse_path, SKILL_PULSE, ".claude/skills/pulse.md")?;

        let narratives_path = skills_dir.join("narratives.md");
        write_file_if_missing(
            &narratives_path,
            SKILL_NARRATIVES,
            ".claude/skills/narratives.md",
        )?;

        let archaeology_path = skills_dir.join("archaeology.md");
        write_file_if_missing(
            &archaeology_path,
            SKILL_ARCHAEOLOGY,
            ".claude/skills/archaeology.md",
        )?;

        // Append to or create CLAUDE.md
        let claude_md_path = cwd.join("CLAUDE.md");
        append_config_md(&claude_md_path, CLAUDE_MD_SECTION, "CLAUDE.md")?;
    }

    // 3b. Create OpenCode configuration (if enabled)
    if setup_opencode {
        opencode::install_opencode(&cwd)?;
    }

    // 3c. Set up Windsurf if requested OR if .windsurf directory exists
    let windsurf_dir = cwd.join(".windsurf");
    let should_setup_windsurf = setup_windsurf || windsurf_dir.exists();
    if should_setup_windsurf {
        // Create .windsurf directory if it doesn't exist (when --windsurf flag used)
        if !windsurf_dir.exists() {
            create_dir_if_missing(&windsurf_dir)?;
        }
        if setup_windsurf {
            println!(
                "\n{}",
                "Setting up Windsurf integration...".cyan()
            );
        } else {
            println!(
                "\n{}",
                "Detected .windsurf directory - setting up Windsurf integration..."
                    .cyan()
            );
        }
        setup_windsurf_integration(&cwd)?;
    }

    // 4. Add .deciduous to .gitignore
    add_to_gitignore(&cwd)?;

    // 5. Create GitHub workflows directory and workflows
    let github_dir = cwd.join(".github");
    if github_dir.exists() || cwd.join(".git").exists() {
        let workflows_dir = github_dir.join("workflows");
        create_dir_if_missing(&workflows_dir)?;

        // Cleanup workflow for PR graph assets
        let cleanup_path = workflows_dir.join("cleanup-decision-graphs.yml");
        write_file_if_missing(
            &cleanup_path,
            CLEANUP_WORKFLOW,
            ".github/workflows/cleanup-decision-graphs.yml",
        )?;

        // Deploy workflow for GitHub Pages
        let deploy_path = workflows_dir.join("deploy-pages.yml");
        write_file_if_missing(
            &deploy_path,
            DEPLOY_PAGES_WORKFLOW,
            ".github/workflows/deploy-pages.yml",
        )?;
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
        fs::write(&nojekyll_path, "").map_err(|e| format!("Could not write .nojekyll: {}", e))?;
        println!("   {} docs/.nojekyll", "Creating".green());
    }

    // Check if Windsurf was set up (use the flag we already computed)
    let windsurf_configured = should_setup_windsurf;

    let final_name = if windsurf_configured {
        format!("{} + Windsurf", assistant_name)
    } else {
        assistant_name.to_string()
    };

    println!(
        "\n{}",
        format!("Deciduous initialized for {}!", final_name)
            .green()
            .bold()
    );
    println!("\nNext steps:");
    println!(
        "  1. Run {} to start the local graph viewer",
        "deciduous serve".cyan()
    );
    println!(
        "  2. Run {} to export graph for GitHub Pages",
        "deciduous sync".cyan()
    );
    println!(
        "  3. Use {} or {} slash commands",
        "/decision".cyan(),
        "/recover".cyan()
    );
    println!();
    println!(
        "  4. Commit and push: {}",
        "git add docs/ .github/ && git push".cyan()
    );
    println!(
        "  5. Enable GitHub Pages (Settings -> Pages -> Source: Deploy from branch, gh-pages)"
    );
    println!();
    println!(
        "Your graph will be live at: {}",
        "https://<user>.github.io/<repo>/".cyan()
    );

    if windsurf_configured {
        println!();
        println!(
            "{}",
            "Windsurf integration:".cyan().bold()
        );
        println!("  - Hooks configured in .windsurf/hooks.json");
        println!("  - Always-on rules in .windsurf/rules/deciduous.md");
        println!("  - Pre-write hook blocks edits without action nodes");
        println!("  - Post-command hook reminds to link commits");
    }

    println!();

    Ok(())
}

/// Update tooling files to the latest version (overwrites existing)
/// Auto-detects which assistants are installed (.claude/ and/or .opencode/)
pub fn update_tooling() -> Result<(), String> {
    let cwd =
        std::env::current_dir().map_err(|e| format!("Could not get current directory: {}", e))?;

    // Verify .deciduous exists
    let deciduous_dir = cwd.join(".deciduous");
    if !deciduous_dir.exists() {
        println!(
            "   {} .deciduous/ not found - run 'deciduous init' first",
            "Warning:".yellow()
        );
    }

    // Auto-detect which assistants are installed
    let claude_dir = cwd.join(".claude");
    let opencode_dir = cwd.join(".opencode");

    let has_claude = claude_dir.exists();
    let has_opencode = opencode_dir.exists();

    if !has_claude && !has_opencode {
        return Err(
            "No assistant integration found. Run 'deciduous init' first, or use 'deciduous init --opencode' / 'deciduous init --claude'."
                .to_string(),
        );
    }

    let assistant_name = match (has_claude, has_opencode) {
        (true, true) => "Claude Code + OpenCode",
        (true, false) => "Claude Code",
        (false, true) => "OpenCode",
        (false, false) => unreachable!(),
    };

    println!(
        "\n{}",
        format!("Updating Deciduous tooling for {}...", assistant_name)
            .cyan()
            .bold()
    );
    println!("   Directory: {}\n", cwd.display());

    // Update Claude Code if installed
    if has_claude {
        update_claude_code(&cwd)?;
    }

    // Update OpenCode if installed
    if has_opencode {
        opencode::update_opencode(&cwd)?;
    }

    // Update Windsurf if .windsurf directory exists
    let windsurf_dir = cwd.join(".windsurf");
    let has_windsurf = windsurf_dir.exists();
    if has_windsurf {
        println!(
            "\n{}",
            "Detected .windsurf directory - updating Windsurf integration..."
                .cyan()
        );
        update_windsurf(&cwd)?;
    }

    // Write version file for auto-update detection
    if deciduous_dir.exists() {
        let version_path = deciduous_dir.join(".version");
        let version = env!("CARGO_PKG_VERSION");
        fs::write(&version_path, version)
            .map_err(|e| format!("Could not write version file: {}", e))?;
        println!("   {} .deciduous/.version ({})", "Updated".green(), version);
    }

    let final_name = if has_windsurf {
        format!("{} + Windsurf", assistant_name)
    } else {
        assistant_name.to_string()
    };

    println!(
        "\n{}",
        format!("Tooling updated for {}!", final_name)
            .green()
            .bold()
    );
    println!("\nUpdated files contain the latest:");
    println!("  - Slash commands (/decision, /recover, /work)");
    println!("  - Skills (/pulse, /narratives, /archaeology)");
    if has_claude {
        println!("  - Enforcement hooks (block edits without action nodes)");
        println!("  - Post-commit reminders (link commits to graph)");
        println!("  - Agent configurations (agents.toml)");
    }
    if has_opencode {
        println!("  - OpenCode plugins (TypeScript hooks)");
        println!("  - OpenCode configuration (opencode.json)");
    }
    if has_windsurf {
        println!("  - Windsurf hooks and rules");
    }
    println!();

    Ok(())
}

/// Update Claude Code tooling files
fn update_claude_code(cwd: &std::path::Path) -> Result<(), String> {
    // Create .claude/commands directory if needed
    let claude_dir = cwd.join(".claude").join("commands");
    create_dir_if_missing(&claude_dir)?;

    // Overwrite decision.md slash command
    let decision_path = claude_dir.join("decision.md");
    write_file_overwrite(&decision_path, DECISION_MD, ".claude/commands/decision.md")?;

    // Overwrite recover.md slash command
    let recover_path = claude_dir.join("recover.md");
    write_file_overwrite(&recover_path, RECOVER_MD, ".claude/commands/recover.md")?;

    // Overwrite work.md slash command
    let work_path = claude_dir.join("work.md");
    write_file_overwrite(&work_path, WORK_MD, ".claude/commands/work.md")?;

    // Create/update hooks directory and enforcement hooks
    let claude_base = cwd.join(".claude");
    let hooks_dir = claude_base.join("hooks");
    create_dir_if_missing(&hooks_dir)?;

    // Overwrite require-action-node.sh hook
    let require_action_path = hooks_dir.join("require-action-node.sh");
    write_executable_overwrite(
        &require_action_path,
        HOOK_REQUIRE_ACTION_NODE,
        ".claude/hooks/require-action-node.sh",
    )?;

    // Overwrite post-commit-reminder.sh hook
    let post_commit_path = hooks_dir.join("post-commit-reminder.sh");
    write_executable_overwrite(
        &post_commit_path,
        HOOK_POST_COMMIT_REMINDER,
        ".claude/hooks/post-commit-reminder.sh",
    )?;

    // Overwrite agents.toml
    let agents_path = claude_base.join("agents.toml");
    write_file_overwrite(&agents_path, CLAUDE_AGENTS_TOML, ".claude/agents.toml")?;

    // Create/update skills directory and files
    let skills_dir = claude_base.join("skills");
    create_dir_if_missing(&skills_dir)?;

    let pulse_path = skills_dir.join("pulse.md");
    write_file_overwrite(&pulse_path, SKILL_PULSE, ".claude/skills/pulse.md")?;

    let narratives_path = skills_dir.join("narratives.md");
    write_file_overwrite(
        &narratives_path,
        SKILL_NARRATIVES,
        ".claude/skills/narratives.md",
    )?;

    let archaeology_path = skills_dir.join("archaeology.md");
    write_file_overwrite(
        &archaeology_path,
        SKILL_ARCHAEOLOGY,
        ".claude/skills/archaeology.md",
    )?;

    // Update CLAUDE.md section
    let claude_md_path = cwd.join("CLAUDE.md");
    replace_config_md_section(&claude_md_path, CLAUDE_MD_SECTION, "CLAUDE.md")?;

    // Update Windsurf if .windsurf directory exists
    let windsurf_dir = cwd.join(".windsurf");
    if windsurf_dir.exists() {
        update_windsurf(cwd)?;
    }

    Ok(())
}

/// Set up Windsurf integration (called during init if --windsurf or .windsurf exists)
fn setup_windsurf_integration(cwd: &Path) -> Result<(), String> {
    // Create .windsurf/hooks directory
    let windsurf_hooks_dir = cwd.join(".windsurf").join("hooks");
    create_dir_if_missing(&windsurf_hooks_dir)?;

    // Create .windsurf/rules directory
    let windsurf_rules_dir = cwd.join(".windsurf").join("rules");
    create_dir_if_missing(&windsurf_rules_dir)?;

    // Write hooks.json
    let hooks_json_path = cwd.join(".windsurf").join("hooks.json");
    write_file_if_missing(&hooks_json_path, WINDSURF_HOOKS_JSON, ".windsurf/hooks.json")?;

    // Write require-action-node.sh hook
    let require_action_path = windsurf_hooks_dir.join("require-action-node.sh");
    write_executable_if_missing(
        &require_action_path,
        WINDSURF_HOOK_REQUIRE_ACTION_NODE,
        ".windsurf/hooks/require-action-node.sh",
    )?;

    // Write post-commit-reminder.sh hook
    let post_commit_path = windsurf_hooks_dir.join("post-commit-reminder.sh");
    write_executable_if_missing(
        &post_commit_path,
        WINDSURF_HOOK_POST_COMMIT_REMINDER,
        ".windsurf/hooks/post-commit-reminder.sh",
    )?;

    // Write deciduous.md rules file
    let rules_path = windsurf_rules_dir.join("deciduous.md");
    write_file_if_missing(&rules_path, WINDSURF_RULES_DECIDUOUS, ".windsurf/rules/deciduous.md")?;

    println!(
        "   {} Windsurf integration",
        "Configured".green()
    );

    Ok(())
}

/// Update Windsurf integration (called during update if .windsurf exists)
fn update_windsurf(cwd: &Path) -> Result<(), String> {
    // Create directories if needed
    let windsurf_hooks_dir = cwd.join(".windsurf").join("hooks");
    create_dir_if_missing(&windsurf_hooks_dir)?;

    let windsurf_rules_dir = cwd.join(".windsurf").join("rules");
    create_dir_if_missing(&windsurf_rules_dir)?;

    // Overwrite hooks.json
    let hooks_json_path = cwd.join(".windsurf").join("hooks.json");
    write_file_overwrite(&hooks_json_path, WINDSURF_HOOKS_JSON, ".windsurf/hooks.json")?;

    // Overwrite require-action-node.sh hook
    let require_action_path = windsurf_hooks_dir.join("require-action-node.sh");
    write_executable_overwrite(
        &require_action_path,
        WINDSURF_HOOK_REQUIRE_ACTION_NODE,
        ".windsurf/hooks/require-action-node.sh",
    )?;

    // Overwrite post-commit-reminder.sh hook
    let post_commit_path = windsurf_hooks_dir.join("post-commit-reminder.sh");
    write_executable_overwrite(
        &post_commit_path,
        WINDSURF_HOOK_POST_COMMIT_REMINDER,
        ".windsurf/hooks/post-commit-reminder.sh",
    )?;

    // Overwrite deciduous.md rules file
    let rules_path = windsurf_rules_dir.join("deciduous.md");
    write_file_overwrite(&rules_path, WINDSURF_RULES_DECIDUOUS, ".windsurf/rules/deciduous.md")?;

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
        println!(
            "   {} {} (already exists)",
            "Skipping".yellow(),
            display_name
        );
    } else {
        fs::write(path, content).map_err(|e| format!("Could not write {}: {}", display_name, e))?;
        println!("   {} {}", "Creating".green(), display_name);
    }
    Ok(())
}

#[cfg(unix)]
fn write_executable_if_missing(
    path: &Path,
    content: &str,
    display_name: &str,
) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    if path.exists() {
        println!(
            "   {} {} (already exists)",
            "Skipping".yellow(),
            display_name
        );
    } else {
        fs::write(path, content).map_err(|e| format!("Could not write {}: {}", display_name, e))?;
        // Make executable (chmod +x)
        let mut perms = fs::metadata(path)
            .map_err(|e| format!("Could not get metadata for {}: {}", display_name, e))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)
            .map_err(|e| format!("Could not set permissions for {}: {}", display_name, e))?;
        println!("   {} {} (executable)", "Creating".green(), display_name);
    }
    Ok(())
}

#[cfg(not(unix))]
fn write_executable_if_missing(
    path: &Path,
    content: &str,
    display_name: &str,
) -> Result<(), String> {
    // On non-Unix systems, just write the file without setting permissions
    write_file_if_missing(path, content, display_name)
}

fn write_file_overwrite(path: &Path, content: &str, display_name: &str) -> Result<(), String> {
    fs::write(path, content).map_err(|e| format!("Could not write {}: {}", display_name, e))?;
    println!("   {} {}", "Updated".green(), display_name);
    Ok(())
}

#[cfg(unix)]
fn write_executable_overwrite(
    path: &Path,
    content: &str,
    display_name: &str,
) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    fs::write(path, content).map_err(|e| format!("Could not write {}: {}", display_name, e))?;
    // Make executable (chmod +x)
    let mut perms = fs::metadata(path)
        .map_err(|e| format!("Could not get metadata for {}: {}", display_name, e))?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)
        .map_err(|e| format!("Could not set permissions for {}: {}", display_name, e))?;
    println!("   {} {} (executable)", "Updated".green(), display_name);
    Ok(())
}

#[cfg(not(unix))]
fn write_executable_overwrite(
    path: &Path,
    content: &str,
    display_name: &str,
) -> Result<(), String> {
    write_file_overwrite(path, content, display_name)
}

fn replace_config_md_section(
    path: &Path,
    section_content: &str,
    file_name: &str,
) -> Result<(), String> {
    // Look for either variant of our section header
    let markers = [
        "## Decision Graph Workflow",
        "## MANDATORY: Decision Graph Workflow",
    ];
    // Our section ends when we hit another ## heading or end of file
    let section_end_pattern = "\n## ";

    if path.exists() {
        let existing =
            fs::read_to_string(path).map_err(|e| format!("Could not read {}: {}", file_name, e))?;

        // Find the start of our section (try each marker)
        let start_idx = markers.iter().filter_map(|m| existing.find(m)).min();

        if let Some(start) = start_idx {
            // Find the end of our section (next ## heading after our section starts)
            let after_marker = existing[start..]
                .find('\n')
                .map(|i| start + i)
                .unwrap_or(start + 10);
            let end_idx = existing[after_marker..]
                .find(section_end_pattern)
                .map(|i| after_marker + i + 1)
                .unwrap_or(existing.len());

            // Rebuild the file: before our section + new section + after our section
            let before = &existing[..start];
            let after = &existing[end_idx..];

            let new_content = if after.is_empty() {
                format!("{}{}", before, section_content.trim_start())
            } else {
                format!(
                    "{}{}\n{}",
                    before,
                    section_content.trim(),
                    after.trim_start()
                )
            };

            fs::write(path, new_content)
                .map_err(|e| format!("Could not write {}: {}", file_name, e))?;
            println!("   {} {} (section replaced)", "Updated".green(), file_name);
        } else {
            // No existing section, append
            let mut file = fs::OpenOptions::new()
                .append(true)
                .open(path)
                .map_err(|e| format!("Could not open {} for append: {}", file_name, e))?;
            use std::io::Write;
            writeln!(file, "\n{}", section_content.trim())
                .map_err(|e| format!("Could not append to {}: {}", file_name, e))?;
            println!("   {} {} (section added)", "Updated".green(), file_name);
        }
    } else {
        // File doesn't exist, create it
        fs::write(path, section_content.trim())
            .map_err(|e| format!("Could not create {}: {}", file_name, e))?;
        println!("   {} {}", "Creating".green(), file_name);
    }
    Ok(())
}

/// Append the Decision Graph Workflow section to CLAUDE.md
fn append_config_md(path: &Path, section_content: &str, file_name: &str) -> Result<(), String> {
    let marker = "## Decision Graph Workflow";

    if path.exists() {
        let existing =
            fs::read_to_string(path).map_err(|e| format!("Could not read {}: {}", file_name, e))?;

        if existing.contains(marker) {
            println!(
                "   {} {} (workflow section already present)",
                "Skipping".yellow(),
                file_name
            );
            return Ok(());
        }

        // Append the section
        let new_content = format!("{}\n{}", existing.trim_end(), section_content);
        fs::write(path, new_content)
            .map_err(|e| format!("Could not update {}: {}", file_name, e))?;
        println!(
            "   {} {} (added workflow section)",
            "Updated".green(),
            file_name
        );
    } else {
        // Create new file
        let content = format!("# Project Instructions\n{}", section_content);
        fs::write(path, content).map_err(|e| format!("Could not create {}: {}", file_name, e))?;
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

        if existing
            .lines()
            .any(|line| line.trim() == entry || line.trim() == ".deciduous")
        {
            // Already in gitignore
            return Ok(());
        }

        // Append
        let new_content = format!(
            "{}\n\n# Deciduous database (local)\n{}\n",
            existing.trim_end(),
            entry
        );
        fs::write(&gitignore_path, new_content)
            .map_err(|e| format!("Could not update .gitignore: {}", e))?;
        println!("   {} .gitignore (added {})", "Updated".green(), entry);
    } else {
        // Create new
        let content = format!("# Deciduous database (local)\n{}\n", entry);
        fs::write(&gitignore_path, content)
            .map_err(|e| format!("Could not create .gitignore: {}", e))?;
        println!("   {} .gitignore", "Creating".green());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_create_dir_if_missing() {
        let tmp = TempDir::new().unwrap();
        let new_dir = tmp.path().join("new_dir");

        assert!(!new_dir.exists());
        create_dir_if_missing(&new_dir).unwrap();
        assert!(new_dir.exists());

        // Should not error on existing dir
        create_dir_if_missing(&new_dir).unwrap();
    }

    #[test]
    fn test_write_file_if_missing() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.txt");

        write_file_if_missing(&file_path, "content", "test.txt").unwrap();
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "content");

        // Should not overwrite existing
        write_file_if_missing(&file_path, "new content", "test.txt").unwrap();
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "content");
    }

    #[test]
    fn test_write_file_overwrite() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.txt");

        fs::write(&file_path, "original").unwrap();
        write_file_overwrite(&file_path, "updated", "test.txt").unwrap();
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "updated");
    }

    #[test]
    fn test_add_to_gitignore_new_file() {
        let tmp = TempDir::new().unwrap();
        add_to_gitignore(tmp.path()).unwrap();

        let content = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(content.contains(".deciduous/"));
    }

    #[test]
    fn test_add_to_gitignore_existing() {
        let tmp = TempDir::new().unwrap();
        let gitignore = tmp.path().join(".gitignore");
        fs::write(&gitignore, "node_modules/\n").unwrap();

        add_to_gitignore(tmp.path()).unwrap();

        let content = fs::read_to_string(&gitignore).unwrap();
        assert!(content.contains("node_modules/"));
        assert!(content.contains(".deciduous/"));
    }

    #[test]
    fn test_add_to_gitignore_already_present() {
        let tmp = TempDir::new().unwrap();
        let gitignore = tmp.path().join(".gitignore");
        fs::write(&gitignore, ".deciduous/\n").unwrap();

        add_to_gitignore(tmp.path()).unwrap();

        let content = fs::read_to_string(&gitignore).unwrap();
        // Should not duplicate
        assert_eq!(content.matches(".deciduous/").count(), 1);
    }
}
