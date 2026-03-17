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
        version: "0.13.11",
        highlights: &[
            "Version checking is now always-on (no opt-in needed)",
            "Patch updates show a quiet one-liner notification",
            "Minor/major updates show a prominent banner encouraging upgrade",
            "Deprecated `deciduous auto-update on/off` and `--no-auto-update` flag",
        ],
    },
    Release {
        version: "0.13.10",
        highlights: &[
            "Opt-in auto-update version check hook - checks crates.io once per 24h, non-blocking",
            "Toggle with `deciduous auto-update on/off`, or `deciduous init --no-auto-update`",
            "Copy markdown button added to Q&A chat responses in web viewer",
            "CI: auto-format on push to non-main branches",
        ],
    },
    Release {
        version: "0.13.9",
        highlights: &[
            "Fix: OpenCode plugins write to .deciduous/plugin.log instead of console.error (stderr also corrupts TUI)",
            "Neither stdout nor stderr is safe for OpenCode TUI - all plugin output now goes to log file",
            "Fix: OpenCode install runs directory migration before creating new structure",
        ],
    },
    Release {
        version: "0.13.8",
        highlights: &[
            "Fix: OpenCode post-commit hook still used console.log (stdout) instead of console.error (stderr)",
            "Completes the stdout fix from 0.13.7 - both pre-edit and post-commit hooks now use stderr",
        ],
    },
    Release {
        version: "0.13.7",
        highlights: &[
            "Fix: OpenCode plugin uses console.error instead of console.log to avoid breaking TUI STDOUT",
            "require-action-node and post-commit-reminder plugins no longer pollute OpenCode display",
        ],
    },
    Release {
        version: "0.13.6",
        highlights: &[
            "decision-graph: new Layer 4 - use gh CLI to find PRs for design context and review discussion",
            "PR descriptions and review threads mined for decision rationale, alternatives considered, and trade-offs",
            "Updated in both Claude Code and OpenCode templates",
        ],
    },
    Release {
        version: "0.13.5",
        highlights: &[
            "OpenCode command templates now match Claude Code verbosity (decision-graph, document, decision, archaeology)",
            "decision-graph: added Hardening Phase, Cross-Narrative Connections, Narrative Discipline, Rich Node Content, Edge Types",
            "document: added documentation structure template, test refinement step, decision criteria, example usage",
            "decision: added Web UI Branch Filter, disconnected node auditing, expanded multi-user sync",
            "archaeology skill: added Querying the Graph section and descriptive details",
        ],
    },
    Release {
        version: "0.13.3",
        highlights: &[
            "OpenCode bootstrapping upgraded to modern directory conventions (plugins/, commands/, skills/)",
            "Skills now use proper SKILL.md format in .opencode/skills/<name>/SKILL.md",
            "New: custom deciduous agent in .opencode/agents/deciduous.md",
            "New: custom deciduous tool in .opencode/tools/deciduous.ts wraps CLI for direct graph ops",
            "Migration logic: deciduous update auto-migrates from old singular dirs to plural",
        ],
    },
    Release {
        version: "0.13.2",
        highlights: &[
            "OpenCode integration fully updated to match Claude Code feature parity",
            "3 new OpenCode commands: /document, /sync, /decision-graph",
            "All 9 OpenCode commands updated with document attachments, event sync, graph integrity",
            "All 3 OpenCode skills updated: /pulse, /narratives, /archaeology use latest CLI commands",
            "OpenCode plugins improved: .deciduous check, better git commit detection",
            "AGENTS.md workflow section now includes full command/skill tables and node flow rule",
        ],
    },
    Release {
        version: "0.13.1",
        highlights: &[
            "Fix: deciduous update no longer deletes user content after the Decision Graph Workflow section in CLAUDE.md",
            "Added <!-- deciduous:start/end --> markers for safe section replacement",
            "Legacy CLAUDE.md files auto-migrate to markers on first update",
            "6 new tests for section replacement edge cases",
        ],
    },
    Release {
        version: "0.13.0",
        highlights: &[
            "Document attachments: attach files (images, PDFs, diagrams) to decision nodes",
            "7 doc subcommands: attach, list, show, describe, open, detach, gc",
            "AI-generated descriptions for attached documents (--ai-describe)",
            "Content-hash deduplication and soft-delete with garbage collection",
            "Theme system: tag and group nodes by theme",
            "Web viewer shows attached documents and themes in detail panel",
            "REST API: /api/documents and /api/documents/file/<id> endpoints",
            "Event-based sync support for document attachments",
            "All skills, commands, and docs updated with document awareness",
            "Init now creates .deciduous/documents/ directory",
        ],
    },
    Release {
        version: "0.12.2",
        highlights: &[
            "README rewritten to lead with /pulse, /archaeology, /narratives skills",
            "New Deep Q&A Interface section documenting POST /api/ask and FTS5 search",
            "Landing page updated: 1,170+ nodes, 1,020+ edges, 74 days of development",
            "Elevator pitch rewritten around three skills and conversational Q&A",
            "Revisit node type added to landing page (7 node types)",
            "Web viewer section updated with Archaeology view and Q&A panel",
        ],
    },
    Release {
        version: "0.12.1",
        highlights: &[
            "Bootstrap all slash commands via init/update: /document, /build-test, /serve-ui, /sync-graph, /decision-graph, /sync",
            "Updated CLAUDE.md template with full command and skill reference tables",
            "Documentation updated across README, QUICK_REFERENCE, ARCHITECTURE",
        ],
    },
    Release {
        version: "0.12.0",
        highlights: &[
            "Redesigned web viewer with hierarchical narrative view",
            "Smart narrative detection - filters to significant 10+ node trees",
            "D3 DAG flowchart visualization with left-to-right hierarchical layout",
            "Full-text search with type filter buttons",
            "Resizable panels - drag to resize all sections",
            "Gold shimmer effect for selected node highlighting",
            "Automatic graph loading when clicking search results",
            "Removed 20k+ lines of old scattered components for unified App.tsx",
        ],
    },
    Release {
        version: "0.11.2",
        highlights: &[
            "Fix: `deciduous opencode install` now works standalone without `deciduous init`",
            "Creates core infrastructure (.deciduous/, config, database path, docs/) automatically",
        ],
    },
    Release {
        version: "0.11.1",
        highlights: &[
            "Pre-built binaries for Linux, macOS, and Windows on GitHub Releases",
            "Added Linux ARM64 support",
            "SHA256 checksums included with releases",
        ],
    },
    Release {
        version: "0.11.0",
        highlights: &[
            "OpenCode integration support via `deciduous init` and `deciduous update`",
            "New `integration-status` command to check Claude Code and OpenCode setup",
            "Auto-generates opencode.json with deciduous plugin configuration",
        ],
    },
    Release {
        version: "0.10.3",
        highlights: &[
            "Fix card stack shrinking - cards now render at consistent size",
            "Remove node count limits - graphs show all nodes without truncation",
        ],
    },
    Release {
        version: "0.10.2",
        highlights: &[
            "Deep linking with parameterized routes (/archaeology/:id, /dag/:nodeId, etc.)",
            "Q&A history view with full-text search across all sessions",
            "Prompt modal for viewing verbatim user prompts on nodes",
            "Improved CardStack navigation and keyboard shortcuts",
            "BrowserRouter for cleaner URLs (no more hash fragments)",
        ],
    },
    Release {
        version: "0.10.1",
        highlights: &[
            "Fix card stack not appearing when clicking nodes in archaeology view",
            "Card stack now shows description, branch, and files by default",
        ],
    },
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
            "Orange color coding in web viewer",
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

/// Returns true if the version difference is patch-only (same major.minor)
pub fn is_patch_only(current: &str, latest: &str) -> bool {
    let (cur_major, cur_minor, _) = parse_version(current);
    let (lat_major, lat_minor, _) = parse_version(latest);
    cur_major == lat_major && cur_minor == lat_minor
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
    fn test_is_patch_only() {
        assert!(is_patch_only("0.13.10", "0.13.11"));
        assert!(is_patch_only("1.2.3", "1.2.9"));
        assert!(!is_patch_only("0.13.10", "0.14.0"));
        assert!(!is_patch_only("0.13.10", "1.0.0"));
        assert!(!is_patch_only("1.0.0", "2.0.0"));
    }

    #[test]
    fn test_format_releases() {
        let releases = get_releases_between("0.9.4", "0.9.5");
        let formatted = format_releases(&releases);
        assert!(formatted.contains("v0.9.5"));
        assert!(formatted.contains("check-update"));
    }
}
