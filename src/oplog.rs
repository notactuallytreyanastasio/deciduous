//! The local log of writes on their way to the shared server.
//!
//! Every graph write the CLI makes to its local database also appends one
//! operation to `.deciduous/remote-log.jsonl`, next to the database, when the
//! project has a `[remote]` configured. `deciduous remote push` (and the
//! automatic replay after each write) sends the unacknowledged tail to the
//! server's `POST /ops`, which applies each op at most once, by `op_id`, and
//! only to the fields the op names.
//!
//! ## Why a log and not a snapshot
//!
//! 1.0.7 pushed by comparing the local graph with the server's and sending
//! whole nodes. That has two holes a log does not:
//!
//! * A snapshot cannot say which fields changed, so re-sending node 2 after
//!   `deciduous status 2 completed` also re-sent its title, and put back the
//!   one an agent had just changed.
//! * A diff by change_id only sees what the server lacks. An edit to a node
//!   the server already has, made while the server was down, was never
//!   found again; neither was a delete.
//!
//! ## The file
//!
//! JSON lines, append-only, one entry per line:
//!
//! ```text
//! {"entry":"op","op_id":"…","at":"…","kind":"update_node","change_id":"…","set":{"status":"completed"}}
//! {"entry":"ack","op_id":"…","result":"applied","at":"…"}
//! ```
//!
//! An op with no ack is pending. An ack whose result is `rejected` keeps its op
//! in the file, with the server's reason, until the user drops it
//! (`remote push --drop-rejected`): the server refused it and something has to
//! decide what that means.
//!
//! It lives beside the database, not in a tracked file: it holds this
//! machine's unsent writes, and the `.deciduous/*` rule `deciduous init`
//! writes already keeps it out of git.
//!
//! ## A line that cannot be read
//!
//! Every append is one `write` of whole lines ending in a newline, so a last
//! line with no newline can only be an append that was cut short: a crash, a
//! kill, a full disk. The next append used to land on that same line, and
//! the pair stopped parsing as one: the new write was lost with the torn
//! one, and the advice printed ("delete that line") finished the job.
//!
//! So the next append, under the lock, first moves a torn tail to
//! `remote-log.unreadable` beside the log (or, if the tail is a whole entry
//! that lost only its newline, gives it one). Reading tolerates a line it
//! cannot parse: it is reported with its number and text on every replay and
//! by `remote status`, and never sent; compaction moves it to the same side
//! file. A line an older build already glued together (fragment, then a
//! whole entry) still yields that entry. Nothing unreadable is deleted: the
//! side file is for a person to look at, and `remote status` exits 1 while
//! it has anything in it.
//!
//! ## Compaction
//!
//! After a replay, ops acknowledged as anything other than `rejected` are
//! removed by rewriting the file (temporary file, then rename). The log is
//! therefore as long as what is waiting, not as long as the project's history.
//!
//! ## Committed with the local write, appended after it
//!
//! A write's op is made before its commit and stored in the database's
//! `remote_outbox` table in the same transaction as the write, then appended
//! here once the commit is done. It used to be built and appended only after
//! the commit, with nothing durable in between: a process killed there (an
//! MCP client closing its server, SIGTERM at shutdown) left a write with no
//! op, and a lost delete was undone by the next `remote pull`. Now the next
//! logged write, or the next process to open the database, moves whatever
//! the outbox still holds into this file, in commit order, skipping an op
//! the file already has.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "remote-log.jsonl";
/// Where lines of the log that could not be read are moved. See the module
/// docs.
pub const UNREADABLE_FILE: &str = "remote-log.unreadable";

/// One graph change, as the server's `POST /ops` receives it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Op {
    pub op_id: String,
    /// When the local write happened.
    pub at: String,
    #[serde(flatten)]
    pub body: OpBody,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OpBody {
    CreateNode {
        change_id: String,
        node_type: String,
        title: String,
        #[serde(default)]
        description: Option<String>,
        status: String,
        #[serde(default)]
        metadata: Map<String, Value>,
        created_at: String,
        updated_at: String,
    },
    /// Only the fields named here change on the server. `set` holds
    /// top-level columns (title, description, status); `metadata` holds keys
    /// merged into the node's metadata map.
    ///
    /// `was` and `was_metadata` hold, for every field named, the value this
    /// edit replaced (null for a key that was absent). The server writes a
    /// field only while it still holds that value, so an op that waited in
    /// the queue cannot put back an older value over an edit made since.
    UpdateNode {
        change_id: String,
        #[serde(default, skip_serializing_if = "Map::is_empty")]
        set: Map<String, Value>,
        #[serde(default, skip_serializing_if = "Map::is_empty")]
        metadata: Map<String, Value>,
        #[serde(default, skip_serializing_if = "Map::is_empty")]
        was: Map<String, Value>,
        #[serde(default, skip_serializing_if = "Map::is_empty")]
        was_metadata: Map<String, Value>,
    },
    DeleteNode {
        change_id: String,
    },
    CreateEdge {
        from_change_id: String,
        to_change_id: String,
        edge_type: String,
        #[serde(default)]
        rationale: Option<String>,
        #[serde(default)]
        weight: Option<f64>,
        created_at: String,
    },
    DeleteEdge {
        from_change_id: String,
        to_change_id: String,
        edge_type: String,
    },
}

