//! Claude Code hooks management
//!
//! Generates and installs hooks for Claude Code integration.
//! Hooks enforce the decision graph workflow by:
//! - Blocking edits without recent action nodes (pre-tool-use)
//! - Reminding to link commits to the graph (post-tool-use)

use crate::config::{Config, Hook};
use crate::init::templates::{HOOK_POST_COMMIT_REMINDER, HOOK_REQUIRE_ACTION_NODE};
use colored::Colorize;
use serde_json::json;
use std::fs;
use std::path::Path;

/// Install hooks from config to .claude/hooks/ and generate settings.json
pub fn install_hooks(project_root: &Path) -> Result<(), String> {
    let config = Config::load();

    if !config.hooks.enabled {
        println!("   {} Hooks disabled in config", "Skipping".yellow());
        return Ok(());
    }

    let claude_dir = project_root.join(".claude");
    let hooks_dir = claude_dir.join("hooks");

    // Create .claude/hooks directory
    if !hooks_dir.exists() {
        fs::create_dir_all(&hooks_dir)
            .map_err(|e| format!("Could not create .claude/hooks: {}", e))?;
        println!("   {} .claude/hooks/", "Creating".green());
    }

    // Install pre-tool-use hooks
    for hook in &config.hooks.pre_tool_use {
        if hook.enabled {
            install_single_hook(&hooks_dir, hook, &config)?;
        }
    }

    // Install post-tool-use hooks
    for hook in &config.hooks.post_tool_use {
        if hook.enabled {
            install_single_hook(&hooks_dir, hook, &config)?;
        }
    }

    // Generate settings.json
    generate_settings_json(&claude_dir, &config)?;

    Ok(())
}

/// Install a single hook script
fn install_single_hook(hooks_dir: &Path, hook: &Hook, config: &Config) -> Result<(), String> {
    let script_content = get_hook_script(hook, config)?;
    let script_path = hooks_dir.join(format!("{}.sh", hook.name));

    fs::write(&script_path, &script_content)
        .map_err(|e| format!("Could not write {}.sh: {}", hook.name, e))?;

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path)
            .map_err(|e| format!("Could not get metadata: {}", e))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms)
            .map_err(|e| format!("Could not set permissions: {}", e))?;
    }

    println!("   {} .claude/hooks/{}.sh", "Installed".green(), hook.name);
    Ok(())
}

/// Get the script content for a hook
fn get_hook_script(hook: &Hook, _config: &Config) -> Result<String, String> {
    // If hook has inline script, use that
    if let Some(script) = &hook.script {
        return Ok(script.clone());
    }

    // If hook has script_path, read from file
    if let Some(path) = &hook.script_path {
        let deciduous_dir = Config::find_deciduous_dir()
            .ok_or_else(|| "Could not find .deciduous directory".to_string())?;
        let full_path = deciduous_dir.join(path);
        return fs::read_to_string(&full_path)
            .map_err(|e| format!("Could not read hook script {}: {}", path, e));
    }

    // Use built-in templates for known hooks
    match hook.name.as_str() {
        "require-action-node" => Ok(HOOK_REQUIRE_ACTION_NODE.to_string()),
        "post-commit-reminder" => Ok(HOOK_POST_COMMIT_REMINDER.to_string()),
        _ => Err(format!(
            "Hook '{}' has no script and is not a built-in hook",
            hook.name
        )),
    }
}

/// Generate .claude/settings.json from config
fn generate_settings_json(claude_dir: &Path, config: &Config) -> Result<(), String> {
    let settings_path = claude_dir.join("settings.json");

    // Build PreToolUse hooks
    let pre_hooks: Vec<serde_json::Value> = config
        .hooks
        .pre_tool_use
        .iter()
        .filter(|h| h.enabled)
        .map(|h| {
            json!({
                "matcher": h.matcher,
                "hooks": [{
                    "type": "command",
                    "command": format!("\"$CLAUDE_PROJECT_DIR/.claude/hooks/{}.sh\"", h.name)
                }]
            })
        })
        .collect();

    // Build PostToolUse hooks
    let post_hooks: Vec<serde_json::Value> = config
        .hooks
        .post_tool_use
        .iter()
        .filter(|h| h.enabled)
        .map(|h| {
            json!({
                "matcher": h.matcher,
                "hooks": [{
                    "type": "command",
                    "command": format!("\"$CLAUDE_PROJECT_DIR/.claude/hooks/{}.sh\"", h.name)
                }]
            })
        })
        .collect();

    let settings = json!({
        "hooks": {
            "PreToolUse": pre_hooks,
            "PostToolUse": post_hooks
        }
    });

    let json_string = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("Could not serialize settings: {}", e))?;

    fs::write(&settings_path, json_string)
        .map_err(|e| format!("Could not write settings.json: {}", e))?;

    println!("   {} .claude/settings.json", "Generated".green());
    Ok(())
}

