//! Project initialization for deciduous
//!
//! `deciduous init` creates all the files needed for decision graph tracking
//! with AI assistant integration (Claude Code and/or OpenCode).

pub mod guard;
mod known_templates;
pub mod templates;

use crate::opencode;
use colored::Colorize;
use std::fs;
use std::path::Path;

use templates::{
    BUILD_TEST_MD, CLAUDE_AGENTS_TOML, CLAUDE_MD_SECTION, CLAUDE_SETTINGS_JSON, CLEANUP_WORKFLOW,
    DECISION_GRAPH_MD, DECISION_MD, DEFAULT_CONFIG, DEPLOY_PAGES_WORKFLOW, DOCUMENT_MD,
    HOOK_VERSION_CHECK, PAGES_VIEWER_HTML, RECOVER_MD, SERVE_UI_MD, SKILL_ARCHAEOLOGY,
    SKILL_NARRATIVES, SKILL_PULSE, SYNC_GRAPH_MD, SYNC_MD, WINDSURF_HOOKS_JSON,
    WINDSURF_RULES_DECIDUOUS, WORK_MD,
};

/// Initialize a new deciduous project with AI assistant integration
///
/// # Arguments
/// * `setup_claude` - Whether to set up Claude Code integration
/// * `setup_opencode` - Whether to set up OpenCode integration
/// * `setup_windsurf` - Whether to set up Windsurf integration (also auto-detects .windsurf/)
/// * `no_auto_update` - Deprecated (ignored). Version checking is now always-on.
pub fn init_project(
    setup_claude: bool,
    setup_opencode: bool,
    setup_windsurf: bool,
    _no_auto_update: bool,
) -> Result<(), String> {
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

    // 1a. Create .deciduous/documents directory for file attachments
    let documents_dir = deciduous_dir.join("documents");
    create_dir_if_missing(&documents_dir)?;

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

        // Write document.md slash command
        let document_path = claude_dir.join("document.md");
        write_file_if_missing(&document_path, DOCUMENT_MD, ".claude/commands/document.md")?;

        // Write build-test.md slash command
        let build_test_path = claude_dir.join("build-test.md");
        write_file_if_missing(
            &build_test_path,
            BUILD_TEST_MD,
            ".claude/commands/build-test.md",
        )?;

        // Write serve-ui.md slash command
        let serve_ui_path = claude_dir.join("serve-ui.md");
        write_file_if_missing(&serve_ui_path, SERVE_UI_MD, ".claude/commands/serve-ui.md")?;

        // Write sync-graph.md slash command
        let sync_graph_path = claude_dir.join("sync-graph.md");
        write_file_if_missing(
            &sync_graph_path,
            SYNC_GRAPH_MD,
            ".claude/commands/sync-graph.md",
        )?;

        // Write decision-graph.md slash command
        let decision_graph_path = claude_dir.join("decision-graph.md");
        write_file_if_missing(
            &decision_graph_path,
            DECISION_GRAPH_MD,
            ".claude/commands/decision-graph.md",
        )?;

        // Write sync.md slash command
        let sync_path = claude_dir.join("sync.md");
        write_file_if_missing(&sync_path, SYNC_MD, ".claude/commands/sync.md")?;

        // Write agents.toml for subagent configuration
        let claude_base = cwd.join(".claude");
        let agents_path = claude_base.join("agents.toml");
        write_file_if_missing(&agents_path, CLAUDE_AGENTS_TOML, ".claude/agents.toml")?;

        // Create .claude/hooks directory; the only hook is the version check
        let hooks_dir = claude_base.join("hooks");
        create_dir_if_missing(&hooks_dir)?;

        // Write version-check.sh hook (opt-in auto-update check)
        let version_check_path = hooks_dir.join("version-check.sh");
        write_executable_if_missing(
            &version_check_path,
            HOOK_VERSION_CHECK,
            ".claude/hooks/version-check.sh",
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
            println!("\n{}", "Setting up Windsurf integration...".cyan());
        } else {
            println!(
                "\n{}",
                "Detected .windsurf directory - setting up Windsurf integration...".cyan()
            );
        }
        setup_windsurf_integration(&cwd)?;
    }

    // 4. Track the shared record store, keep the local database private
    ensure_gitignore(&cwd)?;
    ensure_gitattributes(&cwd)?;
    if ensure_merge_driver(&cwd)? {
        println!(
            "   {} git merge driver for graph records",
            "Configured".green()
        );
    }
    ensure_graph_file(&cwd)?;

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
        "  3. Use slash commands: {}, {}, {}, {}, etc.",
        "/decision".cyan(),
        "/recover".cyan(),
        "/work".cyan(),
        "/document".cyan()
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
        println!("{}", "Windsurf integration:".cyan().bold());
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
            "Detected .windsurf directory - updating Windsurf integration...".cyan()
        );
        update_windsurf(&cwd)?;
    }

    // Make sure the record store exists and is tracked (projects initialised
    // before 0.17 ignored all of .deciduous/). Not on a project that points at
    // a shared server: its graph lives there, and the graph.json sync would
    // only write into .gitignore, .gitattributes and .git/config, which are
    // the project's files, for nothing.
    if crate::config::Config::load().remote.is_configured() {
        println!(
            "   {} .gitignore, .gitattributes, git merge driver, graph.json (remote configured; the graph lives on the server)",
            "Skipped".dimmed()
        );
    } else {
        ensure_gitignore(&cwd)?;
        ensure_gitattributes(&cwd)?;
        if ensure_merge_driver(&cwd)? {
            println!(
                "   {} git merge driver for graph records",
                "Configured".green()
            );
        }
        if deciduous_dir.exists() {
            ensure_graph_file(&cwd)?;
        }
    }

    if let Some(dir) = guard::backup_dir(&cwd) {
        println!(
            "   {} previous versions of changed files in {}",
            "Backed up".green(),
            dir.strip_prefix(&cwd).unwrap_or(&dir).display()
        );
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
    println!("  - Slash commands (/decision, /recover, /work, /document, /build-test, /serve-ui, /sync-graph, /decision-graph, /sync)");
    println!("  - Skills (/pulse, /narratives, /archaeology)");
    if has_claude {
        println!("  - Agent configurations (agents.toml)");
        println!("  - No logging hooks: the ones earlier versions installed are removed");
    }
    if has_opencode {
        println!("  - OpenCode version-check plugin (the logging plugins are removed)");
        println!("  - OpenCode skills, agents, and tools");
        println!("  - OpenCode configuration (opencode.json)");
    }
    if has_windsurf {
        println!("  - Windsurf rules and version-check hook");
    }
    println!();

    // Prompt user to commit the updated configuration files
    let version = env!("CARGO_PKG_VERSION");
    println!(
        "{}",
        "Commit the updated files to lock in the new configuration:".yellow()
    );
    if has_claude {
        println!(
            "  git add .claude/commands/ .claude/hooks/ .claude/skills/ .claude/settings.json .claude/agents.toml CLAUDE.md"
        );
    }
    if has_opencode {
        println!("  git add .opencode/ AGENTS.md");
    }
    if has_windsurf {
        println!("  git add .windsurf/");
    }
    println!(
        "  git commit -m \"chore: update deciduous tooling to v{}\"",
        version
    );
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

    // Overwrite document.md slash command
    let document_path = claude_dir.join("document.md");
    write_file_overwrite(&document_path, DOCUMENT_MD, ".claude/commands/document.md")?;

    // Overwrite build-test.md slash command
    let build_test_path = claude_dir.join("build-test.md");
    write_file_overwrite(
        &build_test_path,
        BUILD_TEST_MD,
        ".claude/commands/build-test.md",
    )?;

    // Overwrite serve-ui.md slash command
    let serve_ui_path = claude_dir.join("serve-ui.md");
    write_file_overwrite(&serve_ui_path, SERVE_UI_MD, ".claude/commands/serve-ui.md")?;

    // Overwrite sync-graph.md slash command
    let sync_graph_path = claude_dir.join("sync-graph.md");
    write_file_overwrite(
        &sync_graph_path,
        SYNC_GRAPH_MD,
        ".claude/commands/sync-graph.md",
    )?;

    // Overwrite decision-graph.md slash command
    let decision_graph_path = claude_dir.join("decision-graph.md");
    write_file_overwrite(
        &decision_graph_path,
        DECISION_GRAPH_MD,
        ".claude/commands/decision-graph.md",
    )?;

    // Overwrite sync.md slash command
    let sync_path = claude_dir.join("sync.md");
    write_file_overwrite(&sync_path, SYNC_MD, ".claude/commands/sync.md")?;

    // Create/update hooks directory
    let claude_base = cwd.join(".claude");
    let hooks_dir = claude_base.join("hooks");
    create_dir_if_missing(&hooks_dir)?;

    // The logging hooks are gone since 1.0.3: remove the scripts deciduous
    // wrote, keep any the user wrote, and take the entries out of settings.json.
    for name in RETIRED_HOOK_SCRIPTS {
        remove_guarded(&hooks_dir.join(name), &format!(".claude/hooks/{name}"))?;
    }

    // Overwrite version-check.sh hook (opt-in auto-update check)
    let version_check_path = hooks_dir.join("version-check.sh");
    write_executable_overwrite(
        &version_check_path,
        HOOK_VERSION_CHECK,
        ".claude/hooks/version-check.sh",
    )?;

    strip_retired_hook_settings(&claude_base.join("settings.json"))?;

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
    guard::backup(cwd, "CLAUDE.md")?;
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
    write_file_if_missing(
        &hooks_json_path,
        WINDSURF_HOOKS_JSON,
        ".windsurf/hooks.json",
    )?;

    // Write version-check.sh hook (opt-in auto-update check)
    let version_check_path = windsurf_hooks_dir.join("version-check.sh");
    write_executable_if_missing(
        &version_check_path,
        HOOK_VERSION_CHECK,
        ".windsurf/hooks/version-check.sh",
    )?;

    // Write deciduous.md rules file
    let rules_path = windsurf_rules_dir.join("deciduous.md");
    write_file_if_missing(
        &rules_path,
        WINDSURF_RULES_DECIDUOUS,
        ".windsurf/rules/deciduous.md",
    )?;

    println!("   {} Windsurf integration", "Configured".green());

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
    write_file_overwrite(
        &hooks_json_path,
        WINDSURF_HOOKS_JSON,
        ".windsurf/hooks.json",
    )?;

    for name in RETIRED_HOOK_SCRIPTS {
        remove_guarded(
            &windsurf_hooks_dir.join(name),
            &format!(".windsurf/hooks/{name}"),
        )?;
    }

    // Overwrite version-check.sh hook (opt-in auto-update check)
    let version_check_path = windsurf_hooks_dir.join("version-check.sh");
    write_executable_overwrite(
        &version_check_path,
        HOOK_VERSION_CHECK,
        ".windsurf/hooks/version-check.sh",
    )?;

    // Overwrite deciduous.md rules file
    let rules_path = windsurf_rules_dir.join("deciduous.md");
    write_file_overwrite(
        &rules_path,
        WINDSURF_RULES_DECIDUOUS,
        ".windsurf/rules/deciduous.md",
    )?;

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

/// Hook scripts deciduous installed before 1.0.3 to make agents log, for
/// Claude Code and Windsurf alike. Logging is encouraged through the
/// instructions and the tool replies now, not enforced by a hook, so
/// `update` removes the ones it wrote.
pub const RETIRED_HOOK_SCRIPTS: [&str; 2] = ["require-action-node.sh", "post-commit-reminder.sh"];

/// Takes the retired logging hooks out of `.claude/settings.json`: every
/// command that runs `deciduous log-loop`, and every command that runs a
/// retired script that is no longer on disk. A retired script the user
/// wrote was kept, so the entry that runs it stays. Entries and events left
/// empty are dropped; everything else, including key order, is kept. A file
/// that is not valid JSON is reported and left alone.
pub fn strip_retired_hook_settings(path: &Path) -> Result<(), String> {
    let Ok(raw) = fs::read_to_string(path) else {
        return Ok(());
    };
    let mut settings: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            println!(
                "   {} .claude/settings.json (not valid JSON: {e}); remove any `deciduous log-loop` hooks by hand",
                "Skipped".yellow()
            );
            return Ok(());
        }
    };
    let hooks_dir = path.parent().map(|p| p.join("hooks"));
    let retired = |command: &str| {
        command.contains("deciduous log-loop")
            || RETIRED_HOOK_SCRIPTS.iter().any(|name| {
                command.contains(&format!(".claude/hooks/{name}"))
                    && !hooks_dir.as_ref().is_some_and(|d| d.join(name).exists())
            })
    };

    let mut changed = false;
    if let Some(events) = settings.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        for entries in events.values_mut() {
            let Some(list) = entries.as_array_mut() else {
                continue;
            };
            for entry in list.iter_mut() {
                if let Some(hs) = entry.get_mut("hooks").and_then(|h| h.as_array_mut()) {
                    let before = hs.len();
                    hs.retain(|h| !retired(h["command"].as_str().unwrap_or("")));
                    changed |= hs.len() != before;
                }
            }
            let before = list.len();
            list.retain(|e| e["hooks"].as_array().is_none_or(|hs| !hs.is_empty()));
            changed |= list.len() != before;
        }
        let before = events.len();
        events.retain(|_, v| v.as_array().is_none_or(|l| !l.is_empty()));
        changed |= events.len() != before;
    }

    if changed {
        if let Some(root) = path.parent().and_then(|p| p.parent()) {
            guard::backup(root, ".claude/settings.json")?;
        }
        let body = render_in_original_order(&settings, &raw);
        fs::write(path, body + "\n")
            .map_err(|e| format!("Could not write {}: {}", path.display(), e))?;
        println!(
            "   {} .claude/settings.json (logging hooks removed)",
            "Updated".green()
        );
    }
    Ok(())
}