impl OpBody {
    /// The nodes this op writes to or links, by change_id.
    pub fn change_ids(&self) -> Vec<&str> {
        match self {
            OpBody::CreateNode { change_id, .. }
            | OpBody::UpdateNode { change_id, .. }
            | OpBody::DeleteNode { change_id } => vec![change_id],
            OpBody::CreateEdge {
                from_change_id,
                to_change_id,
                ..
            }
            | OpBody::DeleteEdge {
                from_change_id,
                to_change_id,
                ..
            } => vec![from_change_id, to_change_id],
        }
    }

    /// A short human description, for warnings and `remote status`.
    pub fn describe(&self) -> String {
        let short = |c: &str| c.chars().take(8).collect::<String>();
        match self {
            OpBody::CreateNode {
                change_id,
                node_type,
                title,
                ..
            } => format!("create {node_type} {} \"{title}\"", short(change_id)),
            OpBody::UpdateNode {
                change_id,
                set,
                metadata,
                ..
            } => {
                let mut fields: Vec<String> = set
                    .iter()
                    .map(|(k, v)| match v {
                        Value::String(s) if s.chars().count() <= 40 => format!("{k}={s}"),
                        _ => k.clone(),
                    })
                    .collect();
                fields.extend(metadata.keys().map(|k| format!("metadata.{k}")));
                format!("update {} {}", short(change_id), fields.join(" "))
            }
            OpBody::DeleteNode { change_id } => format!("delete node {}", short(change_id)),
            OpBody::CreateEdge {
                from_change_id,
                to_change_id,
                edge_type,
                ..
            } => format!(
                "link {} -> {} ({edge_type})",
                short(from_change_id),
                short(to_change_id)
            ),
            OpBody::DeleteEdge {
                from_change_id,
                to_change_id,
                edge_type,
            } => format!(
                "unlink {} -> {} ({edge_type})",
                short(from_change_id),
                short(to_change_id)
            ),
        }
    }
}

/// Where `op` holds a NUL character (`title`, `metadata.prompt`, a key
/// named `a\0b`), if anywhere. Postgres text cannot hold one, so the server
/// refuses such an op, and no write holding one can ever reach it.
pub fn nul_path(op: &Op) -> Option<String> {
    fn find(v: &Value, path: &mut Vec<String>) -> Option<String> {
        match v {
            Value::String(s) if s.contains('\0') => Some(path.join(".")),
            Value::Object(m) => m.iter().find_map(|(k, x)| {
                if k.contains('\0') {
                    let mut p = path.clone();
                    p.push(format!("{k:?}"));
                    return Some(format!("the key {}", p.join(".")));
                }
                path.push(k.clone());
                let found = find(x, path);
                path.pop();
                found
            }),
            Value::Array(a) => a.iter().enumerate().find_map(|(i, x)| {
                path.push(i.to_string());
                let found = find(x, path);
                path.pop();
                found
            }),
            _ => None,
        }
    }
    find(&serde_json::to_value(op).ok()?, &mut Vec::new())
}

/// The server's answer for one op.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Ack {
    pub op_id: String,
    /// `applied`, `exists`, `absent`, `duplicate` or `rejected`.
    pub result: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub at: String,
}

impl Ack {
    pub fn is_rejected(&self) -> bool {
        self.result == "rejected"
    }

    /// Rejected by this machine, not by the server (see [`SET_ASIDE`]).
    pub fn is_set_aside(&self) -> bool {
        self.is_rejected() && self.reason.as_deref().is_some_and(is_set_aside)
    }
}

/// How every reason this machine writes for an op it set aside begins. An
/// op answered this way was not refused by the server: it is a real write,
/// held back here, and `remote push` sends it again.
pub const SET_ASIDE: &str = "set aside by this machine";

