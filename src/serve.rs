//! HTTP server for decision graph viewer
//!
//! `deciduous serve` → starts server, opens browser, shows graph

use crate::db::{Database, DecisionGraph, QaInteraction, QaSearchResult, RoadmapItem};
use serde::Serialize;
use std::collections::HashMap;
use tiny_http::{Header, Method, Request, Response, Server};

#[derive(Serialize)]
struct ApiResponse<T> {
    ok: bool,
    data: Option<T>,
    error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    fn success(data: T) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
        }
    }
}

// Embedded React graph viewer (built with bun from web/ directory)
// To rebuild: cd web && ./build-embed.sh
const GRAPH_VIEWER_HTML: &str = include_str!("viewer.html");

/// Start the decision graph viewer server
pub fn start_graph_server(port: u16) -> std::io::Result<()> {
    let addr = format!("127.0.0.1:{}", port);
    let server = Server::http(&addr)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

    let url = format!("http://localhost:{}", port);

    eprintln!("\n\x1b[1;32m🌳 Deciduous\x1b[0m");
    eprintln!("   Graph viewer: {}", url);
    eprintln!("   Press Ctrl+C to stop\n");

    // Handle requests
    for request in server.incoming_requests() {
        if let Err(e) = handle_request(request) {
            eprintln!("Error: {}", e);
        }
    }

    Ok(())
}

fn handle_request(request: Request) -> std::io::Result<()> {
    let url = request.url().to_string();
    let path = url.split('?').next().unwrap_or("/");
    let method = request.method().clone();

    match (&method, path) {
        // API endpoints first (before SPA fallback)
        // API: Get decision graph
        (&Method::Get, "/api/graph") => {
            let graph = get_decision_graph();
            let json = serde_json::to_string(&ApiResponse::success(graph))?;

            let response = Response::from_string(json).with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
            );
            request.respond(response)
        }

        // API: Get command log
        (&Method::Get, "/api/commands") => {
            let commands = get_command_log();
            let json = serde_json::to_string(&ApiResponse::success(commands))?;

            let response = Response::from_string(json).with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
            );
            request.respond(response)
        }

        // API: Get roadmap items
        (&Method::Get, "/api/roadmap") => {
            let items = get_roadmap_items();
            let json = serde_json::to_string(&ApiResponse::success(items))?;

            let response = Response::from_string(json).with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
            );
            request.respond(response)
        }

        // API: Get git history for linked commits
        (&Method::Get, "/api/git-history") => {
            let history = get_git_history();
            let json = serde_json::to_string(&ApiResponse::success(history))?;

            let response = Response::from_string(json).with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
            );
            request.respond(response)
        }

        // API: Toggle roadmap item checkbox (POST /api/roadmap/checkbox)
        (&Method::Post, "/api/roadmap/checkbox") => handle_toggle_checkbox(request),

        // API: Ask Claude about the code (POST /api/ask)
        (&Method::Post, "/api/ask") => handle_ask_question(request),

        // API: Search Q&A interactions (GET /api/qa/search?q=...&limit=20)
        (&Method::Get, "/api/qa/search") => handle_qa_search(request, &url),

        // API: Get paginated Q&A interactions (GET /api/qa?offset=0&limit=20)
        (&Method::Get, "/api/qa") => handle_qa_list(request, &url),

        // API: Get or delete single Q&A interaction (GET/DELETE /api/qa/:id)
        (&Method::Get, p) if p.starts_with("/api/qa/") => handle_qa_get(request, p),
        (&Method::Delete, p) if p.starts_with("/api/qa/") => handle_qa_delete(request, p),

        // Serve SPA for all other GET requests (client-side routing)
        (&Method::Get, _) => {
            let response = Response::from_string(GRAPH_VIEWER_HTML)
                .with_header(Header::from_bytes(&b"Content-Type"[..], &b"text/html"[..]).unwrap());
            request.respond(response)
        }

        // 404 for non-GET requests to unknown paths
        _ => {
            let response = Response::from_string("Not found").with_status_code(404);
            request.respond(response)
        }
    }
}

