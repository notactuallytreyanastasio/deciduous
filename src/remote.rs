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

/// The main working tree of the repository `dir` is in, also when `dir` is
/// inside a linked worktree.
///
/// `git rev-parse --show-toplevel` answers with the worktree's own directory,
/// so 1.0.7 put `repo/` and `repo-feature/` (a `git worktree add` of it) in
/// two workspaces, while the server's instructions tell agents in that
/// worktree to use the main repository's name (C7). A linked worktree is the
/// case where `--git-dir` and `--git-common-dir` differ; its main working tree
/// is the first entry of `git worktree list`.
pub fn repo_root(dir: &Path) -> Option<std::path::PathBuf> {
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
    };
    let out = git(&[
        "rev-parse",
        "--path-format=absolute",
        "--show-toplevel",
        "--git-dir",
        "--git-common-dir",
    ])?;
    let mut lines = out.lines();
    let (top, git_dir, common) = (lines.next()?, lines.next()?, lines.next()?);
    if git_dir == common {
        return Some(std::path::PathBuf::from(top));
    }
    let list = git(&["worktree", "list", "--porcelain"])?;
    list.lines()
        .find_map(|l| l.strip_prefix("worktree "))
        .map(std::path::PathBuf::from)
}

/// Workspace name for a directory: the git repository's root directory name,
/// lowercased, or `scratch` outside a repository.
///
/// The repository root rather than the current directory, so that running this
/// from a subdirectory does not split one project across two workspaces; and
/// the main working tree rather than a linked worktree, for the same reason.
///
/// This is a default, computed once: `remote init` records the result in
/// `.deciduous/config.toml`, and from then on that is the name. Deriving it on
/// every call is what forked a renamed repository into a second workspace.
pub fn workspace_for(dir: &Path) -> String {
    match repo_root(dir) {
        Some(path) => path
            .file_name()
            .map(|n| {
                let n = n.to_string_lossy().to_lowercase();
                // The main tree of a bare repository is `name.git`.
                n.strip_suffix(".git").map(str::to_string).unwrap_or(n)
            })
            .unwrap_or_else(|| FALLBACK_WORKSPACE.to_string()),
        None => FALLBACK_WORKSPACE.to_string(),
    }
}

/// The root commits of the repository `dir` is in: the same in every clone
/// and worktree of it, different for an unrelated repository that happens to
/// share its directory name, and unchanged by a rename. Sorted.
///
/// * `None` when there is nothing to compare: outside git, or a shallow
///   clone. A shallow clone's `rev-list --max-parents=0` answers with its
///   shallow boundary, a commit in the middle of the history, and sending
///   that got CI clones (`--depth 1`) refused as "another repository".
/// * `Some([])` in a repository with no commit yet. That is not the same as
///   `None`: the server refuses it from a workspace another repository has
///   claimed, since it cannot show it is that repository.
pub fn repo_roots(dir: &Path) -> Option<Vec<String>> {
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .ok()
    };
    let inside = git(&["rev-parse", "--is-shallow-repository"])
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())?;
    if inside.trim() == "true" {
        return None;
    }
    let out = git(&["rev-list", "--max-parents=0", "HEAD"])?;
    if !out.status.success() {
        // Inside git, and HEAD resolves to nothing: no commit yet.
        return Some(Vec::new());
    }
    let mut roots: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    roots.sort();
    roots.dedup();
    Some(roots)
}

/// The nearest `.deciduous/config.toml` at or above `dir`.
fn project_config(dir: &Path) -> Option<std::path::PathBuf> {
    dir.ancestors()
        .map(|d| d.join(".deciduous").join("config.toml"))
        .find(|p| p.exists())
}

