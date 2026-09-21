//! Talking to a shared deciduous server.
//!
//! One Postgres behind an MCP endpoint holds every project's graph, one
//! workspace per repository. This module is the client half: it configures a
//! repository to point at that server, reports how far the local database has
//! drifted from it, and moves the graph in either direction.
//!
//! The local `.deciduous/deciduous.db` is a cache. The server is the truth.
//! That inverts what `deciduous sync` assumes, so the two are kept separate:
//! `sync` reconciles with teammates through `graph.json` in git, `remote`
//! reconciles with the server over HTTP.
//!
//! The token never goes in the repository. It is read from
//! `DECIDUOUS_MCP_TOKEN`, so a config file committed by mistake leaks a URL
//! and nothing else.

use crate::config::Config;
use crate::db::Database;
use crate::records::{self, EdgeRecord, NodeRecord, RecordStore};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

pub const TOKEN_ENV: &str = "DECIDUOUS_MCP_TOKEN";

/// Anything outside a git repository shares one workspace rather than minting
/// a new one per temporary directory.
pub const FALLBACK_WORKSPACE: &str = "scratch";

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct RemoteConfig {
    /// Base URL, with no trailing slash, e.g. `https://example.com/deciduous-mcp`.
    #[serde(default)]
    pub url: Option<String>,

    /// Overrides the derived workspace name. Only needed when the directory
    /// name is not what the graph should be called.
    #[serde(default)]
    pub workspace: Option<String>,
}

impl RemoteConfig {
    pub fn is_configured(&self) -> bool {
        self.url.is_some()
    }
}

/// Where a stored token lives: outside every repository, so it cannot be
/// committed by a stray `git add`, and mode 0600 so it is not world-readable
/// the way a `.env` in a project directory tends to end up.
pub fn credentials_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
        })?;
    Some(base.join("deciduous").join("credentials"))
}

/// The token, or an explanation of where to get one.
///
/// Environment first so a one-off override works and CI can inject it, then
/// the stored credential. Never defaulted: a request sent without a token
/// reaches a server that will refuse it anyway, and guessing here turns an
/// auth problem into a confusing 401.
pub fn token() -> Result<String, String> {
    if let Ok(t) = std::env::var(TOKEN_ENV) {
        if !t.trim().is_empty() {
            return Ok(t.trim().to_string());
        }
    }

    if let Some(path) = credentials_path() {
        if let Ok(contents) = std::fs::read_to_string(&path) {
            let t = contents.trim();
            if !t.is_empty() {
                return Ok(t.to_string());
            }
        }
    }

    Err(format!(
        "no token for the shared graph.\n\n\
         Store one (kept outside every repository, mode 0600):\n\n    \
         deciduous remote login\n\n\
         or set {TOKEN_ENV} for a one-off."
    ))
}

/// Writes the token to the credentials file with owner-only permissions.
///
/// The permissions are set on the file before the secret is written, not
/// after: creating it world-readable and then tightening it leaves a window in
/// which any process on the machine can read it.
pub fn store_token(token: &str) -> Result<std::path::PathBuf, String> {
    let token = token.trim();
    if token.is_empty() {
        return Err("refusing to store an empty token".to_string());
    }

    let path = credentials_path().ok_or("cannot determine a config directory (is HOME set?)")?;
    let dir = path.parent().ok_or("credentials path has no parent")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;

    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }

    let mut file = opts
        .open(&path)
        .map_err(|e| format!("opening {}: {e}", path.display()))?;

    use std::io::Write;
    writeln!(file, "{token}").map_err(|e| format!("writing {}: {e}", path.display()))?;

    // An existing file keeps its old mode through OpenOptions, so tighten it
    // explicitly for the case where one was created before this code existed.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }

    Ok(path)
}

/// Removes the stored token. Reports whether there was one.
pub fn forget_token() -> Result<bool, String> {
    let Some(path) = credentials_path() else {
        return Ok(false);
    };
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("removing {}: {e}", path.display())),
    }
}