fn get_decision_graph() -> DecisionGraph {
    // Load config for external repo support
    let config = crate::config::Config::load();
    let include_config = config.github.commit_repo.is_some();
    let config_opt = if include_config { Some(config) } else { None };

    match Database::open() {
        Ok(db) => db
            .get_graph_with_config(config_opt.clone())
            .unwrap_or_else(|_| DecisionGraph {
                nodes: vec![],
                edges: vec![],
                config: config_opt.clone(),
            }),
        Err(_) => DecisionGraph {
            nodes: vec![],
            edges: vec![],
            config: config_opt,
        },
    }
}

fn get_command_log() -> Vec<crate::db::CommandLog> {
    match Database::open() {
        Ok(db) => db.get_recent_commands(100).unwrap_or_default(),
        Err(_) => vec![],
    }
}

fn get_roadmap_items() -> Vec<RoadmapItem> {
    match Database::open() {
        Ok(db) => db.get_all_roadmap_items().unwrap_or_default(),
        Err(_) => vec![],
    }
}

// === Git History Types and Functions ===

/// Git commit info for timeline view (matches web/src/types/graph.ts GitCommit)
#[derive(Serialize)]
struct GitCommit {
    hash: String,
    short_hash: String,
    author: String,
    date: String,
    message: String,
    files_changed: Option<u32>,
}

/// Get git history for all commits linked to nodes
fn get_git_history() -> Vec<GitCommit> {
    use std::collections::HashSet;

    eprintln!("[git-history] Starting get_git_history");
    eprintln!("[git-history] Current dir: {:?}", std::env::current_dir());

    // Get all nodes from database
    let nodes = match Database::open() {
        Ok(db) => db.get_all_nodes().unwrap_or_default(),
        Err(e) => {
            eprintln!("[git-history] Database error: {:?}", e);
            return vec![];
        }
    };
    eprintln!("[git-history] Got {} nodes from database", nodes.len());

    // Find git repo root by looking for .git directory
    // Start from current dir and walk up
    let repo_root = find_git_repo_root();
    eprintln!("[git-history] Git repo root: {:?}", repo_root);

    // Extract unique commit hashes from node metadata
    let mut hashes = HashSet::new();
    for node in &nodes {
        if let Some(ref meta_json) = node.metadata_json {
            if let Ok(meta) = serde_json::from_str::<serde_json::Value>(meta_json) {
                if let Some(commit) = meta.get("commit").and_then(|c| c.as_str()) {
                    if !commit.is_empty() {
                        hashes.insert(commit.to_string());
                    }
                }
            }
        }
    }
    eprintln!("[git-history] Found {} unique commit hashes", hashes.len());

    // Get commit info from git for each hash
    let mut commits: Vec<GitCommit> = Vec::new();
    let mut failed = 0;
    for hash in &hashes {
        if let Some(commit) = get_git_commit_info(hash, repo_root.as_deref()) {
            commits.push(commit);
        } else {
            failed += 1;
        }
    }
    eprintln!(
        "[git-history] Got {} commits, {} failed lookups",
        commits.len(),
        failed
    );

    // Sort by date (newest first)
    commits.sort_by(|a, b| b.date.cmp(&a.date));
    commits
}

