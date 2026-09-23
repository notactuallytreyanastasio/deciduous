//! The shared graph server (Elixir, `deciduous_mcp`) over real HTTP MCP.
//!
//! Gated on `DECIDUOUS_E2E_SERVER` + `DECIDUOUS_E2E_TOKEN` (see
//! `tests/e2e_support/mod.rs`). Every test works in workspaces it named
//! itself, so a shared test server is safe; production is not.

mod e2e_support;

use e2e_support::*;
use serde_json::json;

/// Error text a client may see: never a stack trace, a struct dump, or a row.
fn assert_clean(e: &str, what: &str) {
    for leak in [
        "Postgrex",
        "Ecto.",
        "stacktrace",
        "** (",
        "%DeciduousMcp",
        "%{__struct__",
        "lib/deciduous_mcp",
        "#PID<",
        "Hermes.",
        "Frame",
    ] {
        assert!(
            !e.contains(leak),
            "{what}: error leaks internals ({leak}): {e}"
        );
    }
}

fn add(m: &mut HttpMcp, ws: &str, node_type: &str, title: &str, parent: Option<&str>) -> String {
    let mut args = json!({"node_type": node_type, "title": title, "workspace": ws});
    if let Some(p) = parent {
        args["parent_id"] = json!(p);
    }
    m.call_ok("add_node", args)["id"]
        .as_str()
        .expect("add_node returns an id")
        .to_string()
}

fn count(server: &Server, ws: &str) -> usize {
    server.export(ws)["nodes"].as_array().map_or(0, Vec::len)
}

// ---------------------------------------------------------------- S1 / S2

/// S1: close_thread's goal_node_id went straight to update_node with no
/// workspace or pin check: a session pinned to one workspace completed a
/// goal in another.
#[test]
fn s1_close_thread_cannot_touch_another_workspace() {
    let Some(server) = remote("s1_close_thread_cannot_touch_another_workspace") else {
        return;
    };
    let other = unique("s1-other");
    let mine = unique("s1-mine");
    let mut o = server.session(Some(&other));
    let goal = add(&mut o, &other, "goal", "other team's goal", None);

    let mut m = server.session(Some(&mine));
    let r = m.call(
        "close_thread",
        json!({"title": "x", "goal_node_id": goal, "workspace": mine}),
    );
    assert!(
        r.is_err(),
        "close_thread completed another workspace's goal: {r:?}"
    );
    let status = server.export(&other)["nodes"][0]["status"].clone();
    assert_eq!(
        status,
        json!("pending"),
        "the other workspace's goal changed"
    );
    assert_eq!(
        count(&server, &mine),
        0,
        "a refused close_thread still wrote an outcome"
    );
}

/// S2: a pin only guarded writes. A pinned session could read another
/// workspace's node (description, prompt), traverse it, and list every
/// project on the server.
#[test]
fn s2_a_pinned_session_cannot_read_other_workspaces() {
    let Some(server) = remote("s2_a_pinned_session_cannot_read_other_workspaces") else {
        return;
    };
    let other = unique("s2-other");
    let mine = unique("s2-mine");
    let mut o = server.session(Some(&other));
    let top = add(&mut o, &other, "goal", "OTHER-SECRET-TOP", None);
    let _child = add(&mut o, &other, "action", "OTHER-SECRET-CHILD", Some(&top));
    let mut m = server.session(Some(&mine));
    add(&mut m, &mine, "goal", "mine", None);

    for tool in ["show_node", "get_descendants", "get_ancestors"] {
        let r = m.call(tool, json!({"node_id": top}));
        let text = format!("{r:?}");
        assert!(
            !text.contains("OTHER-SECRET"),
            "{tool} from a pinned session read another workspace: {text}"
        );
    }
    let r = m.call("list_workspaces", json!({}));
    let text = format!("{r:?}");
    assert!(
        !text.contains(&other),
        "list_workspaces from a pinned session named another workspace: {text}"
    );
}

