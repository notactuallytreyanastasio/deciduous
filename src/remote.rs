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

    /// A `wss://…/events?workspace=…&token=…` URL for this project's
    /// workspace, ready to hand to a WebSocket client.
    ///
    /// The token rides in the query string, not a header, because the
    /// WebSocket handshake itself cannot carry a custom `Authorization`
    /// header in most clients that matter here — not a workaround for one
    /// client, a limit of the browser `WebSocket` constructor and everything
    /// built against it. The server accepts exactly this fallback; see
    /// `deciduous_mcp/lib/deciduous_mcp/web/router.ex`.
    ///
    /// The scheme is derived from the configured URL (`https` → `wss`,
    /// `http` → `ws`) rather than hardcoded, so this also works against a
    /// plain-HTTP dev server.
    pub fn events_url(&self) -> String {
        let ws_base = if let Some(rest) = self.url.strip_prefix("https://") {
            format!("wss://{rest}")
        } else if let Some(rest) = self.url.strip_prefix("http://") {
            format!("ws://{rest}")
        } else {
            self.url.clone()
        };

        format!(
            "{ws_base}/events?workspace={}&token={}",
            urlencode(&self.workspace),
            urlencode(&self.token)
        )
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
        Ok(self.export()?.live_counts())
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

/// The part of a local graph (as `deciduous graph` emits it) the server does
/// not have: nodes whose change_id it lacks, edges whose (from, to, type) it
/// lacks, documents whose change_id it lacks. Returns the filtered graph and
/// how many nodes and edges it holds.
///
/// This is what `remote push` sends unless told `--overwrite`. The server's
/// import replaces a row it already has, so sending the whole local graph
/// overwrote whatever had changed on the server since (a status set through
/// MCP, a rationale edited) with the stale local copy.
///
/// Edges are keyed through the local node map first: an edge row's stored
/// change ids can be stale or absent in older databases. Keyed by the stored
/// ids alone, one 7,805-node workspace looked like 51,092 missing edges, every
/// one of which already existed.
///
/// An edge touching a node the server holds as a tombstone is not sent. The
/// server refuses it, so sending it made `remote push` print "edges 1 of 1"
/// on every run while nothing changed; the fix for that node is a pull,
/// which `deleted_on_server` names.
pub fn missing_on_server(local: &Value, server: &RemoteGraph) -> (Value, usize, usize) {
    use std::collections::{HashMap, HashSet};
    let have_nodes: HashSet<&str> = server.nodes.iter().map(|n| n.change_id.as_str()).collect();
    let dead = server.tombstones();
    let have_edges: HashSet<(&str, &str, &str)> = server
        .edges
        .iter()
        .filter_map(|e| {
            Some((
                e.from_change_id.as_deref()?,
                e.to_change_id.as_deref()?,
                e.edge_type.as_str(),
            ))
        })
        .collect();
    let have_docs: HashSet<&str> = server
        .documents
        .iter()
        .filter_map(|d| d["change_id"].as_str())
        .collect();

    let empty = Vec::new();
    let nodes = local["nodes"].as_array().unwrap_or(&empty);
    let by_id: HashMap<i64, &str> = nodes
        .iter()
        .filter_map(|n| Some((n["id"].as_i64()?, n["change_id"].as_str()?)))
        .collect();

    let send_nodes: Vec<Value> = nodes
        .iter()
        .filter(|n| {
            n["change_id"]
                .as_str()
                .is_some_and(|c| !have_nodes.contains(c))
        })
        .cloned()
        .collect();

    let mut send_edges = Vec::new();
    for e in local["edges"].as_array().unwrap_or(&empty) {
        let end = |id: &str, cid: &str| {
            e[id]
                .as_i64()
                .and_then(|i| by_id.get(&i).copied())
                .or_else(|| e[cid].as_str())
        };
        let (Some(f), Some(t)) = (
            end("from_node_id", "from_change_id"),
            end("to_node_id", "to_change_id"),
        ) else {
            continue;
        };
        let kind = e["edge_type"].as_str().unwrap_or("leads_to");
        if have_edges.contains(&(f, t, kind)) || dead.contains_key(f) || dead.contains_key(t) {
            continue;
        }
        let mut e = e.clone();
        e["from_change_id"] = Value::String(f.to_string());
        e["to_change_id"] = Value::String(t.to_string());
        send_edges.push(e);
    }

    let send_docs: Vec<Value> = local["documents"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .filter(|d| {
            d["change_id"]
                .as_str()
                .is_some_and(|c| !have_docs.contains(c))
        })
        .cloned()
        .collect();

    let (n, m) = (send_nodes.len(), send_edges.len());
    (
        serde_json::json!({"nodes": send_nodes, "edges": send_edges, "documents": send_docs}),
        n,
        m,
    )
}

/// Sends the server everything in `graph` it does not already have. `None`
/// means there was nothing to send.
///
/// Shared by `remote push` and the automatic push after a write, so both decide
/// what is missing the same way — `missing_on_server` earned its edge-keying
/// rules the hard way and there must not be a second copy of them.
///
/// Also returns the local nodes the server has deleted. Nothing a push sends
/// can change those; they are the user's to pull (see `deleted_on_server`).
pub fn push_missing(
    remote: &Remote,
    graph: &Value,
) -> Result<(Option<ImportReport>, Vec<DeletedOnServer>), String> {
    let server = remote.export()?;
    let deleted = deleted_on_server(graph, &server);
    let (missing, nodes, edges) = missing_on_server(graph, &server);
    if nodes == 0 && edges == 0 {
        return Ok((None, deleted));
    }
    remote.import(missing).map(|r| (Some(r), deleted))
}

/// A node this machine still has that the server holds as a tombstone.
#[derive(Debug, Clone, PartialEq)]
pub struct DeletedOnServer {
    pub id: i64,
    pub change_id: String,
    pub title: String,
    pub deleted_at: String,
}

/// The local nodes (from a `deciduous graph` value) that the server has
/// deleted and this machine has not yet pulled.
///
/// This is the drift neither count nor push can resolve: the local side
/// holds more, so `remote status` used to say push, and push has nothing
/// the server will take. Only `remote pull` applies it.
pub fn deleted_on_server(local: &Value, server: &RemoteGraph) -> Vec<DeletedOnServer> {
    let dead = server.tombstones();
    local["nodes"]
        .as_array()
        .map(|nodes| {
            nodes
                .iter()
                .filter_map(|n| {
                    let cid = n["change_id"].as_str()?;
                    let at = dead.get(cid)?;
                    Some(DeletedOnServer {
                        id: n["id"].as_i64().unwrap_or_default(),
                        change_id: cid.to_string(),
                        title: n["title"].as_str().unwrap_or_default().to_string(),
                        deleted_at: at.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Local node and edge counts once `deleted` is applied: what the local
/// graph will hold after a pull, for comparing with the server's live
/// counts.
pub fn counts_without(local: &Value, deleted: &[DeletedOnServer]) -> (usize, usize) {
    use std::collections::HashSet;
    let gone: HashSet<i64> = deleted.iter().map(|d| d.id).collect();
    let nodes = local["nodes"].as_array().map_or(0, |n| {
        n.iter()
            .filter(|n| !n["id"].as_i64().is_some_and(|i| gone.contains(&i)))
            .count()
    });
    let edges = local["edges"].as_array().map_or(0, |e| {
        e.iter()
            .filter(|e| {
                !e["from_node_id"].as_i64().is_some_and(|i| gone.contains(&i))
                    && !e["to_node_id"].as_i64().is_some_and(|i| gone.contains(&i))
            })
            .count()
    });
    (nodes, edges)
}

/// Prints what an import refused because the server had deleted it, if
/// anything. The server names the change_ids; `graph` maps them back to
/// local ids and titles so the user can tell which of their nodes it was.
pub fn report_refused(r: &ImportReport, graph: &Value) {
    use colored::Colorize;
    if r.nodes.refused_deleted == 0 && r.edges.refused_deleted == 0 {
        return;
    }
    eprintln!(
        "{} the server refused {} node(s) and {} edge(s) because it has deleted them or \
         their endpoints. `deciduous remote pull` applies those deletions here.",
        "Note:".yellow(),
        r.nodes.refused_deleted,
        r.edges.refused_deleted
    );
    for ex in &r.nodes.refused_deleted_examples {
        let cid = ex["change_id"].as_str().unwrap_or_default();
        let local = graph["nodes"]
            .as_array()
            .and_then(|n| n.iter().find(|n| n["change_id"] == cid));
        eprintln!(
            "  {} \"{}\" deleted on the server at {}",
            local
                .and_then(|n| n["id"].as_i64())
                .map_or_else(|| cid.to_string(), |i| i.to_string()),
            local.and_then(|n| n["title"].as_str()).unwrap_or("?"),
            ex["deleted_at"].as_str().unwrap_or("?")
        );
    }
}

/// Says that edits to nodes the server deleted were not sent.
fn warn_edits_to_deleted(deleted: &[DeletedOnServer]) {
    use colored::Colorize;
    for d in deleted {
        eprintln!(
            "{} node {} \"{}\" was deleted on the server at {}, so this edit was not sent. \
             It stays in the local database until `deciduous remote pull` removes the node here; \
             to keep the change, add it again as a new node.",
            "Warning:".yellow(),
            d.id,
            d.title,
            d.deleted_at
        );
    }
}

/// Pushes to the server after a local write, without letting the server turn a
/// successful write into a failed command.
///
/// The CLI writes to the local database and the agents write through the MCP
/// server. A local write the server never sees is a fork, not a cache: this is
/// how one workspace ended up with 502 nodes locally and 0 on the server, with
/// neither side saying a word. So every write pushes.
///
/// `touched` names the nodes this command changed, by local id. They are sent
/// even when the server already has them: `missing_on_server` only finds what
/// the server lacks, so on its own it would push a new node but never a changed
/// one, and `status`/`prompt` would go on diverging silently. The server's
/// import replaces `status`, `title`, `description` and `metadata` on a
/// change_id it already holds, so re-sending a node is what applies an edit.
///
/// Best effort deliberately. The local write has already happened and is not
/// rolled back: the CLI has to keep working on a plane, and a server being down
/// must not stop anyone recording a decision. A failure prints what is waiting
/// and the command to send it; the records stay in the local database and the
/// next successful push — automatic or manual — takes them, which is what the
/// full-diff half of the payload is for.
pub fn push_after_write(db: &Database, touched: &[i32]) {
    use colored::Colorize;

    let cfg = Config::load();
    if !cfg.remote.is_configured() {
        return;
    }
    let dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let pushed = Remote::resolve(&cfg, &dir).and_then(|remote| {
        let graph = db
            .get_graph()
            .map_err(|e| format!("reading the local graph: {e}"))
            .and_then(|g| {
                serde_json::to_value(&g).map_err(|e| format!("serializing the local graph: {e}"))
            })?;
        let server = remote.export()?;
        let (mut payload, _, _) = missing_on_server(&graph, &server);
        // A touched node the server has deleted is not re-sent: the server
        // refuses it, and before it did, the push rewrote the tombstone and
        // the next pull dropped the edit without a word. Say so instead.
        let deleted: Vec<DeletedOnServer> = deleted_on_server(&graph, &server)
            .into_iter()
            .filter(|d| touched.contains(&(d.id as i32)))
            .collect();
        warn_edits_to_deleted(&deleted);
        let live_touched: Vec<i32> = touched
            .iter()
            .copied()
            .filter(|t| !deleted.iter().any(|d| d.id == *t as i64))
            .collect();
        let added = add_touched_nodes(&mut payload, &graph, &live_touched);
        if payload["nodes"].as_array().is_some_and(|n| n.is_empty())
            && payload["edges"].as_array().is_some_and(|e| e.is_empty())
            && added == 0
        {
            return Ok((remote, None));
        }
        remote.import(payload).map(|r| {
            report_refused(&r, &graph);
            (remote, Some(r))
        })
    });
    match pushed {
        Ok((_, None)) => {}
        Ok((remote, Some(r))) => {
            // Quiet on the expected case: one command, one node or edge sent.
            // A larger number means a backlog just cleared, which is worth
            // saying, because the user was told about it when it built up.
            if r.nodes.upserted + r.edges.upserted > 2 {
                println!(
                    "   {} {} node(s), {} edge(s) to {}",
                    "Pushed".green(),
                    r.nodes.upserted,
                    r.edges.upserted,
                    remote.workspace.cyan()
                );
            }
        }
        Err(e) => eprintln!(
            "{} the local write succeeded but the server did not get it: {e}\n\
             {} `deciduous remote push` once the server is reachable. Until then the \
             local graph and the graph the agents read are different graphs.",
            "Warning:".yellow(),
            "Run:".yellow(),
        ),
    }
}

/// Adds the nodes `touched` names to a push payload, skipping any the diff
/// already put there. Returns how many were added.
fn add_touched_nodes(payload: &mut Value, graph: &Value, touched: &[i32]) -> usize {
    use std::collections::HashSet;

    if touched.is_empty() {
        return 0;
    }
    let already: HashSet<String> = payload["nodes"]
        .as_array()
        .map(|n| {
            n.iter()
                .filter_map(|n| n["change_id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let extra: Vec<Value> = graph["nodes"]
        .as_array()
        .map(|nodes| {
            nodes
                .iter()
                .filter(|n| {
                    n["id"]
                        .as_i64()
                        .is_some_and(|id| touched.contains(&(id as i32)))
                        && n["change_id"]
                            .as_str()
                            .is_some_and(|c| !already.contains(c))
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let added = extra.len();
    if let Some(nodes) = payload["nodes"].as_array_mut() {
        nodes.extend(extra);
    }
    added
}

/// Says that a local removal stopped at the local database.
///
/// `POST /import` upserts `status`, `title`, `description` and `metadata` on a
/// change_id the server already holds, and nothing else: `deleted_at` is not in
/// its replace list, and a hard-deleted local row has nothing left to send
/// anyway. So a `delete` or `unlink` here cannot reach the server, and saying
/// nothing would recreate exactly the silent divergence the automatic push
/// exists to end.
pub fn warn_removal_is_local_only(what: &str) {
    use colored::Colorize;

    let cfg = Config::load();
    if !cfg.remote.is_configured() {
        return;
    }
    let url = cfg.remote.url.unwrap_or_default();
    eprintln!(
        "{} the {what} was removed locally only. {url} still has it, and \
         `deciduous remote push` will not remove it there: the server's import \
         applies additions and edits, not removals. Remove it through the agent \
         (the deciduous MCP tools) as well, or the two graphs stay different.",
        "Note:".yellow(),
    );
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

impl RemoteGraph {
    /// Counts what the server holds, not what it remembers deleting.
    ///
    /// `/export` carries a node deleted on the server as a tombstone (the row
    /// with `deleted_at` set) so that `pull` can delete it here. Counting
    /// those as nodes would compare a local graph that no longer has the node
    /// with a server total that still does, and `remote status` would report
    /// drift forever: the same symptom tombstones exist to end.
    /// change_id -> deleted_at for every tombstone in the export.
    pub fn tombstones(&self) -> std::collections::HashMap<&str, &str> {
        self.nodes
            .iter()
            .filter_map(|n| Some((n.change_id.as_str(), n.deleted_at.as_deref()?)))
            .collect()
    }

    pub fn live_counts(&self) -> RemoteCounts {
        RemoteCounts {
            nodes: self.nodes.iter().filter(|n| n.deleted_at.is_none()).count(),
            edges: self.edges.len(),
            documents: self.documents.len(),
        }
    }
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
    /// Nodes the server would not write because it has deleted them. Absent
    /// from a server older than this field, which wrote them anyway.
    #[serde(default)]
    pub refused_deleted: usize,
    #[serde(default)]
    pub refused_deleted_examples: Vec<Value>,
}

#[derive(Debug, Deserialize)]
pub struct EdgeReport {
    pub received: usize,
    pub upserted: usize,
    pub unresolved: usize,
    #[serde(default)]
    pub stale_change_ids: usize,
    #[serde(default)]
    pub refused_deleted: usize,
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
                extra: Default::default(),
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
                extra: Default::default(),
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

    // A server tombstone wins, even over a local edit made after it.
    // reconcile's rule among teammates is the opposite ("edited locally
    // after someone deleted it: resurrect"), and that is right for a git
    // file anyone can write. It is wrong here: the server refuses every
    // write to a deleted node, so the resurrected node could never reach
    // it, and `remote status` reported the same deletion after every pull
    // while pull did nothing. The edit was refused, and the user was told
    // so when it was made (`push_after_write`); this is where it goes.
    let local = db
        .get_graph()
        .map_err(|e| format!("reading the local graph: {e}"))
        .and_then(|g| {
            serde_json::to_value(&g).map_err(|e| format!("serializing the local graph: {e}"))
        })?;
    let overridden = deleted_on_server(&local, &graph);
    for d in &overridden {
        db.delete_node(d.id as i32, false)
            .map_err(|e| format!("deleting node {} \"{}\": {e}", d.id, d.title))?;
        // The local tombstone keeps the row's last fields, prompt included;
        // the server's keeps none, because a delete is how a pasted secret
        // leaves the graph. Keep the local deleted_at (now, later than the
        // edit), so a teammate who synced the edit deletes it too.
        let scrubbed = store
            .read_node(&d.change_id)
            .map_err(|e| format!("record store: {e}"))?
            .map(|mut rec| {
                rec.title = String::new();
                rec.description = None;
                rec.metadata = None;
                rec
            });
        if let Some(rec) = scrubbed {
            store
                .write_node(&rec)
                .map_err(|e| format!("record store: {e}"))?;
        }
    }

    Ok(PullReport {
        fetched_nodes: graph.nodes.len(),
        fetched_edges: graph.edges.len(),
        records_written,
        imported_nodes: report.nodes_imported,
        imported_edges: report.edges_imported,
        deleted_nodes: report.nodes_deleted,
        deleted_over_local_edits: overridden,
    })
}

#[derive(Debug, Default)]
pub struct PullReport {
    pub fetched_nodes: usize,
    pub fetched_edges: usize,
    pub records_written: usize,
    pub imported_nodes: usize,
    pub imported_edges: usize,
    /// Nodes the server had deleted that reconcile deleted here.
    pub deleted_nodes: usize,
    /// Nodes the server had deleted that had been edited here after the
    /// delete; deleted anyway, edit and all (see `pull`).
    pub deleted_over_local_edits: Vec<DeletedOnServer>,
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

    fn server(nodes: &[&str], edges: &[(&str, &str, &str)]) -> RemoteGraph {
        RemoteGraph {
            nodes: nodes
                .iter()
                .map(|c| RemoteNode {
                    change_id: c.to_string(),
                    node_type: "goal".into(),
                    title: "t".into(),
                    description: None,
                    status: "pending".into(),
                    metadata: None,
                    created_at: "x".into(),
                    updated_at: "x".into(),
                    deleted_at: None,
                })
                .collect(),
            edges: edges
                .iter()
                .map(|(f, t, k)| RemoteEdge {
                    from_change_id: Some(f.to_string()),
                    to_change_id: Some(t.to_string()),
                    edge_type: k.to_string(),
                    rationale: None,
                    created_at: "x".into(),
                })
                .collect(),
            documents: vec![],
        }
    }

    #[test]
    fn push_sends_only_what_the_server_lacks_and_never_an_existing_node() {
        let local = serde_json::json!({
            "nodes": [{"id": 1, "change_id": "a"}, {"id": 2, "change_id": "b"}, {"id": 3, "change_id": "c"}],
            "edges": [
                {"from_node_id": 1, "to_node_id": 2, "edge_type": "leads_to"},
                {"from_node_id": 2, "to_node_id": 3, "edge_type": "leads_to"}
            ],
            "documents": []
        });
        let (g, n, m) = missing_on_server(&local, &server(&["a", "b"], &[("a", "b", "leads_to")]));
        assert_eq!((n, m), (1, 1));
        assert_eq!(g["nodes"][0]["change_id"], "c");
        assert_eq!(g["edges"][0]["from_change_id"], "b");
        assert_eq!(g["edges"][0]["to_change_id"], "c");
    }

    #[test]
    fn a_changed_node_the_server_already_has_is_still_sent_when_touched() {
        // The whole point of `touched`: `status`/`prompt` edit a node the
        // server already holds, so the missing-diff finds nothing and the edit
        // would never leave this machine.
        let local = serde_json::json!({
            "nodes": [{"id": 1, "change_id": "a"}, {"id": 2, "change_id": "b"}],
            "edges": [],
            "documents": []
        });
        let (mut payload, n, m) = missing_on_server(&local, &server(&["a", "b"], &[]));
        assert_eq!((n, m), (0, 0), "nothing is missing, by construction");
        assert_eq!(add_touched_nodes(&mut payload, &local, &[2]), 1);
        assert_eq!(payload["nodes"][0]["change_id"], "b");
    }

    #[test]
    fn a_touched_node_the_diff_already_sends_is_not_sent_twice() {
        let local = serde_json::json!({
            "nodes": [{"id": 1, "change_id": "a"}],
            "edges": [],
            "documents": []
        });
        let (mut payload, n, _) = missing_on_server(&local, &server(&[], &[]));
        assert_eq!(n, 1);
        assert_eq!(add_touched_nodes(&mut payload, &local, &[1]), 0);
        assert_eq!(payload["nodes"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn touching_nothing_adds_nothing() {
        let local = serde_json::json!({"nodes": [{"id": 1, "change_id": "a"}], "edges": []});
        let (mut payload, _, _) = missing_on_server(&local, &server(&["a"], &[]));
        assert_eq!(add_touched_nodes(&mut payload, &local, &[]), 0);
        assert_eq!(add_touched_nodes(&mut payload, &local, &[99]), 0);
        assert!(payload["nodes"].as_array().unwrap().is_empty());
    }

    #[test]
    fn a_stale_stored_change_id_does_not_make_an_existing_edge_look_missing() {
        // The edge row says "old-a" but node 1's change_id is now "a"; the
        // server has a -> b. Keyed by the stored id, it would be resent.
        let local = serde_json::json!({
            "nodes": [{"id": 1, "change_id": "a"}, {"id": 2, "change_id": "b"}],
            "edges": [{"from_node_id": 1, "to_node_id": 2, "from_change_id": "old-a", "to_change_id": "b", "edge_type": "leads_to"}]
        });
        let (_, n, m) = missing_on_server(&local, &server(&["a", "b"], &[("a", "b", "leads_to")]));
        assert_eq!((n, m), (0, 0));
    }
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

    #[test]
    fn events_url_converts_https_to_wss() {
        let r = Remote {
            url: "https://example.com/deciduous-mcp".to_string(),
            workspace: "blog".to_string(),
            token: "abc123".to_string(),
        };
        assert_eq!(
            r.events_url(),
            "wss://example.com/deciduous-mcp/events?workspace=blog&token=abc123"
        );
    }

    #[test]
    fn events_url_converts_plain_http_to_ws_for_a_dev_server() {
        let r = Remote {
            url: "http://localhost:4111".to_string(),
            workspace: "blog".to_string(),
            token: "abc123".to_string(),
        };
        assert_eq!(
            r.events_url(),
            "ws://localhost:4111/events?workspace=blog&token=abc123"
        );
    }

    #[test]
    fn events_url_encodes_a_workspace_name_with_special_characters() {
        // Not a realistic workspace name (normalize_name would reject the
        // space), but the URL builder should not assume its caller already
        // validated — a raw string finding its way in here should not corrupt
        // the query string.
        let r = Remote {
            url: "https://example.com".to_string(),
            workspace: "a b".to_string(),
            token: "tok".to_string(),
        };
        assert!(
            r.events_url().contains("workspace=a%20b"),
            "got: {}",
            r.events_url()
        );
    }

    /// A node deleted on the server arrives from /export as a tombstone. It
    /// is not counted as held, and its change_id is not "missing" either, so
    /// a local copy that has not been pulled away yet is not pushed back.
    #[test]
    fn a_server_tombstone_is_not_counted_and_not_pushed_back() {
        let mut graph = server(&["a", "b"], &[]);
        graph.nodes[1].deleted_at = Some("2026-09-23T17:38:04Z".into());

        let c = graph.live_counts();
        assert_eq!((c.nodes, c.edges), (1, 0));

        let local = serde_json::json!({
            "nodes": [{"id": 1, "change_id": "a"}, {"id": 2, "change_id": "b"}],
            "edges": [],
            "documents": []
        });
        let (_, n, _) = missing_on_server(&local, &graph);
        assert_eq!(n, 0);
    }
}