/// Show status of installed hooks
pub fn hooks_status() -> Result<(), String> {
    let config = Config::load();

    println!("\n{}", "Hook Configuration".cyan().bold());
    println!(
        "   Enabled: {}",
        if config.hooks.enabled {
            "yes".green()
        } else {
            "no".red()
        }
    );

    if !config.hooks.enabled {
        println!("\n   Hooks are disabled in .deciduous/config.toml");
        return Ok(());
    }

    println!(
        "\n{}",
        "Pre-Tool-Use Hooks (block before Edit/Write/etc):".cyan()
    );
    if config.hooks.pre_tool_use.is_empty() {
        println!("   (none configured)");
    } else {
        for hook in &config.hooks.pre_tool_use {
            let status = if hook.enabled {
                "✓".green()
            } else {
                "✗".red()
            };
            println!("   {} {} (matcher: {})", status, hook.name, hook.matcher);
            if !hook.description.is_empty() {
                println!("      {}", hook.description.dimmed());
            }
        }
    }

    println!("\n{}", "Post-Tool-Use Hooks (run after Bash/etc):".cyan());
    if config.hooks.post_tool_use.is_empty() {
        println!("   (none configured)");
    } else {
        for hook in &config.hooks.post_tool_use {
            let status = if hook.enabled {
                "✓".green()
            } else {
                "✗".red()
            };
            println!("   {} {} (matcher: {})", status, hook.name, hook.matcher);
            if !hook.description.is_empty() {
                println!("      {}", hook.description.dimmed());
            }
        }
    }

    // Check if hooks are actually installed
    if let Some(project_root) = Config::find_project_root() {
        let hooks_dir = project_root.join(".claude").join("hooks");
        println!("\n{}", "Installation Status:".cyan());

        if hooks_dir.exists() {
            for hook in config
                .hooks
                .pre_tool_use
                .iter()
                .chain(config.hooks.post_tool_use.iter())
            {
                let script_path = hooks_dir.join(format!("{}.sh", hook.name));
                let installed = if script_path.exists() {
                    "installed".green()
                } else {
                    "not installed".red()
                };
                println!("   {} {}", hook.name, installed);
            }
        } else {
            println!(
                "   {} .claude/hooks/ directory does not exist",
                "Warning:".yellow()
            );
            println!("   Run 'deciduous hooks install' to install hooks");
        }
    }

    println!();
    Ok(())
}

/// Uninstall hooks (remove .claude/hooks/ and settings.json hooks section)
pub fn uninstall_hooks(project_root: &Path) -> Result<(), String> {
    let hooks_dir = project_root.join(".claude").join("hooks");

    if hooks_dir.exists() {
        fs::remove_dir_all(&hooks_dir)
            .map_err(|e| format!("Could not remove .claude/hooks: {}", e))?;
        println!("   {} .claude/hooks/", "Removed".green());
    }

    // Update settings.json to remove hooks
    let settings_path = project_root.join(".claude").join("settings.json");
    if settings_path.exists() {
        let settings = json!({
            "hooks": {
                "PreToolUse": [],
                "PostToolUse": []
            }
        });
        let json_string = serde_json::to_string_pretty(&settings)
            .map_err(|e| format!("Could not serialize settings: {}", e))?;
        fs::write(&settings_path, json_string)
            .map_err(|e| format!("Could not write settings.json: {}", e))?;
        println!(
            "   {} .claude/settings.json (hooks removed)",
            "Updated".green()
        );
    }

    Ok(())
}

