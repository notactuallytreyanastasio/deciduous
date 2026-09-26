//! The agent message board: how agents working at the same time coordinate.
//!
//! Interface changes, questions and answers between parallel agents go here,
//! never into a scratch markdown file: a file has no ids to reply to, no way
//! to ask "what is addressed to me that I have not answered", and it is lost
//! with the worktree it was written in.
//!
//! Messages are coordination, not graph. They are kept out of everything the
//! graph travels through: not written to `.deciduous/graph.json`, not in
//! `docs/graph-data.json`, not in the op log sent to a server, not synced.
//!
//! Two backends behind one interface, chosen by the project's config:
//!
//! * **local**: the `agent_messages` table of the *main worktree's*
//!   `.deciduous/deciduous.db`. Every linked worktree has a database of its
//!   own (the file is ignored, `.deciduous/` is tracked, and the path is
//!   found by walking up from the working directory), so a board kept beside
//!   the graph would be one board per worktree: agents in `repo-a/` and
//!   `repo-b/` would each post into a room nobody else is in. The graph
//!   database stays per worktree, because `graph.json` is written beside it
//!   and belongs to that worktree's branch. See [`board_db_path`].
//! * **remote**: the project's server, `POST /messages` and `GET /messages`,
//!   when `.deciduous/config.toml` has a `[remote]` url. A server that
//!   answers 404 there predates the board; that is an error naming the
//!   version needed, never a quiet fall back to the local table, which the
//!   other agents (reading the server) would never see.

use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use crate::schema::agent_messages;

/// Server release that first serves `/messages`.
pub const SERVER_VERSION_NEEDED: &str = "1.0.9";

pub const MAX_SUBJECT_CHARS: usize = 300;
pub const MAX_BODY_BYTES: usize = 65_536;
pub const MAX_LABEL_CHARS: usize = 100;
pub const MAX_BRANCH_CHARS: usize = 512;
pub const MAX_QUERY_CHARS: usize = 1000;
pub const DEFAULT_LIMIT: i64 = 50;
pub const MAX_LIMIT: i64 = 200;

/// The table, as `Database::init_schema` and a board-only open both create
/// it. `mentions` is a JSON array of labels.
pub const CREATE_TABLE_SQL: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS agent_messages (
        id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
        branch TEXT,
        author TEXT NOT NULL,
        subject TEXT NOT NULL,
        body TEXT NOT NULL,
        mentions TEXT NOT NULL DEFAULT '[]',
        reply_to INTEGER REFERENCES agent_messages(id),
        created_at TEXT NOT NULL
    )",
    "CREATE INDEX IF NOT EXISTS idx_agent_messages_reply_to ON agent_messages(reply_to)",
    "CREATE INDEX IF NOT EXISTS idx_agent_messages_author ON agent_messages(author)",
];

/// A message as both backends return it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub id: i64,
    pub branch: Option<String>,
    pub author: String,
    pub subject: String,
    pub body: String,
    pub mentions: Vec<String>,
    pub reply_to: Option<i64>,
    pub created_at: String,
}

/// `post_message` output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Posted {
    pub id: i64,
    pub mentions: Vec<String>,
    pub reply_to: Option<i64>,
    pub created_at: String,
}

/// `read_messages` output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadResult {
    pub messages: Vec<Message>,
    pub latest_id: i64,
    pub truncated: bool,
}

/// `post_message` input, minus `workspace`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NewPost {
    pub branch: Option<String>,
    pub author: String,
    pub subject: String,
    pub body: String,
    pub reply_to: Option<i64>,
}

/// `read_messages` filters, minus `workspace`. Every one given must hold.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Filter {
    pub branch: Option<String>,
    pub since_id: Option<i64>,
    pub author: Option<String>,
    pub to: Option<String>,
    pub unanswered_for: Option<String>,
    pub query: Option<String>,
    pub id: Option<i64>,
    pub limit: Option<i64>,
}

// ---------------------------------------------------------------------------
// Mentions
// ---------------------------------------------------------------------------

