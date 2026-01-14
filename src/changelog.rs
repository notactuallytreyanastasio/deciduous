//! Embedded changelog for version update notifications
//!
//! This module contains release notes that are shown when users
//! upgrade deciduous and run `check-update` or other commands.

/// A single release entry
pub struct Release {
    pub version: &'static str,
    pub highlights: &'static [&'static str],
}

/// All releases, newest first
pub const RELEASES: &[Release] = &[
    Release {
        version: "0.10.0",
        highlights: &[
            "Archaeology view is now the default at `deciduous serve`",
            "Narrative-focused exploration with AI-powered explanations",
            "Session-based card stack UI with keyboard navigation (j/k/g/G/Space)",
            "Mobile-responsive with touch swipe gestures",
            "Persistent chat history and filters via localStorage",
            "Graph performance optimizations for large node counts",
        ],
    },
    Release {
        version: "0.9.6",
        highlights: &[
            "Embedded changelog shows what's new when upgrading",
            "Update reminder shown after all commands when outdated",
            "`check-update` now shows detailed feature list",
        ],
    },
    Release {
        version: "0.9.5",
        highlights: &[
            "New `check-update` command - detect when integration files need updating",
            "`update` no longer overwrites your configs (settings.json, config.toml, docs/)",
            "Version tracking via .deciduous/.version",
        ],
    },
    Release {
        version: "0.9.4",
        highlights: &[
            "Improved `update` command help text",
            "Better documentation for update workflow",
        ],
    },
    Release {
        version: "0.9.3",
        highlights: &[
            "Skills added to init/update: /pulse, /narratives, /archaeology",
            "`update` now refreshes skills and agents.toml",
        ],
    },
    Release {
        version: "0.9.2",
        highlights: &[
            "New `revisit` node type for pivots and direction changes",
            "Orange color coding in TUI and web viewer",
        ],
    },
    Release {
        version: "0.9.1",
        highlights: &["Republish of 0.9.0 (yanked)"],
    },
    Release {
        version: "0.9.0",
        highlights: &[
            "New `--date` flag for backdating nodes",
            "Archaeology workflow support",
        ],
    },
    Release {
        version: "0.8.25",
        highlights: &[
            "New `delete` command to remove nodes",
            "New `unlink` command to remove edges",
        ],
    },
];

/// Get releases between two versions (exclusive of from, inclusive of to)
/// Returns releases in order from oldest to newest for display
pub fn get_releases_between(from_version: &str, to_version: &str) -> Vec<&'static Release> {
    let from_parts = parse_version(from_version);
    let to_parts = parse_version(to_version);

    let mut releases: Vec<&Release> = RELEASES
        .iter()
        .filter(|r| {
            let v = parse_version(r.version);
            v > from_parts && v <= to_parts
        })
        .collect();

    // Reverse to show oldest first (chronological order)
    releases.reverse();
    releases
}

/// Parse version string into comparable tuple
fn parse_version(v: &str) -> (u32, u32, u32) {
    let parts: Vec<u32> = v.split('.').filter_map(|p| p.parse().ok()).collect();

    (
        parts.first().copied().unwrap_or(0),
        parts.get(1).copied().unwrap_or(0),
        parts.get(2).copied().unwrap_or(0),
    )
}

/// Format releases for display
pub fn format_releases(releases: &[&Release]) -> String {
    if releases.is_empty() {
        return String::new();
    }

    let mut output = String::new();
    for release in releases {
        output.push_str(&format!("\n  v{}\n", release.version));
        for highlight in release.highlights {
            output.push_str(&format!("    • {}\n", highlight));
        }
    }
    output
}

/// Check if integration files are outdated and return a brief reminder message
/// Returns None if up to date or if version file doesn't exist
pub fn check_version_reminder(current_binary_version: &str) -> Option<String> {
    let version_file = std::path::Path::new(".deciduous/.version");

    if !version_file.exists() {
        return None;
    }

    let installed_version = std::fs::read_to_string(version_file)
        .ok()?
        .trim()
        .to_string();

    if installed_version == current_binary_version {
        return None;
    }

    let releases = get_releases_between(&installed_version, current_binary_version);
    let feature_count: usize = releases.iter().map(|r| r.highlights.len()).sum();

    Some(format!(
        "Update available: v{} → v{} ({} new features). Run 'deciduous check-update' for details.",
        installed_version, current_binary_version, feature_count
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version() {
        assert_eq!(parse_version("0.9.5"), (0, 9, 5));
        assert_eq!(parse_version("1.0.0"), (1, 0, 0));
        assert_eq!(parse_version("0.8.25"), (0, 8, 25));
    }

    #[test]
    fn test_get_releases_between() {
        let releases = get_releases_between("0.9.3", "0.9.5");
        assert_eq!(releases.len(), 2);
        assert_eq!(releases[0].version, "0.9.4");
        assert_eq!(releases[1].version, "0.9.5");
    }

    #[test]
    fn test_get_releases_between_same_version() {
        let releases = get_releases_between("0.9.5", "0.9.5");
        assert!(releases.is_empty());
    }

    #[test]
    fn test_format_releases() {
        let releases = get_releases_between("0.9.4", "0.9.5");
        let formatted = format_releases(&releases);
        assert!(formatted.contains("v0.9.5"));
        assert!(formatted.contains("check-update"));
    }
}