/// The pin holds for writes too (it did; keep it so): a pinned session
/// cannot write into another workspace by argument or by id.
#[test]
fn s2_a_pinned_session_cannot_write_other_workspaces() {
    let Some(server) = remote("s2_a_pinned_session_cannot_write_other_workspaces") else {
        return;
    };
    let other = unique("s2w-other");
    let mine = unique("s2w-mine");
    let mut o = server.session(Some(&other));
    let theirs = add(&mut o, &other, "goal", "theirs", None);
    let mut m = server.session(Some(&mine));
    let _ = m.call(
        "add_node",
        json!({"node_type": "goal", "title": "sneak", "workspace": other}),
    );
    let r1 = m.call(
        "update_node",
        json!({"node_id": theirs, "title": "defaced"}),
    );
    let r2 = m.call("delete_node", json!({"node_id": theirs}));
    let r3 = m.call(
        "add_node",
        json!({"node_type": "action", "title": "x", "parent_id": theirs}),
    );
    assert!(
        r1.is_err() && r2.is_err() && r3.is_err(),
        "{r1:?} {r2:?} {r3:?}"
    );
    let v = server.view(&other);
    assert_eq!(
        v.nodes.len(),
        1,
        "another workspace was written: {:?}",
        v.nodes
    );
    assert_eq!(v.nodes.values().next().unwrap().title, "theirs");
}

// ---------------------------------------------------------------- S3

/// S3: close_thread was not atomic: a bad parent_node_id, goal_node_id, or
/// lessons/next_steps item failed (or crashed) after the outcome was written.
#[test]
fn s3_close_thread_is_all_or_nothing() {
    let Some(server) = remote("s3_close_thread_is_all_or_nothing") else {
        return;
    };
    let ws = unique("s3");
    let mut m = server.session(Some(&ws));
    let cases = [
        json!({"title": "o", "parent_node_id": "00000000-0000-4000-8000-000000000000"}),
        json!({"title": "o", "goal_node_id": "abc"}),
        json!({"title": "o", "goal_node_id": "PLACEHOLDER_SKIP"}),
        json!({"title": "o", "lessons_learned": [1]}),
        json!({"title": "o", "next_steps": ["a string, not an object"]}),
    ];
    for args in cases {
        let r = m.call("close_thread", args.clone());
        match &r {
            Ok(v) => panic!("close_thread {args} succeeded: {v}"),
            Err(e) => assert_clean(e, &format!("close_thread {args}")),
        }
        assert_eq!(
            count(&server, &ws),
            0,
            "close_thread {args} left a partial write"
        );
    }
}

// ---------------------------------------------------------------- S4

/// S4: update_node with metadata replaced the whole map, dropping branch,
/// prompt, commit and files; "999" passed the confidence range check.
#[test]
fn s4_update_node_merges_metadata() {
    let Some(server) = remote("s4_update_node_merges_metadata") else {
        return;
    };
    let ws = unique("s4");
    let mut m = server.session(Some(&ws));
    // One session, so no lock to dodge, and an injected branch on the update
    // would be a second branch value this test is not about.
    m.branch = None;
    let id = m.call_ok(
        "add_node",
        json!({"node_type": "goal", "title": "g", "prompt": "the prompt", "branch": "feat",
               "commit": "abc123", "files": ["a.rs"]}),
    )["id"]
        .as_str()
        .unwrap()
        .to_string();
    m.call_ok(
        "update_node",
        json!({"node_id": id, "metadata": {"confidence": 50}}),
    );
    let meta = server.export(&ws)["nodes"][0]["metadata"].clone();
    for (k, v) in [
        ("prompt", json!("the prompt")),
        ("branch", json!("feat")),
        ("commit", json!("abc123")),
        ("files", json!(["a.rs"])),
        ("confidence", json!(50)),
    ] {
        assert_eq!(
            meta[k], v,
            "metadata.{k} after a confidence-only update: {meta}"
        );
    }
    let r = m.call(
        "update_node",
        json!({"node_id": id, "metadata": {"confidence": "999"}}),
    );
    assert!(r.is_err(), "confidence \"999\" was accepted");
}

// ---------------------------------------------------------------- S5 / S6