/// Whether a rejection's reason is this machine's, not the server's.
pub fn is_set_aside(reason: &str) -> bool {
    reason.starts_with(SET_ASIDE)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "entry", rename_all = "snake_case")]
enum Entry {
    Op(Op),
    Ack(Ack),
}

/// What the log holds right now.
#[derive(Debug, Default)]
pub struct LogState {
    /// Ops with no ack, in the order they were written.
    pub pending: Vec<Op>,
    /// Ops the server refused, with its answer.
    pub rejected: Vec<(Op, Ack)>,
    /// Ops acknowledged and not yet compacted away.
    pub acked: usize,
    /// Lines of the log that are not entries. Never sent.
    pub unreadable: Vec<Unreadable>,
    /// Lines already moved to [`UNREADABLE_FILE`].
    pub set_aside: usize,
}

/// A line of the log that is not an entry.
#[derive(Debug, Clone, PartialEq)]
pub struct Unreadable {
    /// 1-based line number in the log.
    pub line: usize,
    pub text: String,
    pub error: String,
}

impl Unreadable {
    pub fn describe(&self) -> String {
        let text: String = self.text.chars().take(200).collect();
        format!("line {}: {text}  ({})", self.line, self.error)
    }
}

/// The log's lines, parsed as far as they parse.
struct Parsed {
    entries: Vec<Entry>,
    unreadable: Vec<Unreadable>,
}

fn parse(text: &str) -> Parsed {
    let mut out = Parsed {
        entries: Vec::new(),
        unreadable: Vec::new(),
    };
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let err = match serde_json::from_str::<Entry>(line) {
            Ok(e) => {
                out.entries.push(e);
                continue;
            }
            Err(e) => e,
        };
        // A torn append that a later append was glued to (what builds before
        // this one did): the fragment, then a whole entry. The entry is a
        // write that has not been sent, so it is kept.
        let glued = line
            .match_indices("{\"entry\":")
            .map(|(at, _)| at)
            .filter(|&at| at > 0)
            .find_map(|at| {
                serde_json::from_str::<Entry>(&line[at..])
                    .ok()
                    .map(|e| (at, e))
            });
        match glued {
            Some((at, entry)) => {
                out.unreadable.push(Unreadable {
                    line: i + 1,
                    text: line[..at].to_string(),
                    error: "the start of a write that was cut short; the entry after it on the same line is kept".into(),
                });
                out.entries.push(entry);
            }
            None => out.unreadable.push(Unreadable {
                line: i + 1,
                text: line.to_string(),
                error: err.to_string(),
            }),
        }
    }
    out
}

/// What a long-lived process (the stdio MCP server) has to tell whoever made
/// a write, beyond stderr, which an agent never reads.
#[derive(Debug, Clone, PartialEq)]
pub enum NoticeKind {
    /// A write was made locally and not queued: the server will never get
    /// it from this log. The call that made it did not succeed.
    Unqueued,
    /// Queued writes did not get through, and waiting will not change that:
    /// the server refused them, failed on them, or the log is damaged.
    Unsent,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    pub kind: NoticeKind,
    pub text: String,
}

static NOTICES: std::sync::Mutex<Vec<Notice>> = std::sync::Mutex::new(Vec::new());

/// At most this many notices wait to be taken; a process nobody asks (the
/// CLI) must not grow without bound.
const MAX_NOTICES: usize = 50;

/// Records a notice for [`take_notices`].
pub fn notice(kind: NoticeKind, text: impl Into<String>) {
    let mut n = NOTICES.lock().unwrap_or_else(|e| e.into_inner());
    if n.len() >= MAX_NOTICES {
        n.remove(0);
    }
    n.push(Notice {
        kind,
        text: text.into(),
    });
}

