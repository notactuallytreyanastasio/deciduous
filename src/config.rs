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
        let current_dir = std::env::current_dir().ok()?;
        let mut dir = current_dir.as_path();

        loop {
            let config_path = dir.join(".deciduous").join("config.toml");
            if config_path.exists() {
                return Some(config_path);
            }

            match dir.parent() {
                Some(parent) => dir = parent,
                None => break,
            }
        }
        None
    }

    /// Check if a branch is considered a "main" branch
    pub fn is_main_branch(&self, branch: &str) -> bool {
        self.branch.main_branches.iter().any(|b| b == branch)
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
}