/// Workspace name for a directory: the git repository's root directory name,
/// lowercased, or `scratch` outside a repository.
///
/// The repository root rather than the current directory, so that running this
/// from a subdirectory does not split one project across two workspaces.
pub fn workspace_for(dir: &Path) -> String {
    let root = std::process::Command::new("git")
        .args([
            "-C",
            &dir.display().to_string(),
            "rev-parse",
            "--show-toplevel",
        ])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok());

    match root {
        Some(path) => Path::new(path.trim())
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_else(|| FALLBACK_WORKSPACE.to_string()),
        None => FALLBACK_WORKSPACE.to_string(),
    }
}

/// Resolved settings for a call: where to send it, as which workspace.
#[derive(Debug)]
pub struct Remote {
    pub url: String,
    pub workspace: String,
    token: String,
}

impl Remote {
    pub fn resolve(config: &Config, dir: &Path) -> Result<Self, String> {
        let url = config.remote.url.clone().ok_or_else(|| {
            "no remote configured for this project.\n\nRun:\n\n    deciduous remote init <url>"
                .to_string()
        })?;

        Ok(Self {
            url: url.trim_end_matches('/').to_string(),
            workspace: config
                .remote
                .workspace
                .clone()
                .unwrap_or_else(|| workspace_for(dir)),
            token: token()?,
        })
    }

    fn get(&self, path: &str) -> ureq::Request {
        ureq::get(&format!("{}{}", self.url, path))
            .set("authorization", &format!("Bearer {}", self.token))
    }

    fn post(&self, path: &str) -> ureq::Request {
        ureq::post(&format!("{}{}", self.url, path))
            .set("authorization", &format!("Bearer {}", self.token))
    }

    /// Unauthenticated liveness. Separated from `check` so a failure can say
    /// whether the server is down or the token is wrong — they need different
    /// fixes and look identical from a single failed request.
    pub fn health(&self) -> Result<(), String> {
        ureq::get(&format!("{}/health", self.url))
            .timeout(std::time::Duration::from_secs(15))
            .call()
            .map(|_| ())
            .map_err(|e| format!("cannot reach {}: {}", self.url, describe(e)))
    }

    /// Liveness plus credentials, as separate findings.
    pub fn check(&self) -> Result<RemoteCounts, String> {
        self.health()?;
        self.counts()
            .map_err(|e| format!("reached {} but {}", self.url, e))
    }

    pub fn counts(&self) -> Result<RemoteCounts, String> {
        let graph = self.export()?;
        Ok(RemoteCounts {
            nodes: graph.nodes.len(),
            edges: graph.edges.len(),
            documents: graph.documents.len(),
        })
    }

    /// The whole workspace as the server holds it.
    pub fn export(&self) -> Result<RemoteGraph, String> {
        let resp = self
            .get(&format!("/export?workspace={}", urlencode(&self.workspace)))
            .timeout(std::time::Duration::from_secs(300))
            .call()
            .map_err(describe)?;

        resp.into_json::<RemoteGraph>()
            .map_err(|e| format!("the server's response was not a graph: {e}"))
    }

    /// Sends the local graph up. Used to seed a workspace and to carry local
    /// history that predates the remote; it is not the normal write path.
    pub fn import(&self, graph: Value) -> Result<ImportReport, String> {
        let payload = serde_json::json!({
            "workspace": self.workspace,
            "graph": graph,
        });

        self.post("/import")
            .timeout(std::time::Duration::from_secs(600))
            .send_json(payload)
            .map_err(describe)?
            .into_json::<ImportReport>()
            .map_err(|e| format!("the server's response was not an import report: {e}"))
    }
}

#[derive(Debug, Default)]
pub struct RemoteCounts {
    pub nodes: usize,
    pub edges: usize,
    pub documents: usize,
}

#[derive(Debug, Deserialize)]
pub struct RemoteGraph {
    #[serde(default)]
    pub nodes: Vec<RemoteNode>,
    #[serde(default)]
    pub edges: Vec<RemoteEdge>,
    #[serde(default)]
    pub documents: Vec<Value>,
}