/// S5 / S6: soft-deleted nodes stayed readable and writable, were traversed
/// through, and their edges still counted as incoming for find_orphans.
#[test]
fn s5_s6_deleted_nodes_are_gone_for_every_tool() {
    let Some(server) = remote("s5_s6_deleted_nodes_are_gone_for_every_tool") else {
        return;
    };
    let ws = unique("s5");
    let mut m = server.session(Some(&ws));
    let a = add(&mut m, &ws, "goal", "A", None);
    let b = add(&mut m, &ws, "action", "B zombie", Some(&a));
    let c = add(&mut m, &ws, "outcome", "C", Some(&b));
    m.call_ok("delete_node", json!({"node_id": b}));

    let shown = m.call("show_node", json!({"node_id": b}));
    assert!(
        shown.is_err() || format!("{shown:?}").contains("deleted"),
        "show_node returned a deleted node as live: {shown:?}"
    );
    let upd = m.call(
        "update_node",
        json!({"node_id": b, "title": "B edited after delete"}),
    );
    assert!(upd.is_err(), "update_node on a deleted node succeeded");
    let again = m.call("delete_node", json!({"node_id": b}));
    assert!(again.is_err(), "deleting a deleted node succeeded");
    let desc = format!("{:?}", m.call("get_descendants", json!({"node_id": a})));
    assert!(
        !desc.contains("B zombie"),
        "get_descendants walks through a deleted node: {desc}"
    );
    let anc = format!("{:?}", m.call("get_ancestors", json!({"node_id": c})));
    assert!(
        !anc.contains("B zombie"),
        "get_ancestors walks through a deleted node: {anc}"
    );
    let orphans = m.call_ok("find_orphans", json!({}));
    assert!(
        orphans.to_string().contains(&c),
        "C lost its only live parent and find_orphans does not report it: {orphans}"
    );
}

// ---------------------------------------------------------------- S7

/// S7: Workspaces.find_or_create was check-then-insert: parallel first
/// writes to a new workspace failed "has already been taken", and parallel
/// initializes with a new pin header answered HTTP 500 with an empty body.
#[test]
fn s7_first_writes_to_a_new_workspace_race_cleanly() {
    let Some(server) = remote("s7_first_writes_to_a_new_workspace_race_cleanly") else {
        return;
    };
    let ws = unique("s7");
    let failures: Vec<String> = std::thread::scope(|scope| {
        let hs: Vec<_> = (0..40)
            .map(|i| {
                let server = server.clone();
                let ws = ws.clone();
                scope.spawn(move || {
                    let mut m = server.session(None);
                    m.call(
                        "add_node",
                        json!({"node_type": "goal", "title": format!("n{i}"), "workspace": ws}),
                    )
                    .err()
                })
            })
            .collect();
        hs.into_iter().filter_map(|h| h.join().unwrap()).collect()
    });
    assert!(
        failures.is_empty(),
        "{} of 40 first writes failed: {:?}",
        failures.len(),
        failures
    );
    assert_eq!(count(&server, &ws), 40);

    let ws2 = unique("s7h");
    let bad: Vec<u16> = std::thread::scope(|scope| {
        let hs: Vec<_> = (0..20)
            .map(|_| {
                let server = server.clone();
                let ws2 = ws2.clone();
                scope.spawn(move || {
                    let init = json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"e2e","version":"0"}}});
                    server
                        .request(
                            "POST",
                            "/mcp",
                            Some(&server.bearer()),
                            &[
                                ("content-type", "application/json"),
                                ("accept", "application/json, text/event-stream"),
                                ("x-deciduous-workspace", &ws2),
                            ],
                            Some(init.to_string().as_bytes()),
                        )
                        .status
                })
            })
            .collect();
        hs.into_iter()
            .map(|h| h.join().unwrap())
            .filter(|s| *s != 200)
            .collect()
    });
    assert!(
        bad.is_empty(),
        "parallel pinned initializes failed: {bad:?}"
    );
}

// ---------------------------------------------------------------- S8

/// S8: NUL bytes, empty titles and an out-of-range limit crashed the handler
/// with a Postgrex struct, a stack trace, sometimes the full row, in `data`.
#[test]
fn s8_bad_input_gets_a_clean_error() {
    let Some(server) = remote("s8_bad_input_gets_a_clean_error") else {
        return;
    };
    let ws = unique("s8");
    let mut m = server.session(Some(&ws));
    let id = add(&mut m, &ws, "goal", "g", None);
    let cases = [
        (
            "add_node",
            json!({"node_type": "goal", "title": "nul\u{0}byte"}),
        ),
        (
            "add_node",
            json!({"node_type": "goal", "title": "t", "description": "d\u{0}"}),
        ),
        (
            "add_node",
            json!({"node_type": "goal", "title": "t", "prompt": "p\u{0}"}),
        ),
        ("log_decision", json!({"title": "nul\u{0}", "options": []})),
        ("log_observation", json!({"title": "nul\u{0}"})),
        ("log_observation", json!({"title": ""})),
        ("update_node", json!({"node_id": id, "title": ""})),
        (
            "add_node",
            json!({"node_type": "goal", "title": "t", "workspace": "ws\u{0}nul"}),
        ),
    ];
    for (tool, args) in cases {
        match m.call(tool, args.clone()) {
            Ok(v) => panic!("{tool} {args} was accepted: {v}"),
            Err(e) => assert_clean(&e, &format!("{tool} {args}")),
        }
    }
    if let Err(e) = m.call("query_nodes", json!({"limit": 9223372036854775807_u64})) {
        assert_clean(&e, "query_nodes limit 2^63");
    }
}