/// The notices recorded since the last call, oldest first.
pub fn take_notices() -> Vec<Notice> {
    std::mem::take(&mut *NOTICES.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Set when this process appends an op, so the CLI knows to replay on exit
/// without re-reading the file after every command.
static APPENDED: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// The log this process appended to since the last call, and forgets it.
///
/// For a process that does not exit after one write: `deciduous mcp` serves
/// an agent's whole session, and replaying only on exit meant its writes sat
/// in the log for hours and were sent by whichever CLI write came next.
pub fn take_appended() -> Option<OpLog> {
    APPENDED
        .lock()
        .ok()
        .and_then(|mut g| g.take())
        .map(|path| OpLog { path })
}

/// What a rewrite keeps of one op.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Keep {
    /// The op and its answer.
    Both,
    /// The op, pending again.
    Op,
    Nothing,
}

#[derive(Debug, Clone)]
pub struct OpLog {
    path: PathBuf,
}

impl OpLog {
    pub fn at(path: impl Into<PathBuf>) -> Self {
        OpLog { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The log for a database, when that database's project points at a
    /// server. `None` otherwise: a project with no remote keeps no log, and a
    /// log started later would lack everything before it anyway (that history
    /// is what `remote init` seeds).
    ///
    /// A bare file name (`DECIDUOUS_DB_PATH=deciduous.db` from inside
    /// `.deciduous/`) is in the current directory. Its empty parent used to
    /// mean "no log": the write was made, never queued, and nothing said so.
    pub fn for_db(db_path: &Path) -> Option<Self> {
        let db_path = std::path::absolute(db_path).ok()?;
        let dir = db_path.parent()?;
        let config = std::fs::read_to_string(dir.join("config.toml")).ok()?;
        let doc: toml::Value = toml::from_str(&config).ok()?;
        doc.get("remote")?.get("url")?.as_str()?;
        Some(OpLog::at(dir.join(FILE_NAME)))
    }

    /// A new op for `body`: a fresh id, stamped now. Not yet in any log.
    pub fn new_op(body: OpBody) -> Op {
        Op {
            op_id: uuid::Uuid::new_v4().to_string(),
            at: chrono::Utc::now().to_rfc3339(),
            body,
        }
    }

    /// Appends one op and returns it.
    pub fn append(&self, body: OpBody) -> Result<Op, String> {
        let op = Self::new_op(body);
        self.append_ops(std::slice::from_ref(&op))?;
        Ok(op)
    }

    /// Appends ops already made (see `Database`'s outbox), in one write.
    pub fn append_ops(&self, ops: &[Op]) -> Result<(), String> {
        if ops.is_empty() {
            return Ok(());
        }
        let lines = ops
            .iter()
            .map(|op| {
                serde_json::to_string(&Entry::Op(op.clone()))
                    .map_err(|e| format!("serializing op: {e}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let _lock = self.lock()?;
        self.mend_torn_tail()?;
        self.append_lines(&lines)?;
        if let Ok(mut g) = APPENDED.lock() {
            *g = Some(self.path.clone());
        }
        Ok(())
    }

    /// The ids of every op in the log, answered or not.
    pub fn op_ids(&self) -> Result<std::collections::HashSet<String>, String> {
        Ok(self
            .parsed()?
            .entries
            .into_iter()
            .filter_map(|e| match e {
                Entry::Op(op) => Some(op.op_id),
                Entry::Ack(_) => None,
            })
            .collect())
    }

    /// Records the server's answers.
    pub fn record_acks(&self, acks: &[Ack]) -> Result<(), String> {
        if acks.is_empty() {
            return Ok(());
        }
        let lines = acks
            .iter()
            .map(|a| {
                serde_json::to_string(&Entry::Ack(a.clone()))
                    .map_err(|e| format!("serializing ack: {e}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let _lock = self.lock()?;
        self.mend_torn_tail()?;
        self.append_lines(&lines)
    }

    /// Where unreadable lines are moved: beside the log.
    pub fn unreadable_path(&self) -> PathBuf {
        self.path.with_file_name(UNREADABLE_FILE)
    }

    /// Before an append, under the lock: a last line with no newline is an
    /// append that was cut short. Appending after it would glue the new
    /// entry onto it. A tail that is a whole entry only lacks its newline
    /// and gets one; anything else is moved to [`UNREADABLE_FILE`] and said.
    fn mend_torn_tail(&self) -> Result<(), String> {
        let bytes = match std::fs::read(&self.path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(format!("{}: {e}", self.path.display())),
        };
        if bytes.is_empty() || bytes.ends_with(b"\n") {
            return Ok(());
        }
        let start = bytes
            .iter()
            .rposition(|&b| b == b'\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        let tail = &bytes[start..];
        let io = |e: std::io::Error| format!("{}: {e}", self.path.display());
        if serde_json::from_slice::<Entry>(tail).is_ok() {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&self.path)
                .map_err(io)?;
            return f.write_all(b"\n").and_then(|_| f.sync_data()).map_err(io);
        }
        let aside = self.unreadable_path();
        let mut side = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&aside)
            .map_err(|e| format!("{}: {e}", aside.display()))?;
        side.write_all(tail)
            .and_then(|_| side.write_all(b"\n"))
            .and_then(|_| side.sync_data())
            .map_err(|e| format!("{}: {e}", aside.display()))?;
        let f = std::fs::OpenOptions::new()
            .write(true)
            .open(&self.path)
            .map_err(io)?;
        f.set_len(start as u64)
            .and_then(|_| f.sync_data())
            .map_err(io)?;
        eprintln!(
            "Warning: the last line of {} was an append cut short (a crash or a full disk), \
             not a whole write; it was moved to {} so the next write does not land on it:\n  {}",
            self.path.display(),
            aside.display(),
            String::from_utf8_lossy(tail)
                .chars()
                .take(200)
                .collect::<String>()
        );
        Ok(())
    }

    /// What the log holds. Fails only when the file cannot be read at all;
    /// a line that is not an entry is reported in
    /// [`LogState::unreadable`], not an error.
    pub fn read(&self) -> Result<LogState, String> {
        let parsed = self.parsed()?;
        let mut st = state(&parsed.entries);
        st.unreadable = parsed.unreadable;
        st.set_aside = std::fs::read_to_string(self.unreadable_path())
            .map(|t| t.lines().filter(|l| !l.trim().is_empty()).count())
            .unwrap_or(0);
        Ok(st)
    }

    /// Drops acknowledged ops, keeping pending and rejected ones.
    pub fn compact(&self) -> Result<usize, String> {
        self.rewrite(|_, ack| match ack {
            None => true,
            Some(a) => a.is_rejected(),
        })
    }

    /// Drops the ops the server rejected (and acknowledged ones). Returns
    /// how many rejected ops were dropped. Ops this machine set aside are
    /// kept: the server never refused them, and they are real writes.
    pub fn drop_rejected(&self) -> Result<usize, String> {
        let _lock = self.lock()?;
        let before = self
            .read()?
            .rejected
            .iter()
            .filter(|(_, a)| !a.is_set_aside())
            .count();
        self.rewrite(|_, ack| ack.is_none_or(|a| a.is_set_aside()))?;
        Ok(before)
    }

    /// Makes every rejected op pending again (its rejection is forgotten),
    /// so the next replay sends it. Returns how many.
    pub fn retry_rejected(&self) -> Result<usize, String> {
        self.retry_where(|_| true)
    }

    /// Makes the ops this machine set aside pending again. Returns how many.
    pub fn retry_set_aside(&self) -> Result<usize, String> {
        self.retry_where(Ack::is_set_aside)
    }

    fn retry_where(&self, which: impl Fn(&Ack) -> bool) -> Result<usize, String> {
        let _lock = self.lock()?;
        let before = self
            .read()?
            .rejected
            .iter()
            .filter(|(_, a)| which(a))
            .count();
        if before == 0 {
            return Ok(0);
        }
        self.rewrite_with(|_, ack| match ack {
            None => Keep::Op,
            Some(a) if a.is_rejected() && which(a) => Keep::Op,
            Some(a) if a.is_rejected() => Keep::Both,
            Some(_) => Keep::Nothing,
        })?;
        Ok(before)
    }

    /// Drops the op whose id starts with `prefix`, whatever its state.
    /// Refuses a prefix that matches no op, or more than one.
    pub fn drop_op(&self, prefix: &str) -> Result<Op, String> {
        let _lock = self.lock()?;
        let st = self.read()?;
        let matches: Vec<&Op> = st
            .pending
            .iter()
            .chain(st.rejected.iter().map(|(op, _)| op))
            .filter(|op| !prefix.is_empty() && op.op_id.starts_with(prefix))
            .collect();
        let op = match matches.as_slice() {
            [one] => (*one).clone(),
            [] => {
                return Err(format!(
                    "no waiting or rejected op in {} has an id starting with {prefix:?}; \
                     `deciduous remote status` lists them with their ids",
                    self.path.display()
                ))
            }
            many => {
                return Err(format!(
                    "{} ops have an id starting with {prefix:?} ({}); give more of the id",
                    many.len(),
                    many.iter()
                        .map(|o| o.op_id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            }
        };
        self.rewrite(|o, _| o.op_id != op.op_id)?;
        Ok(op)
    }

    /// Drops the rejected ops that touch any of `change_ids`: the nodes the
    /// server deleted, after a pull removed them here. Such an op was
    /// refused because the node is gone and can never apply; kept, it held
    /// `remote status` at "rejected" until `--drop-rejected`, which would
    /// also have thrown away unrelated refusals. Returns how many it dropped.
    pub fn drop_rejected_touching(
        &self,
        change_ids: &std::collections::HashSet<String>,
    ) -> Result<usize, String> {
        self.rewrite(|op, ack| {
            !(ack.is_some_and(|a| a.is_rejected())
                && op.body.change_ids().iter().any(|c| change_ids.contains(*c)))
        })
    }

    fn rewrite(&self, keep: impl Fn(&Op, Option<&Ack>) -> bool) -> Result<usize, String> {
        self.rewrite_with(|op, ack| {
            if keep(op, ack) {
                Keep::Both
            } else {
                Keep::Nothing
            }
        })
    }

    fn rewrite_with(&self, keep: impl Fn(&Op, Option<&Ack>) -> Keep) -> Result<usize, String> {
        let _lock = self.lock()?;
        let Parsed {
            entries,
            unreadable,
        } = self.parsed()?;
        if entries.is_empty() && unreadable.is_empty() {
            return Ok(0);
        }
        // Lines that are not entries are moved aside, never dropped: one
        // may be a damaged write someone wants to repair and put back.
        if !unreadable.is_empty() {
            let aside = self.unreadable_path();
            let mut body = String::new();
            for u in &unreadable {
                body.push_str(&u.text);
                body.push('\n');
            }
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&aside)
                .map_err(|e| format!("{}: {e}", aside.display()))?;
            f.write_all(body.as_bytes())
                .and_then(|_| f.sync_data())
                .map_err(|e| format!("{}: {e}", aside.display()))?;
        }
        let mut acks: std::collections::HashMap<&str, &Ack> = std::collections::HashMap::new();
        for e in &entries {
            if let Entry::Ack(a) = e {
                // The last answer wins: a rejected op sent again may be applied.
                acks.insert(a.op_id.as_str(), a);
            }
        }
        let mut kept = Vec::new();
        let mut dropped = 0;
        for e in &entries {
            if let Entry::Op(op) = e {
                let ack = acks.get(op.op_id.as_str()).copied();
                let k = keep(op, ack);
                if k == Keep::Nothing {
                    dropped += 1;
                    continue;
                }
                kept.push(serde_json::to_string(e).map_err(|e| e.to_string())?);
                if let (Keep::Both, Some(a)) = (k, ack) {
                    kept.push(
                        serde_json::to_string(&Entry::Ack(a.clone())).map_err(|e| e.to_string())?,
                    );
                }
            }
        }
        let tmp = self.path.with_extension("jsonl.tmp");
        let mut body = kept.join("\n");
        if !body.is_empty() {
            body.push('\n');
        }
        // Synced before the rename, and the directory after it: a rename can
        // reach the disk before the data it points at, and a power loss then
        // leaves an empty or partial log, every waiting write with it. A kill
        // cannot do that (rename is atomic); only the disk's own ordering can.
        let tmp_err = |e: std::io::Error| format!("{}: {e}", tmp.display());
        let mut f = std::fs::File::create(&tmp).map_err(tmp_err)?;
        f.write_all(body.as_bytes())
            .and_then(|_| f.sync_all())
            .map_err(tmp_err)?;
        drop(f);
        std::fs::rename(&tmp, &self.path).map_err(|e| format!("{}: {e}", self.path.display()))?;
        if let Some(dir) = self.path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::File::open(dir)
                .and_then(|d| d.sync_all())
                .map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        Ok(dropped)
    }

    fn parsed(&self) -> Result<Parsed, String> {
        match std::fs::read(&self.path) {
            Ok(bytes) => Ok(parse(&String::from_utf8_lossy(&bytes))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(parse("")),
            Err(e) => Err(format!("{}: {e}", self.path.display())),
        }
    }

    fn append_lines(&self, lines: &[String]) -> Result<(), String> {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| format!("{}: {e}", self.path.display()))?;
        let mut buf = lines.join("\n");
        buf.push('\n');
        // One write call: with O_APPEND the kernel places it at the end in
        // one piece, so two processes appending at once do not interleave.
        f.write_all(buf.as_bytes())
            .and_then(|_| f.sync_data())
            .map_err(|e| format!("{}: {e}", self.path.display()))
    }

    /// The log's lock, held while appending or rewriting, and by a
    /// database write from before it changes a row until its op is
    /// appended (see `Database::hold_log`).
    ///
    /// Appends alone would not need it (O_APPEND), but compaction replaces
    /// the file, and an append that lands in the old file after compaction
    /// read it would be lost.
    ///
    /// An OS lock (`flock`) on `remote-log.lock`, which the kernel releases
    /// when the holder dies. It used to be `create_new` on that file and a
    /// remove in Drop: a process killed while holding it (SIGKILL, and
    /// SIGTERM, which runs no destructors either) left the file, and every
    /// write for the next 60 s waited 10 s and then dropped its op. The
    /// file itself now stays, empty; its existence means nothing.
    ///
    /// Re-entrant within a thread: a write holding the lock appends under it.
    pub fn lock(&self) -> Result<LockGuard, String> {
        self.lock_within(LOCK_WAIT)?.ok_or_else(|| {
            format!(
                "{} has been held by another deciduous process for {}s \
                 (a live one: the lock is released when its holder exits, however it exits)",
                self.path.with_extension("lock").display(),
                LOCK_WAIT.as_secs()
            )
        })
    }

    /// The lock if it is free now; `None` if another process holds it.
    pub fn try_lock(&self) -> Result<Option<LockGuard>, String> {
        self.lock_within(std::time::Duration::ZERO)
    }

    fn lock_within(&self, wait: std::time::Duration) -> Result<Option<LockGuard>, String> {
        let path = self.path.with_extension("lock");
        let held = HELD.with(|h| {
            let mut h = h.borrow_mut();
            match h.iter_mut().find(|(p, _, _)| *p == path) {
                Some((_, _, depth)) => {
                    *depth += 1;
                    true
                }
                None => false,
            }
        });
        if held {
            return Ok(Some(LockGuard { path }));
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let deadline = std::time::Instant::now() + wait;
        loop {
            match file.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) => {
                    if std::time::Instant::now() >= deadline {
                        return Ok(None);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(std::fs::TryLockError::Error(e)) => {
                    return Err(format!("locking {}: {e}", path.display()))
                }
            }
        }
        HELD.with(|h| h.borrow_mut().push((path.clone(), file, 1)));
        Ok(Some(LockGuard { path }))
    }
}

/// How long a write waits for another process's hold on the log. A hold
/// lasts one database write and one append, so this is far past any real
/// one; it is the bound on a holder that is alive and stuck.
const LOCK_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

thread_local! {
    /// Locks this thread holds: (lock path, the locked file, depth).
    static HELD: std::cell::RefCell<Vec<(PathBuf, std::fs::File, usize)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// A hold on the log's lock. Dropping the last one on a thread unlocks it.
pub struct LockGuard {
    path: PathBuf,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        HELD.with(|h| {
            let mut h = h.borrow_mut();
            if let Some(i) = h.iter().position(|(p, _, _)| *p == self.path) {
                h[i].2 -= 1;
                if h[i].2 == 0 {
                    // Closing the file releases the lock.
                    h.swap_remove(i);
                }
            }
        });
    }
}

fn state(entries: &[Entry]) -> LogState {
    let mut acks: std::collections::HashMap<&str, &Ack> = std::collections::HashMap::new();
    for e in entries {
        if let Entry::Ack(a) = e {
            acks.insert(a.op_id.as_str(), a);
        }
    }
    let mut st = LogState::default();
    for e in entries {
        if let Entry::Op(op) = e {
            match acks.get(op.op_id.as_str()) {
                None => st.pending.push(op.clone()),
                Some(a) if a.is_rejected() => st.rejected.push((op.clone(), (*a).clone())),
                Some(_) => st.acked += 1,
            }
        }
    }
    st
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(cid: &str, s: &str) -> OpBody {
        let mut set = Map::new();
        set.insert("status".into(), Value::String(s.into()));
        let mut was = Map::new();
        was.insert("status".into(), Value::String("pending".into()));
        OpBody::UpdateNode {
            change_id: cid.into(),
            set,
            metadata: Map::new(),
            was,
            was_metadata: Map::new(),
        }
    }

    fn ack(op: &Op, result: &str) -> Ack {
        Ack {
            op_id: op.op_id.clone(),
            result: result.into(),
            reason: (result == "rejected").then(|| "no".into()),
            at: "t".into(),
        }
    }

    #[test]
    fn an_update_serializes_only_the_fields_it_changed() {
        let v = serde_json::to_value(Entry::Op(Op {
            op_id: "o".into(),
            at: "t".into(),
            body: status("c", "completed"),
        }))
        .unwrap();
        assert_eq!(
            v,
            serde_json::json!({"entry":"op","op_id":"o","at":"t","kind":"update_node","change_id":"c","set":{"status":"completed"},"was":{"status":"pending"}})
        );
    }

    #[test]
    fn acked_ops_are_compacted_away_and_rejected_ones_kept() {
        let dir = tempfile::TempDir::new().unwrap();
        let log = OpLog::at(dir.path().join(FILE_NAME));
        let a = log.append(status("a", "completed")).unwrap();
        let b = log.append(status("b", "completed")).unwrap();
        let c = log.append(status("c", "completed")).unwrap();
        log.record_acks(&[ack(&a, "applied"), ack(&b, "rejected")])
            .unwrap();

        let st = log.read().unwrap();
        assert_eq!(st.pending, vec![c.clone()]);
        assert_eq!(st.rejected.len(), 1);
        assert_eq!(st.acked, 1);

        assert_eq!(log.compact().unwrap(), 1);
        let st = log.read().unwrap();
        assert_eq!((st.pending.len(), st.rejected.len(), st.acked), (1, 1, 0));

        assert_eq!(log.drop_rejected().unwrap(), 1);
        let st = log.read().unwrap();
        assert_eq!((st.pending.len(), st.rejected.len()), (1, 0));
        assert_eq!(st.pending[0].op_id, c.op_id);
    }

    #[test]
    fn a_corrupt_line_is_reported_by_number_and_the_rest_still_read() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join(FILE_NAME);
        let log = OpLog::at(&path);
        let a = log.append(status("a", "completed")).unwrap();
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all(b"{\"entry\":\"op\"\n").unwrap();
        let b = log.append(status("b", "completed")).unwrap();
        let st = log.read().unwrap();
        assert_eq!(st.pending, vec![a, b]);
        assert_eq!(st.unreadable.len(), 1);
        assert_eq!(st.unreadable[0].line, 2);
        // Compaction moves it aside rather than dropping it.
        log.compact().unwrap();
        let st = log.read().unwrap();
        assert_eq!(
            (st.pending.len(), st.unreadable.len(), st.set_aside),
            (2, 0, 1)
        );
    }

    #[test]
    fn a_leftover_lock_file_is_not_a_lock_and_a_live_holder_is_waited_for() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join(FILE_NAME);
        let log = OpLog::at(&path);
        // What a killed holder leaves: the file, with no lock on it.
        std::fs::write(path.with_extension("lock"), b"").unwrap();
        let t = std::time::Instant::now();
        log.append(status("a", "completed")).unwrap();
        assert!(t.elapsed() < std::time::Duration::from_secs(1));

        // A live holder on another descriptor (another process, as far as
        // flock is concerned) is waited for, not broken.
        let other = std::fs::OpenOptions::new()
            .write(true)
            .open(path.with_extension("lock"))
            .unwrap();
        other.lock().unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            drop(other);
        });
        let t = std::time::Instant::now();
        let log2 = log.clone();
        std::thread::spawn(move || log2.append(status("b", "completed")).unwrap())
            .join()
            .unwrap();
        assert!(t.elapsed() >= std::time::Duration::from_millis(250));
        release.join().unwrap();
        assert_eq!(log.read().unwrap().pending.len(), 2);
    }

    #[test]
    fn the_lock_is_reentrant_within_a_thread() {
        let dir = tempfile::TempDir::new().unwrap();
        let log = OpLog::at(dir.path().join(FILE_NAME));
        let _held = log.lock().unwrap();
        log.append(status("a", "completed")).unwrap();
        log.compact().unwrap();
        assert_eq!(log.read().unwrap().pending.len(), 1);
    }

    #[test]
    fn a_tail_that_lost_only_its_newline_is_kept() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join(FILE_NAME);
        let log = OpLog::at(&path);
        let a = log.append(status("a", "completed")).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, text.trim_end()).unwrap();
        let b = log.append(status("b", "completed")).unwrap();
        let st = log.read().unwrap();
        assert_eq!(st.pending, vec![a, b]);
        assert!(st.unreadable.is_empty() && st.set_aside == 0);
    }

    #[test]
    fn a_database_without_a_remote_has_no_log() {
        let dir = tempfile::TempDir::new().unwrap();
        let db = dir.path().join("deciduous.db");
        assert!(OpLog::for_db(&db).is_none());
        std::fs::write(
            dir.path().join("config.toml"),
            "[remote]\nurl = \"http://x\"\n",
        )
        .unwrap();
        assert_eq!(
            OpLog::for_db(&db).unwrap().path(),
            dir.path().join(FILE_NAME)
        );
    }
}
