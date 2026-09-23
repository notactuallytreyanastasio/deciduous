//! Protocol fuzz for both MCP servers: the Rust one over stdio, the Elixir
//! one over HTTP. Seeded mutations of real requests, plus raw garbage,
//! invalid UTF-8, NUL bytes, lone surrogates, deep nesting and huge lines.
//!
//! For every input, whatever it is:
//! * the server answers within a deadline and is still answering after it
//!   (a probe request after each input must get its reply);
//! * every reply is JSON-RPC 2.0, and when the input was a request with an
//!   id, a reply carries that id;
//! * nothing is a crash, a 5xx, or text leaking internals (stack traces,
//!   struct dumps).
//!
//! `DECIDUOUS_E2E_SEED` replays a run; `DECIDUOUS_E2E_STEPS` sets how many
//! inputs. Gated like the rest (see `tests/e2e_support/mod.rs`).

mod e2e_support;

use e2e_support::*;
use serde_json::{json, Value};
use std::time::Duration;

const METHODS: &[&str] = &[
    "tools/call",
    "tools/list",
    "initialize",
    "ping",
    "resources/list",
    "prompts/list",
    "notifications/initialized",
    "notifications/cancelled",
    "no/such/method",
    "",
];
const TOOLS: &[&str] = &[
    "add_node",
    "link_nodes",
    "update_status",
    "show_node",
    "list_nodes",
    "search_nodes",
    "query_nodes",
    "get_graph",
    "update_node",
    "log_observation",
    "no_such_tool",
];

fn random_value(rng: &mut Rng, depth: usize) -> Value {
    match rng.below(if depth > 3 { 6 } else { 9 }) {
        0 => Value::Null,
        1 => json!(rng.chance(50)),
        2 => json!(rng.next() as i64),
        3 => json!(rng.below(20)),
        4 => json!(-(rng.below(5) as i64)),
        5 => Value::String(random_string(rng)),
        6 => json!(1e308 * if rng.chance(50) { 1.0 } else { -1.0 }),
        7 => Value::Array(
            (0..rng.below(4))
                .map(|_| random_value(rng, depth + 1))
                .collect(),
        ),
        _ => {
            let mut m = serde_json::Map::new();
            for _ in 0..rng.below(4) {
                let k = [
                    "node_type",
                    "title",
                    "node_id",
                    "from_id",
                    "to_id",
                    "status",
                    "limit",
                    "x",
                ][rng.below(8)];
                m.insert(k.to_string(), random_value(rng, depth + 1));
            }
            Value::Object(m)
        }
    }
}

fn random_string(rng: &mut Rng) -> String {
    let pieces = [
        "",
        "a",
        "goal",
        "pending",
        "\u{0}",
        "\u{202e}evil",
        "%",
        "_",
        "\\",
        "\"",
        "'; DROP TABLE x;--",
        "\u{1F600}",
        "\n",
        "0",
        "-1",
        "4294967298",
        "../../etc/passwd",
    ];
    let mut s = String::new();
    for _ in 0..rng.below(4) {
        s.push_str(pieces[rng.below(pieces.len())]);
    }
    if rng.chance(5) {
        s.push_str(&"x".repeat(1 + rng.below(200_000)));
    }
    s
}

/// One input, and the id a reply must carry if it is a request.
fn generate(rng: &mut Rng) -> (Vec<u8>, Option<Value>) {
    let id = match rng.below(6) {
        0 => None,
        1 => Some(Value::Null),
        2 => Some(json!(format!("fz-{}", rng.below(1_000_000)))),
        3 => Some(json!(1.5)),
        _ => Some(json!(1000 + rng.below(1_000_000))),
    };
    let method = METHODS[rng.below(METHODS.len())];
    let mut msg = json!({"jsonrpc": "2.0", "method": method});
    if let Some(i) = &id {
        msg["id"] = i.clone();
    }
    if rng.chance(80) {
        msg["params"] = if method == "tools/call" && rng.chance(70) {
            json!({"name": TOOLS[rng.below(TOOLS.len())], "arguments": random_value(rng, 0)})
        } else {
            random_value(rng, 0)
        };
    }
    if rng.chance(10) {
        msg["jsonrpc"] = random_value(rng, 3);
    }
    let mut bytes = msg.to_string().into_bytes();
    let expect_id = id.filter(|v| v.is_string() || v.is_i64() || v.is_u64());
    let mut mutated = false;
    match rng.below(10) {
        0 => {
            // flip bytes
            for _ in 0..1 + rng.below(4) {
                let i = rng.below(bytes.len());
                bytes[i] = (rng.next() & 0xff) as u8;
            }
            mutated = true;
        }
        1 => {
            bytes.truncate(rng.below(bytes.len()));
            mutated = true;
        }
        2 => {
            // invalid UTF-8 inside a string value
            let s = String::from_utf8_lossy(&bytes).replace("\"2.0\"", "\"2.\u{0}0\"");
            bytes = s.into_bytes();
            let at = rng.below(bytes.len());
            bytes.splice(at..at, [0xff, 0xfe, 0xc3]);
            mutated = true;
        }
        3 => {
            let depth = 1 + rng.below(20_000);
            bytes = format!("{}{}", "[".repeat(depth), "]".repeat(depth)).into_bytes();
            mutated = true;
        }
        4 => {
            bytes = (0..1 + rng.below(64))
                .map(|_| (rng.next() & 0xff) as u8)
                .collect();
            mutated = true;
        }
        5 => {
            let s = String::from_utf8_lossy(&bytes).replace("\"2.0\"", "\"2.0\\ud800\"");
            bytes = s.into_bytes();
            mutated = true;
        }
        6 => {
            bytes = format!("[{},{}]", msg, msg).into_bytes();
            mutated = true;
        }
        _ => {}
    }
    // A newline would make two stdio messages; this fuzzer sends one.
    for b in bytes.iter_mut() {
        if *b == b'\n' || *b == b'\r' {
            *b = b' ';
        }
    }
    let expect = if mutated {
        // Only hold the id to account if the mutated bytes still parse to
        // the same request.
        serde_json::from_slice::<Value>(&bytes)
            .ok()
            .filter(|v| v.get("method").is_some_and(Value::is_string))
            .and_then(|v| v.get("id").cloned())
            .filter(|v| v.is_string() || v.is_i64() || v.is_u64())
    } else {
        expect_id
    };
    (bytes, expect)
}

