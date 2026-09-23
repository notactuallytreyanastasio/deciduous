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
fn legacy_workspace(
    dir: &Path,
    url: &str,
    token: &str,
    derived: String,
    deadline: Option<std::time::Instant>,
) -> Result<String, String> {
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
        // Within the replay's budget when one is set: the /locate question
        // and the ops after it used to take 15 s + 10 s per CLI write
        // against a server that never answers (RUST-N2).
        let reply = ureq::post(&format!("{url}/locate"))
            .set("authorization", &format!("Bearer {token}"))
            .timeout(within(std::time::Duration::from_secs(15), deadline))
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
    /// The longest one `POST /ops` request may take, connecting included.
    ops_timeout: std::time::Duration,
    /// When everything this remote sends has to be done by: the replay after
    /// a write, which someone is waiting behind. `None` for an explicit
    /// command.
    deadline: Option<std::time::Instant>,
}

/// `limit`, or less if `deadline` is nearer. Never zero: ureq reads a zero
/// timeout as none.
fn within(limit: std::time::Duration, deadline: Option<std::time::Instant>) -> std::time::Duration {
    let left = deadline.map_or(limit, |d| {
        d.saturating_duration_since(std::time::Instant::now())
    });
    limit.min(left).max(std::time::Duration::from_millis(1))
}

