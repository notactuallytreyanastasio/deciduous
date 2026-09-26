//! Streaming a workspace's writes as they happen: the client half of the
//! server's `GET /events` WebSocket.
//!
//! Every frame is a pointer, not a row: which table, which operation, the
//! node's type, title and branch. This module turns each one into a line a
//! person can read at a glance and keeps the connection alive across the
//! drops a long-lived socket sees in practice.
//!
//! Two rules shape the output, both learned from the first arena that used
//! this feed. Quote, do not count: a watcher that summarises from a running
//! tally can announce a pattern the graph never held, whereas a line that
//! carries the node's own title cannot. And show updates as updates: an
//! `UPDATE` to a goal is not a second goal.

use serde_json::Value;
use std::collections::HashMap;
use std::io::{ErrorKind, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};
use tungstenite::error::UrlError;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

/// The node types a `--types` filter may name. Kept here rather than
/// imported so the watcher does not pull the pulse report into scope.
pub const NODE_TYPES: [&str; 7] = [
    "goal",
    "option",
    "decision",
    "action",
    "outcome",
    "observation",
    "revisit",
];

/// How long a read may sit with no bytes before the socket is declared
/// dead. The server pings every 30 seconds, so a live connection carries
/// traffic well inside this; a half-open one, after a laptop sleep or a NAT
/// entry expiring, is the case this exists for. Without it `read()` blocks
/// forever and the watcher goes silent with no reconnect.
pub const READ_TIMEOUT: Duration = Duration::from_secs(75);

/// What to print, chosen on the command line.
#[derive(Debug, Default, Clone)]
pub struct Filter {
    /// Node types to show. Empty means every type.
    pub types: Vec<String>,
    /// Branches to show. Empty means every branch. An event with no branch
    /// is excluded when this is set, since it cannot be on the asked-for one.
    pub branches: Vec<String>,
    /// Include edge events. Off by default: an edge is a link between two
    /// nodes already announced, and ten agents linking as they go is most of
    /// the traffic.
    pub edges: bool,
    /// Print the raw frame instead of a formatted line.
    pub json: bool,
}

impl Filter {
    /// The names in `types` that are not node types at all. A typo in
    /// `--types` would otherwise print nothing, forever, indistinguishable
    /// from a quiet graph.
    pub fn unknown_types(&self) -> Vec<&str> {
        self.types
            .iter()
            .map(String::as_str)
            .filter(|t| !NODE_TYPES.contains(t))
            .collect()
    }

    /// Whether an event passes the filters. Anything that is not a node or
    /// edge event is passed through, so a new event kind from a newer server
    /// is visible rather than silently dropped.
    pub fn admits(&self, event: &Value) -> bool {
        let branch = event.get("branch").and_then(Value::as_str);
        if !self.branches.is_empty()
            && !branch.is_some_and(|b| self.branches.iter().any(|f| f == b))
        {
            return false;
        }

        match event.get("table").and_then(Value::as_str) {
            Some("decision_nodes") => {
                let node_type = event.get("node_type").and_then(Value::as_str).unwrap_or("");
                self.types.is_empty() || self.types.iter().any(|t| t == node_type)
            }
            Some("decision_edges") => self.edges,
            _ => true,
        }
    }
}