fn show(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(&bytes[..bytes.len().min(300)]).into_owned();
    if bytes.len() > 300 {
        format!("{s}... ({} bytes)", bytes.len())
    } else {
        s
    }
}

const LEAKS: &[&str] = &[
    "panicked at",
    "RUST_BACKTRACE",
    "stack backtrace",
    "Postgrex",
    "Ecto.",
    "** (",
    "%DeciduousMcp",
    "Frame",
    "#PID<",
];

#[test]
fn fuzz_stdio_mcp_never_crashes_hangs_or_drops_an_id() {
    let Some(()) = local("fuzz_stdio_mcp_never_crashes_hangs_or_drops_an_id") else {
        return;
    };
    let seed = seed();
    let n = steps(300);
    eprintln!("stdio fuzz: seed = {seed}, {n} inputs (replay with DECIDUOUS_E2E_SEED={seed})");
    let sb = Sandbox::new();
    let p = sb.project("fuzz", None);
    p.add("goal", "a node to aim at");
    let mut m = StdioMcp::spawn(&sb, &p.dir);
    let mut rng = Rng::new(seed);
    for k in 0..n {
        let (input, expect) = generate(&mut rng);
        let fail = |why: String| -> ! {
            panic!(
                "stdio fuzz input #{k}: {why}\ninput: {}\nseed = {seed} (replay with DECIDUOUS_E2E_SEED={seed})",
                show(&input)
            )
        };
        m.send_raw(&input);
        let probe = format!("probe-{k}");
        m.send_raw(
            json!({"jsonrpc":"2.0","id":probe,"method":"ping"})
                .to_string()
                .as_bytes(),
        );
        let mut seen_expected = expect.is_none();
        loop {
            let Some(reply) = m.recv(Duration::from_secs(15)) else {
                let alive = m.alive();
                fail(format!(
                    "no answer to the probe within 15s (server {}); stderr:\n{}",
                    if alive { "hung" } else { "exited" },
                    m.stderr.lock().unwrap()
                ));
            };
            let text = reply.to_string();
            for leak in LEAKS {
                if text.contains(leak) {
                    fail(format!("reply leaks internals ({leak}): {text}"));
                }
            }
            if reply["jsonrpc"] != json!("2.0") {
                fail(format!("reply is not JSON-RPC 2.0: {text}"));
            }
            if Some(&reply["id"]) == expect.as_ref() {
                seen_expected = true;
            }
            if reply["id"] == json!(probe) {
                break;
            }
        }
        if !seen_expected {
            fail(format!(
                "no reply carried the request id {:?}",
                expect.unwrap()
            ));
        }
    }
    assert!(m.alive(), "the server exited");
    drop(m);
    let _ = p.graph_doc();
}

#[test]
fn fuzz_http_mcp_never_5xx_hangs_or_drops_an_id() {
    let Some(server) = remote("fuzz_http_mcp_never_5xx_hangs_or_drops_an_id") else {
        return;
    };
    let seed = seed();
    let n = steps(300);
    eprintln!("http fuzz: seed = {seed}, {n} inputs (replay with DECIDUOUS_E2E_SEED={seed})");
    let ws = unique("fuzz");
    let m = server.session(Some(&ws));
    let mut rng = Rng::new(seed);
    for k in 0..n {
        let (input, expect) = generate(&mut rng);
        let fail = |why: String| -> ! {
            panic!(
                "http fuzz input #{k}: {why}\ninput: {}\nseed = {seed} (replay with DECIDUOUS_E2E_SEED={seed})",
                show(&input)
            )
        };
        let h = m.headers();
        let hr: Vec<(&str, &str)> = h.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        let r = server
            .try_request(
                "POST",
                "/mcp",
                Some(&server.bearer()),
                &hr,
                Some(&input),
                Duration::from_secs(30),
            )
            .unwrap_or_else(|e| fail(format!("no answer within 30s: {e}")));
        if r.status >= 500 {
            fail(format!("HTTP {}: {}", r.status, show(r.body.as_bytes())));
        }
        for leak in LEAKS {
            if r.body.contains(leak) {
                fail(format!(
                    "reply leaks internals ({leak}): {}",
                    show(r.body.as_bytes())
                ));
            }
        }
        if let Some(id) = &expect {
            match r.rpc() {
                Some(v) if &v["id"] == id => {}
                Some(v) if v.is_array() => {}
                other => fail(format!(
                    "HTTP {}: no reply carried the request id {id}: {other:?}",
                    r.status
                )),
            }
        }
        if k % 10 == 9 {
            let pong = m.post(br#"{"jsonrpc":"2.0","id":"probe","method":"ping"}"#);
            if pong.status != 200 {
                fail(format!(
                    "the session stopped answering: {} {}",
                    pong.status, pong.body
                ));
            }
        }
    }
}