// ---------------------------------------------------------------- S9

/// S9: query_nodes search did not escape LIKE metacharacters: "%" and "_"
/// matched everything, and a backslash could not be searched for.
#[test]
fn s9_search_is_literal() {
    let Some(server) = remote("s9_search_is_literal") else {
        return;
    };
    let ws = unique("s9");
    let mut m = server.session(Some(&ws));
    for t in ["abc", "axc", "meta back\\slash", "100% sure"] {
        add(&mut m, &ws, "goal", t, None);
    }
    let titles = |m: &mut HttpMcp, q: &str| -> Vec<String> {
        let r = m.call_ok("query_nodes", json!({"search": q, "workspace": ws}));
        let mut v: Vec<String> = r["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["title"].as_str().unwrap().to_string())
            .collect();
        v.sort();
        v
    };
    assert_eq!(titles(&mut m, "%"), vec!["100% sure"]);
    assert_eq!(titles(&mut m, "a_c"), Vec::<String>::new());
    assert_eq!(titles(&mut m, "back\\slash"), vec!["meta back\\slash"]);
    assert_eq!(titles(&mut m, "\\"), vec!["meta back\\slash"]);
}

// ---------------------------------------------------------------- S10

fn raw(server: &Server, session: Option<&str>, body: &[u8]) -> HttpResp {
    let mut h = vec![
        ("content-type", "application/json"),
        ("accept", "application/json, text/event-stream"),
    ];
    if let Some(s) = session {
        h.push(("mcp-session-id", s));
    }
    server.request("POST", "/mcp", Some(&server.bearer()), &h, Some(body))
}

/// S10: an empty body was a 500 with no body; batches, id:null and unknown
/// notifications were all "Parse error"; tools/call with no params dumped the
/// session Frame; a call without a session header got a fabricated id.
#[test]
fn s10_protocol_edges_get_protocol_answers() {
    let Some(server) = remote("s10_protocol_edges_get_protocol_answers") else {
        return;
    };
    let m = server.session(Some(&unique("s10")));
    let sid = m.session.clone();
    let sid = sid.as_deref();

    let r = raw(&server, sid, b"");
    assert!(
        r.status < 500,
        "empty body -> HTTP {}: {:?}",
        r.status,
        r.body
    );

    let r = raw(
        &server,
        sid,
        br#"{"jsonrpc":"2.0","method":"notifications/unknown_thing"}"#,
    );
    assert!(
        r.status == 202 || r.status == 200 || r.status == 204,
        "an unknown notification was refused: {} {}",
        r.status,
        r.body
    );

    let r = raw(
        &server,
        sid,
        br#"[{"jsonrpc":"2.0","id":1,"method":"ping"},{"jsonrpc":"2.0","id":2,"method":"ping"}]"#,
    );
    let code = r.rpc().map(|v| v["error"]["code"].clone());
    assert_ne!(
        code,
        Some(json!(-32700)),
        "a well-formed batch was called a parse error: {}",
        r.body
    );

    let r = raw(
        &server,
        sid,
        br#"{"jsonrpc":"2.0","id":77,"method":"tools/call"}"#,
    );
    let v = r
        .rpc()
        .unwrap_or_else(|| panic!("no JSON-RPC reply: {} {}", r.status, r.body));
    assert_eq!(
        v["id"],
        json!(77),
        "tools/call with no params lost its id: {v}"
    );
    assert_clean(&r.body, "tools/call with no params");

    let r = raw(
        &server,
        None,
        br#"{"jsonrpc":"2.0","id":78,"method":"tools/call","params":{"name":"list_workspaces","arguments":{}}}"#,
    );
    if let Some(v) = r.rpc() {
        assert_eq!(
            v["id"],
            json!(78),
            "a sessionless call got someone else's id: {v}"
        );
    }
}

// ---------------------------------------------------------------- S11

/// S11: reads created workspaces, " *" became a literal workspace, schema
/// bounds and enums were not enforced, and there was no size limit.
#[test]
fn s11_reads_have_no_side_effects_and_input_is_validated() {
    let Some(server) = remote("s11_reads_have_no_side_effects_and_input_is_validated") else {
        return;
    };
    let mut m = server.session(None);
    let ghost = unique("s11-ghost");
    let _ = m.call("query_nodes", json!({"workspace": ghost}));
    let _ = m.call("get_graph", json!({"workspace": ghost}));
    let list = m.call_ok("list_workspaces", json!({}));
    assert!(
        !list.to_string().contains(&ghost),
        "a read created workspace {ghost}"
    );

    let r = m.call(
        "add_node",
        json!({"node_type": "goal", "title": "t", "workspace": " *"}),
    );
    assert!(r.is_err(), "workspace \" *\" was accepted");

    let ws = unique("s11");
    let mut p = server.session(Some(&ws));
    let seed = add(&mut p, &ws, "goal", "seed", None);
    let bad = [
        ("get_graph", json!({"max_nodes": 50000})),
        ("get_graph", json!({"max_nodes": 0})),
        ("get_graph", json!({"max_nodes": -1})),
        // get_descendants is the tool the finding named; get_graph takes no
        // max_depth, and an argument a tool does not declare is ignored.
        ("get_descendants", json!({"node_id": seed, "max_depth": 0})),
        ("add_node", json!({"node_type": "feedback", "title": "t"})),
        (
            "add_node",
            json!({"node_type": "goal", "title": "t", "status": "done"}),
        ),
        (
            "add_node",
            json!({"node_type": "goal", "title": "t", "files": 5}),
        ),
        (
            "add_node",
            json!({"node_type": "goal", "title": "t", "commit": {"x": 1}}),
        ),
        (
            "add_node",
            json!({"node_type": "goal", "title": "x".repeat(1 << 20)}),
        ),
    ];
    for (tool, args) in bad {
        let short: String = args.to_string().chars().take(80).collect();
        let r = p.call(tool, args);
        assert!(r.is_err(), "{tool} {short} was accepted");
    }
    assert_eq!(count(&server, &ws), 1, "an invalid add_node still wrote");
}

// ---------------------------------------------------------------- auth

/// Auth on every route (it held; keep it held), including any route added
/// for the operation log. Health and readiness are the only open routes.
#[test]
fn auth_every_route_refuses_a_missing_or_wrong_token() {
    let Some(server) = remote("auth_every_route_refuses_a_missing_or_wrong_token") else {
        return;
    };
    let ws = unique("auth");
    let routes: &[(&str, String)] = &[
        ("POST", "/mcp".to_string()),
        ("GET", "/mcp".to_string()),
        ("DELETE", "/mcp".to_string()),
        ("POST", "/import".to_string()),
        ("GET", format!("/export?workspace={ws}")),
        ("GET", format!("/events?workspace={ws}")),
        (
            "GET",
            "/documents/00000000-0000-4000-8000-000000000000".to_string(),
        ),
        ("PUT", format!("/blob/{}", "0".repeat(64))),
        ("POST", "/ops".to_string()),
        ("POST", format!("/ops?workspace={ws}")),
        ("GET", format!("/ops?workspace={ws}")),
        ("POST", "/replay".to_string()),
        ("GET", format!("/status?workspace={ws}")),
    ];
    for (method, path) in routes {
        for auth in [None, Some("Bearer wrong-token"), Some("Bearer ")] {
            let r = server.request(
                method,
                path,
                auth,
                &[("content-type", "application/json")],
                (*method != "GET").then_some(b"{}".as_slice()),
            );
            assert!(
                r.status == 401 || r.status == 404 || r.status == 405,
                "{method} {path} with auth {auth:?} answered {}: {}",
                r.status,
                r.body.chars().take(200).collect::<String>()
            );
            assert!(
                r.status != 200,
                "{method} {path} served without a valid token"
            );
        }
    }
    let h = server.request("GET", "/health", None, &[], None);
    assert_eq!(h.status, 200);
    assert_eq!(count(&server, &ws), 0);
}