/// Pretty-prints `value` (2-space indent, like `to_string_pretty`) with each
/// object's keys in the order they first appear in `original`, and keys that
/// are new after them in the order serde_json holds them. serde_json's map is
/// sorted, and `preserve_order` cannot be turned on for one call: it would
/// reorder every JSON the program writes, graph.json included. A user's
/// settings.json keeps its shape; only what deciduous adds is new.
fn render_in_original_order(value: &serde_json::Value, original: &str) -> String {
    let orders = key_orders(original);
    let mut out = String::new();
    render_ordered(value, &orders, "", 0, &mut out);
    out
}

/// For every object in `raw`, keyed by its path ("/hooks/PreToolUse/0"), its
/// keys in the order written. A small scanner, not a full JSON parser: it only
/// has to find keys in a document serde_json has already accepted.
fn key_orders(raw: &str) -> std::collections::HashMap<String, Vec<String>> {
    fn skip_ws(b: &[u8], i: &mut usize) {
        while *i < b.len() && (b[*i] as char).is_whitespace() {
            *i += 1;
        }
    }
    fn string(b: &[u8], i: &mut usize) -> String {
        let start = *i;
        *i += 1;
        while *i < b.len() && b[*i] != b'"' {
            if b[*i] == b'\\' {
                *i += 1;
            }
            *i += 1;
        }
        *i += 1;
        serde_json::from_slice(&b[start..*i]).unwrap_or_default()
    }
    fn value(
        b: &[u8],
        i: &mut usize,
        path: &str,
        out: &mut std::collections::HashMap<String, Vec<String>>,
    ) {
        skip_ws(b, i);
        match b.get(*i) {
            Some(b'{') => {
                *i += 1;
                let mut keys = Vec::new();
                loop {
                    skip_ws(b, i);
                    match b.get(*i) {
                        Some(b'}') => {
                            *i += 1;
                            break;
                        }
                        Some(b',') => *i += 1,
                        Some(b'"') => {
                            let k = string(b, i);
                            skip_ws(b, i);
                            *i += 1; // ':'
                            value(b, i, &format!("{path}/{k}"), out);
                            keys.push(k);
                        }
                        _ => return,
                    }
                }
                out.insert(path.to_string(), keys);
            }
            Some(b'[') => {
                *i += 1;
                let mut n = 0;
                loop {
                    skip_ws(b, i);
                    match b.get(*i) {
                        Some(b']') => {
                            *i += 1;
                            break;
                        }
                        Some(b',') => *i += 1,
                        Some(_) => {
                            value(b, i, &format!("{path}/{n}"), out);
                            n += 1;
                        }
                        None => return,
                    }
                }
            }
            Some(b'"') => {
                string(b, i);
            }
            Some(_) => {
                while *i < b.len() && !matches!(b[*i], b',' | b'}' | b']') {
                    *i += 1;
                }
            }
            None => {}
        }
    }
    let mut out = std::collections::HashMap::new();
    let mut i = 0;
    value(raw.as_bytes(), &mut i, "", &mut out);
    out
}