/// Show full Claude Code integration status (hooks, commands, skills, agents)
pub fn integration_status() -> Result<(), String> {
    let project_root = Config::find_project_root();

    println!("\n{}", "Claude Code Integration Status".cyan().bold());
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let Some(project_root) = project_root else {
        println!(
            "\n   {} Could not find project root (.deciduous directory)",
            "Error:".red()
        );
        println!("   Run 'deciduous init' to initialize the project.");
        return Ok(());
    };

    let claude_dir = project_root.join(".claude");
    if !claude_dir.exists() {
        println!("\n   {} .claude directory not found", "Warning:".yellow());
        println!("   Run 'deciduous init' or 'deciduous update' to create it.");
        return Ok(());
    }

    // === Hooks ===
    println!("\n{}", "Hooks:".cyan());
    let hooks_dir = claude_dir.join("hooks");
    if hooks_dir.exists() {
        let entries: Vec<_> = fs::read_dir(&hooks_dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().is_some_and(|ext| ext == "sh"))
                    .collect()
            })
            .unwrap_or_default();

        if entries.is_empty() {
            println!("   {} (no hooks installed)", "○".yellow());
        } else {
            for entry in entries {
                let name = entry.file_name().to_string_lossy().to_string();
                println!("   {} {}", "✓".green(), name);
            }
        }
    } else {
        println!("   {} (not installed)", "○".yellow());
        println!("   Run 'deciduous hooks install' to install hooks");
    }

    // === Commands (slash commands) ===
    println!("\n{}", "Slash Commands:".cyan());
    let commands_dir = claude_dir.join("commands");
    if commands_dir.exists() {
        let entries: Vec<_> = fs::read_dir(&commands_dir)
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
        println!("   {} (not installed)", "○".yellow());
    }

    // === Skills ===
    println!("\n{}", "Skills:".cyan());
    let skills_dir = claude_dir.join("skills");
    if skills_dir.exists() {
        let entries: Vec<_> = fs::read_dir(&skills_dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
                    .collect()
            })
            .unwrap_or_default();

        if entries.is_empty() {
            println!("   {} (no skills installed)", "○".yellow());
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
        println!("   {} (not installed)", "○".yellow());
    }

    // === Agents ===
    println!("\n{}", "Agents:".cyan());
    let agents_path = claude_dir.join("agents.toml");
    if agents_path.exists() {
        // Try to parse agents.toml to count agents
        if let Ok(contents) = fs::read_to_string(&agents_path) {
            let agent_count = contents.matches("[agents.").count();
            if agent_count > 0 {
                println!("   {} {} agent(s) configured", "✓".green(), agent_count);

                // Extract agent names
                for line in contents.lines() {
                    if line.starts_with("[agents.") && line.ends_with(']') {
                        let name = line.trim_start_matches("[agents.").trim_end_matches(']');
                        println!("      • {}", name);
                    }
                }
            } else {
                println!(
                    "   {} agents.toml exists but no agents defined",
                    "○".yellow()
                );
            }
        } else {
            println!("   {} agents.toml exists", "✓".green());
        }
    } else {
        println!("   {} (not configured)", "○".yellow());
    }

    // === Settings ===
    println!("\n{}", "Settings:".cyan());
    let settings_path = claude_dir.join("settings.json");
    let settings_local_path = claude_dir.join("settings.local.json");

    if settings_path.exists() {
        println!("   {} settings.json", "✓".green());
    } else {
        println!("   {} settings.json (not found)", "○".yellow());
    }

    if settings_local_path.exists() {
        println!("   {} settings.local.json", "✓".green());
    }

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!(
        "\nRun {} to install/update Claude Code integration files.",
        "'deciduous update'".cyan()
    );
    println!();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_get_builtin_hook_script() {
        let hook = Hook::default_require_action_node();
        let script = get_hook_script(&hook, &Config::default()).unwrap();
        assert!(script.contains("require-action-node"));
        assert!(script.contains("deciduous nodes"));
    }

    #[test]
    fn test_get_custom_inline_script() {
        let hook = Hook {
            name: "custom".to_string(),
            description: "test".to_string(),
            matcher: "Edit".to_string(),
            enabled: true,
            script: Some("echo 'custom hook'".to_string()),
            script_path: None,
        };
        let script = get_hook_script(&hook, &Config::default()).unwrap();
        assert_eq!(script, "echo 'custom hook'");
    }

    #[test]
    fn test_install_hooks() {
        let tmp = TempDir::new().unwrap();
        let project_root = tmp.path();

        // Create .deciduous with config
        let deciduous_dir = project_root.join(".deciduous");
        fs::create_dir_all(&deciduous_dir).unwrap();

        // Create minimal config
        let config_content = r#"
[hooks]
enabled = true
"#;
        fs::write(deciduous_dir.join("config.toml"), config_content).unwrap();

        // Set current dir for Config::load to work
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(project_root).unwrap();

        let result = install_hooks(project_root);

        // Restore dir
        std::env::set_current_dir(original_dir).unwrap();

        assert!(result.is_ok());
        assert!(project_root
            .join(".claude/hooks/require-action-node.sh")
            .exists());
        assert!(project_root
            .join(".claude/hooks/post-commit-reminder.sh")
            .exists());
        assert!(project_root.join(".claude/settings.json").exists());
    }
}
