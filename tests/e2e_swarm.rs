//! `deciduous demo-swarm`'s lead-gated start (commit 53b17f6), driven
//! headless: the arena is built with `--no-window`, then every pane runs the
//! exact command the window would give it (under `script`, which supplies the
//! pty), against a stub `claude` on PATH.
//!
//! The stub behaves like Claude Code where it matters: in a git repository
//! nobody has trusted it prints the trust dialog and waits for an answer on
//! its terminal; trust covers a repository and its worktrees; once a session
//! is up it writes `$CLAUDE_CONFIG_DIR/projects/<dir>/<session-id>.jsonl`.
//!
//! macOS only (the launcher refuses anything else), gated on
//! `DECIDUOUS_E2E=1`. `DECIDUOUS_E2E_SWARM_SCRIPT=<path>` runs the panes with
//! another version of demo-swarm.zsh, for bisecting.

mod e2e_support;

use e2e_support::*;
use std::io::{Read, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const STUB: &str = r#"#!/bin/sh
# A stand-in for Claude Code's start-up: the trust dialog, then a transcript.
sid=""
while [ $# -gt 0 ]; do
  case "$1" in --session-id) sid=$2; shift 2 ;; *) shift ;; esac
done
cfg=${CLAUDE_CONFIG_DIR:-$HOME/.claude}
mkdir -p "$cfg"
echo "start $$ $sid $(pwd)" >> "$STUB_LOG"
root=$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null)
root=${root%/.git}
if ! grep -qxF "$root" "$cfg/trusted" 2>/dev/null; then
  echo "dialog $$ $sid" >> "$STUB_LOG"
  printf 'Quick safety check: Is this a project you created or one you trust?\n  No, exit\n  Yes, I trust this folder\n'
  if ! IFS= read -r ans; then echo "eof $$" >> "$STUB_LOG"; exit 1; fi
  case "$ans" in
    y*|Y*) echo "$root" >> "$cfg/trusted" ;;
    *) echo "declined $$" >> "$STUB_LOG"; exit 1 ;;
  esac
fi
d="$cfg/projects/$(pwd | tr / -)"
mkdir -p "$d"
echo '{"type":"summary","summary":"stub session"}' > "$d/$sid.jsonl"
echo "transcript $$ $sid" >> "$STUB_LOG"
sleep 4
echo "exit $$" >> "$STUB_LOG"
"#;

struct Pane {
    name: String,
    child: Child,
    out: Arc<Mutex<String>>,
}

impl Pane {
    fn output(&self) -> String {
        self.out.lock().unwrap().clone()
    }
    fn send(&mut self, s: &str) {
        let stdin = self.child.stdin.as_mut().unwrap();
        stdin.write_all(s.as_bytes()).unwrap();
        stdin.flush().unwrap();
    }
    /// Closing a terminal pane hangs up its whole process group.
    fn close(&mut self) {
        let pgid = self.child.id();
        let _ = Command::new("kill")
            .args(["-HUP", "--", &format!("-{pgid}")])
            .stderr(Stdio::null())
            .status();
    }
}

impl Drop for Pane {
    fn drop(&mut self) {
        let pgid = self.child.id();
        let _ = Command::new("kill")
            .args(["-KILL", "--", &format!("-{pgid}")])
            .stderr(Stdio::null())
            .status();
        let _ = self.child.wait();
    }
}

struct Arena {
    _sb: Sandbox,
    dir: PathBuf,
    config: PathBuf,
    log: PathBuf,
    env: Vec<(String, String)>,
    commands: Vec<(String, String)>,
}

fn available() -> bool {
    cfg!(target_os = "macos")
        && ["zsh", "script", "uuidgen", "osascript"].iter().all(|b| {
            Command::new("which")
                .arg(b)
                .output()
                .is_ok_and(|o| o.status.success())
        })
}