/// How long `deciduous remote push` lets one batch of ops take.
const OPS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// How long the replay after a write may take in all: resolving the
/// workspace, every /ops request, and the one-op resends after a 500. A
/// server that accepts connections and never answers (a paused container, a
/// stalled tunnel, a captive portal) held every CLI write for 2:15 at 120 s
/// plus a health check, a legacy config's /locate added 15 s to the ops'
/// 10 s, and a server whose database was down answered each single-op
/// resend 500 after 3.5 s: 108 s for a write with 30 ops queued. What is not
/// sent within the budget waits: its ops stay pending, and a later replay
/// that gets through is answered "duplicate" for any the server did apply.
pub const QUICK_OPS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// The config of the project whose database lives in `data_dir` (its
/// `.deciduous/`), read from `data_dir/config.toml`, not found by walking up
/// from the current directory. Default when there is none.
pub fn config_at(data_dir: &Path) -> Result<Config, String> {
    let path = data_dir.join("config.toml");
    match std::fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

impl Remote {
    /// The server, workspace and repository of the project that owns the
    /// database in `data_dir`.
    ///
    /// A write goes to the log beside its database, and the log has to be
    /// replayed as that database's project: its config's url and workspace,
    /// and the root commits of the repository it sits in. Resolving from the
    /// current directory sent project 1's writes to project 3's workspace
    /// whenever DECIDUOUS_DB_PATH pointed across (RUST-N1), and said nothing
    /// at all when the current directory had no project (BRIDGE-N3).
    ///
    /// The url is read at replay time, not recorded per op when the op is
    /// written. Queued writes are meant to follow a corrected url: a typo
    /// fixed in config.toml, or a server that moved, must receive what was
    /// queued while it was wrong.
    pub fn for_data_dir(data_dir: &Path) -> Result<Self, String> {
        Self::for_data_dir_by(data_dir, None)
    }

    /// [`Remote::for_data_dir`] for the replay after a write: everything it
    /// sends, from resolving the workspace to the last op, within
    /// [`QUICK_OPS_TIMEOUT`] from now.
    pub fn for_replay_after_write(data_dir: &Path) -> Result<Self, String> {
        let deadline = std::time::Instant::now() + QUICK_OPS_TIMEOUT;
        let mut r = Self::for_data_dir_by(data_dir, Some(deadline))?;
        r.ops_timeout = QUICK_OPS_TIMEOUT;
        r.deadline = Some(deadline);
        Ok(r)
    }

    fn for_data_dir_by(
        data_dir: &Path,
        deadline: Option<std::time::Instant>,
    ) -> Result<Self, String> {
        let config = config_at(data_dir)?;
        let data_dir = std::path::absolute(data_dir).unwrap_or_else(|_| data_dir.to_path_buf());
        let project = data_dir.parent().unwrap_or(&data_dir);
        Self::resolve_by(&config, project, deadline)
    }

    pub fn resolve(config: &Config, dir: &Path) -> Result<Self, String> {
        Self::resolve_by(config, dir, None)
    }

    fn resolve_by(
        config: &Config,
        dir: &Path,
        deadline: Option<std::time::Instant>,
    ) -> Result<Self, String> {
        let url = config.remote.url.clone().ok_or_else(|| {
            "no remote configured for this project.\n\nRun:\n\n    deciduous remote init <url>"
                .to_string()
        })?;

        let url = url.trim_end_matches('/').to_string();
        let token = token()?;
        let workspace = match &config.remote.workspace {
            Some(ws) => ws.clone(),
            None => legacy_workspace(dir, &url, &token, workspace_for(dir), deadline)?,
        };

        Ok(Self {
            url,
            workspace,
            token,
            repo_roots: repo_roots(dir),
            ops_timeout: OPS_TIMEOUT,
            deadline: None,
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

/// Sends the server everything in `graph` it does not already have: nodes,
/// edges, and documents with their bytes. `None` means there was nothing to
/// send.
///
/// Shared by `remote push` and the automatic push after a write, so both decide
/// what is missing the same way — `missing_on_server` earned its edge-keying
/// rules the hard way and there must not be a second copy of them.
///
/// Also returns the local nodes the server has deleted. Nothing a push sends
/// can change those; they are the user's to pull (see `deleted_on_server`).
///
/// Documents used to be computed and then ignored: `nodes == 0 && edges == 0`
/// returned "nothing to send" with a document missing, and a document that
/// was sent had no bytes behind it, because nothing uploaded them.
/// What [`push_missing`] did: the import's report (`None` when nothing was
/// sent), the local nodes the server has deleted, and the rows withheld for
/// holding a NUL.
pub type Seeded = (Option<ImportReport>, Vec<DeletedOnServer>, Vec<String>);

pub fn push_missing(remote: &Remote, graph: &Value) -> Result<Seeded, String> {
    let server = remote.export()?;
    let deleted = deleted_on_server(graph, &server);
    let (mut missing, _, _) = missing_on_server(graph, &server);
    let withheld = withhold_nul(&mut missing);
    let nodes = missing["nodes"].as_array().map_or(0, Vec::len);
    let edges = missing["edges"].as_array().map_or(0, Vec::len);
    let docs = missing["documents"].as_array().cloned().unwrap_or_default();
    if nodes == 0 && edges == 0 && docs.is_empty() {
        return Ok((None, deleted, withheld));
    }
    if !docs.is_empty() {
        let dir = Database::db_path()
            .parent()
            .map(|p| p.join("documents"))
            .ok_or("the database path has no directory, so there are no documents to read")?;
        for d in &docs {
            let (Some(hash), Some(file)) =
                (d["content_hash"].as_str(), d["storage_filename"].as_str())
            else {
                return Err(format!(
                    "a local document has no content_hash or storage_filename: {d}"
                ));
            };
            let path = dir.join(file);
            let bytes = std::fs::read(&path).map_err(|e| {
                format!(
                    "document {} ({}) cannot be sent: {e}. Nothing was sent.",
                    d["original_filename"].as_str().unwrap_or("?"),
                    path.display()
                )
            })?;
            remote.upload_blob(hash, &bytes, d["mime_type"].as_str())?;
        }
    }
    remote.import(missing).map(|r| (Some(r), deleted, withheld))
}

/// Where a local row (a node, an edge or a document, as `deciduous graph`
/// writes it) holds a NUL character, if anywhere: in a field, or inside a
/// `*_json` field once decoded, where it is stored escaped. Postgres text
/// cannot hold one, so the server refuses the row.
pub fn row_nul(row: &Value) -> Option<String> {
    fn find(v: &Value, path: &str) -> Option<String> {
        match v {
            Value::String(s) if s.contains('\0') => Some(path.to_string()),
            Value::String(s) if path.ends_with("_json") => serde_json::from_str::<Value>(s)
                .ok()
                .and_then(|inner| find(&inner, path.trim_end_matches("_json"))),
            Value::Object(m) => m.iter().find_map(|(k, x)| {
                let at = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                if k.contains('\0') {
                    Some(format!("the key {at:?}"))
                } else {
                    find(x, &at)
                }
            }),
            Value::Array(a) => a
                .iter()
                .enumerate()
                .find_map(|(i, x)| find(x, &format!("{path}.{i}"))),
            _ => None,
        }
    }
    find(row, "")
}

/// Takes out of an import the rows holding a NUL, and the edges of a node
/// taken out, and describes each. The server refuses a whole import for one
/// such row ("nothing was imported"), so one of them stopped `remote push
/// --seed` from sending any other row, including writes whose ops a crash
/// had lost, for which --seed is the only way back.
fn withhold_nul(missing: &mut Value) -> Vec<String> {
    let mut out = Vec::new();
    let mut gone = std::collections::HashSet::new();
    if let Some(nodes) = missing["nodes"].as_array_mut() {
        nodes.retain(|n| match row_nul(n) {
            None => true,
            Some(at) => {
                let cid = n["change_id"].as_str().unwrap_or_default().to_string();
                out.push(format!(
                    "{} {} \"{}\" (NUL at {at})",
                    n["node_type"].as_str().unwrap_or("node"),
                    cid.chars().take(8).collect::<String>(),
                    n["title"].as_str().unwrap_or_default().replace('\0', "\\0")
                ));
                gone.insert(cid);
                false
            }
        });
    }
    if let Some(edges) = missing["edges"].as_array_mut() {
        edges.retain(|e| {
            let end = |k: &str| e[k].as_str().is_some_and(|c| gone.contains(c));
            let why = row_nul(e).map(|at| format!("NUL at {at}")).or_else(|| {
                (end("from_change_id") || end("to_change_id"))
                    .then(|| "an endpoint is withheld".to_string())
            });
            match why {
                None => true,
                Some(why) => {
                    out.push(format!(
                        "edge {} -> {} ({why})",
                        e["from_change_id"]
                            .as_str()
                            .unwrap_or("?")
                            .chars()
                            .take(8)
                            .collect::<String>(),
                        e["to_change_id"]
                            .as_str()
                            .unwrap_or("?")
                            .chars()
                            .take(8)
                            .collect::<String>()
                    ));
                    false
                }
            }
        });
    }
    if let Some(docs) = missing["documents"].as_array_mut() {
        docs.retain(|d| match row_nul(d) {
            None => true,
            Some(at) => {
                out.push(format!(
                    "document {} (NUL at {at})",
                    d["original_filename"]
                        .as_str()
                        .unwrap_or("?")
                        .replace('\0', "\\0")
                ));
                false
            }
        });
    }
    out
}

/// Prints the rows [`push_missing`] did not send because they hold a NUL.
pub fn print_withheld(withheld: &[String]) {
    use colored::Colorize;
    if withheld.is_empty() {
        return;
    }
    eprintln!(
        "{} {} row(s) hold a NUL character, which the server cannot store; they were not sent \
         (the other rows were):",
        "Not sent:".red().bold(),
        withheld.len()
    );
    for w in withheld {
        eprintln!("  {w}");
    }
    eprintln!(
        "Change that text here (a prompt: `deciduous prompt <id> ...`), then run \
         `deciduous remote push --seed` again."
    );
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
                !e["from_node_id"]
                    .as_i64()
                    .is_some_and(|i| gone.contains(&i))
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
    /// Lines of the log that are not entries: not sent, and moved to the
    /// side file by the compaction that ends the replay.
    pub unreadable: Vec<crate::oplog::Unreadable>,
}

/// Why a replay stopped. The three have different fixes, and saying "the
/// server refused it" about a line of a local file sent people to the
/// server for a problem on their own disk.
#[derive(Debug, Clone)]
pub enum ReplayError {
    /// The log file could not be read or written. No server was involved.
    Log(String),
    /// No answer: nothing listening, a timeout, DNS. Passes by itself.
    Unreachable(String),
    /// The server answered, and not with a report: a refusal of the whole
    /// request (another repository's workspace, a bad token), a server
    /// error, or a body that is not an ops report.
    Server(String),
    /// The server failed on the request (HTTP 500): not a refusal, and not
    /// an outage. See [`replay`] for how the op responsible is found.
    Failed(String),
    /// Nothing was sent because this machine's settings are incomplete: no
    /// token, a workspace that could not be resolved.
    Config(String),
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplayError::Log(e)
            | ReplayError::Unreachable(e)
            | ReplayError::Server(e)
            | ReplayError::Failed(e)
            | ReplayError::Config(e) => f.write_str(e),
        }
    }
}

impl From<ReplayError> for String {
    fn from(e: ReplayError) -> String {
        e.to_string()
    }
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

    /// Uploads one document's bytes to `PUT /blob/:hash`. The server checks
    /// them against the hash.
    pub fn upload_blob(&self, hash: &str, bytes: &[u8], mime: Option<&str>) -> Result<(), String> {
        ureq::put(&format!("{}/blob/{hash}", self.url))
            .set("authorization", &format!("Bearer {}", self.token))
            .set("content-type", mime.unwrap_or("application/octet-stream"))
            .timeout(std::time::Duration::from_secs(300))
            .send_bytes(bytes)
            .map(|_| ())
            .map_err(|e| format!("uploading document {hash}: {}", self.describe(e)))
    }

    /// Sends ops to `POST /ops` and returns the server's answer for each.
    pub fn post_ops(
        &self,
        ops: &[crate::oplog::Op],
    ) -> Result<Vec<crate::oplog::Ack>, ReplayError> {
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
        if self
            .deadline
            .is_some_and(|d| std::time::Instant::now() >= d)
        {
            return Err(ReplayError::Unreachable(format!(
                "the replay after a write gets {} s in all, and they are spent; \
                 what was not answered waits",
                QUICK_OPS_TIMEOUT.as_secs()
            )));
        }
        let reply: Reply = self
            .post("/ops")
            .timeout(within(self.ops_timeout, self.deadline))
            .send_json(payload)
            .map_err(|e| match e {
                ureq::Error::Status(409, _) => ReplayError::Server(self.claim_refused()),
                e @ ureq::Error::Status(500, _) => ReplayError::Failed(describe(e)),
                // A proxy's answer that the server behind it is down or
                // slow: an outage, which passes by itself.
                e @ ureq::Error::Status(502..=504, _) => ReplayError::Unreachable(describe(e)),
                e @ ureq::Error::Status(..) => ReplayError::Server(describe(e)),
                e @ ureq::Error::Transport(_) => ReplayError::Unreachable(describe(e)),
            })?
            .into_json()
            .map_err(|e| {
                ReplayError::Server(format!("the server's response was not an ops report: {e}"))
            })?;

        let now = chrono::Utc::now().to_rfc3339();
        let answers: std::collections::HashMap<String, Answer> = reply
            .results
            .into_iter()
            .map(|a| (a.op_id.clone(), a))
            .collect();
        ops.iter()
            .map(|op| {
                let a = answers.get(&op.op_id).ok_or_else(|| {
                    ReplayError::Server(format!(
                        "the server answered for {} of {} ops and not for {} ({}); nothing was marked sent",
                        answers.len(),
                        ops.len(),
                        op.op_id,
                        op.body.describe()
                    ))
                })?;
                match a.result.as_str() {
                    "applied" | "exists" | "absent" | "duplicate" | "rejected" => {}
                    other => {
                        return Err(ReplayError::Server(format!(
                            "the server answered {other:?} for {} ({}); this CLI knows applied, exists, absent, duplicate and rejected",
                            op.op_id,
                            op.body.describe()
                        )))
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

pub use crate::oplog::{is_set_aside, SET_ASIDE};

/// Ops set aside by this machine, and what later ops have to wait for them.
///
/// An op set aside leaves a gap in the log's order: the edits of a node
/// after its set-aside create were sent anyway and refused ("no node ... on
/// the server"), and an update after a set-aside update carries a `was`
/// the server never saw. So an op that names what a set-aside op wrote is
/// set aside with it, and they go again together, in order.
#[derive(Default)]
struct Held {
    /// key -> the first 8 characters of the op id that holds it.
    keys: std::collections::HashMap<String, String>,
}

impl Held {
    /// What an op writes: its node, or its edge.
    fn own(op: &crate::oplog::Op) -> Vec<String> {
        use crate::oplog::OpBody::*;
        match &op.body {
            CreateNode { change_id, .. }
            | UpdateNode { change_id, .. }
            | DeleteNode { change_id, .. } => vec![format!("n:{change_id}")],
            CreateEdge {
                from_change_id,
                to_change_id,
                edge_type,
                ..
            }
            | DeleteEdge {
                from_change_id,
                to_change_id,
                edge_type,
                ..
            } => vec![format!("e:{from_change_id}|{to_change_id}|{edge_type}")],
        }
    }

    /// What an op depends on: what it writes, and an edge's endpoints.
    fn needs(op: &crate::oplog::Op) -> Vec<String> {
        let mut k = Self::own(op);
        if let crate::oplog::OpBody::CreateEdge {
            from_change_id,
            to_change_id,
            ..
        }
        | crate::oplog::OpBody::DeleteEdge {
            from_change_id,
            to_change_id,
            ..
        } = &op.body
        {
            k.push(format!("n:{from_change_id}"));
            k.push(format!("n:{to_change_id}"));
        }
        k
    }

    fn add(&mut self, op: &crate::oplog::Op, by: &str) {
        for k in Self::own(op) {
            self.keys.entry(k).or_insert_with(|| by.to_string());
        }
    }

    /// The set-aside op `op` waits for, if any.
    fn blocking(&self, op: &crate::oplog::Op) -> Option<String> {
        Self::needs(op)
            .iter()
            .find_map(|k| self.keys.get(k).cloned())
    }

    /// Answers `op` locally as set aside behind `by`, and holds its keys.
    fn hold_behind(&mut self, op: &crate::oplog::Op, by: &str) -> crate::oplog::Ack {
        self.add(op, by);
        crate::oplog::Ack {
            op_id: op.op_id.clone(),
            result: "rejected".into(),
            reason: Some(format!(
                "{SET_ASIDE}, not sent: it follows op {by}, which is set aside, and sent without \
                 it the server would refuse it for want of that op. `deciduous remote push` sends \
                 them again, in order; `deciduous remote push --drop {}` discards this one",
                short_id(op)
            )),
            at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

fn short_id(op: &crate::oplog::Op) -> String {
    op.op_id.chars().take(8).collect()
}

/// Sends every pending op in the log to the server, in order, records the
/// answers and compacts the log.
///
/// Acks are written batch by batch, so a failure halfway leaves the first
/// half marked and the rest pending. A batch the server applied but whose
/// answer never arrived is sent again next time and answered `duplicate`.
///
/// A pending op that depends on an op this machine set aside earlier is set
/// aside with it, unsent (see [`Held`]).
pub fn replay(remote: &Remote, log: &crate::oplog::OpLog) -> Result<ReplayReport, ReplayError> {
    let state = log.read().map_err(ReplayError::Log)?;
    let mut report = ReplayReport {
        unreadable: state.unreadable.clone(),
        ..Default::default()
    };
    print_unreadable(&state, log);

    let mut held = Held::default();
    for (op, ack) in &state.rejected {
        if ack.reason.as_deref().is_some_and(is_set_aside) {
            held.add(op, &short_id(op));
        }
    }
    let by_id: std::collections::HashMap<&str, &crate::oplog::Op> = state
        .pending
        .iter()
        .map(|op| (op.op_id.as_str(), op))
        .collect();
    let tally = |report: &mut ReplayReport, acks: &[crate::oplog::Ack]| {
        for ack in acks {
            let Some(op) = by_id.get(ack.op_id.as_str()) else {
                continue;
            };
            report.sent += 1;
            match ack.result.as_str() {
                "applied" => report.applied += 1,
                "rejected" => report.rejected.push((
                    (*op).clone(),
                    ack.reason
                        .clone()
                        .unwrap_or_else(|| "no reason given".into()),
                )),
                _ => report.already += 1,
            }
        }
    };

    let mut sendable = Vec::with_capacity(state.pending.len());
    let mut behind = Vec::new();
    for op in &state.pending {
        match held.blocking(op) {
            Some(by) => behind.push(held.hold_behind(op, &by)),
            None => sendable.push(op.clone()),
        }
    }
    log.record_acks(&behind).map_err(ReplayError::Log)?;
    tally(&mut report, &behind);

    for batch in sendable.chunks(REPLAY_BATCH) {
        let (acks, stop) = match remote.post_ops(batch) {
            Ok(acks) => (acks, None),
            Err(ReplayError::Failed(e)) => isolate(remote, batch, &e, &mut held),
            Err(e) => return Err(e),
        };
        log.record_acks(&acks).map_err(ReplayError::Log)?;
        tally(&mut report, &acks);
        if let Some(e) = stop {
            // What was answered is recorded; the rest waits.
            log.compact().map_err(ReplayError::Log)?;
            return Err(e);
        }
    }

    log.compact().map_err(ReplayError::Log)?;
    Ok(report)
}

/// At most this many ops, in a row, that the server fails on even when sent
/// alone and twice, before the replay concludes that the server is failing
/// and not one op.
const MAX_SUSPECTS: usize = 3;

/// The server failed (HTTP 500) on a batch. One op it cannot handle fails
/// the whole request, and every later write joins the same batch, so
/// sending it again as it was would fail the same way forever (SERVER-N1:
/// one NUL in a prompt stopped every write after it from reaching the
/// server). The batch is sent again one op at a time.
///
/// An op is set aside (answered locally as rejected, kept in the log) only
/// when the server fails on it alone, twice, then answers another op, then
/// fails on it again: a verdict about that op, not about the server. A 500
/// from a server whose database is restarting or whose pool timed out
/// passes: the first rule of the previous version, "fails alone while the
/// server answered others", counted an answer from before the outage, and
/// set aside every healthy op after it. So:
///
/// * a 500 is retried once at once, which absorbs a blip;
/// * an op that fails twice is a suspect, and the ops after it that do not
///   depend on it are tried, to learn whether the server answers at all;
/// * when one is answered, each suspect is sent once more, and set aside
///   only if it fails again;
/// * [`MAX_SUSPECTS`] suspects in a row, or none answered after them, and
///   the server is failing: the replay stops and every unanswered op waits.
///
/// An op that depends on a suspect is not sent before the suspect is
/// decided, and is set aside with it if it is set aside (see [`Held`]).
///
/// Returns the answers it has, and why it stopped, if it did.
fn isolate(
    remote: &Remote,
    batch: &[crate::oplog::Op],
    first: &str,
    held: &mut Held,
) -> (Vec<crate::oplog::Ack>, Option<ReplayError>) {
    let send = |op: &crate::oplog::Op| -> Result<crate::oplog::Ack, ReplayError> {
        let once = remote.post_ops(std::slice::from_ref(op));
        let twice = match once {
            Err(ReplayError::Failed(_)) => remote.post_ops(std::slice::from_ref(op)),
            other => other,
        };
        twice.map(|mut a| a.remove(0))
    };
    let mut acks = Vec::with_capacity(batch.len());
    let mut suspects: Vec<(usize, String)> = Vec::new();
    let mut deferred: Vec<usize> = Vec::new();
    let mut queue: std::collections::VecDeque<usize> = (0..batch.len()).collect();
    while let Some(i) = queue.pop_front() {
        let op = &batch[i];
        if let Some(by) = held.blocking(op) {
            acks.push(held.hold_behind(op, &by));
            continue;
        }
        let waits_on_suspect = suspects
            .iter()
            .map(|(s, _)| *s)
            .chain(deferred.iter().copied())
            .any(|s| {
                let own = Held::own(&batch[s]);
                Held::needs(op).iter().any(|k| own.contains(k))
            });
        if waits_on_suspect {
            deferred.push(i);
            continue;
        }
        match send(op) {
            Ok(ack) => {
                acks.push(ack);
                // The server answers: each suspect gets one more try.
                for (s, _) in std::mem::take(&mut suspects) {
                    let op = &batch[s];
                    match send(op) {
                        Ok(ack) => acks.push(ack),
                        Err(ReplayError::Failed(e)) => {
                            held.add(op, &short_id(op));
                            acks.push(crate::oplog::Ack {
                                op_id: op.op_id.clone(),
                                result: "rejected".into(),
                                reason: Some(format!(
                                    "{SET_ASIDE}, not answered by the server: the server failed \
                                     on this op ({e}) each time it was sent alone, and answered \
                                     the op sent between those tries, so this op, not the \
                                     server, is what it fails on. It was taken out of the queue \
                                     to let the writes after it through. `deciduous remote push` \
                                     sends it again; `deciduous remote push --drop {}` discards it",
                                    short_id(op)
                                )),
                                at: chrono::Utc::now().to_rfc3339(),
                            });
                        }
                        Err(e) => return (acks, Some(e)),
                    }
                }
                for d in std::mem::take(&mut deferred).into_iter().rev() {
                    queue.push_front(d);
                }
            }
            Err(ReplayError::Failed(e)) => {
                suspects.push((i, e));
                if suspects.len() >= MAX_SUSPECTS {
                    return (
                        acks,
                        Some(ReplayError::Failed(format!(
                            "{first}; sent one at a time, the server failed on {} ops in a row, \
                             each twice, so the server is failing, not one op. Nothing was set \
                             aside; every write not answered waits",
                            suspects.len()
                        ))),
                    );
                }
            }
            Err(e) => return (acks, Some(e)),
        }
    }
    if suspects.is_empty() {
        return (acks, None);
    }
    (
        acks,
        Some(ReplayError::Failed(format!(
            "{first}; sent alone, twice each, the server failed on {} and answered nothing \
             after, so there is no telling whether they or the server are at fault. Nothing \
             was set aside; they wait",
            suspects
                .iter()
                .map(|(s, _)| batch[*s].body.describe())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    )
}

/// Says which lines of the log could not be read, every time the log is
/// replayed, until someone deals with them. They are not sent.
pub fn print_unreadable(state: &crate::oplog::LogState, log: &crate::oplog::OpLog) {
    use colored::Colorize;
    if state.unreadable.is_empty() {
        return;
    }
    eprintln!(
        "{} {} line(s) of {} are not log entries and are not sent (the file is damaged; no server was involved):",
        "Warning:".yellow(),
        state.unreadable.len(),
        log.path().display()
    );
    for u in &state.unreadable {
        eprintln!("  {}", u.describe());
    }
    eprintln!(
        "They are moved to {} the next time the log is compacted. If one is a write you want sent, \
         repair its JSON and append it to the log again; every other line is sent as usual.",
        log.unreadable_path().display()
    );
}

/// Prints the ops a server refused, loudly, with what to do about them,
/// and apart from them the ops this machine set aside, which are real
/// writes the server never judged: the advice for one is wrong for the
/// other.
/// The server's refusals that are over: for everything the refused op
/// wrote, this copy and the server now hold the same thing (a pull took the
/// server's value, git brought the newer one, the same edit reached the
/// server another way). Such a refusal asks nothing of anyone, and kept, it
/// made `remote status` exit 1 while every row matched (round-2 BRIDGE-N10).
/// An op this machine set aside is never settled here: it is a write the
/// server has not seen.
pub fn settled_refusals(
    rejected: &[(crate::oplog::Op, crate::oplog::Ack)],
    nodes: &[crate::db::DecisionNode],
    edges: &[crate::db::DecisionEdge],
    server: &RemoteGraph,
) -> Vec<crate::oplog::Op> {
    use crate::oplog::OpBody;
    let here: std::collections::HashMap<&str, &crate::db::DecisionNode> =
        nodes.iter().map(|n| (n.change_id.as_str(), n)).collect();
    let there: std::collections::HashMap<&str, &RemoteNode> = server
        .nodes
        .iter()
        .filter(|n| n.deleted_at.is_none())
        .map(|n| (n.change_id.as_str(), n))
        .collect();
    let by_id: std::collections::HashMap<i32, &str> =
        nodes.iter().map(|n| (n.id, n.change_id.as_str())).collect();
    let edges_here: std::collections::HashSet<(String, String, String)> = edges
        .iter()
        .filter_map(|e| {
            Some((
                by_id.get(&e.from_node_id)?.to_string(),
                by_id.get(&e.to_node_id)?.to_string(),
                e.edge_type.clone(),
            ))
        })
        .collect();
    let edges_there: std::collections::HashSet<(String, String, String)> = server
        .edges
        .iter()
        .filter_map(|e| {
            let (f, t) = (e.from_change_id.as_deref()?, e.to_change_id.as_deref()?);
            (there.contains_key(f) && there.contains_key(t))
                .then(|| (f.to_string(), t.to_string(), e.edge_type.clone()))
        })
        .collect();
    let same_node = |cid: &str| match (here.get(cid), there.get(cid)) {
        (None, None) => true,
        (Some(n), Some(s)) => node_fields_differ(n, s).is_empty(),
        _ => false,
    };
    let same_edge = |f: &str, t: &str, k: &str| {
        let key = (f.to_string(), t.to_string(), k.to_string());
        edges_here.contains(&key) == edges_there.contains(&key)
    };
    rejected
        .iter()
        .filter(|(_, a)| !a.is_set_aside())
        .filter(|(op, _)| match &op.body {
            OpBody::UpdateNode {
                change_id,
                set,
                metadata,
                ..
            } => match (here.get(change_id.as_str()), there.get(change_id.as_str())) {
                (None, None) => true,
                (Some(n), Some(s)) => {
                    let differ = node_fields_differ(n, s);
                    set.keys()
                        .cloned()
                        .chain(metadata.keys().map(|k| format!("metadata.{k}")))
                        .all(|f| !differ.contains(&f))
                }
                _ => false,
            },
            OpBody::CreateNode { change_id, .. } | OpBody::DeleteNode { change_id, .. } => {
                same_node(change_id)
            }
            OpBody::CreateEdge {
                from_change_id,
                to_change_id,
                edge_type,
                ..
            }
            | OpBody::DeleteEdge {
                from_change_id,
                to_change_id,
                edge_type,
                ..
            } => same_edge(from_change_id, to_change_id, edge_type),
        })
        .map(|(op, _)| op.clone())
        .collect()
}

/// The fields (`title`, `status`, `description`, `metadata.<key>`) in
/// which this copy's node and the server's differ.
fn node_fields_differ(n: &crate::db::DecisionNode, s: &RemoteNode) -> Vec<String> {
    let mut out = Vec::new();
    if n.title != s.title {
        out.push("title".to_string());
    }
    if n.status != s.status {
        out.push("status".to_string());
    }
    if n.description.as_deref().unwrap_or("") != s.description.as_deref().unwrap_or("") {
        out.push("description".to_string());
    }
    if n.node_type != s.node_type {
        out.push("type".to_string());
    }
    let here = metadata_map(n.metadata_json.as_deref());
    let there = s
        .metadata
        .as_ref()
        .and_then(|m| m.as_object().cloned())
        .unwrap_or_default();
    let keys: std::collections::BTreeSet<&String> = here.keys().chain(there.keys()).collect();
    for k in keys {
        if here.get(k) != there.get(k) {
            out.push(format!("metadata.{k}"));
        }
    }
    out
}

fn metadata_map(json: Option<&str>) -> serde_json::Map<String, Value> {
    json.and_then(|m| serde_json::from_str::<Value>(m).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

/// Drops the settled refusals (see [`settled_refusals`]) and says so.
pub fn drop_settled(
    log: &crate::oplog::OpLog,
    db: &Database,
    server: &RemoteGraph,
) -> Result<Vec<crate::oplog::Op>, String> {
    let state = log.read()?;
    if state.rejected.iter().all(|(_, a)| a.is_set_aside()) {
        return Ok(Vec::new());
    }
    let nodes = db
        .get_all_nodes()
        .map_err(|e| format!("reading the local graph: {e}"))?;
    let edges = db
        .get_all_edges()
        .map_err(|e| format!("reading the local graph: {e}"))?;
    let settled = settled_refusals(&state.rejected, &nodes, &edges, server);
    let ids: std::collections::HashSet<String> = settled.iter().map(|o| o.op_id.clone()).collect();
    log.drop_refused(&ids)?;
    Ok(settled)
}

pub fn print_rejected(rejected: &[(crate::oplog::Op, String)], log: &crate::oplog::OpLog) {
    use colored::Colorize;
    let (aside, refused): (Vec<_>, Vec<_>) = rejected
        .iter()
        .partition(|(_, reason)| is_set_aside(reason));
    if !refused.is_empty() {
        eprintln!(
            "{} the server refused {} write(s):",
            "Rejected:".red().bold(),
            refused.len()
        );
        for (op, reason) in &refused {
            eprintln!("  {}  {}", op.body.describe(), reason.dimmed());
        }
        eprintln!(
            "They stay in {} and the local graph keeps them. \
             `deciduous remote status` lists them; `deciduous remote push --drop-rejected` discards them.",
            log.path().display()
        );
        // The one refusal with a fix that is not a choice: the node is gone
        // on the server, so the write can never apply, and pull both removes
        // the node here and drops the refusals that touch it.
        if refused
            .iter()
            .any(|(_, reason)| reason.contains("was deleted on the server"))
        {
            eprintln!(
                "A node these writes touch was deleted on the server: `deciduous remote pull` removes it here and drops the refusals with it."
            );
        }
    }
    if !aside.is_empty() {
        eprintln!(
            "{} {} write(s) were not refused by the server; this machine held them back:",
            "Set aside:".red().bold(),
            aside.len()
        );
        for (op, reason) in &aside {
            eprintln!("  {}  {}", op.body.describe(), reason.dimmed());
        }
        eprintln!(
            "They are writes the server has not got. They stay in {}; `deciduous remote push` \
             sends them again (`--drop <op id>` discards one; `--drop-rejected` keeps them).",
            log.path().display()
        );
    }
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
    replay_after_write_quietly(log, &mut None);
}

/// [`replay_after_write`] for a process that replays after every write for
/// hours (the stdio MCP server): a failure that is the same as the previous
/// replay's is not printed again. Offline, each write printed about 430
/// bytes of the same warning to stderr, and a client that does not read
/// stderr stopped the server after about 150 writes (RUST-N7).
pub fn replay_after_write_quietly(log: &crate::oplog::OpLog, last: &mut Option<String>) {
    use colored::Colorize;

    // The log's own project, not the current directory's: see
    // `Remote::for_data_dir`.
    let data_dir = log.path().parent().unwrap_or(Path::new("."));
    let remote = Remote::for_replay_after_write(data_dir);
    let result = remote
        .as_ref()
        .map_err(|e| ReplayError::Config(e.clone()))
        .and_then(|r| replay(r, log));
    let waiting = || match log.read() {
        Ok(s) => format!("{} write(s)", s.pending.len()),
        Err(e) => format!("an unknown number of writes (the log could not be read: {e})"),
    };
    let unsent = |text: String| crate::oplog::notice(crate::oplog::NoticeKind::Unsent, text);
    match &result {
        Ok(report) => {
            if !report.rejected.is_empty() {
                let aside = report
                    .rejected
                    .iter()
                    .filter(|(_, r)| is_set_aside(r))
                    .count();
                let mut text = format!(
                    "{} earlier write(s) did not reach the shared server ({} refused by it, {} set \
                     aside by this machine because the server failed on them); they were made \
                     here and are not on the server:",
                    report.rejected.len(),
                    report.rejected.len() - aside,
                    aside
                );
                for (op, reason) in &report.rejected {
                    text.push_str(&format!("\n  {}: {reason}", op.body.describe()));
                }
                text.push_str(&format!(
                    "\nThey stay in {}. `deciduous remote status` lists them.",
                    log.path().display()
                ));
                unsent(text);
            }
            if !report.unreadable.is_empty() {
                unsent(format!(
                    "{} line(s) of {} are not log entries and were not sent (the file is damaged): {}",
                    report.unreadable.len(),
                    log.path().display(),
                    report
                        .unreadable
                        .iter()
                        .map(|u| u.describe())
                        .collect::<Vec<_>>()
                        .join("; ")
                ));
            }
        }
        // An outage passes by itself; the writes are queued and go later.
        Err(ReplayError::Unreachable(_)) => {}
        Err(e) => unsent(format!(
            "Writes made here did not reach the shared server, and will not until this is fixed: {e}\n{} wait in {}.",
            waiting(),
            log.path().display()
        )),
    }
    let this = result.as_ref().err().map(|e| e.to_string());
    let repeat = this.is_some() && *last == this;
    *last = this;
    if repeat {
        return;
    }
    match result {
        Ok(report) => print_rejected(&report.rejected, log),
        // Only a server that does not answer is an outage that passes by
        // itself. A refusal (another repository's workspace, a bad token)
        // or a config problem is answered the same way on every retry, and
        // "once the server is reachable" sent people to wait for something
        // that was never going to happen.
        Err(ReplayError::Unreachable(e)) => eprintln!(
            "{} the local write succeeded but the server did not get it: {e}\n\
             {} queued in {}. They are sent on the next write, or now with \
             `deciduous remote push` once the server is reachable.",
            "Warning:".yellow(),
            waiting(),
            log.path().display(),
        ),
        Err(ReplayError::Server(e)) => eprintln!(
            "{} the local write succeeded but the server refused it: {e}\n\
             {} wait in {}, and every later write will be refused the same way \
             until that is fixed. `deciduous remote status` lists them.",
            "Warning:".yellow(),
            waiting(),
            log.path().display(),
        ),
        Err(ReplayError::Failed(e)) => eprintln!(
            "{} the local write succeeded but the server failed on the request: {e}\n\
             {} wait in {}. They are sent again on the next write. An op is set aside only \
             when the server fails on it alone, answers another, and fails on it again.",
            "Warning:".yellow(),
            waiting(),
            log.path().display(),
        ),
        Err(ReplayError::Config(e)) => eprintln!(
            "{} the local write succeeded but could not be sent: {e}\n{} wait in {}.",
            "Warning:".yellow(),
            waiting(),
            log.path().display(),
        ),
        Err(ReplayError::Log(e)) => eprintln!(
            "{} the local write succeeded but the log of writes for the server could not be \
             read or written, so nothing was sent: {e}\n{} wait in {}.",
            "Warning:".yellow(),
            waiting(),
            log.path().display(),
        ),
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
    /// This copy's updated_at is later than the server's: the difference is
    /// most likely an edit made here that never reached the server.
    pub here_newer: bool,
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
                here_newer: records::parse_ts(&n.updated_at) > records::parse_ts(&s.updated_at),
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
    // A local edge into a node the server deleted is not "only here" either:
    // --seed will not send it (see `missing_on_server`), and the pull that
    // removes the node removes it too, so it is listed with the node.
    d.edges_only_local = local_edges
        .difference(&server_edges)
        .filter(|(f, t, _)| !tombstones.contains(f.as_str()) && !tombstones.contains(t.as_str()))
        .map(|(f, t, k)| edge_label(f, t, k))
        .collect();
    d.edges_only_server = server_edges
        .difference(&local_edges)
        .map(|(f, t, k)| edge_label(f, t, k))
        .collect();
    d
}

/// Documents attached here that the server lacks, as "file on node" labels.
pub fn documents_only_here(docs: &[crate::db::NodeDocument], server: &RemoteGraph) -> Vec<String> {
    let have: std::collections::HashSet<&str> = server
        .documents
        .iter()
        .filter_map(|d| d["change_id"].as_str())
        .collect();
    docs.iter()
        .filter(|d| d.detached_at.is_none() && !have.contains(d.change_id.as_str()))
        .map(|d| {
            format!(
                "{} on {}",
                d.original_filename,
                d.node_change_id.chars().take(8).collect::<String>()
            )
        })
        .collect()
}

/// Update ops that make the server's copy of every node both sides hold
/// match this one, field by field: what `remote push --repair` queues.
///
/// This is the remedy for a write whose op never reached the log: made
/// before this clone's config had a `[remote]`, made while the log could not
/// be written, lost to a crash between the local commit and the append, or
/// in a log someone deleted. Each op says what it replaces (the server's
/// current value), so an edit an agent makes between `remote status` and
/// this is refused rather than overwritten.
///
/// Returns the ops and what they cannot carry: a node type (not settable
/// over /ops) and a metadata key the server has and this copy lacks (an op
/// merges keys; it does not remove them).
pub fn repair_ops(
    nodes: &[crate::db::DecisionNode],
    server: &RemoteGraph,
) -> (Vec<crate::oplog::OpBody>, Vec<String>) {
    use serde_json::Map;
    let live: std::collections::HashMap<&str, &RemoteNode> = server
        .nodes
        .iter()
        .filter(|n| n.deleted_at.is_none())
        .map(|n| (n.change_id.as_str(), n))
        .collect();
    let short = |c: &str| c.chars().take(8).collect::<String>();
    let mut ops = Vec::new();
    let mut skipped = Vec::new();
    for n in nodes {
        let Some(s) = live.get(n.change_id.as_str()) else {
            continue;
        };
        let (mut set, mut was) = (Map::new(), Map::new());
        let opt = |v: &Option<String>| v.clone().map(Value::String).unwrap_or(Value::Null);
        if n.title != s.title {
            set.insert("title".into(), Value::String(n.title.clone()));
            was.insert("title".into(), Value::String(s.title.clone()));
        }
        if n.status != s.status {
            set.insert("status".into(), Value::String(n.status.clone()));
            was.insert("status".into(), Value::String(s.status.clone()));
        }
        if n.description.as_deref().unwrap_or("") != s.description.as_deref().unwrap_or("") {
            set.insert("description".into(), opt(&n.description));
            was.insert("description".into(), opt(&s.description));
        }
        if n.node_type != s.node_type {
            skipped.push(format!(
                "{} type: here {}, server {} (a node's type cannot be changed over /ops)",
                short(&n.change_id),
                n.node_type,
                s.node_type
            ));
        }
        let here: Map<String, Value> = n
            .metadata_json
            .as_deref()
            .and_then(|m| serde_json::from_str::<Value>(m).ok())
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        let there = s
            .metadata
            .as_ref()
            .and_then(|m| m.as_object().cloned())
            .unwrap_or_default();
        let (mut meta, mut was_meta) = (Map::new(), Map::new());
        for (k, v) in &here {
            if there.get(k) != Some(v) {
                meta.insert(k.clone(), v.clone());
                was_meta.insert(k.clone(), there.get(k).cloned().unwrap_or(Value::Null));
            }
        }
        for k in there.keys().filter(|k| !here.contains_key(*k)) {
            skipped.push(format!(
                "{} metadata.{k}: only on the server (an op merges keys and cannot remove one)",
                short(&n.change_id)
            ));
        }
        if !set.is_empty() || !meta.is_empty() {
            ops.push(crate::oplog::OpBody::UpdateNode {
                change_id: n.change_id.clone(),
                set,
                metadata: meta,
                was,
                was_metadata: was_meta,
            });
        }
    }
    (ops, skipped)
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
    /// Unlinks the server remembers. Absent from servers before 1.0.8's
    /// second round, which hard-deleted edges.
    #[serde(default)]
    pub edge_tombstones: Vec<RemoteEdgeTombstone>,
}

#[derive(Debug, Deserialize)]
pub struct RemoteEdgeTombstone {
    pub from_change_id: String,
    pub to_change_id: String,
    pub edge_type: String,
    pub deleted_at: String,
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

    // First what git brought (a `git pull` with no `deciduous sync` after
    // it), applied and queued as edits that came through git; then the
    // server's rows, which are not queued: the server has them.
    let git = records::reconcile(db, store, false)?;
    let log = db.oplog();
    db.set_oplog(None);
    let result = pull_server_rows(db, store, &graph, &mut written, log.as_ref());
    db.set_oplog(log.clone());
    let mut report = result?;
    if let Some(log) = &log {
        report.settled = drop_settled(log, db, &graph)?;
    }
    report.imported_nodes += git.nodes_imported;
    report.updated_nodes += git.nodes_updated;
    report.removed_nodes += git.nodes_deleted;
    report.imported_edges += git.edges_imported;
    report.removed_edges += git.edges_deleted;
    Ok(report)
}

fn pull_server_rows(
    db: &Database,
    store: &RecordStore,
    graph: &RemoteGraph,
    written: &mut usize,
    log: Option<&crate::oplog::OpLog>,
) -> Result<PullReport, String> {
    let written_before = *written;

    // Fields the server refused an edit of: the refusal means the server
    // holds something newer, so the pull takes it, whatever the stamps say.
    // A refused edit is the newest thing here, so the node's stamp alone
    // kept it, and the refusal's advice ("`remote pull` takes the server's
    // value") was false (round-2 BRIDGE-N9).
    let refused: std::collections::HashMap<String, std::collections::BTreeSet<String>> = match log {
        Some(log) => refused_fields(&log.read()?.rejected),
        None => Default::default(),
    };

    let result = store.batch(|| -> Result<usize, String> {
        for n in graph.nodes.iter().filter(|n| n.deleted_at.is_none()) {
            let Some(fields) = refused.get(&n.change_id) else {
                continue;
            };
            let Some(mut rec) = store.read_node(&n.change_id).map_err(|e| e.to_string())? else {
                continue;
            };
            if rec.is_tombstone() {
                continue;
            }
            let before = rec.clone();
            for f in fields {
                match f.as_str() {
                    "title" => rec.title = n.title.clone(),
                    "status" => rec.status = n.status.clone(),
                    "description" => rec.description = n.description.clone(),
                    key => {
                        let Some(key) = key.strip_prefix("metadata.") else {
                            continue;
                        };
                        let mut meta = rec
                            .metadata
                            .as_ref()
                            .and_then(|m| m.as_object().cloned())
                            .unwrap_or_default();
                        match n.metadata.as_ref().and_then(|m| m.get(key)) {
                            Some(v) => meta.insert(key.to_string(), v.clone()),
                            None => meta.remove(key),
                        };
                        rec.metadata = Some(Value::Object(meta));
                    }
                }
            }
            if rec != before {
                // Stamped just after the version it replaces, so it is the
                // newer copy for reconcile and for any teammate's merge.
                rec.updated_at = (records::parse_ts(&before.updated_at)
                    + chrono::Duration::milliseconds(1))
                .to_rfc3339();
                store.write_node(&rec).map_err(|e| e.to_string())?;
                *written += 1;
            }
        }

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
                *written += 1;
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
                *written += 1;
            }
        }

        // An unlink on the server (an agent's, or another clone's that
        // reached the server first). Merged like a teammate's tombstone: it
        // removes the edge here unless the edge here was made after it.
        for t in &graph.edge_tombstones {
            let id = records::edge_id(&t.from_change_id, &t.to_change_id, &t.edge_type);
            let Some(mut rec) = store.read_edge(&id).map_err(|e| e.to_string())? else {
                continue;
            };
            if rec.is_tombstone() {
                continue;
            }
            rec.deleted_at = Some(t.deleted_at.clone());
            if store.absorb_edge(&rec).map_err(|e| e.to_string())? {
                *written += 1;
            }
        }

        Ok(*written - written_before)
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
    let overridden = deleted_on_server(&local, graph);
    // The log is detached (see `pull`): applying the server's own delete
    // is not a write the server needs to hear about. Logged, the
    // delete_node and delete_edge ops for it were refused ("was deleted on
    // the server") and left `remote status` reporting rejected writes after
    // every such pull.
    overridden.iter().try_for_each(|d| {
        db.delete_node(d.id as i32, false)
            .map(|_| ())
            .map_err(|e| format!("deleting node {} \"{}\": {e}", d.id, d.title))
    })?;
    for d in &overridden {
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

    let dead: std::collections::HashSet<String> = graph
        .nodes
        .iter()
        .filter(|n| n.deleted_at.is_some())
        .map(|n| n.change_id.clone())
        .collect();
    let dropped_rejected = match log {
        Some(log) if !dead.is_empty() => log.drop_rejected_touching(&dead)?,
        _ => 0,
    };

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
        dropped_rejected,
        settled: Vec::new(),
    })
}

/// change_id -> the fields the server refused an edit of (`title`,
/// `status`, `description`, `metadata.<key>`). Only the server's refusals:
/// an op this machine set aside says nothing about what the server holds.
fn refused_fields(
    rejected: &[(crate::oplog::Op, crate::oplog::Ack)],
) -> std::collections::HashMap<String, std::collections::BTreeSet<String>> {
    let mut out: std::collections::HashMap<String, std::collections::BTreeSet<String>> =
        Default::default();
    for (op, ack) in rejected {
        if ack.is_set_aside() {
            continue;
        }
        if let crate::oplog::OpBody::UpdateNode {
            change_id,
            set,
            metadata,
            ..
        } = &op.body
        {
            let e = out.entry(change_id.clone()).or_default();
            e.extend(set.keys().cloned());
            e.extend(metadata.keys().map(|k| format!("metadata.{k}")));
        }
    }
    out
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
    /// Refused ops in the log that touched a node the server deleted,
    /// dropped because they can never apply.
    pub dropped_rejected: usize,
    /// Refused ops dropped because this copy and the server now agree on
    /// everything they wrote.
    pub settled: Vec<crate::oplog::Op>,
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
            edge_tombstones: vec![],
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
            ops_timeout: OPS_TIMEOUT,
            deadline: None,
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
            ops_timeout: OPS_TIMEOUT,
            deadline: None,
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
            ops_timeout: OPS_TIMEOUT,
            deadline: None,
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