/// Labels mentioned in `subject` and `body`: `@label`, first appearance
/// first, each once.
///
/// A label is at least two characters, starts and ends with a letter or
/// digit, and may hold `_ . -` inside, so the full stop in "ask @lead." is
/// not part of it. An `@` preceded by a letter, digit, `_` or `.` is an
/// email address, not a mention. The regex crate has no lookbehind, so that
/// check is made by hand on the byte before each match.
pub fn extract_mentions(subject: &str, body: &str) -> Vec<String> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"@([A-Za-z0-9][A-Za-z0-9_.\-]*[A-Za-z0-9])").expect("mention regex")
    });
    let text = format!("{subject} {body}");
    let bytes = text.as_bytes();
    let mut out: Vec<String> = Vec::new();
    for caps in re.captures_iter(&text) {
        let at = caps.get(0).expect("whole match").start();
        if at > 0 {
            let before = bytes[at - 1];
            if before.is_ascii_alphanumeric() || before == b'_' || before == b'.' {
                continue;
            }
        }
        let label = caps[1].to_string();
        if !out.contains(&label) {
            out.push(label);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Validation (the same limits the server enforces)
// ---------------------------------------------------------------------------

/// Whitespace, or characters that print as nothing.
fn blank(s: &str) -> bool {
    s.chars().all(|c| {
        c.is_whitespace() || matches!(c, '\u{200B}'..='\u{200D}' | '\u{2060}' | '\u{FEFF}')
    })
}

fn check_text(field: &str, value: &str, max_chars: usize) -> Result<(), String> {
    if blank(value) {
        return Err(format!("{field} is empty"));
    }
    let n = value.chars().count();
    if n > max_chars {
        return Err(format!(
            "{field} is {n} characters; at most {max_chars} are allowed"
        ));
    }
    Ok(())
}

impl NewPost {
    pub fn validate(&self) -> Result<(), String> {
        check_text("author", &self.author, MAX_LABEL_CHARS)?;
        check_text("subject", &self.subject, MAX_SUBJECT_CHARS)?;
        if blank(&self.body) {
            return Err("body is empty".into());
        }
        if self.body.len() > MAX_BODY_BYTES {
            return Err(format!(
                "body is {} bytes; at most {MAX_BODY_BYTES} (64 KiB) are allowed",
                self.body.len()
            ));
        }
        if let Some(b) = &self.branch {
            if b.chars().count() > MAX_BRANCH_CHARS {
                return Err(format!(
                    "branch is longer than {MAX_BRANCH_CHARS} characters"
                ));
            }
        }
        if let Some(n) = self.reply_to.filter(|n| *n < 1) {
            return Err(format!("reply_to must be 1 or more, not {n}"));
        }
        Ok(())
    }
}

impl Filter {
    pub fn validate(&self) -> Result<(), String> {
        for (field, v) in [
            ("author", &self.author),
            ("to", &self.to),
            ("unanswered_for", &self.unanswered_for),
        ] {
            if let Some(v) = v {
                check_text(field, v, MAX_LABEL_CHARS)?;
            }
        }
        if let Some(q) = &self.query {
            check_text("query", q, MAX_QUERY_CHARS)?;
        }
        if let Some(l) = self.limit {
            if !(1..=MAX_LIMIT).contains(&l) {
                return Err(format!("limit must be between 1 and {MAX_LIMIT}, not {l}"));
            }
        }
        if let Some(n) = self.since_id.filter(|n| *n < 0) {
            return Err(format!("since_id must be 0 or more, not {n}"));
        }
        if let Some(n) = self.id.filter(|n| *n < 1) {
            return Err(format!("id must be 1 or more, not {n}"));
        }
        if let Some(b) = self
            .branch
            .as_deref()
            .filter(|b| b.chars().count() > MAX_BRANCH_CHARS)
        {
            return Err(format!(
                "branch is {} characters; at most {MAX_BRANCH_CHARS} are allowed",
                b.chars().count()
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Where the local board lives
// ---------------------------------------------------------------------------

fn git_out(dir: &Path, args: &[&str]) -> Option<String> {
    std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
}

/// The database the local board is kept in, for a graph database at
/// `graph_db`.
///
/// The graph database itself when it is not inside a linked worktree. When
/// it is, the same place in the main worktree: `wt/.deciduous/deciduous.db`
/// becomes `main/.deciduous/deciduous.db` (and `wt/sub/.deciduous/...`
/// becomes `main/sub/.deciduous/...`), provided the main worktree has that
/// `.deciduous/` directory. So every worktree of one repository shares one
/// board, whether it reached its database by walking up or through
/// `DECIDUOUS_DB_PATH`.
pub fn board_db_path(graph_db: &Path) -> PathBuf {
    let abs = std::path::absolute(graph_db).unwrap_or_else(|_| graph_db.to_path_buf());
    let Some(data_dir) = abs.parent() else {
        return abs;
    };
    let Some(project) = data_dir.parent() else {
        return abs;
    };
    if !project.is_dir() {
        return abs;
    }
    let (Some(top), Some(main)) = (
        git_out(
            project,
            &["rev-parse", "--path-format=absolute", "--show-toplevel"],
        ),
        crate::remote::repo_root(project),
    ) else {
        return abs;
    };
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let (top, main) = (canon(Path::new(&top)), canon(&main));
    if top == main {
        return abs;
    }
    let Ok(rel) = canon(data_dir).strip_prefix(&top).map(Path::to_path_buf) else {
        return abs;
    };
    let target_dir = main.join(rel);
    if !target_dir.is_dir() {
        return abs;
    }
    target_dir.join(abs.file_name().unwrap_or_else(|| "deciduous.db".as_ref()))
}

// ---------------------------------------------------------------------------
// Local backend
// ---------------------------------------------------------------------------

#[derive(Queryable, Selectable, Debug)]
#[diesel(table_name = agent_messages)]
struct Row {
    id: i64,
    branch: Option<String>,
    author: String,
    subject: String,
    body: String,
    mentions: String,
    reply_to: Option<i64>,
    created_at: String,
}

impl From<Row> for Message {
    fn from(r: Row) -> Self {
        Message {
            id: r.id,
            branch: r.branch,
            author: r.author,
            subject: r.subject,
            body: r.body,
            mentions: serde_json::from_str(&r.mentions).unwrap_or_default(),
            reply_to: r.reply_to,
            created_at: r.created_at,
        }
    }
}

#[derive(Insertable)]
#[diesel(table_name = agent_messages)]
struct NewRow<'a> {
    branch: Option<&'a str>,
    author: &'a str,
    subject: &'a str,
    body: &'a str,
    mentions: &'a str,
    reply_to: Option<i64>,
    created_at: &'a str,
}

/// The board in a SQLite database.
pub struct LocalBoard {
    conn: SqliteConnection,
    path: PathBuf,
}

/// Creates the table and its indexes on `conn` if they are missing.
pub fn create_table(conn: &mut SqliteConnection) -> diesel::QueryResult<()> {
    for sql in CREATE_TABLE_SQL {
        diesel::sql_query(*sql).execute(conn)?;
    }
    Ok(())
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

/// `%`, `_` and the escape itself taken literally in a LIKE pattern.
fn like_pattern(q: &str) -> String {
    let mut s = String::from("%");
    for c in q.chars() {
        if matches!(c, '%' | '_' | '\\') {
            s.push('\\');
        }
        s.push(c);
    }
    s.push('%');
    s
}

impl LocalBoard {
    /// Opens (creating if need be) the board in the database at `path`.
    pub fn open_at(path: &Path) -> Result<Self, String> {
        use diesel::connection::SimpleConnection;
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("creating {}: {e}", parent.display()))?;
            }
        }
        let mut conn = SqliteConnection::establish(&path.to_string_lossy())
            .map_err(|e| format!("opening the board at {}: {e}", path.display()))?;
        conn.batch_execute(&format!(
            "PRAGMA busy_timeout = {}; PRAGMA journal_mode = WAL;",
            crate::db::BUSY_TIMEOUT_MS
        ))
        .map_err(|e| format!("opening the board at {}: {e}", path.display()))?;
        create_table(&mut conn)
            .map_err(|e| format!("creating the board table in {}: {e}", path.display()))?;
        Ok(Self {
            conn,
            path: path.to_path_buf(),
        })
    }

    /// The board shared by every worktree of the project whose graph
    /// database is `graph_db`.
    pub fn for_graph_db(graph_db: &Path) -> Result<Self, String> {
        Self::open_at(&board_db_path(graph_db))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn post(&mut self, p: &NewPost) -> Result<Posted, String> {
        p.validate()?;
        let mentions = extract_mentions(&p.subject, &p.body);
        let mentions_json = serde_json::to_string(&mentions).unwrap_or_else(|_| "[]".into());
        let created_at = now();
        let branch = p.branch.as_deref().filter(|b| !b.is_empty());
        let id = self
            .conn
            .immediate_transaction::<i64, diesel::result::Error, _>(|conn| {
                if let Some(r) = p.reply_to {
                    let exists: i64 = agent_messages::table
                        .filter(agent_messages::id.eq(r))
                        .count()
                        .get_result(conn)?;
                    if exists == 0 {
                        return Err(diesel::result::Error::NotFound);
                    }
                }
                diesel::insert_into(agent_messages::table)
                    .values(NewRow {
                        branch,
                        author: &p.author,
                        subject: &p.subject,
                        body: &p.body,
                        mentions: &mentions_json,
                        reply_to: p.reply_to,
                        created_at: &created_at,
                    })
                    .execute(conn)?;
                diesel::select(diesel::dsl::sql::<diesel::sql_types::BigInt>(
                    "last_insert_rowid()",
                ))
                .first(conn)
            })
            .map_err(|e| match e {
                diesel::result::Error::NotFound => format!(
                    "reply_to {}: no such message on this board; nothing was posted",
                    p.reply_to.unwrap_or_default()
                ),
                e => format!("posting to the board: {e}"),
            })?;
        Ok(Posted {
            id,
            mentions,
            reply_to: p.reply_to,
            created_at,
        })
    }

    pub fn read(&mut self, f: &Filter) -> Result<ReadResult, String> {
        use diesel::dsl::sql;
        use diesel::sql_types::{Bool, Text};
        f.validate()?;
        let limit = f.limit.unwrap_or(DEFAULT_LIMIT);
        let mut q = agent_messages::table
            .select(Row::as_select())
            .order(agent_messages::id.asc())
            .into_boxed();
        if let Some(b) = &f.branch {
            q = q.filter(agent_messages::branch.eq(b.clone()));
        }
        if let Some(s) = f.since_id {
            q = q.filter(agent_messages::id.gt(s));
        }
        if let Some(a) = &f.author {
            q = q.filter(agent_messages::author.eq(a.clone()));
        }
        if let Some(id) = f.id {
            q = q.filter(agent_messages::id.eq(id));
        }
        if let Some(to) = &f.to {
            q = q.filter(
                sql::<Bool>("EXISTS (SELECT 1 FROM json_each(agent_messages.mentions) WHERE json_each.value = ")
                    .bind::<Text, _>(to.clone())
                    .sql(")"),
            );
        }
        if let Some(who) = &f.unanswered_for {
            q = q.filter(
                sql::<Bool>("EXISTS (SELECT 1 FROM json_each(agent_messages.mentions) WHERE json_each.value = ")
                    .bind::<Text, _>(who.clone())
                    .sql(") AND NOT EXISTS (SELECT 1 FROM agent_messages AS r WHERE r.reply_to = agent_messages.id AND r.author = ")
                    .bind::<Text, _>(who.clone())
                    .sql(")"),
            );
        }
        if let Some(text) = &f.query {
            q = q.filter(
                sql::<Bool>("(agent_messages.subject || ' ' || agent_messages.body) LIKE ")
                    .bind::<Text, _>(like_pattern(text))
                    .sql(" ESCAPE '\\'"),
            );
        }
        let mut rows: Vec<Row> = q
            .limit(limit + 1)
            .load(&mut self.conn)
            .map_err(|e| format!("reading the board: {e}"))?;
        let truncated = rows.len() as i64 > limit;
        rows.truncate(limit as usize);
        let messages: Vec<Message> = rows.into_iter().map(Message::from).collect();
        let latest_id = messages
            .last()
            .map(|m| m.id)
            .unwrap_or(f.since_id.unwrap_or(0));
        Ok(ReadResult {
            messages,
            latest_id,
            truncated,
        })
    }
}

// ---------------------------------------------------------------------------
// One interface over both backends
// ---------------------------------------------------------------------------

pub enum Board {
    Local(LocalBoard),
    Remote(crate::remote::Remote),
}

impl Board {
    /// The board of the project whose graph database is `graph_db`: its
    /// server when `config.toml` beside the database names one, otherwise
    /// the local table shared by all its worktrees.
    pub fn open_for(graph_db: &Path) -> Result<Self, String> {
        let abs = std::path::absolute(graph_db).unwrap_or_else(|_| graph_db.to_path_buf());
        let data_dir = abs.parent().unwrap_or(Path::new("."));
        let config = crate::remote::config_at(data_dir)?;
        if config.remote.url.is_some() {
            return crate::remote::Remote::for_data_dir(data_dir).map(Board::Remote);
        }
        LocalBoard::for_graph_db(&abs).map(Board::Local)
    }

    /// The workspace this board belongs to: the configured one, else the
    /// repository's name (the main worktree's, from a linked one).
    pub fn workspace(&self, graph_db: &Path) -> String {
        match self {
            Board::Remote(r) => r.workspace.clone(),
            Board::Local(_) => {
                let abs = std::path::absolute(graph_db).unwrap_or_else(|_| graph_db.to_path_buf());
                let project = abs
                    .parent()
                    .and_then(Path::parent)
                    .unwrap_or(Path::new("."))
                    .to_path_buf();
                crate::remote::workspace_for(&project)
            }
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Board::Local(l) => format!("local board in {}", l.path().display()),
            Board::Remote(r) => format!("board on {} (workspace {})", r.url, r.workspace),
        }
    }

    pub fn post(&mut self, p: &NewPost) -> Result<Posted, String> {
        match self {
            Board::Local(l) => l.post(p),
            Board::Remote(r) => {
                p.validate()?;
                let mut body = json!({
                    "workspace": r.workspace,
                    "author": p.author,
                    "subject": p.subject,
                    "body": p.body,
                });
                if let Some(b) = &p.branch {
                    body["branch"] = json!(b);
                }
                if let Some(id) = p.reply_to {
                    body["reply_to"] = json!(id);
                }
                let v = r.board_post(&body)?;
                serde_json::from_value(v).map_err(|e| {
                    format!(
                        "the server's answer to POST /messages was not a post_message result: {e}"
                    )
                })
            }
        }
    }

    pub fn read(&mut self, f: &Filter) -> Result<ReadResult, String> {
        match self {
            Board::Local(l) => l.read(f),
            Board::Remote(r) => {
                f.validate()?;
                let mut params: Vec<(&str, String)> = vec![("workspace", r.workspace.clone())];
                let mut opt = |k: &'static str, v: Option<String>| {
                    if let Some(v) = v {
                        params.push((k, v));
                    }
                };
                opt("branch", f.branch.clone());
                opt("since_id", f.since_id.map(|n| n.to_string()));
                opt("author", f.author.clone());
                opt("to", f.to.clone());
                opt("unanswered_for", f.unanswered_for.clone());
                opt("query", f.query.clone());
                opt("id", f.id.map(|n| n.to_string()));
                opt("limit", f.limit.map(|n| n.to_string()));
                let v = r.board_read(&params)?;
                serde_json::from_value(v).map_err(|e| {
                    format!(
                        "the server's answer to GET /messages was not a read_messages result: {e}"
                    )
                })
            }
        }
    }
}

// ---------------------------------------------------------------------------
// CLI output
// ---------------------------------------------------------------------------

/// The environment variable naming this session's label: the default for
/// `board post --as`, and what the session-start hook reads mentions for.
pub const LABEL_ENV: &str = "DECIDUOUS_AGENT_LABEL";

/// Lines of a body `board read` prints before pointing at `board show`.
pub const READ_BODY_LINES: usize = 12;

pub fn render_posted(p: &Posted) -> String {
    let mut s = format!("Posted #{}", p.id);
    if let Some(r) = p.reply_to {
        s.push_str(&format!(" (reply to #{r})"));
    }
    if p.mentions.is_empty() {
        s.push_str(", addressed to nobody (no @label in subject or body)");
    } else {
        s.push_str(&format!(", to {}", p.mentions.join(", ")));
    }
    s
}

/// One message: a header line, the subject, the body indented. At most
/// `body_lines` lines of body when given.
pub fn render_message(m: &Message, body_lines: Option<usize>) -> String {
    let mut head = format!("#{}  {}", m.id, m.author);
    if !m.mentions.is_empty() {
        head.push_str(&format!(" -> {}", m.mentions.join(", ")));
    }
    if let Some(r) = m.reply_to {
        head.push_str(&format!("  re #{r}"));
    }
    if let Some(b) = &m.branch {
        head.push_str(&format!("  [{b}]"));
    }
    head.push_str(&format!("  {}", m.created_at));
    let mut out = format!("{head}\n    {}\n", m.subject);
    let lines: Vec<&str> = m.body.trim_end().lines().collect();
    let shown = body_lines.unwrap_or(lines.len()).min(lines.len());
    for l in &lines[..shown] {
        out.push_str(&format!("    | {l}\n"));
    }
    if shown < lines.len() {
        out.push_str(&format!(
            "    | ... {} more lines: deciduous board show {}\n",
            lines.len() - shown,
            m.id
        ));
    }
    out
}

pub fn render_read(r: &ReadResult, f: &Filter) -> String {
    if r.messages.is_empty() {
        return match &f.unanswered_for {
            Some(who) => format!("Nothing addressed to {who} is waiting for an answer.\n"),
            None => "No messages.\n".to_string(),
        };
    }
    let mut out = String::new();
    for m in &r.messages {
        out.push_str(&render_message(m, Some(READ_BODY_LINES)));
        out.push('\n');
    }
    if r.truncated {
        out.push_str(&format!(
            "More messages match: rerun with --since {}\n",
            r.latest_id
        ));
    }
    out
}

// ---------------------------------------------------------------------------
// MCP arguments (shared by the stdio server and tests)
// ---------------------------------------------------------------------------

fn arg_str(args: &Value, key: &str) -> Result<Option<String>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(other) => Err(format!("{key} must be a string, not {other}")),
    }
}

fn arg_int(args: &Value, key: &str) -> Result<Option<i64>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_i64()
            .map(Some)
            .ok_or_else(|| format!("{key} must be an integer, not {v}")),
    }
}