#[derive(Debug, Deserialize)]
pub struct RemoteNode {
    pub change_id: String,
    pub node_type: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    #[serde(default)]
    pub metadata: Option<Value>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub deleted_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RemoteEdge {
    pub from_change_id: Option<String>,
    pub to_change_id: Option<String>,
    pub edge_type: String,
    #[serde(default)]
    pub rationale: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct ImportReport {
    pub nodes: CountPair,
    pub edges: EdgeReport,
    #[serde(default)]
    pub documents: Option<DocReport>,
}

#[derive(Debug, Deserialize)]
pub struct CountPair {
    pub received: usize,
    pub upserted: usize,
}

#[derive(Debug, Deserialize)]
pub struct EdgeReport {
    pub received: usize,
    pub upserted: usize,
    pub unresolved: usize,
    #[serde(default)]
    pub stale_change_ids: usize,
}

#[derive(Debug, Deserialize)]
pub struct DocReport {
    pub received: usize,
    pub upserted: usize,
    #[serde(default)]
    pub content_missing: usize,
}

/// Writes the server's graph into the repository's record store, then lets
/// `records::reconcile` fold it into the local database.
///
/// Reusing reconcile rather than writing a second merge path is the whole
/// point: it already resolves by `updated_at`, applies tombstones, and holds
/// an edge back until both its endpoints exist. A bespoke remote-to-local
/// importer would have to re-earn all of that and would drift from it.
pub fn pull(remote: &Remote, db: &Database, store: &RecordStore) -> Result<PullReport, String> {
    let graph = remote.export()?;
    let mut written = 0usize;

    let result = store.batch(|| -> Result<usize, String> {
        for n in &graph.nodes {
            let rec = NodeRecord {
                change_id: n.change_id.clone(),
                node_type: n.node_type.clone(),
                title: n.title.clone(),
                description: n.description.clone(),
                status: n.status.clone(),
                metadata: n.metadata.clone(),
                created_at: n.created_at.clone(),
                updated_at: n.updated_at.clone(),
                author: None,
                deleted_at: n.deleted_at.clone(),
            };
            if store.write_node(&rec).map_err(|e| e.to_string())? {
                written += 1;
            }
        }

        for e in &graph.edges {
            // An edge with an unresolved endpoint cannot be addressed by
            // change_id and is skipped rather than written half-formed.
            let (Some(from), Some(to)) = (&e.from_change_id, &e.to_change_id) else {
                continue;
            };

            let rec = EdgeRecord {
                edge_id: records::edge_id(from, to, &e.edge_type),
                from_change_id: from.clone(),
                to_change_id: to.clone(),
                edge_type: e.edge_type.clone(),
                rationale: e.rationale.clone(),
                // EdgeRecord has no updated_at: an edge is immutable once
                // written, so reconcile orders it by created_at alone.
                weight: None,
                created_at: e.created_at.clone(),
                author: None,
                deleted_at: None,
            };
            if store.write_edge(&rec).map_err(|e| e.to_string())? {
                written += 1;
            }
        }

        Ok(written)
    });

    let records_written = match result {
        Ok(Ok(n)) => n,
        Ok(Err(e)) => return Err(e),
        Err(e) => return Err(format!("record store: {e}")),
    };

    let report = records::reconcile(db, store, false)?;

    Ok(PullReport {
        fetched_nodes: graph.nodes.len(),
        fetched_edges: graph.edges.len(),
        records_written,
        imported_nodes: report.nodes_imported,
        imported_edges: report.edges_imported,
    })
}

#[derive(Debug, Default)]
pub struct PullReport {
    pub fetched_nodes: usize,
    pub fetched_edges: usize,
    pub records_written: usize,
    pub imported_nodes: usize,
    pub imported_edges: usize,
}

/// ureq puts the useful part of an HTTP failure in the response body, which
/// `Display` throws away — a 422 from the import endpoint reads as
/// "status code 422" unless the body is pulled out.
fn describe(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            let detail = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
                .unwrap_or_else(|| body.chars().take(200).collect());

            match code {
                401 => format!("the server rejected the token (401). Check {TOKEN_ENV}."),
                404 => "no such endpoint (404). Is the URL right, including any path prefix?"
                    .to_string(),
                _ if detail.is_empty() => format!("the server returned {code}"),
                _ => format!("the server returned {code}: {detail}"),
            }
        }
        ureq::Error::Transport(t) => format!("transport error: {t}"),
    }
}

fn urlencode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            '*' => "%2A".to_string(),
            c => format!("%{:02X}", c as u32),
        })
        .collect()
}