/// Find git repository root by looking for .deciduous folder (same as db.rs)
/// The .deciduous folder is in the project root, which also has .git
fn find_git_repo_root() -> Option<std::path::PathBuf> {
    let current_dir = std::env::current_dir().ok()?;
    let mut dir = current_dir.as_path();
    loop {
        // Look for .deciduous - that's the project root
        let deciduous_dir = dir.join(".deciduous");
        if deciduous_dir.exists() && deciduous_dir.is_dir() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

/// Get commit info from git for a given hash
fn get_git_commit_info(hash: &str, repo_root: Option<&std::path::Path>) -> Option<GitCommit> {
    use std::process::Command;

    // Get commit info: hash, author, date (ISO), full message body
    // Use %x00 (null byte) as separator since message can have newlines
    let mut cmd = Command::new("git");
    if let Some(root) = repo_root {
        cmd.current_dir(root);
    }
    let output = cmd
        .args(["log", "-1", "--format=%H%x00%an%x00%aI%x00%B", hash])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = stdout.trim().split('\x00').collect();
    if parts.len() < 4 {
        return None;
    }

    // Clean up the message - trim whitespace
    let message = parts[3].trim().to_string();

    // Get files changed count
    let mut files_cmd = Command::new("git");
    if let Some(root) = repo_root {
        files_cmd.current_dir(root);
    }
    let files_output = files_cmd
        .args(["diff-tree", "--no-commit-id", "--name-only", "-r", hash])
        .output()
        .ok();

    let files_changed = files_output.and_then(|o| {
        if o.status.success() {
            let count = String::from_utf8_lossy(&o.stdout).trim().lines().count();
            Some(count as u32)
        } else {
            None
        }
    });

    Some(GitCommit {
        hash: parts[0].to_string(),
        short_hash: parts[0].chars().take(7).collect(),
        author: parts[1].to_string(),
        date: parts[2].to_string(),
        message,
        files_changed,
    })
}

#[derive(serde::Deserialize)]
struct ToggleCheckboxRequest {
    item_id: i32,
    checkbox_state: String,
}

// === Ask Question Types ===

#[derive(serde::Deserialize)]
struct AskRequest {
    question: String,
    context: Option<AskContext>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct AskContext {
    selected_node_id: Option<i32>,
    visible_node_ids: Option<Vec<i32>>,
    current_branch: Option<String>,
    narrative: Option<NarrativeContext>,
}

/// Narrative context from archaeology view
#[derive(serde::Deserialize, serde::Serialize)]
struct NarrativeContext {
    name: String,
    root_id: i32,
    #[serde(default)]
    node_ids: Vec<i32>,
    #[serde(default)]
    pivots: Vec<PivotContext>,
    #[serde(default)]
    github_links: Vec<GithubLinkContext>,
}

/// Pivot context - where an approach changed
#[derive(serde::Deserialize, serde::Serialize)]
struct PivotContext {
    revisit_id: i32,
    observation_ids: Vec<i32>,
    superseded_ids: Vec<i32>,
    new_approach_ids: Vec<i32>,
}

/// GitHub link context
#[derive(serde::Deserialize, serde::Serialize)]
struct GithubLinkContext {
    #[serde(rename = "type")]
    link_type: String,
    identifier: String,
    repo: String,
}

#[derive(serde::Serialize)]
struct AskResponse {
    answer: String,
}

fn handle_ask_question(mut request: Request) -> std::io::Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    // Read request body
    let mut body = String::new();
    if let Err(e) = request.as_reader().read_to_string(&mut body) {
        let json = serde_json::to_string(&ApiResponse::<()> {
            ok: false,
            data: None,
            error: Some(format!("Failed to read body: {}", e)),
        })?;
        let response = Response::from_string(json)
            .with_status_code(400)
            .with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
            );
        return request.respond(response);
    }

    // Parse JSON body
    let req: AskRequest = match serde_json::from_str(&body) {
        Ok(r) => r,
        Err(e) => {
            let json = serde_json::to_string(&ApiResponse::<()> {
                ok: false,
                data: None,
                error: Some(format!("Invalid JSON: {}", e)),
            })?;
            let response = Response::from_string(json)
                .with_status_code(400)
                .with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                );
            return request.respond(response);
        }
    };

    // Build the prompt with context
    let prompt = build_claude_prompt(&req);

    // Execute claude -p command, piping prompt via stdin for reliability
    let mut child = match Command::new("claude")
        .arg("-p")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            let error = if e.kind() == std::io::ErrorKind::NotFound {
                "Claude CLI not found. Install with: npm install -g @anthropic-ai/claude-code"
                    .to_string()
            } else {
                format!("Failed to spawn claude: {}", e)
            };
            let json = serde_json::to_string(&ApiResponse::<AskResponse> {
                ok: false,
                data: None,
                error: Some(error),
            })?;
            let response = Response::from_string(json)
                .with_status_code(500)
                .with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                );
            return request.respond(response);
        }
    };

    // Write prompt to stdin
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(prompt.as_bytes());
    }

    // Wait for output
    let output = child.wait_with_output();

    let (json, status) = match output {
        Ok(output) => {
            if output.status.success() {
                let answer = String::from_utf8_lossy(&output.stdout).to_string();

                // Save Q&A interaction to database (best effort - don't fail if DB unavailable)
                if let Ok(db) = Database::open() {
                    let context_json = req
                        .context
                        .as_ref()
                        .and_then(|ctx| serde_json::to_string(ctx).ok());
                    if let Err(e) = db.save_qa_interaction(
                        &req.question,
                        &prompt,
                        &answer,
                        context_json.as_deref(),
                    ) {
                        eprintln!("Warning: Failed to save Q&A interaction: {}", e);
                    }
                }

                (
                    serde_json::to_string(&ApiResponse::success(AskResponse {
                        answer: answer.clone(),
                    }))?,
                    200,
                )
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let error = if stderr.is_empty() {
                    "Claude command failed with no error message".to_string()
                } else {
                    stderr
                };
                (
                    serde_json::to_string(&ApiResponse::<AskResponse> {
                        ok: false,
                        data: None,
                        error: Some(error),
                    })?,
                    500,
                )
            }
        }
        Err(e) => {
            let error = if e.kind() == std::io::ErrorKind::NotFound {
                "Claude CLI not found. Install with: npm install -g @anthropic-ai/claude-code"
                    .to_string()
            } else {
                format!("Failed to execute claude: {}", e)
            };
            (
                serde_json::to_string(&ApiResponse::<AskResponse> {
                    ok: false,
                    data: None,
                    error: Some(error),
                })?,
                500,
            )
        }
    };

    let response = Response::from_string(json)
        .with_status_code(status)
        .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
    request.respond(response)
}

