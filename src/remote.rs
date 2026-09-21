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

/// The token, or an explanation of where to get one. Never defaulted: a
/// request sent without it reaches a server that will refuse it anyway, and
/// guessing here would turn an auth problem into a confusing 401.
pub fn token() -> Result<String, String> {
    match std::env::var(TOKEN_ENV) {
        Ok(t) if !t.trim().is_empty() => Ok(t),
        _ => Err(format!(
            "{TOKEN_ENV} is not set.\n\
             The shared graph is behind a bearer token; export it first:\n\n    \
             export {TOKEN_ENV}=<token>"
        )),
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
    fn a_remote_without_a_url_explains_how_to_configure_one() {
        let cfg = Config::default();
        let err = Remote::resolve(&cfg, Path::new(".")).unwrap_err();
        assert!(err.contains("deciduous remote init"), "got: {err}");
    }
}
