use clap::Subcommand;
use colored::Colorize;
use deciduous::github::{ensure_roadmap_label, GitHubClient};
use deciduous::roadmap::{
    generate_issue_body, parse_roadmap, write_roadmap_with_metadata, RoadmapSection,
};
use deciduous::Database;
use std::path::PathBuf;

use super::super::truncate;

#[derive(Subcommand, Debug)]
pub enum RoadmapAction {
    /// Initialize roadmap sync (parses ROADMAP.md and adds metadata)
    Init {
        /// Path to ROADMAP.md (default: ROADMAP.md)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },

    /// Refresh roadmap items (clears and re-parses ROADMAP.md, preserving decision graph)
    Refresh {
        /// Path to ROADMAP.md (default: ROADMAP.md)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },

    /// Sync ROADMAP.md with GitHub Issues (dry-run by default, use --execute to apply)
    Sync {
        /// Path to ROADMAP.md (default: ROADMAP.md)
        #[arg(short, long)]
        path: Option<PathBuf>,

        /// GitHub repo in owner/repo format (auto-detected from git remote)
        #[arg(short, long)]
        repo: Option<String>,

        /// Actually apply changes (default is dry-run mode)
        #[arg(long)]
        execute: bool,

        /// Create GitHub issues for new sections
        #[arg(long, default_value = "true")]
        create_issues: bool,
    },

    /// List roadmap items with status
    List {
        /// Path to ROADMAP.md (default: ROADMAP.md)
        #[arg(short, long)]
        path: Option<PathBuf>,

        /// Filter by section name
        #[arg(short, long)]
        section: Option<String>,

        /// Show only items with GitHub issues
        #[arg(long)]
        with_issues: bool,

        /// Show only items without GitHub issues
        #[arg(long)]
        without_issues: bool,
    },

    /// Link a roadmap item to a decision graph outcome node
    Link {
        /// Roadmap item change_id or title (partial match)
        item: String,

        /// Outcome node ID to link
        outcome_id: i32,
    },

    /// Remove outcome link from a roadmap item
    Unlink {
        /// Roadmap item change_id or title (partial match)
        item: String,
    },

    /// Show sync conflicts
    Conflicts {
        /// Resolve conflicts interactively
        #[arg(long)]
        resolve: bool,
    },

    /// Show sync status summary
    Status {
        /// Path to ROADMAP.md (default: ROADMAP.md)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },

    /// Audit completion status of roadmap items
    Check {
        /// Path to ROADMAP.md (default: ROADMAP.md)
        #[arg(short, long)]
        path: Option<PathBuf>,

        /// Show only incomplete items
        #[arg(long)]
        incomplete: bool,

        /// Show only complete items
        #[arg(long)]
        complete: bool,
    },
}

pub fn handle_roadmap(db: &Database, action: RoadmapAction) {
    match action {
        RoadmapAction::Init { path } => {
            let roadmap_path = path.unwrap_or_else(|| PathBuf::from("ROADMAP.md"));

            if !roadmap_path.exists() {
                eprintln!(
                    "{} File not found: {}",
                    "Error:".red(),
                    roadmap_path.display()
                );
                std::process::exit(1);
            }

            // Parse the roadmap
            let parsed = match parse_roadmap(&roadmap_path) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("{} Parsing roadmap: {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            };

            println!(
                "{} Found {} sections in {}",
                "Parsed:".green(),
                parsed.sections.len(),
                roadmap_path.display()
            );

            // Read original content for rewriting
            let content = match std::fs::read_to_string(&roadmap_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("{} Reading file: {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            };

            // Write back with metadata
            let updated = match write_roadmap_with_metadata(
                &roadmap_path,
                &parsed.sections,
                &content,
            ) {
                Ok(u) => u,
                Err(e) => {
                    eprintln!("{} Writing metadata: {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            };
            if let Err(e) = std::fs::write(&roadmap_path, &updated) {
                eprintln!("{} Writing file: {}", "Error:".red(), e);
                std::process::exit(1);
            }

            // Track current level-2 parent section for grouping
            let mut current_l2_parent: Option<String> = None;

            // Store sections in database
            for section in &parsed.sections {
                // Level 2 headers (## Section) are top-level groupings
                // Level 3 headers (### Subsection) contain the actual tasks
                let (section_parent, items_section) = if section.level == 2 {
                    current_l2_parent = Some(section.title.clone());
                    // Level 2 sections have no parent, their items go under them
                    (None, Some(section.title.as_str()))
                } else {
                    // Level 3 sections belong to the current L2 parent
                    // Their items belong directly to this L3 section
                    (current_l2_parent.as_deref(), Some(section.title.as_str()))
                };

                // Create the section header entry (checkbox_state = "none")
                if let Err(e) = db.create_roadmap_item(
                    &section.title,
                    section.description.as_deref(),
                    section_parent,
                    None, // parent_id - we don't track hierarchy by ID yet
                    "none",
                ) {
                    eprintln!("{} Creating roadmap item: {}", "Warning:".yellow(), e);
                }

                // Create items for checkboxes - they belong to THIS section
                for item in &section.items {
                    let state = if item.checked { "checked" } else { "unchecked" };
                    if let Err(e) = db.create_roadmap_item(
                        &item.text,
                        None,
                        items_section, // Items belong to the section that contains them
                        None,          // parent_id
                        state,
                    ) {
                        eprintln!("{} Creating roadmap item: {}", "Warning:".yellow(), e);
                    }
                }
            }

            // Count items
            let total_items: usize = parsed.sections.iter().map(|s| s.items.len()).sum();
            println!(
                "{} Initialized {} sections with {} items",
                "Success:".green(),
                parsed.sections.len(),
                total_items
            );
            println!("  Metadata comments added to {}", roadmap_path.display());
        }

        RoadmapAction::Refresh { path } => {
            let roadmap_path = path.unwrap_or_else(|| PathBuf::from("ROADMAP.md"));

            if !roadmap_path.exists() {
                eprintln!(
                    "{} File not found: {}",
                    "Error:".red(),
                    roadmap_path.display()
                );
                std::process::exit(1);
            }

            // Clear existing roadmap items
            let cleared = match db.clear_roadmap_items() {
                Ok(n) => n,
                Err(e) => {
                    eprintln!("{} Clearing roadmap items: {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            };
            println!(
                "{} Cleared {} existing roadmap items",
                "Info:".cyan(),
                cleared
            );

            // Re-parse the roadmap
            let parsed = match parse_roadmap(&roadmap_path) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("{} Parsing roadmap: {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            };

            // Track current level-2 parent section for grouping
            let mut current_l2_parent: Option<String> = None;

            // Store sections in database
            for section in &parsed.sections {
                let (section_parent, items_section) = if section.level == 2 {
                    current_l2_parent = Some(section.title.clone());
                    (None, Some(section.title.as_str()))
                } else {
                    (current_l2_parent.as_deref(), Some(section.title.as_str()))
                };

                // Create the section header entry
                if let Err(e) = db.create_roadmap_item(
                    &section.title,
                    section.description.as_deref(),
                    section_parent,
                    None,
                    "none",
                ) {
                    eprintln!("{} Creating roadmap item: {}", "Warning:".yellow(), e);
                }

                // Create items for checkboxes
                for item in &section.items {
                    let state = if item.checked { "checked" } else { "unchecked" };
                    if let Err(e) =
                        db.create_roadmap_item(&item.text, None, items_section, None, state)
                    {
                        eprintln!("{} Creating roadmap item: {}", "Warning:".yellow(), e);
                    }
                }
            }

            let total_items: usize = parsed.sections.iter().map(|s| s.items.len()).sum();
            println!(
                "{} Refreshed {} sections with {} items",
                "Success:".green(),
                parsed.sections.len(),
                total_items
            );
        }

        RoadmapAction::Sync {
            path,
            repo,
            execute,
            create_issues,
        } => {
            let dry_run = !execute; // Default is dry-run mode
            let roadmap_path = path.unwrap_or_else(|| PathBuf::from("ROADMAP.md"));

            if !roadmap_path.exists() {
                eprintln!(
                    "{} File not found: {}",
                    "Error:".red(),
                    roadmap_path.display()
                );
                eprintln!("Run 'deciduous roadmap init' first");
                std::process::exit(1);
            }

            // Initialize GitHub client
            let gh_client = match repo {
                Some(r) => GitHubClient::new(Some(r)),
                None => match GitHubClient::auto_detect() {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("{} Auto-detecting repo: {}", "Error:".red(), e);
                        eprintln!("Specify repo with --repo owner/repo");
                        std::process::exit(1);
                    }
                },
            };

            // Check auth
            match GitHubClient::check_auth() {
                Ok(true) => {}
                Ok(false) | Err(_) => {
                    eprintln!("{} Not authenticated with GitHub", "Error:".red());
                    eprintln!("Run 'gh auth login' first");
                    std::process::exit(1);
                }
            }

            // Parse roadmap
            let parsed = match parse_roadmap(&roadmap_path) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("{} Parsing roadmap: {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            };

            // Only sync level 3 sections (actual items, not parent headers)
            let syncable_sections: Vec<&RoadmapSection> =
                parsed.sections.iter().filter(|s| s.level == 3).collect();

            if dry_run {
                println!(
                    "{} {} sections (use --execute to apply changes)",
                    "Roadmap (dry run):".yellow(),
                    syncable_sections.len()
                );
            } else {
                println!(
                    "{} Syncing {} sections",
                    "Roadmap:".cyan(),
                    syncable_sections.len()
                );
            }

            if let Some(repo_name) = gh_client.repo_name() {
                println!("  Repository: {}", repo_name);
            }

            // Ensure 'roadmap' label exists if we're creating issues
            if !dry_run && create_issues {
                match ensure_roadmap_label(&gh_client) {
                    Ok(true) => println!("  {} Created 'roadmap' label", "✓".green()),
                    Ok(false) => {} // Label already exists
                    Err(e) => eprintln!(
                        "  {} Creating label: {} (issues may fail)",
                        "Warning:".yellow(),
                        e
                    ),
                }
            }

            let mut created = 0;
            let mut updated = 0;
            let mut skipped = 0;

            for section in &syncable_sections {
                // Check if section already has an issue
                if let Some(issue_num) = section.github_issue_number {
                    // Update existing issue
                    let body = generate_issue_body(section);

                    if dry_run {
                        println!(
                            "  {} Would update issue #{}: {}",
                            "[DRY]".yellow(),
                            issue_num,
                            section.title
                        );
                        updated += 1;
                    } else {
                        match gh_client.update_issue_body(issue_num, &body) {
                            Ok(()) => {
                                println!(
                                    "  {} Updated issue #{}: {}",
                                    "✓".green(),
                                    issue_num,
                                    section.title
                                );
                                updated += 1;
                            }
                            Err(e) => {
                                eprintln!(
                                    "  {} Updating issue #{}: {}",
                                    "✗".red(),
                                    issue_num,
                                    e
                                );
                            }
                        }
                    }
                } else if create_issues {
                    // Create new issue
                    let body = generate_issue_body(section);

                    if dry_run {
                        println!(
                            "  {} Would create issue: {}",
                            "[DRY]".yellow(),
                            section.title
                        );
                        created += 1;
                    } else {
                        match gh_client.create_issue(&section.title, &body, &["roadmap"]) {
                            Ok(issue) => {
                                println!(
                                    "  {} Created issue #{}: {}",
                                    "✓".green(),
                                    issue.number,
                                    section.title
                                );
                                created += 1;

                                // Update database with issue number
                                if let Err(e) = db.update_roadmap_item_github_by_title(
                                    &section.title,
                                    issue.number,
                                    &issue.state,
                                ) {
                                    eprintln!(
                                        "    {} Updating database: {}",
                                        "Warning:".yellow(),
                                        e
                                    );
                                }

                                // Cache issue for web display
                                if let Some(repo_name) = gh_client.repo_name() {
                                    if let Err(e) = db.cache_github_issue(
                                        issue.number,
                                        repo_name,
                                        &issue.title,
                                        Some(&issue.body),
                                        &issue.state,
                                        &issue.html_url,
                                        &issue.created_at,
                                        &issue.updated_at,
                                    ) {
                                        eprintln!(
                                            "    {} Caching issue: {}",
                                            "Warning:".yellow(),
                                            e
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!(
                                    "  {} Creating issue for '{}': {}",
                                    "✗".red(),
                                    section.title,
                                    e
                                );
                            }
                        }
                    }
                } else {
                    println!("  {} Skipping (no issue): {}", "-".dimmed(), section.title);
                    skipped += 1;
                }
            }

            // Write updated roadmap with issue metadata
            if !dry_run && created > 0 {
                let content = std::fs::read_to_string(&roadmap_path).unwrap_or_default();
                match write_roadmap_with_metadata(&roadmap_path, &parsed.sections, &content)
                {
                    Ok(updated_content) => {
                        if let Err(e) = std::fs::write(&roadmap_path, &updated_content) {
                            eprintln!("{} Writing roadmap: {}", "Warning:".yellow(), e);
                        }
                    }
                    Err(e) => eprintln!("{} Updating metadata: {}", "Warning:".yellow(), e),
                }
            }

            println!(
                "\n{} {} created, {} updated, {} skipped",
                if dry_run {
                    "Summary (dry run):".yellow()
                } else {
                    "Summary:".green()
                },
                created,
                updated,
                skipped
            );
        }

        RoadmapAction::List {
            path,
            section,
            with_issues,
            without_issues,
        } => {
            let roadmap_path = path.unwrap_or_else(|| PathBuf::from("ROADMAP.md"));

            if !roadmap_path.exists() {
                eprintln!(
                    "{} File not found: {}",
                    "Error:".red(),
                    roadmap_path.display()
                );
                std::process::exit(1);
            }

            let parsed = match parse_roadmap(&roadmap_path) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("{} Parsing roadmap: {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            };

            // Filter sections
            let filtered: Vec<_> = parsed
                .sections
                .iter()
                .filter(|s| {
                    if let Some(ref sect) = section {
                        s.title.to_lowercase().contains(&sect.to_lowercase())
                    } else {
                        true
                    }
                })
                .filter(|s| {
                    if with_issues {
                        s.github_issue_number.is_some()
                    } else if without_issues {
                        s.github_issue_number.is_none()
                    } else {
                        true
                    }
                })
                .collect();

            if filtered.is_empty() {
                println!("No roadmap items found matching filters.");
                return;
            }

            println!("{} ({} sections)\n", "ROADMAP.md".cyan(), filtered.len());

            for s in &filtered {
                // Show section header based on level
                let header_prefix = if s.level == 2 { "##" } else { "###" };

                let issue_str = match s.github_issue_number {
                    Some(n) => format!("#{}", n).green().to_string(),
                    None => "no issue".dimmed().to_string(),
                };

                let completed: usize = s.items.iter().filter(|i| i.checked).count();
                let total = s.items.len();

                if total > 0 {
                    println!(
                        "{} {} [{}/{}] ({})",
                        header_prefix.yellow(),
                        s.title,
                        completed,
                        total,
                        issue_str
                    );
                } else {
                    println!("{} {} ({})", header_prefix.yellow(), s.title, issue_str);
                }

                // Show checkbox items
                for item in &s.items {
                    let check = if item.checked {
                        "✓".green()
                    } else {
                        "○".dimmed()
                    };
                    println!("    {} {}", check, item.text);
                }
            }
        }

        RoadmapAction::Link { item, outcome_id } => {
            // Find roadmap item by title or change_id
            let items = match db.get_all_roadmap_items() {
                Ok(i) => i,
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            };

            let target = items.iter().find(|i| {
                i.change_id == item || i.title.to_lowercase().contains(&item.to_lowercase())
            });

            match target {
                Some(roadmap_item) => {
                    // Verify outcome node exists and is an outcome
                    match db.get_all_nodes() {
                        Ok(nodes) => {
                            let node = nodes.iter().find(|n| n.id == outcome_id);
                            match node {
                                Some(n) if n.node_type == "outcome" => {
                                    // Link them
                                    match db.link_roadmap_to_outcome(
                                        roadmap_item.id,
                                        outcome_id,
                                        &n.change_id,
                                    ) {
                                        Ok(()) => {
                                            println!(
                                                "{} Linked '{}' to outcome #{}: {}",
                                                "Success:".green(),
                                                roadmap_item.title,
                                                outcome_id,
                                                n.title
                                            );
                                        }
                                        Err(e) => {
                                            eprintln!("{} {}", "Error:".red(), e);
                                            std::process::exit(1);
                                        }
                                    }
                                }
                                Some(n) => {
                                    eprintln!(
                                        "{} Node #{} is a {}, not an outcome",
                                        "Error:".red(),
                                        outcome_id,
                                        n.node_type
                                    );
                                    std::process::exit(1);
                                }
                                None => {
                                    eprintln!(
                                        "{} Node #{} not found",
                                        "Error:".red(),
                                        outcome_id
                                    );
                                    std::process::exit(1);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            std::process::exit(1);
                        }
                    }
                }
                None => {
                    eprintln!("{} Roadmap item '{}' not found", "Error:".red(), item);
                    eprintln!("Run 'deciduous roadmap list' to see available items");
                    std::process::exit(1);
                }
            }
        }

        RoadmapAction::Unlink { item } => {
            let items = match db.get_all_roadmap_items() {
                Ok(i) => i,
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            };

            let target = items.iter().find(|i| {
                i.change_id == item || i.title.to_lowercase().contains(&item.to_lowercase())
            });

            match target {
                Some(roadmap_item) => {
                    match db.unlink_roadmap_from_outcome(roadmap_item.id) {
                        Ok(()) => {
                            println!(
                                "{} Unlinked '{}' from outcome",
                                "Success:".green(),
                                roadmap_item.title
                            );
                        }
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            std::process::exit(1);
                        }
                    }
                }
                None => {
                    eprintln!("{} Roadmap item '{}' not found", "Error:".red(), item);
                    std::process::exit(1);
                }
            }
        }

        RoadmapAction::Conflicts { resolve } => {
            let conflicts = match db.get_unresolved_conflicts() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            };

            if conflicts.is_empty() {
                println!("{} No sync conflicts", "Success:".green());
                return;
            }

            println!(
                "{} {} conflicts found:\n",
                "Conflicts:".yellow(),
                conflicts.len()
            );

            for conflict in &conflicts {
                println!(
                    "  Item: {} ({})",
                    conflict.item_change_id, conflict.conflict_type
                );
                println!(
                    "    Local:  {}",
                    conflict.local_value.as_deref().unwrap_or("(none)")
                );
                println!(
                    "    Remote: {}",
                    conflict.remote_value.as_deref().unwrap_or("(none)")
                );
                if let Some(ref res) = conflict.resolution {
                    println!("    Resolution: {}", res);
                }
                println!();
            }

            if resolve {
                println!(
                    "{} Interactive conflict resolution not yet implemented",
                    "TODO:".yellow()
                );
                println!(
                    "For now, manually edit ROADMAP.md and run 'deciduous roadmap sync'"
                );
            }
        }

        RoadmapAction::Status { path } => {
            let roadmap_path = path.unwrap_or_else(|| PathBuf::from("ROADMAP.md"));

            // Get sync state from database
            match db.get_roadmap_sync_state(&roadmap_path.to_string_lossy()) {
                Ok(Some(state)) => {
                    println!("{}", "Roadmap Sync Status".cyan());
                    println!("  Path: {}", roadmap_path.display());
                    if let Some(ref repo) = state.github_repo {
                        println!("  GitHub Repo: {}", repo);
                    }
                    if let Some(ref last_sync) = state.last_github_sync {
                        println!("  Last GitHub Sync: {}", last_sync);
                    }
                    if let Some(ref last_parse) = state.last_markdown_parse {
                        println!("  Last Parse: {}", last_parse);
                    }
                    if state.conflict_count > 0 {
                        println!("  {} {} conflicts", "⚠".yellow(), state.conflict_count);
                    } else {
                        println!("  {} No conflicts", "✓".green());
                    }
                }
                Ok(None) => {
                    println!("{} Roadmap not initialized", "Status:".yellow());
                    println!("Run 'deciduous roadmap init' to get started");
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            }

            // Show item counts from database
            match db.get_all_roadmap_items() {
                Ok(items) => {
                    let with_issues = items
                        .iter()
                        .filter(|i| i.github_issue_number.is_some())
                        .count();
                    let with_outcomes =
                        items.iter().filter(|i| i.outcome_node_id.is_some()).count();
                    let completed = items
                        .iter()
                        .filter(|i| i.checkbox_state == "checked")
                        .count();

                    println!("\n{}", "Items:".cyan());
                    println!("  Total: {}", items.len());
                    println!("  With GitHub Issues: {}", with_issues);
                    println!("  With Outcome Links: {}", with_outcomes);
                    println!("  Completed: {}", completed);
                }
                Err(_) => {
                    println!("\n{} No items in database yet", "Items:".dimmed());
                }
            }
        }

        RoadmapAction::Check {
            path: _,
            incomplete,
            complete,
        } => {
            // Get all roadmap items from database
            let items = match db.get_all_roadmap_items() {
                Ok(i) => i,
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            };

            if items.is_empty() {
                println!("{} No roadmap items in database", "Status:".yellow());
                println!("Run 'deciduous roadmap init' first");
                return;
            }

            // Check completion for each item
            let mut complete_count = 0;
            let mut incomplete_count = 0;
            let mut results: Vec<(String, bool, bool, bool, bool)> = Vec::new();

            for item in &items {
                match db.check_roadmap_item_completion(item.id) {
                    Ok((is_complete, has_outcome, issue_closed)) => {
                        let checkbox_checked = item.checkbox_state == "checked";

                        if is_complete && checkbox_checked {
                            complete_count += 1;
                        } else {
                            incomplete_count += 1;
                        }

                        results.push((
                            item.title.clone(),
                            is_complete && checkbox_checked,
                            checkbox_checked,
                            has_outcome,
                            issue_closed,
                        ));
                    }
                    Err(e) => {
                        eprintln!("{} Checking {}: {}", "Warning:".yellow(), item.title, e);
                    }
                }
            }

            // Print header
            println!("{}", "Roadmap Completion Audit".cyan().bold());
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!();

            // Print results based on filters
            for (title, is_complete, checkbox, outcome, issue) in &results {
                // Apply filters
                if incomplete && *is_complete {
                    continue;
                }
                if complete && !*is_complete {
                    continue;
                }

                let status_icon = if *is_complete {
                    "✓".green()
                } else {
                    "○".yellow()
                };

                let checkbox_icon = if *checkbox {
                    "☑".green()
                } else {
                    "☐".dimmed()
                };
                let outcome_icon = if *outcome {
                    "⚡".green()
                } else {
                    "⚡".dimmed()
                };
                let issue_icon = if *issue {
                    "🔒".green()
                } else {
                    "🔓".dimmed()
                };

                println!(
                    "{} {} {} {} {}",
                    status_icon,
                    checkbox_icon,
                    outcome_icon,
                    issue_icon,
                    truncate(title, 60)
                );
            }

            // Print summary
            println!();
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!();
            println!("{}", "Legend:".dimmed());
            println!(
                "  {} = checkbox checked    {} = outcome linked    {} = issue closed",
                "☑".green(),
                "⚡".green(),
                "🔒".green()
            );
            println!();
            println!("{}", "Summary:".cyan());
            println!("  {} {} complete", "✓".green(), complete_count);
            println!("  {} {} incomplete", "○".yellow(), incomplete_count);
            println!("  {} total items", items.len());

            if incomplete_count > 0 {
                println!();
                println!(
                    "{} Completion requires: checkbox ☑ AND outcome ⚡ AND issue closed 🔒",
                    "Note:".dimmed()
                );
            }
        }
    }
}
