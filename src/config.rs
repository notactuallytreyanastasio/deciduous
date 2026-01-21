//! Configuration file support for deciduous
//!
//! Reads from .deciduous/config.toml

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration structure
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct Config {
    /// Branch settings
    #[serde(default)]
    pub branch: BranchConfig,

    /// GitHub settings for external repository references
    #[serde(default)]
    pub github: GithubConfig,

    /// Claude Code hooks configuration
    #[serde(default)]
    pub hooks: HooksConfig,

    /// Claude Code commands configuration
    #[serde(default)]
    pub commands: CommandsConfig,

    /// Claude Code skills configuration
    #[serde(default)]
    pub skills: SkillsConfig,
}

/// Hooks configuration for Claude Code integration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HooksConfig {
    /// Whether hooks are enabled
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Pre-tool-use hooks (run before Edit/Write/etc)
    #[serde(default = "default_pre_tool_use_hooks")]
    pub pre_tool_use: Vec<Hook>,

    /// Post-tool-use hooks (run after Bash/etc)
    #[serde(default = "default_post_tool_use_hooks")]
    pub post_tool_use: Vec<Hook>,
}

fn default_pre_tool_use_hooks() -> Vec<Hook> {
    vec![Hook::default_require_action_node()]
}

fn default_post_tool_use_hooks() -> Vec<Hook> {
    vec![Hook::default_post_commit_reminder()]
}

impl Default for HooksConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            pre_tool_use: default_pre_tool_use_hooks(),
            post_tool_use: default_post_tool_use_hooks(),
        }
    }
}

/// A single hook definition
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Hook {
    /// Hook name (used for filename)
    pub name: String,

    /// Description of what this hook does
    #[serde(default)]
    pub description: String,

    /// Regex pattern for matching tools (e.g., "Edit|Write" or "Bash")
    pub matcher: String,

    /// Whether this hook is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Script content (if inline) - mutually exclusive with script_path
    #[serde(default)]
    pub script: Option<String>,

    /// Path to script file (relative to .deciduous/) - mutually exclusive with script
    #[serde(default)]
    pub script_path: Option<String>,
}

impl Hook {
    /// Default pre-edit hook that requires an action node
    pub fn default_require_action_node() -> Self {
        Self {
            name: "require-action-node".to_string(),
            description: "Blocks Edit/Write if no recent action/goal node exists".to_string(),
            matcher: "Edit|Write".to_string(),
            enabled: true,
            script: None, // Uses built-in template
            script_path: None,
        }
    }

    /// Default post-commit hook that reminds to link commits
    pub fn default_post_commit_reminder() -> Self {
        Self {
            name: "post-commit-reminder".to_string(),
            description: "Reminds to link commits to the decision graph".to_string(),
            matcher: "Bash".to_string(),
            enabled: true,
            script: None, // Uses built-in template
            script_path: None,
        }
    }

    /// Check if this hook uses a built-in script
    pub fn uses_builtin(&self) -> bool {
        self.script.is_none() && self.script_path.is_none()
    }
}

/// Commands configuration for Claude Code slash commands
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CommandsConfig {
    /// Whether to install default commands
    #[serde(default = "default_true")]
    pub install_defaults: bool,

    /// Custom commands to install (paths relative to .deciduous/commands/)
    #[serde(default)]
    pub custom: Vec<String>,
}

impl Default for CommandsConfig {
    fn default() -> Self {
        Self {
            install_defaults: true,
            custom: vec![],
        }
    }
}

/// Skills configuration for Claude Code skills
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SkillsConfig {
    /// Whether to install default skills
    #[serde(default = "default_true")]
    pub install_defaults: bool,

    /// Custom skills to install (paths relative to .deciduous/skills/)
    #[serde(default)]
    pub custom: Vec<String>,
}

impl Default for SkillsConfig {
    fn default() -> Self {
        Self {
            install_defaults: true,
            custom: vec![],
        }
    }
}

/// GitHub-related configuration for commit/PR links
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct GithubConfig {
    /// External repository for commit links (e.g., "phoenixframework/phoenix")
    /// When set, commit hashes in nodes will link to this repo instead of the local one.
    /// Format: "owner/repo"
    #[serde(default)]
    pub commit_repo: Option<String>,
}

/// Branch-related configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BranchConfig {
    /// Main/default branch names (nodes on these branches won't trigger special grouping)
    /// Default: ["main", "master"]
    #[serde(default = "default_main_branches")]
    pub main_branches: Vec<String>,

    /// Whether to auto-detect and store branch on node creation
    /// Default: true
    #[serde(default = "default_true")]
    pub auto_detect: bool,
}

fn default_main_branches() -> Vec<String> {
    vec!["main".to_string(), "master".to_string()]
}

fn default_true() -> bool {
    true
}

impl Default for BranchConfig {
    fn default() -> Self {
        Self {
            main_branches: default_main_branches(),
            auto_detect: true,
        }
    }
}

