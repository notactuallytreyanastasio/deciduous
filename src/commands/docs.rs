use clap::Subcommand;
use colored::Colorize;
use deciduous::Database;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

use super::super::truncate;

#[derive(Subcommand, Debug)]
pub enum DocAction {
    /// Attach a file to a decision graph node
    Attach {
        /// Node ID to attach the file to
        node_id: i32,

        /// Path to the file to attach
        file: PathBuf,

        /// Manual description
        #[arg(short, long)]
        description: Option<String>,

        /// Generate AI description using claude CLI
        #[arg(long)]
        ai_describe: bool,
    },

    /// List documents attached to a node (or all nodes)
    List {
        /// Node ID to list documents for (omit for all)
        node_id: Option<i32>,

        /// Show detached (removed) documents too
        #[arg(long)]
        include_detached: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Set or update the description of a document
    Describe {
        /// Document ID
        doc_id: i32,

        /// Description text (omit to read from stdin)
        description: Option<String>,

        /// Generate AI description using claude CLI
        #[arg(long)]
        ai: bool,
    },

    /// Detach (soft-delete) a document from its node
    Detach {
        /// Document ID to detach
        doc_id: i32,
    },

    /// Show details of a specific document
    Show {
        /// Document ID
        doc_id: i32,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Open the attached file in the default application
    Open {
        /// Document ID
        doc_id: i32,
    },

    /// Garbage-collect orphaned files (no active document records reference them)
    Gc {
        /// Only show what would be deleted
        #[arg(long)]
        dry_run: bool,
    },
}

pub fn handle_doc(db: &Database, action: DocAction) {
    match action {
        DocAction::Attach {
            node_id,
            file,
            description,
            ai_describe,
        } => {
            if !file.exists() {
                eprintln!("{} File not found: {}", "Error:".red(), file.display());
                std::process::exit(1);
            }

            let original_filename = file
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());

            // Compute SHA-256 hash
            let file_bytes = match std::fs::read(&file) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("{} Failed to read file: {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            };
            let hash = format!("{:x}", Sha256::digest(&file_bytes));
            let hash_prefix = &hash[..8];

            // Storage filename: original_name.sha_prefix
            let storage_filename = format!("{}.{}", original_filename, hash_prefix);

            // Detect MIME type
            let mime_type = detect_mime_type(&original_filename);

            let file_size = file_bytes.len() as i32;

            // Store file in .deciduous/documents/
            let docs_dir = PathBuf::from(".deciduous/documents");
            if let Err(e) = std::fs::create_dir_all(&docs_dir) {
                eprintln!("{} Failed to create documents dir: {}", "Error:".red(), e);
                std::process::exit(1);
            }

            let dest_path = docs_dir.join(&storage_filename);
            if !dest_path.exists() {
                if let Err(e) = std::fs::copy(&file, &dest_path) {
                    eprintln!("{} Failed to copy file: {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            }

            // Get description (manual, AI, or none)
            let desc = if let Some(d) = description {
                Some((d, "manual"))
            } else if ai_describe {
                match generate_ai_description(&original_filename, &file) {
                    Some(d) => Some((d, "ai")),
                    None => {
                        eprintln!(
                            "{} Could not generate AI description (is claude CLI installed?)",
                            "Warning:".yellow()
                        );
                        None
                    }
                }
            } else {
                None
            };

            let (desc_text, desc_source) = match &desc {
                Some((text, source)) => (Some(text.as_str()), *source),
                None => (None, "none"),
            };

            match db.attach_document(
                node_id,
                &hash,
                &original_filename,
                &storage_filename,
                mime_type,
                file_size,
                desc_text,
                desc_source,
                None,
            ) {
                Ok(id) => {
                    println!(
                        "{} document {} to node {} ({})",
                        "Attached".green(),
                        id,
                        node_id,
                        original_filename
                    );
                    if let Some((text, _)) = &desc {
                        println!("  Description: {}", truncate(text, 80));
                    }
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            }
        }

        DocAction::List {
            node_id,
            include_detached,
            json,
        } => match db.get_node_documents(node_id, include_detached) {
            Ok(docs) => {
                if json {
                    println!("{}", serde_json::to_string_pretty(&docs).unwrap());
                } else if docs.is_empty() {
                    println!("No documents found.");
                } else {
                    println!("{} documents:", docs.len());
                    println!(
                        "{:<5} {:<8} {:<25} {:<10} {:<8} DESCRIPTION",
                        "ID", "NODE", "FILENAME", "TYPE", "SIZE"
                    );
                    println!("{}", "-".repeat(80));
                    for d in docs {
                        let size_str = format_file_size(d.file_size);
                        let desc = d
                            .description
                            .as_deref()
                            .map(|s| truncate(s, 30))
                            .unwrap_or_default();
                        println!(
                            "{:<5} {:<8} {:<25} {:<10} {:<8} {}",
                            d.id,
                            d.node_id,
                            truncate(&d.original_filename, 24),
                            truncate(&d.mime_type, 9),
                            size_str,
                            desc
                        );
                    }
                }
            }
            Err(e) => {
                eprintln!("{} {}", "Error:".red(), e);
                std::process::exit(1);
            }
        },

        DocAction::Describe {
            doc_id,
            description,
            ai,
        } => {
            let desc = if let Some(d) = description {
                (d, "manual")
            } else if ai {
                let doc = match db.get_document(doc_id) {
                    Ok(Some(d)) => d,
                    Ok(None) => {
                        eprintln!("{} Document {} not found", "Error:".red(), doc_id);
                        std::process::exit(1);
                    }
                    Err(e) => {
                        eprintln!("{} {}", "Error:".red(), e);
                        std::process::exit(1);
                    }
                };
                let file_path =
                    PathBuf::from(".deciduous/documents").join(&doc.storage_filename);
                match generate_ai_description(&doc.original_filename, &file_path) {
                    Some(d) => (d, "ai"),
                    None => {
                        eprintln!("{} Could not generate AI description", "Error:".red());
                        std::process::exit(1);
                    }
                }
            } else {
                // Read from stdin
                let mut input = String::new();
                std::io::Read::read_to_string(&mut std::io::stdin(), &mut input)
                    .unwrap_or_default();
                (input.trim().to_string(), "manual")
            };

            match db.update_document_description(doc_id, &desc.0, desc.1) {
                Ok(()) => println!("{} description for document {}", "Updated".green(), doc_id),
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            }
        }

        DocAction::Detach { doc_id } => match db.detach_document(doc_id) {
            Ok(()) => println!("{} document {}", "Detached".red(), doc_id),
            Err(e) => {
                eprintln!("{} {}", "Error:".red(), e);
                std::process::exit(1);
            }
        },

        DocAction::Show { doc_id, json } => match db.get_document(doc_id) {
            Ok(Some(doc)) => {
                if json {
                    println!("{}", serde_json::to_string_pretty(&doc).unwrap());
                } else {
                    println!("{}", "Document Details".bold().underline());
                    println!("  ID:          {}", doc.id);
                    println!("  Node:        {}", doc.node_id);
                    println!("  Filename:    {}", doc.original_filename);
                    println!("  MIME type:   {}", doc.mime_type);
                    println!("  Size:        {}", format_file_size(doc.file_size));
                    println!("  Hash:        {}", doc.content_hash);
                    println!(
                        "  Storage:     .deciduous/documents/{}",
                        doc.storage_filename
                    );
                    println!("  Attached:    {}", doc.attached_at);
                    if let Some(by) = &doc.attached_by {
                        println!("  Attached by: {}", by);
                    }
                    if let Some(desc) = &doc.description {
                        println!("  Description: {} ({})", desc, doc.description_source);
                    }
                    if doc.detached_at.is_some() {
                        println!("  {}", "DETACHED".red());
                    }
                }
            }
            Ok(None) => {
                eprintln!("{} Document {} not found", "Error:".red(), doc_id);
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("{} {}", "Error:".red(), e);
                std::process::exit(1);
            }
        },

        DocAction::Open { doc_id } => match db.get_document(doc_id) {
            Ok(Some(doc)) => {
                let file_path =
                    PathBuf::from(".deciduous/documents").join(&doc.storage_filename);
                if !file_path.exists() {
                    eprintln!(
                        "{} File not found on disk: {}",
                        "Error:".red(),
                        file_path.display()
                    );
                    std::process::exit(1);
                }

                // Copy to temp with original filename for better OS handling
                let temp_dir = std::env::temp_dir().join("deciduous-docs");
                std::fs::create_dir_all(&temp_dir).ok();
                let temp_path = temp_dir.join(&doc.original_filename);
                if let Err(e) = std::fs::copy(&file_path, &temp_path) {
                    eprintln!("{} Failed to copy file: {}", "Error:".red(), e);
                    std::process::exit(1);
                }

                #[cfg(target_os = "macos")]
                let open_cmd = "open";
                #[cfg(not(target_os = "macos"))]
                let open_cmd = "xdg-open";

                match std::process::Command::new(open_cmd).arg(&temp_path).spawn() {
                    Ok(_) => println!("{} {}", "Opened".green(), doc.original_filename),
                    Err(e) => {
                        eprintln!("{} Failed to open file: {}", "Error:".red(), e);
                        std::process::exit(1);
                    }
                }
            }
            Ok(None) => {
                eprintln!("{} Document {} not found", "Error:".red(), doc_id);
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("{} {}", "Error:".red(), e);
                std::process::exit(1);
            }
        },

        DocAction::Gc { dry_run } => {
            let docs_dir = PathBuf::from(".deciduous/documents");
            if !docs_dir.exists() {
                println!("No documents directory found.");
                return;
            }

            let active_hashes = db.get_active_content_hashes().unwrap_or_default();
            let mut orphans = Vec::new();

            if let Ok(entries) = std::fs::read_dir(&docs_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        let fname = path
                            .file_name()
                            .map(|f| f.to_string_lossy().to_string())
                            .unwrap_or_default();

                        // Check if any active doc references this storage filename
                        let is_active = db
                            .get_node_documents(None, false)
                            .unwrap_or_default()
                            .iter()
                            .any(|d| d.storage_filename == fname);

                        if !is_active {
                            orphans.push(path);
                        }
                    }
                }
            }

            if orphans.is_empty() {
                println!("No orphaned files found.");
            } else {
                println!("{} orphaned files:", orphans.len());
                for p in &orphans {
                    println!("  {}", p.display());
                }
                if dry_run {
                    println!("(dry run - no files deleted)");
                } else {
                    for p in &orphans {
                        std::fs::remove_file(p).ok();
                    }
                    println!("{} {} orphaned files", "Deleted".red(), orphans.len());
                }
            }

            drop(active_hashes);
        }
    }
}

fn detect_mime_type(filename: &str) -> &'static str {
    match filename
        .rsplit('.')
        .next()
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("md" | "markdown") => "text/markdown",
        Some("txt") => "text/plain",
        Some("rs") => "text/x-rust",
        Some("ts" | "tsx") => "text/typescript",
        Some("js" | "jsx") => "text/javascript",
        Some("py") => "text/x-python",
        Some("json") => "application/json",
        Some("yaml" | "yml") => "text/yaml",
        Some("toml") => "text/toml",
        Some("html" | "htm") => "text/html",
        Some("css") => "text/css",
        Some("zip") => "application/zip",
        Some("tar") => "application/x-tar",
        Some("gz") => "application/gzip",
        Some("csv") => "text/csv",
        Some("xml") => "text/xml",
        Some("sql") => "text/x-sql",
        Some("sh" | "bash") => "text/x-shellscript",
        Some("go") => "text/x-go",
        Some("rb") => "text/x-ruby",
        Some("java") => "text/x-java",
        Some("c" | "h") => "text/x-c",
        Some("cpp" | "hpp" | "cc") => "text/x-c++",
        _ => "application/octet-stream",
    }
}

fn format_file_size(bytes: i32) -> String {
    let bytes = bytes as f64;
    if bytes < 1024.0 {
        format!("{}B", bytes as i64)
    } else if bytes < 1024.0 * 1024.0 {
        format!("{:.1}KB", bytes / 1024.0)
    } else {
        format!("{:.1}MB", bytes / (1024.0 * 1024.0))
    }
}

fn generate_ai_description(filename: &str, file_path: &std::path::Path) -> Option<String> {
    let prompt = format!(
        "Analyze this file and provide a concise 1-2 sentence description of its contents, purpose, and key details. File: {}",
        filename
    );

    // Try to read text content for context
    let content_context = if let Ok(content) = std::fs::read_to_string(file_path) {
        let preview: String = content.chars().take(2000).collect();
        format!("{}\n\nFile content preview:\n{}", prompt, preview)
    } else {
        prompt
    };

    let output = std::process::Command::new("claude")
        .args(["-p", &content_context])
        .output()
        .ok()?;

    if output.status.success() {
        String::from_utf8(output.stdout)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    } else {
        None
    }
}