fn build_claude_prompt(req: &AskRequest) -> String {
    let mut prompt = String::new();

    // Check if this is an archaeology query (has narrative context)
    let is_archaeology = req
        .context
        .as_ref()
        .is_some_and(|ctx| ctx.narrative.is_some());

    if is_archaeology {
        // Archaeology-focused system prompt
        prompt.push_str(r#"You are an expert in this codebase. You have meticulously crafted a narrative graph that lets someone understand and explore the entire decision history you have built up in order to understand the *why* of certain pieces of code. This was built using your archaeology tool.

The narrative graph captures:
- **Goals**: What we set out to accomplish
- **Decisions**: Choices made along the way
- **Actions**: Implementation steps taken
- **Observations**: What we learned during the process
- **Pivots**: Where we changed approach based on new information
- **Outcomes**: Results of our decisions

When answering questions:
1. Ground your answers in the specific nodes and decisions from the narrative
2. Explain the *why* behind decisions, not just the *what*
3. Highlight pivots and course corrections - these often contain the most valuable insights
4. Reference GitHub artifacts (commits, PRs, issues) when relevant
5. If the narrative doesn't contain enough information, say so explicitly

Format your response in Markdown.

---

"#);
    } else {
        // Generic decision graph system prompt
        prompt.push_str(r#"You are to use deciduous to answer questions about the codebase and decision graph.

You can use your skills/tools/commands to query the graph with the various deciduous helpers.

You can also use SQLite directly to query the graph database at .deciduous/deciduous.db to answer questions and traverse relationships.

The schema has these tables:
- decision_nodes (id, node_type, title, description, status, created_at, updated_at, metadata_json, change_id)
  - metadata_json may contain: branch, commit, files, confidence, prompt
- decision_edges (id, from_node_id, to_node_id, edge_type, weight, rationale, created_at, from_change_id, to_change_id)

Example queries:
- List all nodes: SELECT id, title, node_type, status FROM decision_nodes;
- Find commits: SELECT id, title, json_extract(metadata_json, '$.commit') as commit FROM decision_nodes WHERE metadata_json LIKE '%commit%';
- Get edges for a node: SELECT e.*, n.title FROM decision_edges e JOIN decision_nodes n ON e.to_node_id = n.id WHERE e.from_node_id = ?;

Make sure to be detailed in your response, and format it in markdown.

IMPORTANT: If information is missing or incomplete, tell the user explicitly and suggest a prompt they could give Claude to fill in that information using public sources or codebase exploration.

---

"#);
    }

    // Add context if provided
    if let Some(ctx) = &req.context {
        prompt.push_str("Context from deciduous decision graph:\n\n");

        // Add selected node context
        if let Some(node_id) = ctx.selected_node_id {
            if let Ok(db) = Database::open() {
                if let Ok(Some(node)) = db.get_node(node_id) {
                    prompt.push_str(&format!(
                        "Currently viewing node #{}: \"{}\" ({})\n",
                        node.id, node.title, node.node_type
                    ));
                    if let Some(desc) = &node.description {
                        prompt.push_str(&format!("Description: {}\n", desc));
                    }
                    // Parse metadata for additional context
                    if let Some(meta_str) = &node.metadata_json {
                        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(meta_str) {
                            if let Some(branch) = meta.get("branch").and_then(|v| v.as_str()) {
                                prompt.push_str(&format!("Branch: {}\n", branch));
                            }
                            if let Some(prompt_text) = meta.get("prompt").and_then(|v| v.as_str()) {
                                prompt.push_str(&format!("Original prompt: {}\n", prompt_text));
                            }
                        }
                    }
                    prompt.push('\n');
                }
            }
        }

        // Add visible nodes count
        if let Some(visible_ids) = &ctx.visible_node_ids {
            prompt.push_str(&format!(
                "User is viewing {} nodes in the graph.\n\n",
                visible_ids.len()
            ));
        }

        // Add branch context
        if let Some(branch) = &ctx.current_branch {
            prompt.push_str(&format!("Current git branch: {}\n\n", branch));
        }

        // Add narrative context from archaeology view
        if let Some(narrative) = &ctx.narrative {
            prompt.push_str("## Narrative Context (Archaeology View)\n\n");
            prompt.push_str(&format!("**Narrative:** {}\n", narrative.name));
            prompt.push_str(&format!(
                "**Scope:** {} nodes, starting from node #{}\n\n",
                narrative.node_ids.len(),
                narrative.root_id
            ));

            // Load and display nodes in the narrative
            if let Ok(db) = Database::open() {
                prompt.push_str("### Nodes in this narrative:\n\n");
                for node_id in &narrative.node_ids {
                    if let Ok(Some(node)) = db.get_node(*node_id) {
                        let status_marker = match node.status.as_str() {
                            "superseded" => " [SUPERSEDED]",
                            "abandoned" => " [ABANDONED]",
                            _ => "",
                        };
                        prompt.push_str(&format!(
                            "- **#{}** ({}{}) {}\n",
                            node.id, node.node_type, status_marker, node.title
                        ));
                        if let Some(desc) = &node.description {
                            if !desc.is_empty() {
                                prompt.push_str(&format!("  - {}\n", desc));
                            }
                        }
                    }
                }
                prompt.push('\n');
            }

            // Add pivots - where the approach changed
            if !narrative.pivots.is_empty() {
                prompt.push_str("### Pivots (approach changes):\n\n");
                for (i, pivot) in narrative.pivots.iter().enumerate() {
                    prompt.push_str(&format!(
                        "**Pivot {}:** Revisit node #{}\n",
                        i + 1,
                        pivot.revisit_id
                    ));
                    if !pivot.observation_ids.is_empty() {
                        prompt.push_str(&format!(
                            "- Triggered by observations: {}\n",
                            pivot
                                .observation_ids
                                .iter()
                                .map(|id| format!("#{}", id))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                    }
                    if !pivot.superseded_ids.is_empty() {
                        prompt.push_str(&format!(
                            "- Superseded nodes: {}\n",
                            pivot
                                .superseded_ids
                                .iter()
                                .map(|id| format!("#{}", id))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                    }
                    if !pivot.new_approach_ids.is_empty() {
                        prompt.push_str(&format!(
                            "- New approach nodes: {}\n",
                            pivot
                                .new_approach_ids
                                .iter()
                                .map(|id| format!("#{}", id))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                    }
                    prompt.push('\n');
                }
            }

            // Add GitHub links
            if !narrative.github_links.is_empty() {
                prompt.push_str("### GitHub Artifacts:\n\n");
                for link in &narrative.github_links {
                    let link_desc = match link.link_type.as_str() {
                        "commit" => format!(
                            "Commit {}",
                            &link.identifier[..7.min(link.identifier.len())]
                        ),
                        "pr" => format!("PR #{}", link.identifier),
                        "issue" => format!("Issue #{}", link.identifier),
                        _ => format!("{} {}", link.link_type, link.identifier),
                    };
                    prompt.push_str(&format!("- {} ({})\n", link_desc, link.repo));
                }
                prompt.push('\n');
            }

            prompt.push_str("---\n\n");
        }
    }

    // Add the user's question
    prompt.push_str("User question: ");
    prompt.push_str(&req.question);

    prompt
}

// === Toggle Checkbox Types ===

fn handle_toggle_checkbox(mut request: Request) -> std::io::Result<()> {
    // Read request body
    let mut body = String::new();
    if let Err(e) = request.as_reader().read_to_string(&mut body) {
        let json = serde_json::to_string(&ApiResponse::<()> {
            ok: false,
            data: None,
            error: Some(format!("Failed to read body: {}", e)),
        })?;
        let response = Response::from_string(json)
            .with_status_code(400)
            .with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
            );
        return request.respond(response);
    }

    // Parse JSON body
    let req: ToggleCheckboxRequest = match serde_json::from_str(&body) {
        Ok(r) => r,
        Err(e) => {
            let json = serde_json::to_string(&ApiResponse::<()> {
                ok: false,
                data: None,
                error: Some(format!("Invalid JSON: {}", e)),
            })?;
            let response = Response::from_string(json)
                .with_status_code(400)
                .with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                );
            return request.respond(response);
        }
    };

    // Update database
    let result = match Database::open() {
        Ok(db) => db.update_roadmap_item_checkbox(req.item_id, &req.checkbox_state),
        Err(e) => Err(e),
    };

    let (json, status) = match result {
        Ok(()) => (serde_json::to_string(&ApiResponse::success(true))?, 200),
        Err(e) => (
            serde_json::to_string(&ApiResponse::<bool> {
                ok: false,
                data: None,
                error: Some(format!("Database error: {}", e)),
            })?,
            500,
        ),
    };

    let response = Response::from_string(json)
        .with_status_code(status)
        .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
    request.respond(response)
}

// === Q&A API Handlers ===

/// Parse query parameters from URL (e.g., "?q=test&limit=20")
fn parse_query_params(url: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    if let Some(query_start) = url.find('?') {
        let query = &url[query_start + 1..];
        for pair in query.split('&') {
            if let Some(eq_pos) = pair.find('=') {
                let key = &pair[..eq_pos];
                let value = &pair[eq_pos + 1..];
                // URL decode the value (basic: just handle %20 for spaces)
                let decoded = value.replace("%20", " ").replace("+", " ");
                params.insert(key.to_string(), decoded);
            }
        }
    }
    params
}

/// Response wrapper for paginated Q&A list
#[derive(Serialize)]
struct QaListResponse {
    items: Vec<QaInteraction>,
    total: i64,
}

fn handle_qa_search(request: Request, url: &str) -> std::io::Result<()> {
    let params = parse_query_params(url);
    let query = params.get("q").map(|s| s.as_str()).unwrap_or("");
    let limit: i32 = params
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);

    if query.is_empty() {
        let json = serde_json::to_string(&ApiResponse::<Vec<QaSearchResult>> {
            ok: false,
            data: None,
            error: Some("Missing search query parameter 'q'".to_string()),
        })?;
        let response = Response::from_string(json)
            .with_status_code(400)
            .with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
            );
        return request.respond(response);
    }

    let results = match Database::open() {
        Ok(db) => db.search_qa_interactions(query, limit).unwrap_or_default(),
        Err(_) => vec![],
    };

    let json = serde_json::to_string(&ApiResponse::success(results))?;
    let response = Response::from_string(json)
        .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
    request.respond(response)
}

fn handle_qa_list(request: Request, url: &str) -> std::io::Result<()> {
    let params = parse_query_params(url);
    let offset: i64 = params
        .get("offset")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let limit: i64 = params
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);

    let (items, total) = match Database::open() {
        Ok(db) => {
            let items = db
                .get_qa_interactions_paginated(offset, limit)
                .unwrap_or_default();
            let total = db.count_qa_interactions().unwrap_or(0);
            (items, total)
        }
        Err(_) => (vec![], 0),
    };

    let json = serde_json::to_string(&ApiResponse::success(QaListResponse { items, total }))?;
    let response = Response::from_string(json)
        .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
    request.respond(response)
}

fn handle_qa_get(request: Request, path: &str) -> std::io::Result<()> {
    // Extract ID from path: /api/qa/123 -> 123
    let id: i32 = match path.strip_prefix("/api/qa/").and_then(|s| s.parse().ok()) {
        Some(id) => id,
        None => {
            let json = serde_json::to_string(&ApiResponse::<QaInteraction> {
                ok: false,
                data: None,
                error: Some("Invalid Q&A ID".to_string()),
            })?;
            let response = Response::from_string(json)
                .with_status_code(400)
                .with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                );
            return request.respond(response);
        }
    };

    let result = match Database::open() {
        Ok(db) => db.get_qa_interaction(id),
        Err(e) => Err(e),
    };

    match result {
        Ok(Some(interaction)) => {
            let json = serde_json::to_string(&ApiResponse::success(interaction))?;
            let response = Response::from_string(json).with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
            );
            request.respond(response)
        }
        Ok(None) => {
            let json = serde_json::to_string(&ApiResponse::<QaInteraction> {
                ok: false,
                data: None,
                error: Some("Q&A interaction not found".to_string()),
            })?;
            let response = Response::from_string(json)
                .with_status_code(404)
                .with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                );
            request.respond(response)
        }
        Err(e) => {
            let json = serde_json::to_string(&ApiResponse::<QaInteraction> {
                ok: false,
                data: None,
                error: Some(format!("Database error: {}", e)),
            })?;
            let response = Response::from_string(json)
                .with_status_code(500)
                .with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                );
            request.respond(response)
        }
    }
}