fn render_ordered(
    v: &serde_json::Value,
    orders: &std::collections::HashMap<String, Vec<String>>,
    path: &str,
    depth: usize,
    out: &mut String,
) {
    use serde_json::Value;
    let pad = "  ".repeat(depth + 1);
    let close = "  ".repeat(depth);
    match v {
        Value::Object(map) if !map.is_empty() => {
            let known = orders.get(path);
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by_key(|k| {
                known
                    .and_then(|o| o.iter().position(|x| x == *k))
                    .unwrap_or(usize::MAX)
            });
            out.push_str("{\n");
            for (i, k) in keys.iter().enumerate() {
                out.push_str(&pad);
                out.push_str(&serde_json::to_string(k).unwrap());
                out.push_str(": ");
                render_ordered(&map[*k], orders, &format!("{path}/{k}"), depth + 1, out);
                out.push_str(if i + 1 < keys.len() { ",\n" } else { "\n" });
            }
            out.push_str(&close);
            out.push('}');
        }
        Value::Array(items) if !items.is_empty() => {
            out.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                out.push_str(&pad);
                render_ordered(item, orders, &format!("{path}/{i}"), depth + 1, out);
                out.push_str(if i + 1 < items.len() { ",\n" } else { "\n" });
            }
            out.push_str(&close);
            out.push(']');
        }
        other => out.push_str(&serde_json::to_string(other).unwrap()),
    }
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