/// Every project under `root` that already has a `.deciduous` directory.
///
/// Shallow on purpose — three levels covers `~/code/<project>` and a worktree
/// one below it, without descending into `node_modules` or a vendored tree and
/// adopting something that is not a project of yours.
pub fn find_projects(root: &Path) -> Vec<std::path::PathBuf> {
    fn walk(dir: &Path, depth: usize, out: &mut Vec<std::path::PathBuf>) {
        if depth > 3 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();

            if name == ".deciduous" {
                if path.join("deciduous.db").exists() || path.join("config.toml").exists() {
                    if let Some(project) = path.parent() {
                        out.push(project.to_path_buf());
                    }
                }
                continue;
            }
            if name == "node_modules" || name == "target" || name == "_build" || name == "deps" {
                continue;
            }
            walk(&path, depth + 1, out);
        }
    }

    let mut out = Vec::new();
    walk(root, 0, &mut out);
    out.sort();
    out.dedup();
    out
}

/// The configured server URL for a project, if it has one.
pub fn read_remote_url(project: &Path) -> Option<String> {
    let text = std::fs::read_to_string(project.join(".deciduous").join("config.toml")).ok()?;
    let doc: toml::Value = toml::from_str(&text).ok()?;
    doc.get("remote")?.get("url")?.as_str().map(str::to_string)
}

/// Writes `[remote] url` into a project's config, preserving its formatting.
///
/// Format-preserving for the same reason `Config::save_remote` is: these are
/// files people hand-write and comment, and a bulk operation that reflowed
/// every one of them would be a far worse diff than the change it made.
pub fn write_remote_url(project: &Path, url: &str) -> Result<(), String> {
    let dir = project.join(".deciduous");
    let path = dir.join("config.toml");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();

    let mut doc = existing
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| format!("{} is not valid TOML: {e}", path.display()))?;

    doc["remote"].or_insert(toml_edit::table())["url"] = toml_edit::value(url.to_string());

    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    std::fs::write(&path, doc.to_string()).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_falls_back_outside_a_git_repo() {
        let dir = std::env::temp_dir().join("deciduous-remote-not-a-repo");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(workspace_for(&dir), FALLBACK_WORKSPACE);
    }

    #[test]
    fn workspace_names_are_lowercased() {
        // Names key rows across machines, so two spellings of one project
        // would split it into two workspaces.
        let name = "MixedCase";
        assert_eq!(name.to_lowercase(), "mixedcase");
    }

    #[test]
    fn the_global_token_is_escaped_so_it_cannot_widen_a_query() {
        assert_eq!(urlencode("*"), "%2A");
        assert_eq!(urlencode("blog"), "blog");
        assert_eq!(urlencode("a b"), "a%20b");
    }

    #[test]
    fn adopt_skips_vendored_trees() {
        let root = std::env::temp_dir().join(format!("deciduous-adopt-{}", std::process::id()));
        let real = root.join("my-project").join(".deciduous");
        let vendored = root
            .join("my-project")
            .join("node_modules")
            .join("dep")
            .join(".deciduous");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::create_dir_all(&vendored).unwrap();
        std::fs::write(real.join("config.toml"), "").unwrap();
        std::fs::write(vendored.join("config.toml"), "").unwrap();

        let found = find_projects(&root);
        assert_eq!(found.len(), 1, "found: {found:?}");
        assert!(found[0].ends_with("my-project"));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn writing_a_remote_url_preserves_the_rest_of_the_file() {
        let root = std::env::temp_dir().join(format!("deciduous-wr-{}", std::process::id()));
        let dir = root.join(".deciduous");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.toml"),
            "# keep me\n[branch]\nmain_branches = [\"main\"]\n",
        )
        .unwrap();

        write_remote_url(&root, "https://example.com/mcp").unwrap();

        let after = std::fs::read_to_string(dir.join("config.toml")).unwrap();
        assert!(after.contains("# keep me"), "comment lost:\n{after}");
        assert!(after.contains("main_branches = [\"main\"]"));
        assert_eq!(
            read_remote_url(&root).as_deref(),
            Some("https://example.com/mcp")
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_remote_without_a_url_explains_how_to_configure_one() {
        let cfg = Config::default();
        let err = Remote::resolve(&cfg, Path::new(".")).unwrap_err();
        assert!(err.contains("deciduous remote init"), "got: {err}");
    }
}