fn handle_qa_delete(request: Request, path: &str) -> std::io::Result<()> {
    // Extract ID from path: /api/qa/123 -> 123
    let id: i32 = match path.strip_prefix("/api/qa/").and_then(|s| s.parse().ok()) {
        Some(id) => id,
        None => {
            let json = serde_json::to_string(&ApiResponse::<bool> {
                ok: false,
                data: None,
                error: Some("Invalid Q&A ID".to_string()),
            })?;
            let response = Response::from_string(json)
                .with_status_code(400)
                .with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                );
            return request.respond(response);
        }
    };

    let result = match Database::open() {
        Ok(db) => db.soft_delete_qa_interaction(id),
        Err(e) => Err(e),
    };

    let (json, status) = match result {
        Ok(()) => (serde_json::to_string(&ApiResponse::success(true))?, 200),
        Err(e) => (
            serde_json::to_string(&ApiResponse::<bool> {
                ok: false,
                data: None,
                error: Some(format!("Database error: {}", e)),
            })?,
            500,
        ),
    };

    let response = Response::from_string(json)
        .with_status_code(status)
        .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
    request.respond(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    // === ApiResponse Tests ===

    #[test]
    fn test_api_response_success() {
        let response: ApiResponse<String> = ApiResponse::success("hello".to_string());
        assert!(response.ok);
        assert_eq!(response.data, Some("hello".to_string()));
        assert!(response.error.is_none());
    }

    #[test]
    fn test_api_response_success_with_vec() {
        let data = vec![1, 2, 3];
        let response: ApiResponse<Vec<i32>> = ApiResponse::success(data.clone());
        assert!(response.ok);
        assert_eq!(response.data, Some(data));
    }

    #[test]
    fn test_api_response_serializes_to_json() {
        let response: ApiResponse<String> = ApiResponse::success("test".to_string());
        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains("\"ok\":true"));
        assert!(json.contains("\"data\":\"test\""));
        assert!(json.contains("\"error\":null"));
    }

    #[test]
    fn test_api_response_with_complex_data() {
        #[derive(Serialize, PartialEq, Debug)]
        struct TestData {
            name: String,
            count: u32,
        }

        let data = TestData {
            name: "test".to_string(),
            count: 42,
        };
        let response = ApiResponse::success(data);

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"name\":\"test\""));
        assert!(json.contains("\"count\":42"));
    }

    // === Graph Viewer HTML Tests ===

    #[test]
    fn test_viewer_html_is_valid() {
        // The embedded viewer should be valid HTML
        assert!(
            GRAPH_VIEWER_HTML.contains("<!DOCTYPE html>") || GRAPH_VIEWER_HTML.contains("<html")
        );
        assert!(GRAPH_VIEWER_HTML.contains("</html>"));
    }

    #[test]
    fn test_viewer_html_has_react() {
        // The embedded viewer should have React components
        assert!(
            GRAPH_VIEWER_HTML.contains("React") || GRAPH_VIEWER_HTML.contains("react"),
            "Viewer should include React"
        );
    }
}