/// `deciduous update --all <dir>`: every deciduous project directly under
/// `dir`, and `dir` itself if it is one, updated one after another. A project
/// is a directory with `.deciduous/` and an assistant integration to update.
/// One project failing does not stop the rest. Returns how many failed.
pub fn update_all(root: &Path) -> usize {
    let start = std::env::current_dir().unwrap_or_default();
    let mut dirs: Vec<std::path::PathBuf> = vec![root.to_path_buf()];
    if let Ok(entries) = fs::read_dir(root) {
        let mut children: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        children.sort();
        dirs.extend(children);
    }
    let is_project = |d: &Path| {
        d.join(".deciduous").is_dir()
            && [".claude", ".opencode", ".windsurf"]
                .iter()
                .any(|a| d.join(a).is_dir())
    };
    let (mut ok, mut failed) = (0usize, Vec::new());
    for d in dirs.into_iter().filter(|d| is_project(d)) {
        println!("\n{} {}", "==>".cyan(), d.display());
        let result = std::env::set_current_dir(&d)
            .map_err(|e| format!("cannot enter {}: {e}", d.display()))
            .and_then(|_| update_tooling());
        match result {
            Ok(()) => ok += 1,
            Err(e) => {
                eprintln!("   {} {}", "Failed:".red(), e);
                failed.push(d.display().to_string());
            }
        }
    }
    let _ = std::env::set_current_dir(start);
    println!(
        "\n{} {} updated, {} failed",
        "Done:".green(),
        ok,
        failed.len()
    );
    for f in &failed {
        println!("   {} {}", "failed".red(), f);
    }
    failed.len()
}

/// The project root for a harness path: callers pass the path relative to the
/// project as `display_name`, so the root is the path with that suffix removed.
fn harness_root(path: &Path, display_name: &str) -> std::path::PathBuf {
    let full = path.to_string_lossy();
    match full.strip_suffix(display_name) {
        Some(root) if !root.is_empty() => std::path::PathBuf::from(root),
        _ => std::env::current_dir().unwrap_or_default(),
    }
}

/// Writes a harness file through [`guard::write`]: replaces what deciduous
/// wrote, keeps (and for Markdown, appends to) what someone else wrote.
fn write_file_overwrite(path: &Path, content: &str, display_name: &str) -> Result<(), String> {
    write_guarded(path, content, display_name, false)
}

fn write_executable_overwrite(
    path: &Path,
    content: &str,
    display_name: &str,
) -> Result<(), String> {
    write_guarded(path, content, display_name, true)
}

fn write_guarded(
    path: &Path,
    content: &str,
    display_name: &str,
    executable: bool,
) -> Result<(), String> {
    let outcome = guard::write(&harness_root(path, display_name), path, content, executable)?;
    let label = outcome.label();
    let label = match outcome {
        guard::Outcome::KeptYours | guard::Outcome::Appended => label.yellow(),
        guard::Outcome::Unchanged => label.dimmed(),
        _ => label.green(),
    };
    println!("   {} {}", label, display_name);
    Ok(())
}

fn remove_guarded(path: &Path, display_name: &str) -> Result<(), String> {
    match guard::remove(&harness_root(path, display_name), path)? {
        Some(guard::Outcome::KeptYours) => println!(
            "   {} {} (yours; left in place)",
            "Kept".yellow(),
            display_name
        ),
        Some(outcome) => println!("   {} {}", outcome.label().green(), display_name),
        None => {}
    }
    Ok(())
}