impl Config {
    /// Load config from .deciduous/config.toml
    /// Returns default config if file doesn't exist
    pub fn load() -> Self {
        if let Some(path) = Self::find_config_path() {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                if let Ok(config) = toml::from_str(&contents) {
                    return config;
                }
            }
        }
        Self::default()
    }

    /// Find config.toml by walking up directory tree
    fn find_config_path() -> Option<PathBuf> {
        Self::find_deciduous_dir().map(|d| d.join("config.toml"))
    }

    /// Find .deciduous directory by walking up directory tree
    pub fn find_deciduous_dir() -> Option<PathBuf> {
        let current_dir = std::env::current_dir().ok()?;
        let mut dir = current_dir.as_path();

        loop {
            let deciduous_dir = dir.join(".deciduous");
            if deciduous_dir.exists() {
                return Some(deciduous_dir);
            }

            match dir.parent() {
                Some(parent) => dir = parent,
                None => break,
            }
        }
        None
    }

    /// Find the project root (parent of .deciduous)
    pub fn find_project_root() -> Option<PathBuf> {
        Self::find_deciduous_dir().and_then(|d| d.parent().map(|p| p.to_path_buf()))
    }

    /// Check if a branch is considered a "main" branch
    pub fn is_main_branch(&self, branch: &str) -> bool {
        self.branch.main_branches.iter().any(|b| b == branch)
    }

    /// Get all enabled pre-tool-use hooks
    pub fn enabled_pre_hooks(&self) -> Vec<&Hook> {
        if !self.hooks.enabled {
            return vec![];
        }
        self.hooks
            .pre_tool_use
            .iter()
            .filter(|h| h.enabled)
            .collect()
    }

    /// Get all enabled post-tool-use hooks
    pub fn enabled_post_hooks(&self) -> Vec<&Hook> {
        if !self.hooks.enabled {
            return vec![];
        }
        self.hooks
            .post_tool_use
            .iter()
            .filter(|h| h.enabled)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.is_main_branch("main"));
        assert!(config.is_main_branch("master"));
        assert!(!config.is_main_branch("feature-x"));
        assert!(config.branch.auto_detect);
    }

    #[test]
    fn test_parse_config() {
        let toml = r#"
[branch]
main_branches = ["main", "master", "develop"]
auto_detect = true
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert!(config.is_main_branch("develop"));
        assert!(!config.is_main_branch("feature-x"));
    }

    #[test]
    fn test_default_hooks() {
        let config = Config::default();
        assert!(config.hooks.enabled);

        // Should have default pre-tool-use hook
        assert_eq!(config.hooks.pre_tool_use.len(), 1);
        assert_eq!(config.hooks.pre_tool_use[0].name, "require-action-node");
        assert_eq!(config.hooks.pre_tool_use[0].matcher, "Edit|Write");
        assert!(config.hooks.pre_tool_use[0].enabled);

        // Should have default post-tool-use hook
        assert_eq!(config.hooks.post_tool_use.len(), 1);
        assert_eq!(config.hooks.post_tool_use[0].name, "post-commit-reminder");
        assert_eq!(config.hooks.post_tool_use[0].matcher, "Bash");
        assert!(config.hooks.post_tool_use[0].enabled);
    }

    #[test]
    fn test_parse_hooks_config() {
        let toml = r#"
[hooks]
enabled = true

[[hooks.pre_tool_use]]
name = "my-custom-hook"
description = "A custom pre-edit hook"
matcher = "Edit"
enabled = true
script = "echo 'hello'"

[[hooks.post_tool_use]]
name = "my-post-hook"
description = "A custom post hook"
matcher = "Bash"
enabled = false
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert!(config.hooks.enabled);
        assert_eq!(config.hooks.pre_tool_use.len(), 1);
        assert_eq!(config.hooks.pre_tool_use[0].name, "my-custom-hook");
        assert_eq!(
            config.hooks.pre_tool_use[0].script,
            Some("echo 'hello'".to_string())
        );

        // Check enabled_post_hooks filters correctly
        assert_eq!(config.enabled_post_hooks().len(), 0);
    }

    #[test]
    fn test_hooks_disabled() {
        let toml = r#"
[hooks]
enabled = false
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert!(!config.hooks.enabled);
        // Even with default hooks, enabled_*_hooks should return empty
        assert_eq!(config.enabled_pre_hooks().len(), 0);
        assert_eq!(config.enabled_post_hooks().len(), 0);
    }

    #[test]
    fn test_hook_uses_builtin() {
        let hook = Hook::default_require_action_node();
        assert!(hook.uses_builtin());

        let custom_hook = Hook {
            name: "custom".to_string(),
            description: "".to_string(),
            matcher: "Edit".to_string(),
            enabled: true,
            script: Some("echo hi".to_string()),
            script_path: None,
        };
        assert!(!custom_hook.uses_builtin());
    }
}