impl NewPost {
    pub fn from_args(args: &Value) -> Result<Self, String> {
        Ok(NewPost {
            branch: arg_str(args, "branch")?,
            author: arg_str(args, "author")?.unwrap_or_default(),
            subject: arg_str(args, "subject")?.unwrap_or_default(),
            body: arg_str(args, "body")?.unwrap_or_default(),
            reply_to: arg_int(args, "reply_to")?,
        })
    }
}

impl Filter {
    pub fn from_args(args: &Value) -> Result<Self, String> {
        Ok(Filter {
            branch: arg_str(args, "branch")?,
            since_id: arg_int(args, "since_id")?,
            author: arg_str(args, "author")?,
            to: arg_str(args, "to")?,
            unanswered_for: arg_str(args, "unanswered_for")?,
            query: arg_str(args, "query")?,
            id: arg_int(args, "id")?,
            limit: arg_int(args, "limit")?,
        })
    }
}

/// Refuses a `workspace` argument that names another project than this
/// board's. The argument exists so the tool reads the same on the server
/// and here; a local board has exactly one workspace, and posting a message
/// meant for another project into this one would be silently wrong.
pub fn check_workspace(args: &Value, board_workspace: &str) -> Result<(), String> {
    match arg_str(args, "workspace")? {
        None => Ok(()),
        Some(w) if w == "*" => Err(
            "workspace \"*\" is refused for messages: labels are not unique across projects".into(),
        ),
        Some(w) if w.eq_ignore_ascii_case(board_workspace) => Ok(()),
        Some(w) => Err(format!(
            "workspace {w:?} is not this project's; this board belongs to {board_workspace:?}"
        )),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn board() -> (tempfile::TempDir, LocalBoard) {
        let t = tempfile::TempDir::new().unwrap();
        let b = LocalBoard::open_at(&t.path().join(".deciduous/deciduous.db")).unwrap();
        (t, b)
    }

    fn post(
        b: &mut LocalBoard,
        author: &str,
        subject: &str,
        body: &str,
        reply: Option<i64>,
    ) -> i64 {
        b.post(&NewPost {
            author: author.into(),
            subject: subject.into(),
            body: body.into(),
            reply_to: reply,
            branch: None,
        })
        .unwrap()
        .id
    }

    fn ids(r: &ReadResult) -> Vec<i64> {
        r.messages.iter().map(|m| m.id).collect()
    }

    #[test]
    fn mentions_trailing_punctuation_is_not_part_of_the_label() {
        assert_eq!(extract_mentions("ask @lead.", ""), ["lead"]);
        assert_eq!(
            extract_mentions("", "hi @A-retrieval- and @b_2,"),
            ["A-retrieval", "b_2"]
        );
        assert_eq!(
            extract_mentions("", "(@x.y) @board-cli: yes"),
            ["x.y", "board-cli"]
        );
        assert_eq!(extract_mentions("", "@lead"), ["lead"]);
    }

    #[test]
    fn mentions_emails_are_not_mentions() {
        assert!(extract_mentions("", "mail bob@example.com or a.b@c.io").is_empty());
        assert!(extract_mentions("", "x_@lead").is_empty());
        assert_eq!(
            extract_mentions("", "mail bob@example.com, @lead"),
            ["lead"]
        );
    }

    #[test]
    fn mentions_single_character_is_not_a_label() {
        assert!(extract_mentions("@A", "@a @-").is_empty());
    }

    #[test]
    fn mentions_dedup_in_order_of_first_appearance_subject_first() {
        assert_eq!(
            extract_mentions("@bb to @aa", "@cc @aa @bb @cc @dd"),
            ["bb", "aa", "cc", "dd"]
        );
    }

    #[test]
    fn post_returns_mentions_and_a_utc_timestamp() {
        let (_t, mut b) = board();
        let p = b
            .post(&NewPost {
                author: "lead".into(),
                subject: "for @w1".into(),
                body: "and @w2".into(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(p.id, 1);
        assert_eq!(p.mentions, ["w1", "w2"]);
        assert!(
            p.created_at.ends_with('Z') && p.created_at.len() == 27,
            "{}",
            p.created_at
        );
    }

    #[test]
    fn post_refuses_empty_and_over_length_fields() {
        let (_t, mut b) = board();
        let ok = NewPost {
            author: "a".into(),
            subject: "s".into(),
            body: "b".into(),
            ..Default::default()
        };
        for (p, want) in [
            (
                NewPost {
                    author: " ".into(),
                    ..ok.clone()
                },
                "author is empty",
            ),
            (
                NewPost {
                    subject: "\u{200B}".into(),
                    ..ok.clone()
                },
                "subject is empty",
            ),
            (
                NewPost {
                    body: "\n".into(),
                    ..ok.clone()
                },
                "body is empty",
            ),
            (
                NewPost {
                    subject: "x".repeat(301),
                    ..ok.clone()
                },
                "at most 300",
            ),
            (
                NewPost {
                    body: "x".repeat(65_537),
                    ..ok.clone()
                },
                "65536",
            ),
            (
                NewPost {
                    author: "x".repeat(101),
                    ..ok.clone()
                },
                "at most 100",
            ),
        ] {
            let e = b.post(&p).unwrap_err();
            assert!(e.contains(want), "{e}");
        }
        assert!(b
            .post(&NewPost {
                subject: "é".repeat(300),
                ..ok.clone()
            })
            .is_ok());
        assert!(b
            .post(&NewPost {
                body: "x".repeat(65_536),
                ..ok
            })
            .is_ok());
    }

    #[test]
    fn reply_to_a_nonexistent_id_is_refused_and_nothing_is_posted() {
        let (_t, mut b) = board();
        let e = b
            .post(&NewPost {
                author: "a".into(),
                subject: "s".into(),
                body: "b".into(),
                reply_to: Some(42),
                ..Default::default()
            })
            .unwrap_err();
        assert!(e.contains("reply_to 42: no such message"), "{e}");
        assert!(b.read(&Filter::default()).unwrap().messages.is_empty());
    }

    #[test]
    fn read_every_filter() {
        let (_t, mut b) = board();
        let m1 = post(&mut b, "lead", "interface", "@w1 @w2 build to this", None);
        let m2 = post(&mut b, "w1", "question", "@lead is 404 fatal?", Some(m1));
        let m3 = post(&mut b, "w2", "Done", "all green, 100% of tests", None);
        b.post(&NewPost {
            author: "w2".into(),
            subject: "branch only".into(),
            body: "x".into(),
            branch: Some("feat".into()),
            ..Default::default()
        })
        .unwrap();

        let mut r = |f: Filter| ids(&b_read(&mut b, f));
        fn b_read(b: &mut LocalBoard, f: Filter) -> ReadResult {
            b.read(&f).unwrap()
        }
        assert_eq!(r(Filter::default()), [1, 2, 3, 4]);
        assert_eq!(
            r(Filter {
                since_id: Some(m2),
                ..Default::default()
            }),
            [3, 4]
        );
        assert_eq!(
            r(Filter {
                author: Some("w2".into()),
                ..Default::default()
            }),
            [3, 4]
        );
        assert_eq!(
            r(Filter {
                author: Some("W2".into()),
                ..Default::default()
            }),
            Vec::<i64>::new()
        );
        assert_eq!(
            r(Filter {
                to: Some("w2".into()),
                ..Default::default()
            }),
            [m1]
        );
        assert_eq!(
            r(Filter {
                to: Some("lead".into()),
                ..Default::default()
            }),
            [m2]
        );
        assert_eq!(
            r(Filter {
                query: Some("FATAL".into()),
                ..Default::default()
            }),
            [m2]
        );
        assert_eq!(
            r(Filter {
                query: Some("interface".into()),
                ..Default::default()
            }),
            [m1]
        );
        // % is literal, not a wildcard
        assert_eq!(
            r(Filter {
                query: Some("100%".into()),
                ..Default::default()
            }),
            [m3]
        );
        assert_eq!(
            r(Filter {
                query: Some("%".into()),
                ..Default::default()
            }),
            [m3]
        );
        assert_eq!(
            r(Filter {
                id: Some(m3),
                ..Default::default()
            }),
            [m3]
        );
        assert_eq!(
            r(Filter {
                id: Some(99),
                ..Default::default()
            }),
            Vec::<i64>::new()
        );
        assert_eq!(
            r(Filter {
                branch: Some("feat".into()),
                ..Default::default()
            }),
            [4]
        );
        assert_eq!(
            r(Filter {
                author: Some("w2".into()),
                since_id: Some(3),
                ..Default::default()
            }),
            [4]
        );

        let page = b
            .read(&Filter {
                limit: Some(2),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            (ids(&page), page.latest_id, page.truncated),
            (vec![1, 2], 2, true)
        );
        let rest = b
            .read(&Filter {
                limit: Some(2),
                since_id: Some(page.latest_id),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            (ids(&rest), rest.latest_id, rest.truncated),
            (vec![3, 4], 4, false)
        );
        let none = b
            .read(&Filter {
                since_id: Some(4),
                ..Default::default()
            })
            .unwrap();
        assert_eq!((none.latest_id, none.truncated), (4, false));
        let empty = b
            .read(&Filter {
                id: Some(99),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(empty.latest_id, 0);

        for bad in [0, 201, -1] {
            let e = b
                .read(&Filter {
                    limit: Some(bad),
                    ..Default::default()
                })
                .unwrap_err();
            assert!(e.contains("between 1 and 200"), "{e}");
        }
    }

    #[test]
    fn unanswered_for_is_mentioned_and_not_replied_to_by_that_label() {
        let (_t, mut b) = board();
        let q1 = post(&mut b, "lead", "q1", "@w1 @w2 which port?", None);
        let q2 = post(&mut b, "lead", "q2", "@w1 which db?", None);
        // w2 answers q1; someone else answering q2 does not count for w1.
        post(&mut b, "w2", "re q1", "4000", Some(q1));
        post(&mut b, "w2", "re q2", "sqlite, but @w1 owns it", Some(q2));
        let un = |b: &mut LocalBoard, who: &str| {
            ids(&b
                .read(&Filter {
                    unanswered_for: Some(who.into()),
                    ..Default::default()
                })
                .unwrap())
        };
        assert_eq!(un(&mut b, "w1"), [q1, q2, 4]);
        assert_eq!(un(&mut b, "w2"), Vec::<i64>::new());
        // A reply by w1 to q1 closes only q1; a post by w1 that is not a
        // reply closes nothing.
        post(&mut b, "w1", "note", "unrelated", None);
        post(&mut b, "w1", "re q1", "port 4000 then", Some(q1));
        assert_eq!(un(&mut b, "w1"), [q2, 4]);
        // Replying to a message does not answer the one it replied to.
        post(&mut b, "w1", "re re", "ok", Some(4));
        assert_eq!(un(&mut b, "w1"), [q2]);
    }

    #[test]
    fn mentions_round_trip_through_the_table() {
        let (_t, mut b) = board();
        post(&mut b, "a", "@xx", "@yy @xx", None);
        let m = &b.read(&Filter::default()).unwrap().messages[0];
        assert_eq!(m.mentions, ["xx", "yy"]);
        assert_eq!(m.branch, None);
    }

    #[test]
    fn check_workspace_accepts_this_project_only() {
        assert!(check_workspace(&json!({}), "deciduous").is_ok());
        assert!(check_workspace(&json!({"workspace": "Deciduous"}), "deciduous").is_ok());
        assert!(check_workspace(&json!({"workspace": "blog"}), "deciduous")
            .unwrap_err()
            .contains("not this project's"));
        assert!(check_workspace(&json!({"workspace": "*"}), "deciduous").is_err());
    }

    #[test]
    fn args_of_the_wrong_type_are_refused() {
        assert!(Filter::from_args(&json!({"since_id": "3"}))
            .unwrap_err()
            .contains("integer"));
        assert!(NewPost::from_args(&json!({"author": 3}))
            .unwrap_err()
            .contains("string"));
    }

    #[test]
    fn a_path_outside_any_repository_is_its_own_board() {
        let t = tempfile::TempDir::new().unwrap();
        let db = t.path().join(".deciduous/deciduous.db");
        std::fs::create_dir_all(db.parent().unwrap()).unwrap();
        assert_eq!(board_db_path(&db), db);
    }
}