/// The workspace a config written by 1.0.7 (a `[remote] url` and no
/// workspace) has been writing to, recorded in the config so it stops being
/// derived.
///
/// 1.0.7 derived the name from the directory on every call. Recording the
/// *current* directory name, as the first 1.0.8 build did, forked a project
/// renamed since its last 1.0.7 write into a second, empty graph. So the
/// server is asked first which workspaces hold this database's nodes
/// (`POST /locate`), and the one that does is recorded. If none does (never
/// pushed), the derived name is. If the server cannot be asked, nothing is
/// recorded and the derived name is used for this call only, so the question
/// is asked again next time.
fn legacy_workspace(dir: &Path, url: &str, token: &str, derived: String) -> Result<String, String> {
    use colored::Colorize;
    let Some(config) = project_config(dir) else {
        return Ok(derived);
    };
    let ids = local_change_ids(&config.with_file_name("deciduous.db"));
    let chosen = if ids.is_empty() {
        derived.clone()
    } else {
        #[derive(Deserialize)]
        struct Held {
            name: String,
            nodes: usize,
        }
        #[derive(Deserialize)]
        struct Reply {
            workspaces: Vec<Held>,
        }
        let reply = ureq::post(&format!("{url}/locate"))
            .set("authorization", &format!("Bearer {token}"))
            .timeout(std::time::Duration::from_secs(15))
            .send_json(serde_json::json!({ "change_ids": ids }))
            .map_err(describe)
            .and_then(|r| {
                r.into_json::<Reply>()
                    .map_err(|e| format!("the server's /locate answer was not a list: {e}"))
            });
        let held = match reply {
            Ok(r) => r.workspaces,
            // Asked again on the next call; see above.
            Err(_) => return Ok(derived),
        };
        match held.as_slice() {
            [] => derived.clone(),
            _ if held.iter().any(|h| h.name == derived) => derived.clone(),
            [one] => one.name.clone(),
            many => {
                return Err(format!(
                    "this project's config names no workspace (it was written by 1.0.7, which used the \
                     directory name), and this project's nodes are in more than one workspace on {url}: {}.\n\n\
                     Record the one it should write to:\n\n    deciduous remote init {url} --workspace <name>",
                    many.iter()
                        .map(|h| format!("{} ({} nodes)", h.name, h.nodes))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            }
        }
    };

    let Ok(text) = std::fs::read_to_string(&config) else {
        return Ok(chosen);
    };
    let Ok(mut doc) = text.parse::<toml_edit::DocumentMut>() else {
        return Ok(chosen);
    };
    let same_url = doc
        .get("remote")
        .and_then(|r| r.get("url"))
        .and_then(|u| u.as_str())
        .is_some_and(|u| u.trim_end_matches('/') == url);
    if !same_url || doc.get("remote").and_then(|r| r.get("workspace")).is_some() {
        return Ok(chosen);
    }
    doc["remote"]["workspace"] = toml_edit::value(chosen.as_str());
    let why = if chosen == derived {
        String::new()
    } else {
        format!(
            " (the server holds this project's nodes there: the name this directory had when 1.0.7 wrote them, not \"{derived}\")"
        )
    };
    match std::fs::write(&config, doc.to_string()) {
        Ok(()) => eprintln!(
            "{} recorded workspace = \"{chosen}\"{why} in {}, so renaming or cloning this \
             repository keeps writing to the same graph. Commit that file.",
            "Note:".yellow(),
            config.display()
        ),
        Err(e) => eprintln!(
            "{} could not record workspace = \"{chosen}\" in {}: {e}. \
             Until it is recorded, renaming this directory changes which graph it writes to.",
            "Warning:".yellow(),
            config.display()
        ),
    }
    Ok(chosen)
}

/// Up to 1,000 change_ids from a local database, newest first; empty when
/// there is no database or it cannot be read.
fn local_change_ids(db: &Path) -> Vec<String> {
    let Ok(conn) =
        rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
    else {
        return Vec::new();
    };
    let Ok(mut stmt) =
        conn.prepare("SELECT change_id FROM decision_nodes WHERE change_id IS NOT NULL ORDER BY id DESC LIMIT 1000")
    else {
        return Vec::new();
    };
    stmt.query_map([], |r| r.get::<_, String>(0))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
}

/// Resolved settings for a call: where to send it, as which workspace.
#[derive(Debug)]
pub struct Remote {
    pub url: String,
    pub workspace: String,
    token: String,
    /// This repository's root commit ids, sent so the server can tell two
    /// repositories with the same directory name apart. See [`repo_roots`].
    pub repo_roots: Option<Vec<String>>,
}

impl Remote {
    pub fn resolve(config: &Config, dir: &Path) -> Result<Self, String> {
        let url = config.remote.url.clone().ok_or_else(|| {
            "no remote configured for this project.\n\nRun:\n\n    deciduous remote init <url>"
                .to_string()
        })?;

        let url = url.trim_end_matches('/').to_string();
        let token = token()?;
        let workspace = match &config.remote.workspace {
            Some(ws) => ws.clone(),
            None => legacy_workspace(dir, &url, &token, workspace_for(dir))?,
        };

        Ok(Self {
            url,
            workspace,
            token,
            repo_roots: repo_roots(dir),
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
        let req = ureq::get(&format!("{}{}", self.url, path))
            .set("authorization", &format!("Bearer {}", self.token));
        // The server checks the claim on /export too, so a pull or a status
        // cannot read another repository's graph.
        match &self.repo_roots {
            Some(roots) => req.set("x-deciduous-repo-roots", &roots.join(",")),
            None => req,
        }
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
            .map_err(|e| self.describe(e))?;

        resp.into_json::<RemoteGraph>()
            .map_err(|e| format!("the server's response was not a graph: {e}"))
    }

    /// Sends the local graph up. Used to seed a workspace and to carry local
    /// history that predates the remote; it is not the normal write path.
    pub fn import(&self, graph: Value) -> Result<ImportReport, String> {
        let payload = serde_json::json!({
            "workspace": self.workspace,
            "repo_roots": self.repo_roots,
            "graph": graph,
        });

        self.post("/import")
            .timeout(std::time::Duration::from_secs(600))
            .send_json(payload)
            .map_err(|e| self.describe(e))?
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
    // Tombstones count as "has": a node deleted on the server is not
    // missing from it, and sending it would write into the deleted row.
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

/// What one replay of the log did.
#[derive(Debug, Default)]
pub struct ReplayReport {
    /// Ops sent.
    pub sent: usize,
    /// Ops the server changed something for.
    pub applied: usize,
    /// Ops whose effect the server already had: a create for a row it holds,
    /// a delete for one it does not, or an op id it applied before.
    pub already: usize,
    /// Ops the server refused, with its reason. They stay in the log.
    pub rejected: Vec<(crate::oplog::Op, String)>,
}

/// Ops per request. The server takes up to 5,000; a smaller batch keeps one
/// request well inside its body limit and gets acks written sooner.
const REPLAY_BATCH: usize = 500;

impl Remote {
    /// A failed request, described; a 409 is the server refusing this
    /// repository, and says so.
    fn describe(&self, e: ureq::Error) -> String {
        match e {
            ureq::Error::Status(409, _) => self.claim_refused(),
            e => describe(e),
        }
    }

    /// What to say when the server refuses this repository's roots.
    fn claim_refused(&self) -> String {
        if self.repo_roots.as_ref().is_some_and(|r| r.is_empty()) {
            return format!(
                "workspace \"{ws}\" on {url} belongs to a repository, and this one has no commit yet, \
                 so the server cannot tell whether it is that repository or an unrelated project \
                 also called {ws}; nothing it sends is accepted until it can.\n\n\
                 Make the first commit (a clone of that repository has its commits already), or give \
                 this project its own workspace:\n\n    \
                 deciduous remote init {url} --workspace <another-name>",
                ws = self.workspace,
                url = self.url
            );
        }
        format!(
            "workspace \"{ws}\" on {url} belongs to another repository: an unrelated project \
             whose directory is also called {ws} (the name is the directory name, lowercased) \
             claimed it first, and this repository shares none of its root commits.\n\n\
             Give this project its own workspace:\n\n    \
             deciduous remote init {url} --workspace <another-name>\n\n\
             or, if the two repositories really should write one graph, say so by naming it:\n\n    \
             deciduous remote init {url} --workspace {ws}",
            ws = self.workspace,
            url = self.url
        )
    }

    /// Ties the workspace to this repository on the server, or finds out it
    /// belongs to another one. `adopt` is for a workspace the user named
    /// explicitly: sharing it is then a choice, not an accident.
    ///
    /// Returns the server's word for what happened: claimed, verified,
    /// adopted, or unchecked (no root commits to send).
    pub fn claim(&self, adopt: bool) -> Result<String, String> {
        #[derive(Deserialize)]
        struct Reply {
            claim: String,
        }
        let payload = serde_json::json!({
            "workspace": self.workspace,
            "repo_roots": self.repo_roots,
            "adopt": adopt,
        });
        match self
            .post("/claim")
            .timeout(std::time::Duration::from_secs(30))
            .send_json(payload)
        {
            Ok(resp) => resp
                .into_json::<Reply>()
                .map(|r| r.claim)
                .map_err(|e| format!("the server's response was not a claim: {e}")),
            Err(e) => Err(self.describe(e)),
        }
    }

    /// Sends ops to `POST /ops` and returns the server's answer for each.
    pub fn post_ops(&self, ops: &[crate::oplog::Op]) -> Result<Vec<crate::oplog::Ack>, String> {
        #[derive(Deserialize)]
        struct Answer {
            op_id: String,
            result: String,
            #[serde(default)]
            reason: Option<String>,
        }
        #[derive(Deserialize)]
        struct Reply {
            results: Vec<Answer>,
        }

        let payload = serde_json::json!({
            "workspace": self.workspace,
            "repo_roots": self.repo_roots,
            "ops": ops,
        });
        let reply: Reply = self
            .post("/ops")
            .timeout(std::time::Duration::from_secs(120))
            .send_json(payload)
            .map_err(|e| match e {
                ureq::Error::Status(409, _) => self.claim_refused(),
                e => describe(e),
            })?
            .into_json()
            .map_err(|e| format!("the server's response was not an ops report: {e}"))?;

        let now = chrono::Utc::now().to_rfc3339();
        let answers: std::collections::HashMap<String, Answer> = reply
            .results
            .into_iter()
            .map(|a| (a.op_id.clone(), a))
            .collect();
        ops.iter()
            .map(|op| {
                let a = answers.get(&op.op_id).ok_or_else(|| {
                    format!(
                        "the server answered for {} of {} ops and not for {} ({}); nothing was marked sent",
                        answers.len(),
                        ops.len(),
                        op.op_id,
                        op.body.describe()
                    )
                })?;
                match a.result.as_str() {
                    "applied" | "exists" | "absent" | "duplicate" | "rejected" => {}
                    other => {
                        return Err(format!(
                            "the server answered {other:?} for {} ({}); this CLI knows applied, exists, absent, duplicate and rejected",
                            op.op_id,
                            op.body.describe()
                        ))
                    }
                }
                Ok(crate::oplog::Ack {
                    op_id: op.op_id.clone(),
                    result: a.result.clone(),
                    reason: a.reason.clone(),
                    at: now.clone(),
                })
            })
            .collect()
    }
}

/// Sends every pending op in the log to the server, in order, records the
/// answers and compacts the log.
///
/// Acks are written batch by batch, so a failure halfway leaves the first
/// half marked and the rest pending. A batch the server applied but whose
/// answer never arrived is sent again next time and answered `duplicate`.
pub fn replay(remote: &Remote, log: &crate::oplog::OpLog) -> Result<ReplayReport, String> {
    let state = log.read()?;
    let mut report = ReplayReport::default();

    for batch in state.pending.chunks(REPLAY_BATCH) {
        let acks = remote.post_ops(batch)?;
        log.record_acks(&acks)?;
        report.sent += batch.len();
        for (op, ack) in batch.iter().zip(&acks) {
            match ack.result.as_str() {
                "applied" => report.applied += 1,
                "rejected" => report.rejected.push((
                    op.clone(),
                    ack.reason
                        .clone()
                        .unwrap_or_else(|| "no reason given".into()),
                )),
                _ => report.already += 1,
            }
        }
    }

    log.compact()?;
    Ok(report)
}

/// Prints the ops a server refused, loudly, with what to do about them.
pub fn print_rejected(rejected: &[(crate::oplog::Op, String)], log: &crate::oplog::OpLog) {
    use colored::Colorize;
    if rejected.is_empty() {
        return;
    }
    eprintln!(
        "{} the server refused {} write(s):",
        "Rejected:".red().bold(),
        rejected.len()
    );
    for (op, reason) in rejected {
        eprintln!("  {}  {}", op.body.describe(), reason.dimmed());
    }
    eprintln!(
        "They stay in {} and the local graph keeps them. \
         `deciduous remote status` lists them; `deciduous remote push --drop-rejected` discards them.",
        log.path().display()
    );
}

/// Replays the log after a command that wrote to it, without letting the
/// server turn a successful local write into a failed command.
///
/// The CLI writes to the local database and the agents write through the MCP
/// server. A local write the server never sees is a fork, not a cache: this is
/// how one workspace ended up with 502 nodes locally and 0 on the server, with
/// neither side saying a word. So every write is logged, and every command
/// that logged one replays the log before it exits.
///
/// Best effort deliberately. The CLI has to keep working on a plane, and a
/// server being down must not stop anyone recording a decision. A failure
/// says how many writes are waiting and where; they stay in the log, and the
/// next write or `deciduous remote push` sends them.
pub fn replay_after_write(log: &crate::oplog::OpLog) {
    use colored::Colorize;

    let cfg = Config::load();
    if !cfg.remote.is_configured() {
        return;
    }
    let dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let remote = Remote::resolve(&cfg, &dir);
    let result = remote
        .as_ref()
        .map_err(|e| e.clone())
        .and_then(|r| replay(r, log));
    match result {
        Ok(report) => print_rejected(&report.rejected, log),
        Err(e) => {
            let waiting = log.read().map(|s| s.pending.len()).unwrap_or(0);
            // Only a server that does not answer is an outage that passes by
            // itself. A refusal (another repository's workspace, a bad
            // token) or a config problem is answered the same way on every
            // retry, and "once the server is reachable" sent people to wait
            // for something that was never going to happen.
            let unreachable = remote.as_ref().is_ok_and(|r| r.health().is_err());
            if unreachable {
                eprintln!(
                    "{} the local write succeeded but the server did not get it: {e}\n\
                     {waiting} write(s) queued in {}. They are sent on the next write, or now with \
                     `deciduous remote push` once the server is reachable.",
                    "Warning:".yellow(),
                    log.path().display(),
                );
            } else {
                eprintln!(
                    "{} the local write succeeded but the server refused it: {e}\n\
                     {waiting} write(s) wait in {}, and every later write will be refused the same way \
                     until that is fixed. `deciduous remote status` lists them.",
                    "Warning:".yellow(),
                    log.path().display(),
                );
            }
        }
    }
}

/// Replays the log when dropped, if this process appended to it.
///
/// Held for the life of `main`, so every command that writes (add, link,
/// status, prompt, delete, the archaeology commands, anything added later)
/// sends its ops without each one having to remember to.
pub struct ReplayOnExit;

impl Drop for ReplayOnExit {
    fn drop(&mut self) {
        if let Some(log) = crate::oplog::take_appended() {
            replay_after_write(&log);
        }
    }
}

/// How the local graph and the server's differ, row by row.
///
/// `remote status` used to compare counts, and said "OK: counts match" when
/// one side had an unpushed node and the other an agent's different node
/// (31 vs 31), or when every count matched and a status did not (C2, C9).
/// Equal counts say nothing about equal content, so this compares content:
/// live nodes by change_id, field by field, and edges by
/// (from, to, type).
#[derive(Debug, Default)]
pub struct ContentDiff {
    pub local_nodes: usize,
    pub server_nodes: usize,
    pub local_edges: usize,
    pub server_edges: usize,
    /// (change_id, "type \"title\"")
    pub only_local: Vec<(String, String)>,
    /// Here, and deleted on the server: a pull removes them here. Listed
    /// apart from `only_local`, whose remedy (sending them) would write
    /// into a deleted row.
    pub deleted_on_server: Vec<(String, String)>,
    pub only_server: Vec<(String, String)>,
    pub differ: Vec<NodeDifference>,
    pub edges_only_local: Vec<String>,
    pub edges_only_server: Vec<String>,
}

impl ContentDiff {
    pub fn is_empty(&self) -> bool {
        self.only_local.is_empty()
            && self.deleted_on_server.is_empty()
            && self.only_server.is_empty()
            && self.differ.is_empty()
            && self.edges_only_local.is_empty()
            && self.edges_only_server.is_empty()
    }
}

/// One node both sides hold with different content.
#[derive(Debug)]
pub struct NodeDifference {
    pub change_id: String,
    pub title: String,
    /// (field, here, on the server)
    pub fields: Vec<(String, String, String)>,
}

pub fn content_diff(
    nodes: &[crate::db::DecisionNode],
    edges: &[crate::db::DecisionEdge],
    server: &RemoteGraph,
) -> ContentDiff {
    use std::collections::{BTreeMap, BTreeSet, HashMap};

    let short = |c: &str| c.chars().take(8).collect::<String>();
    let label = |t: &str, title: &str, cid: &str| format!("{t} {} \"{title}\"", short(cid));
    let shown = |s: &str| {
        let s: String = s.chars().take(60).collect();
        format!("{s:?}")
    };

    // A server that exports tombstones marks them with deleted_at; they are
    // not part of the live graph on either side.
    let server_nodes: BTreeMap<&str, &RemoteNode> = server
        .nodes
        .iter()
        .filter(|n| n.deleted_at.is_none())
        .map(|n| (n.change_id.as_str(), n))
        .collect();
    let local_nodes: BTreeMap<&str, &crate::db::DecisionNode> =
        nodes.iter().map(|n| (n.change_id.as_str(), n)).collect();

    let mut d = ContentDiff {
        local_nodes: local_nodes.len(),
        server_nodes: server_nodes.len(),
        ..Default::default()
    };

    let tombstones: BTreeSet<&str> = server
        .nodes
        .iter()
        .filter(|n| n.deleted_at.is_some())
        .map(|n| n.change_id.as_str())
        .collect();

    for (cid, n) in &local_nodes {
        let Some(s) = server_nodes.get(cid) else {
            let row = (cid.to_string(), label(&n.node_type, &n.title, cid));
            if tombstones.contains(cid) {
                d.deleted_on_server.push(row);
            } else {
                d.only_local.push(row);
            }
            continue;
        };
        let mut fields = Vec::new();
        let mut cmp = |name: &str, here: &str, there: &str| {
            if here != there {
                fields.push((name.to_string(), shown(here), shown(there)));
            }
        };
        cmp("type", &n.node_type, &s.node_type);
        cmp("title", &n.title, &s.title);
        cmp("status", &n.status, &s.status);
        cmp(
            "description",
            n.description.as_deref().unwrap_or(""),
            s.description.as_deref().unwrap_or(""),
        );
        let local_meta: serde_json::Map<String, Value> = n
            .metadata_json
            .as_deref()
            .and_then(|m| serde_json::from_str::<Value>(m).ok())
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        let server_meta = s
            .metadata
            .as_ref()
            .and_then(|m| m.as_object().cloned())
            .unwrap_or_default();
        let keys: BTreeSet<&String> = local_meta.keys().chain(server_meta.keys()).collect();
        for k in keys {
            let here = local_meta.get(k).map(|v| v.to_string()).unwrap_or_default();
            let there = server_meta
                .get(k)
                .map(|v| v.to_string())
                .unwrap_or_default();
            if here != there {
                fields.push((format!("metadata.{k}"), here, there));
            }
        }
        if !fields.is_empty() {
            d.differ.push(NodeDifference {
                change_id: cid.to_string(),
                title: n.title.clone(),
                fields,
            });
        }
    }
    for (cid, s) in &server_nodes {
        if !local_nodes.contains_key(cid) {
            d.only_server
                .push((cid.to_string(), label(&s.node_type, &s.title, cid)));
        }
    }

    let by_id: HashMap<i32, &str> = nodes.iter().map(|n| (n.id, n.change_id.as_str())).collect();
    let edge_label = |f: &str, t: &str, k: &str| format!("{} -> {} ({k})", short(f), short(t));
    let local_edges: BTreeSet<(String, String, String)> = edges
        .iter()
        .filter_map(|e| {
            Some((
                by_id.get(&e.from_node_id)?.to_string(),
                by_id.get(&e.to_node_id)?.to_string(),
                e.edge_type.clone(),
            ))
        })
        .collect();
    // An edge from or to a node the server deleted is not in its live graph.
    let server_edges: BTreeSet<(String, String, String)> = server
        .edges
        .iter()
        .filter_map(|e| {
            let (f, t) = (e.from_change_id.as_deref()?, e.to_change_id.as_deref()?);
            (server_nodes.contains_key(f) && server_nodes.contains_key(t))
                .then(|| (f.to_string(), t.to_string(), e.edge_type.clone()))
        })
        .collect();
    d.local_edges = local_edges.len();
    d.server_edges = server_edges.len();
    d.edges_only_local = local_edges
        .difference(&server_edges)
        .map(|(f, t, k)| edge_label(f, t, k))
        .collect();
    d.edges_only_server = server_edges
        .difference(&local_edges)
        .map(|(f, t, k)| edge_label(f, t, k))
        .collect();
    d
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

/// Merges the server's graph into the repository's record store, then lets
/// `records::reconcile` fold it into the local database.
///
/// Merges, not writes: the server's records carry none of the fields it
/// does not model (a newer version's, cascade markers), no local tombstone
/// of an edge whose removal was local only, and no local relink since its
/// last copy. Writing them over the file's records lost all three; the
/// merge driver's rules keep them and still let a newer server copy win.
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
            if store.absorb_node(&rec).map_err(|e| e.to_string())? {
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
            if store.absorb_edge(&rec).map_err(|e| e.to_string())? {
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
    // while pull did nothing. The edit's op was refused by name when it
    // was replayed; this is where the edit goes.
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
        fetched_nodes: graph
            .nodes
            .iter()
            .filter(|n| n.deleted_at.is_none())
            .count(),
        fetched_tombstones: graph
            .nodes
            .iter()
            .filter(|n| n.deleted_at.is_some())
            .count(),
        fetched_edges: graph.edges.len(),
        records_written,
        imported_nodes: report.nodes_imported,
        updated_nodes: report.nodes_updated,
        removed_nodes: report.nodes_deleted + overridden.len(),
        imported_edges: report.edges_imported,
        removed_edges: report.edges_deleted,
        deleted_over_local_edits: overridden,
    })
}

/// What a pull changed locally. "imported 0 nodes" after a pull that applied
/// an agent's edits read as "nothing happened"; edits and removals are
/// counted separately so the report says what did.
#[derive(Debug, Default)]
pub struct PullReport {
    pub fetched_nodes: usize,
    /// Nodes the server deleted, sent so the local copy can go too.
    pub fetched_tombstones: usize,
    pub fetched_edges: usize,
    pub records_written: usize,
    pub imported_nodes: usize,
    pub updated_nodes: usize,
    pub removed_nodes: usize,
    pub imported_edges: usize,
    pub removed_edges: usize,
    /// Nodes the server had deleted that had been edited here after the
    /// delete; deleted anyway, edit and all (see `pull`). Counted in
    /// `removed_nodes` too.
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

/// Percent-encodes a query value, byte by byte over its UTF-8.
///
/// 1.0.7 encoded each `char` as `%{code point}`, so `é` (U+00E9) became
/// `%E9`, which is not UTF-8, and the server answered every request from a
/// repository called `bridge-café` with a bare 400 (C8). A percent escape
/// names one byte; `é` is two (`%C3%A9`).
fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b => format!("%{b:02X}"),
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

    let remote = doc["remote"].or_insert(toml_edit::table());
    remote["url"] = toml_edit::value(url.to_string());
    // Recorded now, while the directory still has the name it was set up
    // under; see `workspace_for`.
    if remote.get("workspace").is_none() {
        remote["workspace"] = toml_edit::value(workspace_for(project));
    }

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
    fn non_ascii_is_encoded_as_utf8_bytes() {
        assert_eq!(urlencode("café"), "caf%C3%A9");
        assert_eq!(urlencode("ü"), "%C3%BC");
        assert_eq!(urlencode("日"), "%E6%97%A5");
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
            repo_roots: None,
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
            repo_roots: None,
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
            repo_roots: None,
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