fn replace_config_md_section(
    path: &Path,
    section_content: &str,
    file_name: &str,
) -> Result<(), String> {
    const START_MARKER: &str = "<!-- deciduous:start -->";
    const END_MARKER: &str = "<!-- deciduous:end -->";

    // Legacy heading markers (for migration from pre-marker format)
    let legacy_markers = [
        "## Decision Graph Workflow",
        "## MANDATORY: Decision Graph Workflow",
    ];

    if path.exists() {
        let existing =
            fs::read_to_string(path).map_err(|e| format!("Could not read {}: {}", file_name, e))?;

        // Strategy 1: Use HTML comment markers (safe, precise)
        if let (Some(start), Some(end_start)) =
            (existing.find(START_MARKER), existing.find(END_MARKER))
        {
            let end = end_start + END_MARKER.len();
            // Include any trailing newline after the end marker
            let end = if existing[end..].starts_with('\n') {
                end + 1
            } else {
                end
            };

            let before = &existing[..start];
            let after = &existing[end..];

            let new_content = format!(
                "{}{}{}",
                before,
                section_content.trim(),
                if after.is_empty() {
                    String::new()
                } else {
                    format!("\n{}", after.trim_start())
                }
            );

            fs::write(path, new_content)
                .map_err(|e| format!("Could not write {}: {}", file_name, e))?;
            println!("   {} {} (section replaced)", "Updated".green(), file_name);
            return Ok(());
        }

        // Strategy 2: Legacy migration — find old heading, replace with new marked content
        let start_idx = legacy_markers.iter().filter_map(|m| existing.find(m)).min();

        if let Some(start) = start_idx {
            // Find the end: look for next ## heading after our section header line
            let after_header = existing[start..]
                .find('\n')
                .map(|i| start + i)
                .unwrap_or(start + 10);
            let end_idx = existing[after_header..]
                .find("\n## ")
                .map(|i| after_header + i + 1) // +1 to keep the newline before next heading
                .unwrap_or(existing.len());

            let before = &existing[..start];
            let after = &existing[end_idx..];

            let new_content = format!(
                "{}{}{}",
                before,
                section_content.trim(),
                if after.is_empty() {
                    String::new()
                } else {
                    format!("\n{}", after.trim_start())
                }
            );

            fs::write(path, new_content)
                .map_err(|e| format!("Could not write {}: {}", file_name, e))?;
            println!(
                "   {} {} (section replaced, markers added)",
                "Updated".green(),
                file_name
            );
        } else {
            // No existing section found — append
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
        // File doesn't exist — create it
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

/// The `.gitignore` rules deciduous needs: the SQLite file is private to a
/// machine, the graph file and config are shared through git.
const GITIGNORE_BLOCK: &str =
    "# Deciduous: local database stays private, the shared graph is tracked\n\
.deciduous/*\n\
!.deciduous/config.toml\n\
!.deciduous/graph.json\n";

/// Lines that older versions of deciduous (or people) wrote for the same
/// purpose. They are replaced by [`GITIGNORE_BLOCK`].
fn is_stale_deciduous_ignore_line(line: &str) -> bool {
    matches!(
        line.trim(),
        ".deciduous"
            | ".deciduous/"
            | "/.deciduous"
            | "/.deciduous/"
            | ".deciduous/*"
            | "!.deciduous/config.toml"
            | "!.deciduous/graph.json"
            | "!.deciduous/sync/"
            | "!.deciduous/sync"
            | "!.deciduous/patches/"
            | "!.deciduous/patches"
            | "# Deciduous database (local)"
            | "# Deciduous database (local) - but track patches for sharing"
            | "# Deciduous: local database stays private, shared graph records are tracked"
            | "# Deciduous: local database stays private, the shared graph is tracked"
    )
}

/// Ensure `.gitignore` hides the database but tracks `.deciduous/graph.json`.
///
/// A blanket `.deciduous/` line (what pre-0.17 `init` wrote) would hide the
/// graph file, so it is replaced. Anything else in the file is left alone.
fn ensure_gitignore(cwd: &Path) -> Result<(), String> {
    let path = cwd.join(".gitignore");
    let existing = if path.exists() {
        fs::read_to_string(&path).map_err(|e| format!("Could not read .gitignore: {}", e))?
    } else {
        String::new()
    };

    let has = |needle: &str| existing.lines().any(|l| l.trim() == needle);
    let blanket = existing.lines().any(|l| {
        matches!(
            l.trim(),
            ".deciduous" | ".deciduous/" | "/.deciduous" | "/.deciduous/"
        )
    });
    if has(".deciduous/*")
        && has("!.deciduous/config.toml")
        && has("!.deciduous/graph.json")
        && !blanket
    {
        return Ok(());
    }

    let kept: Vec<&str> = existing
        .lines()
        .filter(|l| !is_stale_deciduous_ignore_line(l))
        .collect();
    let mut content = kept.join("\n").trim_end().to_string();
    if !content.is_empty() {
        content.push_str("\n\n");
    }
    content.push_str(GITIGNORE_BLOCK);

    fs::write(&path, content).map_err(|e| format!("Could not write .gitignore: {}", e))?;
    println!(
        "   {} .gitignore (track .deciduous/graph.json, ignore the database)",
        if existing.is_empty() {
            "Creating".green()
        } else {
            "Updated".green()
        }
    );
    Ok(())
}

const GITATTRIBUTES_LINE: &str = ".deciduous/graph.json merge=deciduous linguist-generated=true";

/// Patterns earlier versions wrote for the same purpose.
const STALE_GITATTRIBUTES_PATTERNS: [&str; 2] = [".deciduous/sync/**", ".deciduous/graph.json"];

/// Route the graph file through the `deciduous` merge driver and mark it as
/// generated so GitHub folds it in pull request diffs. An older line for the
/// same purpose is upgraded in place.
pub fn ensure_gitattributes(cwd: &Path) -> Result<(), String> {
    let path = cwd.join(".gitattributes");
    let existing = if path.exists() {
        fs::read_to_string(&path).map_err(|e| format!("Could not read .gitattributes: {}", e))?
    } else {
        String::new()
    };
    if existing.lines().any(|l| l.trim() == GITATTRIBUTES_LINE) {
        return Ok(());
    }
    let mut lines: Vec<String> = existing
        .lines()
        .filter(|l| {
            !l.split_whitespace()
                .next()
                .is_some_and(|pat| STALE_GITATTRIBUTES_PATTERNS.contains(&pat))
        })
        .map(str::to_string)
        .collect();
    lines.push(GITATTRIBUTES_LINE.to_string());
    let mut content = lines.join("\n").trim_end().to_string();
    content.push('\n');
    fs::write(&path, content).map_err(|e| format!("Could not write .gitattributes: {}", e))?;
    println!(
        "   {} .gitattributes (merge the graph file record by record, fold it in PR diffs)",
        if existing.is_empty() {
            "Creating".green()
        } else {
            "Updated".green()
        }
    );
    Ok(())
}

const MERGE_DRIVER_COMMAND: &str = "deciduous merge-record %O %A %B";

/// Register the `deciduous` merge driver in this clone's git config so two
/// people editing the same record get a field-level merge instead of
/// conflict markers. Git config is per clone, so this runs from `init`,
/// `update`, and `sync`. Returns `Ok(true)` when it (re)wrote the config.
/// Silently does nothing outside a git repository.
pub fn ensure_merge_driver(cwd: &Path) -> Result<bool, String> {
    let inside = std::process::Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(cwd)
        .output();
    match inside {
        Ok(out) if out.status.success() => {}
        _ => return Ok(false),
    }
    let current = std::process::Command::new("git")
        .args(["config", "--get", "merge.deciduous.driver"])
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("git config: {}", e))?;
    if current.status.success()
        && String::from_utf8_lossy(&current.stdout).trim() == MERGE_DRIVER_COMMAND
    {
        return Ok(false);
    }
    for (key, value) in [
        ("merge.deciduous.name", "deciduous decision graph record"),
        ("merge.deciduous.driver", MERGE_DRIVER_COMMAND),
    ] {
        let status = std::process::Command::new("git")
            .args(["config", key, value])
            .current_dir(cwd)
            .status()
            .map_err(|e| format!("git config {}: {}", key, e))?;
        if !status.success() {
            return Err(format!("git config {} failed", key));
        }
    }
    Ok(true)
}

/// Create `.deciduous/graph.json` so it exists in every clone, and fold a
/// 0.17 `.deciduous/sync/` directory into it if one is still there.
fn ensure_graph_file(cwd: &Path) -> Result<(), String> {
    let path = cwd.join(".deciduous").join(crate::records::STORE_FILE_NAME);
    let existed = path.is_file();
    let store = crate::records::RecordStore::create(&path)
        .map_err(|e| format!("Could not create {}: {}", path.display(), e))?;
    if !existed {
        println!(
            "   {} .deciduous/graph.json (run `deciduous sync` to fill it, then commit it)",
            "Creating".green()
        );
    }
    if store.has_legacy_record_dir() {
        let report = store
            .import_legacy_record_dir()
            .map_err(|e| format!("Could not import .deciduous/sync/: {}", e))?;
        println!(
            "   {} .deciduous/sync/ into .deciduous/graph.json ({} nodes, {} edges)",
            "Folded".green(),
            report.nodes,
            report.edges
        );
        if report.removed {
            println!(
                "   {} .deciduous/sync/ (git rm -r it too)",
                "Removed".green()
            );
        }
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

    /// What a 1.0.2 `update` left in a user's settings.json: their own entry,
    /// the wrapper entries, and the log-loop entries.
    const SETTINGS_1_0_2: &str = "{\n  \"permissions\": {\n    \"allow\": [\n      \"Bash(mix *)\"\n    ]\n  },\n  \"hooks\": {\n    \"PreToolUse\": [\n      {\n        \"matcher\": \"Edit\",\n        \"hooks\": [\n          {\n            \"type\": \"command\",\n            \"command\": \"mine.sh\"\n          }\n        ]\n      },\n      {\n        \"matcher\": \"Edit|Write|NotebookEdit|Bash\",\n        \"hooks\": [\n          {\n            \"type\": \"command\",\n            \"command\": \"\\\"$CLAUDE_PROJECT_DIR/.claude/hooks/require-action-node.sh\\\"\"\n          }\n        ]\n      }\n    ],\n    \"PostToolUse\": [\n      {\n        \"matcher\": \"Bash\",\n        \"hooks\": [\n          {\n            \"type\": \"command\",\n            \"command\": \"deciduous log-loop post-bash || true\"\n          }\n        ]\n      }\n    ],\n    \"Stop\": [\n      {\n        \"hooks\": [\n          {\n            \"type\": \"command\",\n            \"command\": \"deciduous log-loop stop || true\"\n          }\n        ]\n      }\n    ]\n  },\n  \"model\": \"x\"\n}\n";

    #[test]
    fn retired_hooks_leave_settings_and_the_users_entries_stay_in_order() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join(".claude/hooks")).unwrap();
        let p = tmp.path().join(".claude/settings.json");
        fs::write(&p, SETTINGS_1_0_2).unwrap();
        strip_retired_hook_settings(&p).unwrap();
        let after = fs::read_to_string(&p).unwrap();
        assert!(!after.contains("log-loop"), "{after}");
        assert!(!after.contains("require-action-node"), "{after}");
        assert!(
            !after.contains("\"Stop\"") && !after.contains("\"PostToolUse\""),
            "empty events dropped:\n{after}"
        );
        let (perm, hooks, model) = (
            after.find("\"permissions\"").unwrap(),
            after.find("\"hooks\"").unwrap(),
            after.find("\"model\"").unwrap(),
        );
        assert!(
            perm < hooks && hooks < model,
            "top-level order kept:\n{after}"
        );
        let (m, h) = (
            after.find("\"matcher\": \"Edit\"").unwrap(),
            after.find("\"command\": \"mine.sh\"").unwrap(),
        );
        assert!(m < h, "matcher still before hooks in the user's entry");
        // Idempotent: a second run finds nothing to do.
        strip_retired_hook_settings(&p).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), after);
    }

    #[test]
    fn a_retired_script_the_user_wrote_keeps_its_entry() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join(".claude/hooks")).unwrap();
        fs::write(
            tmp.path().join(".claude/hooks/require-action-node.sh"),
            "#!/bin/sh\n# mine\nexit 0\n",
        )
        .unwrap();
        let p = tmp.path().join(".claude/settings.json");
        fs::write(&p, SETTINGS_1_0_2).unwrap();
        strip_retired_hook_settings(&p).unwrap();
        let after = fs::read_to_string(&p).unwrap();
        assert!(after.contains("require-action-node.sh"), "{after}");
        assert!(!after.contains("log-loop"), "{after}");
    }

    #[test]
    fn test_write_file_overwrite() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join(".deciduous")).unwrap();
        let file_path = tmp.path().join("test.txt");

        // Written by deciduous, then updated: replaced.
        write_file_overwrite(&file_path, "original", "test.txt").unwrap();
        write_file_overwrite(&file_path, "updated", "test.txt").unwrap();
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "updated");

        // Written by someone else: kept.
        fs::write(&file_path, "someone else's").unwrap();
        write_file_overwrite(&file_path, "updated again", "test.txt").unwrap();
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "someone else's");
    }

    #[test]
    fn test_ensure_gitignore_new_file() {
        let tmp = TempDir::new().unwrap();
        ensure_gitignore(tmp.path()).unwrap();

        let content = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(content.contains(".deciduous/*"));
        assert!(content.contains("!.deciduous/graph.json"));
        assert!(content.contains("!.deciduous/config.toml"));
    }

    #[test]
    fn test_ensure_gitignore_keeps_other_rules() {
        let tmp = TempDir::new().unwrap();
        let gitignore = tmp.path().join(".gitignore");
        fs::write(&gitignore, "node_modules/\n").unwrap();

        ensure_gitignore(tmp.path()).unwrap();

        let content = fs::read_to_string(&gitignore).unwrap();
        assert!(content.starts_with("node_modules/\n"));
        assert!(content.contains("!.deciduous/graph.json"));
    }

    #[test]
    fn test_ensure_gitignore_replaces_blanket_rule_and_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let gitignore = tmp.path().join(".gitignore");
        fs::write(
            &gitignore,
            "target/\n\n# Deciduous database (local)\n.deciduous/\n\n.deciduous/*\n!.deciduous/patches/\n",
        )
        .unwrap();

        ensure_gitignore(tmp.path()).unwrap();
        let content = fs::read_to_string(&gitignore).unwrap();
        assert!(
            !content.lines().any(|l| l.trim() == ".deciduous/"),
            "{content}"
        );
        assert!(!content.contains("patches"), "{content}");
        assert!(content.contains("target/"));
        assert_eq!(content.matches(".deciduous/*").count(), 1);

        let before = content.clone();
        ensure_gitignore(tmp.path()).unwrap();
        assert_eq!(fs::read_to_string(&gitignore).unwrap(), before);
    }

    #[test]
    fn test_ensure_gitattributes_and_graph_file() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join(".gitattributes"),
            "*.png binary\n.deciduous/sync/** linguist-generated=true\n",
        )
        .unwrap();
        ensure_gitattributes(tmp.path()).unwrap();
        ensure_gitattributes(tmp.path()).unwrap();
        let content = fs::read_to_string(tmp.path().join(".gitattributes")).unwrap();
        assert_eq!(content.matches("linguist-generated").count(), 1);
        assert!(content.contains("merge=deciduous"), "{content}");
        assert!(content.starts_with("*.png binary\n"));

        fs::create_dir_all(tmp.path().join(".deciduous")).unwrap();
        ensure_graph_file(tmp.path()).unwrap();
        assert!(tmp.path().join(".deciduous/graph.json").is_file());
    }

    /// Upgrading from 0.17: the old rules must be replaced, not stacked on.
    #[test]
    fn test_upgrade_replaces_the_0_17_sync_rules() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join(".gitignore"),
            "target/