fn build(test: &str) -> Option<Arena> {
    local(test)?;
    if !available() {
        eprintln!("skipped: {test} (needs macOS with zsh, script, uuidgen, osascript)");
        return None;
    }
    let sb = Sandbox::new();
    let base = sb.base();
    let stub_dir = base.join("stub-bin");
    std::fs::create_dir_all(&stub_dir).unwrap();
    let stub = stub_dir.join("claude");
    std::fs::write(&stub, STUB).unwrap();
    std::fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let config = base.join("claude-config");
    let log = base.join("stub.log");
    std::fs::write(&log, "").unwrap();
    let dir = base.join("arena");
    let env = vec![
        ("HOME".to_string(), sb.home.display().to_string()),
        (
            "CLAUDE_CONFIG_DIR".to_string(),
            config.display().to_string(),
        ),
        (
            "PATH".to_string(),
            format!("{}:{}", stub_dir.display(), sb.path_env),
        ),
        ("STUB_LOG".to_string(), log.display().to_string()),
        ("TERM_PROGRAM".to_string(), "iTerm.app".to_string()),
        ("TERM".to_string(), "xterm-256color".to_string()),
    ];
    let mut c = Command::new(bin());
    c.args(["demo-swarm", "--no-window", "--dir", dir.to_str().unwrap()])
        .current_dir(&base)
        .stdin(Stdio::null());
    sb.apply(&mut c);
    for (k, v) in &env {
        c.env(k, v);
    }
    let out: Out = c.output().unwrap().into();
    assert!(out.ok(), "demo-swarm --no-window failed:\n{}", out.all());
    // `--no-window` is a dry run; these panes run for real.
    let env_file = dir.join(".swarm/env");
    let text = std::fs::read_to_string(&env_file).unwrap();
    assert!(text.contains("DRY=1"), "{text}");
    std::fs::write(&env_file, text.replace("DRY=1", "DRY=0")).unwrap();
    // For bisecting: run the panes with another version of the script.
    if let Ok(other) = std::env::var("DECIDUOUS_E2E_SWARM_SCRIPT") {
        std::fs::copy(&other, dir.join(".swarm/demo-swarm.zsh")).unwrap();
    }
    let commands = out
        .stdout
        .lines()
        .filter_map(|l| l.trim().strip_prefix("cd "))
        .map(|rest| {
            let cmd = format!("cd {rest}");
            let name = cmd.rsplit(' ').next().unwrap().to_string();
            (name, cmd)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        commands.len(),
        5,
        "expected lead + 4 panes:\n{}",
        out.stdout
    );
    Some(Arena {
        _sb: sb,
        dir,
        config,
        log,
        env,
        commands,
    })
}

impl Arena {
    fn open(&self, name: &str) -> Pane {
        let cmd = &self.commands.iter().find(|(n, _)| n == name).unwrap().1;
        let mut c = Command::new("zsh");
        c.arg("-c")
            .arg(cmd)
            .env_clear()
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in &self.env {
            c.env(k, v);
        }
        c.env("COLUMNS", "100").env("LINES", "30");
        let mut child = c.spawn().unwrap();
        let out = Arc::new(Mutex::new(String::new()));
        for stream in [
            Box::new(child.stdout.take().unwrap()) as Box<dyn Read + Send>,
            Box::new(child.stderr.take().unwrap()),
        ] {
            let out = out.clone();
            std::thread::spawn(move || {
                let mut stream = stream;
                let mut buf = [0u8; 4096];
                while let Ok(n) = stream.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    out.lock()
                        .unwrap()
                        .push_str(&String::from_utf8_lossy(&buf[..n]));
                }
            });
        }
        Pane {
            name: name.to_string(),
            child,
            out,
        }
    }

    fn open_all(&self) -> Vec<Pane> {
        ["lead", "w1", "w2", "w3", "w4"]
            .iter()
            .map(|n| self.open(n))
            .collect()
    }

    fn rec(&self, file: &str) -> PathBuf {
        self.dir.join(".swarm/rec").join(file)
    }

    fn log(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    fn transcripts(&self) -> usize {
        fn walk(p: &Path) -> usize {
            std::fs::read_dir(p)
                .map(|rd| {
                    rd.flatten()
                        .map(|e| {
                            let p = e.path();
                            if p.is_dir() {
                                walk(&p)
                            } else {
                                usize::from(p.extension().is_some_and(|x| x == "jsonl"))
                            }
                        })
                        .sum()
                })
                .unwrap_or(0)
        }
        walk(&self.config.join("projects"))
    }
}

fn wait_dialog(lead: &mut Pane) {
    // Any key fast-forwards the tour.
    lead.send("x");
    let shown = wait_for(Duration::from_secs(60), || {
        lead.output().contains("trust this folder").then_some(())
    });
    assert!(
        shown.is_some(),
        "the lead pane never showed the trust dialog:\n{}",
        lead.output()
    );
}

fn stub_pids(log: &str) -> Vec<String> {
    log.lines()
        .filter_map(|l| l.strip_prefix("start "))
        .filter_map(|r| r.split(' ').next())
        .map(str::to_string)
        .collect()
}

#[test]
fn swarm_lead_says_yes_every_pane_gets_a_transcript() {
    let Some(a) = build("swarm_lead_says_yes_every_pane_gets_a_transcript") else {
        return;
    };
    let mut panes = a.open_all();
    wait_dialog(&mut panes[0]);
    panes[0].send("y\n");
    let all = wait_for(Duration::from_secs(90), || {
        ["lead", "w1", "w2", "w3", "w4"]
            .iter()
            .all(|n| a.rec(&format!("{n}.transcript")).exists())
            .then_some(())
    });
    let log = a.log();
    assert!(
        all.is_some(),
        "not every pane recorded its transcript\nstub log:\n{log}\nlead:\n{}",
        panes[0].output()
    );
    assert_eq!(a.transcripts(), 5, "stub log:\n{log}");
    assert_eq!(
        log.matches("dialog ").count(),
        1,
        "the trust question was asked more than once:\n{log}"
    );
    for p in &panes[1..] {
        assert!(
            !p.output().contains("trust this folder"),
            "worker {} showed the trust dialog",
            p.name
        );
    }
    for n in ["lead", "w1", "w2", "w3", "w4"] {
        assert!(
            !a.rec(&format!("{n}.no-transcript")).exists(),
            "{n} wrote a no-transcript note despite a session"
        );
    }
}

#[test]
fn swarm_lead_says_no_workers_refuse_to_start() {
    let Some(a) = build("swarm_lead_says_no_workers_refuse_to_start") else {
        return;
    };
    let mut panes = a.open_all();
    wait_dialog(&mut panes[0]);
    panes[0].send("n\n");
    let workers_done = wait_for(Duration::from_secs(60), || {
        panes[1..]
            .iter_mut()
            .all(|p| matches!(p.child.try_wait(), Ok(Some(_))))
            .then_some(())
    });
    assert!(
        workers_done.is_some(),
        "workers kept waiting after the lead declined:\n{}",
        panes[1].output()
    );
    for p in &panes[1..] {
        assert!(
            p.output().contains("ended before it began"),
            "worker {} did not say why it stopped:\n{}",
            p.name,
            p.output()
        );
    }
    let note = wait_for(Duration::from_secs(10), || {
        a.rec("lead.no-transcript").exists().then_some(())
    });
    assert!(note.is_some(), "the lead's declined session left no note");
    let log = a.log();
    assert_eq!(
        stub_pids(&log).len(),
        1,
        "a worker started claude anyway:\n{log}"
    );
    assert_eq!(a.transcripts(), 0);
}

#[test]
fn swarm_unanswered_and_closed_leaves_notes_and_no_processes() {
    let Some(a) = build("swarm_unanswered_and_closed_leaves_notes_and_no_processes") else {
        return;
    };
    let mut panes = a.open_all();
    wait_dialog(&mut panes[0]);
    std::thread::sleep(Duration::from_secs(1));
    for p in panes.iter_mut() {
        p.close();
    }
    let note = wait_for(Duration::from_secs(15), || {
        a.rec("lead.no-transcript").exists().then_some(())
    });
    assert!(
        note.is_some(),
        "closing an unanswered lead pane left no lead.no-transcript\nstub log:\n{}",
        a.log()
    );
    std::thread::sleep(Duration::from_secs(2));
    let log = a.log();
    for pid in stub_pids(&log) {
        let alive = Command::new("kill")
            .args(["-0", &pid])
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        assert!(!alive, "claude (stub pid {pid}) outlived its closed pane");
    }
    assert_eq!(a.transcripts(), 0);
    for n in ["w1", "w2", "w3", "w4"] {
        assert!(!a.rec(&format!("{n}.transcript")).exists());
    }
}