/// A title inside double quotes, escaping only what would break the line:
/// the quote, the backslash and control characters. `{:?}` would also
/// escape variation selectors, zero-width joiners and combining marks, so a
/// title with a heart emoji or an accented name written in NFD came out as
/// `\u{fe0f}`, which is not quoting the title.
pub fn quote(title: &str) -> String {
    let mut out = String::with_capacity(title.len() + 2);
    out.push('"');
    for c in title.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// One event as one line. `at` is the wall-clock time to print first,
/// passed in so the formatting is a pure function of its inputs.
///
/// Returns `None` for a frame with no `table`, which is not an event this
/// client knows how to read.
pub fn format_event(event: &Value, at: &str) -> Option<String> {
    let table = event.get("table").and_then(Value::as_str)?;
    let op = event.get("op").and_then(Value::as_str).unwrap_or("INSERT");
    let branch = event
        .get("branch")
        .and_then(Value::as_str)
        .filter(|b| !b.is_empty())
        .unwrap_or("-");
    let short = |key: &str| -> String {
        event
            .get(key)
            .and_then(Value::as_str)
            .map(|s| s.chars().take(8).collect())
            .unwrap_or_else(|| "????????".to_string())
    };

    let line = match table {
        "decision_nodes" => {
            let node_type = event
                .get("node_type")
                .and_then(Value::as_str)
                .unwrap_or("node");
            let kind = match op {
                "INSERT" => node_type.to_string(),
                "UPDATE" => match event.get("changed").and_then(Value::as_array) {
                    Some(fields) if !fields.is_empty() => format!(
                        "{node_type} (updated: {})",
                        fields
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    _ => format!("{node_type} (updated)"),
                },
                "DELETE" => format!("{node_type} (deleted)"),
                other => format!("{node_type} ({})", other.to_lowercase()),
            };
            // An older server sends no title; the change_id prefix is the
            // next best handle, and it is what `show_node` accepts.
            let what = match event.get("title").and_then(Value::as_str) {
                Some(title) => quote(title),
                None => short("change_id"),
            };
            format!("{at}  {branch:<8}  {kind:<11} {what}")
        }
        "decision_edges" => {
            let edge_type = event
                .get("edge_type")
                .and_then(Value::as_str)
                .unwrap_or("leads_to");
            let verb = match op {
                "INSERT" => format!("edge {edge_type}"),
                "DELETE" => format!("edge {edge_type} (deleted)"),
                other => format!("edge {edge_type} ({})", other.to_lowercase()),
            };
            format!(
                "{at}  {branch:<8}  {verb}  {} -> {}",
                short("from_change_id"),
                short("to_change_id")
            )
        }
        // A board post (server 1.0.9+). Printed rather than passed through
        // as a bare "agent_messages INSERT": who wrote to whom is the point.
        "agent_messages" => {
            let id = event
                .get("id")
                .map(|v| v.to_string().trim_matches('"').to_string())
                .unwrap_or_else(|| "?".into());
            let author = event.get("author").and_then(Value::as_str).unwrap_or("?");
            let to: Vec<&str> = event
                .get("mentions")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let to = if to.is_empty() {
                String::new()
            } else {
                format!(" -> {}", to.join(", "))
            };
            let re = event
                .get("reply_to")
                .and_then(Value::as_i64)
                .map(|r| format!(" re #{r}"))
                .unwrap_or_default();
            let subject = event.get("subject").and_then(Value::as_str).unwrap_or("");
            format!(
                "{at}  {branch:<8}  message #{id}{re}  {author}{to} {}",
                quote(subject)
            )
        }
        other => format!("{at}  {branch:<8}  {other} {op}"),
    };

    Some(line)
}

/// Remembers recent frames so one delivered twice prints once.
///
/// Keyed on the whole frame text, not on table, id and operation: two
/// genuine updates to one node inside the window differ in what they say
/// (title, status) and both must print. A frame that is byte-identical to
/// one seen in the last minute is the only thing suppressed, and that is
/// the only thing a delivery hiccup can produce.
pub struct Dedup {
    seen: HashMap<String, Instant>,
    window: Duration,
}

impl Dedup {
    pub fn new(window: Duration) -> Self {
        Self {
            seen: HashMap::new(),
            window,
        }
    }

    /// True the first time this exact frame is seen inside the window.
    pub fn first_sighting(&mut self, frame: &str) -> bool {
        self.first_sighting_at(frame, Instant::now())
    }

    fn first_sighting_at(&mut self, frame: &str, now: Instant) -> bool {
        self.seen
            .retain(|_, at| now.duration_since(*at) < self.window);

        match self.seen.get(frame) {
            Some(_) => false,
            None => {
                self.seen.insert(frame.to_string(), now);
                true
            }
        }
    }
}

/// Why a connection attempt failed, as a line safe to print.
///
/// tungstenite's own message for a refused connection is "Unable to connect
/// to <url>", and the URL carries the token. That string must never reach
/// stderr: a deploy restarts the server, every backoff tick would print it,
/// and stderr ends up in scrollback, tmux logs and `2>&1 | tee` files.
pub fn connect_failure(e: &tungstenite::Error) -> String {
    match e {
        tungstenite::Error::Url(UrlError::UnableToConnect(_)) => {
            "connect failed: unable to connect".to_string()
        }
        tungstenite::Error::Io(io) => format!("connect failed: {}", io.kind()),
        tungstenite::Error::Http(resp) => format!("connect failed: HTTP {}", resp.status()),
        other => format!("connect failed: {other}"),
    }
}

/// A handshake refusal that retrying cannot change.
fn is_permanent(e: &tungstenite::Error) -> Option<String> {
    match e {
        tungstenite::Error::Http(resp) if resp.status().is_client_error() => Some(format!(
            "the server refused the connection with HTTP {}; check the token and the workspace",
            resp.status()
        )),
        _ => None,
    }
}

/// Connects to `url` and prints events until the process is killed.
///
/// A dropped socket is reconnected with a backoff that starts at one second
/// and doubles to thirty. The backoff resets once a connection has delivered
/// a frame, not on the handshake: a server that accepts and immediately
/// closes would otherwise be hammered once a second. A 4xx at the handshake
/// ends the run, since no retry will fix a bad token. The reason for each
/// reconnect goes to stderr, without the URL, so stdout stays one event per
/// line.
/// Where a stream resumes: the `seq` of the last event this watcher saw.
///
/// The server numbers every event (graph_events, round-2 BRIDGE-N6). A
/// reconnect asks for what came after it (`&since=`), so an update made
/// while the connection was down is shown instead of lost; and an event is
/// shown once by its number, not by its bytes, so the third identical-
/// looking status change of a node is a line of its own. A server that
/// sends no `seq` (older than this) gets the old content filter.
#[derive(Debug, Default)]
pub struct Cursor {
    pub last: Option<u64>,
}

impl Cursor {
    /// Whether `event` is new, recording it if it is.
    pub fn admit(&mut self, event: &Value) -> Option<bool> {
        let seq = event.get("seq").and_then(Value::as_u64)?;
        if self.last.is_some_and(|l| seq <= l) {
            return Some(false);
        }
        self.last = Some(seq);
        Some(true)
    }

    /// `url`, resuming after the last event seen.
    pub fn resume(&self, url: &str) -> String {
        match self.last {
            Some(n) if url.contains('?') => format!("{url}&since={n}"),
            Some(n) => format!("{url}?since={n}"),
            None => url.to_string(),
        }
    }
}

pub fn run(url: &str, filter: &Filter, out: &mut dyn Write) -> Result<(), String> {
    let mut backoff = Duration::from_secs(1);
    let max_backoff = Duration::from_secs(30);
    let mut dedup = Dedup::new(Duration::from_secs(60));
    let mut cursor = Cursor::default();

    loop {
        let url = cursor.resume(url);
        match tungstenite::connect(url.as_str()) {
            Ok((socket, _response)) => {
                set_read_timeout(&socket);
                let (why, delivered) =
                    read_until_closed(socket, filter, &mut dedup, &mut cursor, out)?;
                if delivered {
                    backoff = Duration::from_secs(1);
                }
                eprintln!("reconnecting ({why})");
            }
            Err(e) => {
                if let Some(reason) = is_permanent(&e) {
                    return Err(reason);
                }
                eprintln!("reconnecting ({})", connect_failure(&e));
            }
        }

        std::thread::sleep(backoff);
        backoff = (backoff * 2).min(max_backoff);
    }
}

fn set_read_timeout(socket: &WebSocket<MaybeTlsStream<TcpStream>>) {
    let tcp = match socket.get_ref() {
        MaybeTlsStream::Plain(s) => s,
        MaybeTlsStream::Rustls(s) => s.get_ref(),
        _ => return,
    };
    // A failure here leaves the old behaviour, blocking reads, which is
    // worse but not wrong; there is nothing to tell the user that would help.
    let _ = tcp.set_read_timeout(Some(READ_TIMEOUT));
}

/// Reads frames until the socket closes, returning why it closed and
/// whether any frame arrived on this connection.
///
/// Pings are answered inside `read()`; tungstenite queues the pong and
/// flushes it on the next read or write, so a server that keeps the
/// connection alive by pinging needs nothing from this loop.
fn read_until_closed(
    mut socket: WebSocket<MaybeTlsStream<TcpStream>>,
    filter: &Filter,
    dedup: &mut Dedup,
    cursor: &mut Cursor,
    out: &mut dyn Write,
) -> Result<(String, bool), String> {
    let mut delivered = false;
    loop {
        match socket.read() {
            Ok(Message::Text(text)) => {
                delivered = true;
                let text = text.as_str();
                let event: Value = match serde_json::from_str(text) {
                    Ok(v) => v,
                    Err(_) => {
                        // Not JSON: still worth seeing, still not worth dying over.
                        emit(out, text)?;
                        continue;
                    }
                };

                if event.get("gap").and_then(Value::as_bool) == Some(true) {
                    let why = event.get("reason").and_then(Value::as_str).unwrap_or("");
                    eprintln!("warning: some events were missed while disconnected: {why}");
                    continue;
                }
                let new = match cursor.admit(&event) {
                    Some(new) => new,
                    None => dedup.first_sighting(text),
                };
                if !new || !filter.admits(&event) {
                    continue;
                }

                if filter.json {
                    emit(out, text)?;
                } else if let Some(line) = format_event(&event, &now_hms()) {
                    emit(out, &line)?;
                }
            }
            Ok(Message::Close(frame)) => {
                let why = match frame {
                    Some(f) => format!("close {}", u16::from(f.code)),
                    None => "close".to_string(),
                };
                return Ok((why, delivered));
            }
            Ok(_) => {}
            Err(tungstenite::Error::ConnectionClosed) => {
                return Ok(("closed".to_string(), delivered))
            }
            Err(tungstenite::Error::Io(io))
                if matches!(io.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
            {
                return Ok((
                    format!("read timed out after {}s", READ_TIMEOUT.as_secs()),
                    delivered,
                ));
            }
            Err(e) => return Ok((format!("error: {e}"), delivered)),
        }
    }
}

/// One line to stdout, flushed. A closed pipe (`| head`) ends the run
/// cleanly instead of as an error: the reader got what it asked for.
fn emit(out: &mut dyn Write, line: &str) -> Result<(), String> {
    match writeln!(out, "{line}").and_then(|_| out.flush()) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::BrokenPipe => std::process::exit(0),
        Err(e) => Err(e.to_string()),
    }
}

fn now_hms() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node(op: &str) -> Value {
        json!({
            "table": "decision_nodes", "op": op, "workspace": "tetris-arena",
            "id": "d43f55ba-7c6e-4fdb-b528-9850fabf4c20",
            "change_id": "83c0f4bb-1870-4eb2-86d3-eedc62c6e25b",
            "node_type": "outcome", "title": "Round 4: clamp DAS through the clear flash",
            "branch": "agent-3"
        })
    }

    #[test]
    fn an_insert_quotes_the_title() {
        assert_eq!(
            format_event(&node("INSERT"), "05:01:12").unwrap(),
            "05:01:12  agent-3   outcome     \"Round 4: clamp DAS through the clear flash\""
        );
    }

    #[test]
    fn branches_line_up_across_one_and_two_digit_agents() {
        let mut ten = node("INSERT");
        ten["branch"] = json!("agent-10");
        let a = format_event(&node("INSERT"), "t").unwrap();
        let b = format_event(&ten, "t").unwrap();
        assert_eq!(a.find("outcome"), b.find("outcome"), "{a}\n{b}");
    }

    #[test]
    fn a_title_keeps_its_emoji_and_escapes_its_newlines() {
        let mut e = node("INSERT");
        e["title"] = json!("I \u{2764}\u{fe0f} kicks\nline two \"quoted\"");
        let line = format_event(&e, "t").unwrap();
        assert!(line.contains("I \u{2764}\u{fe0f} kicks"), "{line}");
        assert!(line.contains("\\n"), "{line}");
        assert!(line.contains("\\\"quoted\\\""), "{line}");
        assert!(!line.contains("u{fe0f}"), "{line}");
    }

    #[test]
    fn a_refused_connection_never_prints_the_url() {
        let url = "wss://example.test/events?workspace=w&token=SECRET".to_string();
        let e = tungstenite::Error::Url(UrlError::UnableToConnect(url));
        let msg = connect_failure(&e);
        assert!(!msg.contains("token="), "{msg}");
        assert!(!msg.contains("SECRET"), "{msg}");
        assert_eq!(msg, "connect failed: unable to connect");
    }

    #[test]
    fn a_typo_in_types_is_named() {
        let f = Filter {
            types: vec!["outcome".into(), "outcomes".into()],
            ..Default::default()
        };
        assert_eq!(f.unknown_types(), vec!["outcomes"]);
        assert!(Filter::default().unknown_types().is_empty());
    }

    #[test]
    fn bridge_n6_an_update_names_what_changed() {
        let mut e = node("UPDATE");
        e["changed"] = json!(["status", "metadata"]);
        let line = format_event(&e, "t").unwrap();
        assert!(
            line.contains("outcome (updated: status, metadata)"),
            "{line}"
        );
    }

    #[test]
    fn bridge_n6_events_are_told_apart_by_seq_and_resume_after_the_last() {
        let mut c = Cursor::default();
        assert_eq!(
            c.resume("ws://h/events?workspace=w"),
            "ws://h/events?workspace=w"
        );
        let mut a = node("UPDATE");
        a["seq"] = json!(7);
        let mut b = a.clone();
        b["seq"] = json!(8);
        // Byte-identical but for the seq: two real updates, two lines.
        assert_eq!(c.admit(&a), Some(true));
        assert_eq!(c.admit(&b), Some(true));
        assert_eq!(c.admit(&a), Some(false), "a replayed event is shown once");
        assert_eq!(
            c.resume("ws://h/events?workspace=w"),
            "ws://h/events?workspace=w&since=8"
        );
        assert_eq!(
            c.admit(&node("UPDATE")),
            None,
            "no seq: the old filter decides"
        );
    }

    #[test]
    fn a_node_delete_says_deleted() {
        let line = format_event(&node("DELETE"), "t").unwrap();
        assert!(line.contains("outcome (deleted)"), "{line}");
    }

    #[test]
    fn an_update_is_not_a_second_node() {
        let line = format_event(&node("UPDATE"), "05:01:12").unwrap();
        assert!(line.contains("outcome (updated)"), "{line}");
    }

    #[test]
    fn a_missing_title_falls_back_to_the_change_id_prefix() {
        let mut e = node("INSERT");
        e.as_object_mut().unwrap().remove("title");
        assert_eq!(
            format_event(&e, "05:01:12").unwrap(),
            "05:01:12  agent-3   outcome     83c0f4bb"
        );
    }

    #[test]
    fn a_missing_branch_prints_a_dash() {
        let mut e = node("INSERT");
        e.as_object_mut().unwrap().remove("branch");
        assert!(format_event(&e, "t")
            .unwrap()
            .starts_with("t  -         outcome"));
    }

    #[test]
    fn an_edge_shows_both_endpoints() {
        let e = json!({
            "table": "decision_edges", "op": "INSERT", "workspace": "tetris-arena",
            "id": "x", "edge_type": "chosen", "branch": "agent-3",
            "from_change_id": "a1b2c3d4-0000", "to_change_id": "e5f6a7b8-0000"
        });
        assert_eq!(
            format_event(&e, "t").unwrap(),
            "t  agent-3   edge chosen  a1b2c3d4 -> e5f6a7b8"
        );
    }

    #[test]
    fn a_board_post_shows_who_wrote_to_whom() {
        // The frame the 1.0.9 server emits (board post 18, item 12).
        let e = json!({
            "table": "agent_messages", "op": "INSERT", "event": "message_posted",
            "workspace": "deciduous", "id": 18, "author": "board-server",
            "subject": "choices", "mentions": ["board-cli", "orchestrator"],
            "reply_to": 16, "branch": null, "seq": 3, "at": "2026-09-26T18:00:00Z"
        });
        assert_eq!(
            format_event(&e, "t").unwrap(),
            "t  -         message #18 re #16  board-server -> board-cli, orchestrator \"choices\""
        );
        assert!(Filter::default().admits(&e));
    }

    #[test]
    fn a_frame_without_a_table_is_not_an_event() {
        assert!(format_event(&json!({"hello": 1}), "t").is_none());
    }

    #[test]
    fn edges_are_hidden_unless_asked_for() {
        let e = json!({"table": "decision_edges", "op": "INSERT", "id": "x"});
        assert!(!Filter::default().admits(&e));
        let f = Filter {
            edges: true,
            ..Default::default()
        };
        assert!(f.admits(&e));
    }

    #[test]
    fn type_and_branch_filters_narrow_nodes() {
        let f = Filter {
            types: vec!["outcome".into()],
            branches: vec!["agent-3".into()],
            ..Default::default()
        };
        assert!(f.admits(&node("INSERT")));

        let mut other_type = node("INSERT");
        other_type["node_type"] = json!("option");
        assert!(!f.admits(&other_type));

        let mut other_branch = node("INSERT");
        other_branch["branch"] = json!("agent-4");
        assert!(!f.admits(&other_branch));

        let mut no_branch = node("INSERT");
        no_branch.as_object_mut().unwrap().remove("branch");
        assert!(
            !f.admits(&no_branch),
            "no branch cannot be the asked-for branch"
        );
    }

    #[test]
    fn a_repeated_frame_prints_once_inside_the_window() {
        let mut d = Dedup::new(Duration::from_secs(60));
        let t0 = Instant::now();
        let frame = node("INSERT").to_string();
        assert!(d.first_sighting_at(&frame, t0));
        assert!(!d.first_sighting_at(&frame, t0 + Duration::from_secs(1)));
        assert!(d.first_sighting_at(&frame, t0 + Duration::from_secs(61)));
    }

    #[test]
    fn two_different_updates_to_one_node_both_print() {
        let mut d = Dedup::new(Duration::from_secs(60));
        let t0 = Instant::now();
        let mut first = node("UPDATE");
        first["status"] = json!("active");
        let mut second = node("UPDATE");
        second["status"] = json!("completed");
        assert!(d.first_sighting_at(&first.to_string(), t0));
        assert!(
            d.first_sighting_at(&second.to_string(), t0 + Duration::from_secs(1)),
            "a second update that says something different is a second event"
        );
    }
}