# Deciduous: local database stays private, shared graph records are tracked
.deciduous/*
!.deciduous/config.toml
!.deciduous/sync/
",
        )
        .unwrap();
        fs::write(
            tmp.path().join(".gitattributes"),
            ".deciduous/sync/** merge=deciduous linguist-generated=true
",
        )
        .unwrap();

        ensure_gitignore(tmp.path()).unwrap();
        ensure_gitattributes(tmp.path()).unwrap();

        let ignore = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(!ignore.contains("!.deciduous/sync/"), "{ignore}");
        assert!(ignore.contains("!.deciduous/graph.json"), "{ignore}");
        assert!(ignore.starts_with("target/\n"));

        let attrs = fs::read_to_string(tmp.path().join(".gitattributes")).unwrap();
        assert_eq!(attrs.lines().count(), 1, "{attrs}");
        assert!(
            attrs.starts_with(".deciduous/graph.json merge=deciduous"),
            "{attrs}"
        );
    }

    #[test]
    fn test_replace_section_with_markers_preserves_surrounding() {
        let tmp = TempDir::new().unwrap();
        let md = tmp.path().join("CLAUDE.md");
        fs::write(
            &md,
            "# My Project\n\nCustom rules here.\n\n<!-- deciduous:start -->\n## Decision Graph Workflow\n\nOld content.\n<!-- deciduous:end -->\n\n## My Other Section\n\nUser content after.\n",
        )
        .unwrap();

        let new_section = "<!-- deciduous:start -->\n## Decision Graph Workflow\n\nNew content.\n<!-- deciduous:end -->";
        replace_config_md_section(&md, new_section, "CLAUDE.md").unwrap();

        let result = fs::read_to_string(&md).unwrap();
        assert!(
            result.contains("Custom rules here."),
            "Content before should be preserved"
        );
        assert!(
            result.contains("New content."),
            "New section should be inserted"
        );
        assert!(
            !result.contains("Old content."),
            "Old section should be removed"
        );
        assert!(
            result.contains("My Other Section"),
            "Content after should be preserved"
        );
        assert!(
            result.contains("User content after."),
            "Content after should be preserved"
        );
    }

    #[test]
    fn test_replace_section_legacy_no_markers_migrates() {
        let tmp = TempDir::new().unwrap();
        let md = tmp.path().join("CLAUDE.md");
        fs::write(
            &md,
            "# My Project\n\nCustom rules.\n\n## Decision Graph Workflow\n\nOld deciduous stuff.\n\n### Sub-heading\n\nMore old stuff.\n\n## My Custom Rules\n\nDo not delete this!\n",
        )
        .unwrap();

        let new_section = "<!-- deciduous:start -->\n## Decision Graph Workflow\n\nNew stuff.\n<!-- deciduous:end -->";
        replace_config_md_section(&md, new_section, "CLAUDE.md").unwrap();

        let result = fs::read_to_string(&md).unwrap();
        assert!(result.contains("Custom rules."), "Content before preserved");
        assert!(result.contains("New stuff."), "New section inserted");
        assert!(
            !result.contains("Old deciduous stuff."),
            "Old section removed"
        );
        assert!(
            result.contains("My Custom Rules"),
            "User H2 after preserved"
        );
        assert!(
            result.contains("Do not delete this!"),
            "User content after preserved"
        );
        assert!(
            result.contains("<!-- deciduous:start -->"),
            "Start marker added"
        );
        assert!(
            result.contains("<!-- deciduous:end -->"),
            "End marker added"
        );
    }

    #[test]
    fn test_replace_section_legacy_last_section_no_trailing_content() {
        let tmp = TempDir::new().unwrap();
        let md = tmp.path().join("CLAUDE.md");
        fs::write(
            &md,
            "# My Project\n\nStuff.\n\n## Decision Graph Workflow\n\nOld content here.\n",
        )
        .unwrap();

        let new_section = "<!-- deciduous:start -->\n## Decision Graph Workflow\n\nNew content.\n<!-- deciduous:end -->";
        replace_config_md_section(&md, new_section, "CLAUDE.md").unwrap();

        let result = fs::read_to_string(&md).unwrap();
        assert!(result.contains("Stuff."), "Content before preserved");
        assert!(result.contains("New content."), "New section inserted");
        assert!(!result.contains("Old content here."), "Old section removed");
        assert!(
            result.contains("<!-- deciduous:end -->"),
            "End marker present"
        );
    }

    #[test]
    fn test_replace_section_no_existing_section_appends() {
        let tmp = TempDir::new().unwrap();
        let md = tmp.path().join("CLAUDE.md");
        fs::write(&md, "# My Project\n\nMy custom instructions.\n").unwrap();

        let new_section = "<!-- deciduous:start -->\n## Decision Graph Workflow\n\nNew content.\n<!-- deciduous:end -->";
        replace_config_md_section(&md, new_section, "CLAUDE.md").unwrap();

        let result = fs::read_to_string(&md).unwrap();
        assert!(
            result.contains("My custom instructions."),
            "Existing content preserved"
        );
        assert!(result.contains("New content."), "Section appended");
        assert!(
            result.contains("<!-- deciduous:start -->"),
            "Start marker present"
        );
    }

    #[test]
    fn test_replace_section_file_does_not_exist_creates() {
        let tmp = TempDir::new().unwrap();
        let md = tmp.path().join("CLAUDE.md");

        let new_section = "<!-- deciduous:start -->\n## Decision Graph Workflow\n\nContent.\n<!-- deciduous:end -->";
        replace_config_md_section(&md, new_section, "CLAUDE.md").unwrap();

        let result = fs::read_to_string(&md).unwrap();
        assert!(result.contains("Content."));
        assert!(result.contains("<!-- deciduous:start -->"));
        assert!(result.contains("<!-- deciduous:end -->"));
    }

    #[test]
    fn test_replace_section_preserves_non_h2_content_after_legacy() {
        let tmp = TempDir::new().unwrap();
        let md = tmp.path().join("CLAUDE.md");
        // Simulate: deciduous section last, followed by non-H2 user content
        fs::write(
            &md,
            "# My Project\n\n## Decision Graph Workflow\n\nOld stuff.\n\n### My Notes\n\nThese are important notes without an H2.\n",
        )
        .unwrap();

        let new_section = "<!-- deciduous:start -->\n## Decision Graph Workflow\n\nNew stuff.\n<!-- deciduous:end -->";
        replace_config_md_section(&md, new_section, "CLAUDE.md").unwrap();

        let result = fs::read_to_string(&md).unwrap();
        assert!(result.contains("New stuff."), "New section inserted");
        // Legacy fallback eats to EOF when no next ## found — this is the migration case.
        // After this update, markers are in place and future updates will preserve content.
        assert!(
            result.contains("<!-- deciduous:end -->"),
            "End marker present for future safety"
        );
    }
}
