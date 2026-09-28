//! `deciduous update` in a project whose graph lives on a server.
//!
//! A customer case study on 1.0.10: a project with `[remote]` had rewritten
//! its CLAUDE.md section and several command files by hand to describe
//! `remote status` / `remote pull`. `update` replaced the section with the
//! git workflow (`deciduous sync`, graph.json, the merge driver), left two
//! start markers and one end marker behind, and appended the git-model
//! templates under each command file. These build that project in a
//! temporary directory and run the real binary against it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

const START: &str = "<!-- deciduous:start -->";
const END: &str = "<!-- deciduous:end -->";

fn deciduous(dir: &Path, home: &Path, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_deciduous"))
        .args(args)
        .current_dir(dir)
        .env("DECIDUOUS_NO_SERVER", "1")
        .env("DECIDUOUS_DB_PATH", dir.join(".deciduous/deciduous.db"))
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("NO_COLOR", "1")
        .env_remove("DECIDUOUS_MCP_TOKEN")
        .output()
        .expect("run deciduous");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "deciduous {args:?} failed:\n{text}");
    text
}

struct Project {
    _tmp: TempDir,
    dir: PathBuf,
    home: PathBuf,
}

impl Project {
    /// `git init`, `deciduous init`, then a `[remote]` in config.toml.
    fn remote() -> Self {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("proj");
        let home = tmp.path().join("home");
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir_all(&home).unwrap();
        let git = Command::new("git")
            .args(["init", "-q"])
            .current_dir(&dir)
            .status()
            .expect("git");
        assert!(git.success());
        deciduous(&dir, &home, &["init"]);
        let cfg = dir.join(".deciduous/config.toml");
        let mut text = fs::read_to_string(&cfg).unwrap();
        text.push_str("\n[remote]\nurl = \"https://graph.invalid/mcp\"\nworkspace = \"proj\"\n");
        fs::write(&cfg, text).unwrap();
        Project {
            _tmp: tmp,
            dir,
            home,
        }
    }

    fn update(&self) -> String {
        deciduous(&self.dir, &self.home, &["update"])
    }

    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.dir.join(rel)).unwrap()
    }

    fn write(&self, rel: &str, text: &str) {
        fs::write(self.dir.join(rel), text).unwrap();
    }

    /// Every file update may write, for comparing two runs.
    fn snapshot(&self) -> Vec<(String, String)> {
        let mut files = vec![];
        let mut stack = vec![self.dir.join(".claude")];
        while let Some(d) = stack.pop() {
            for e in fs::read_dir(&d).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    files.push((p.display().to_string(), fs::read_to_string(&p).unwrap()));
                }
            }
        }
        files.push(("CLAUDE.md".into(), self.read("CLAUDE.md")));
        files.sort();
        files
    }
}

const HAND_SECTION: &str = "## Decision Graph Workflow\n\n\
The graph lives on the shared server. At session start:\n\n\
```bash\n\
deciduous remote status   # how far this copy has drifted\n\
deciduous remote pull     # refresh from the server\n\
```\n\n\
Never run `deciduous sync` here; there is no graph.json.\n";

const HAND_RECOVER: &str =
    "# Recover\n\nRun `deciduous remote status`, then `deciduous remote pull`, then `deciduous nodes`.\n";

#[test]
fn a_hand_written_section_and_command_on_a_remote_project_are_left_alone() {
    let p = Project::remote();
    let claude = format!(
        "# Project\n\nOur rules.\n\n{START}\n{HAND_SECTION}{END}\n\n## Build\n\nmake test\n"
    );
    p.write("CLAUDE.md", &claude);
    p.write(".claude/commands/recover.md", HAND_RECOVER);

    let out = p.update();

    assert_eq!(p.read("CLAUDE.md"), claude, "CLAUDE.md changed:\n{out}");
    assert_eq!(p.read(".claude/commands/recover.md"), HAND_RECOVER);
    assert!(out.contains("Kept yours CLAUDE.md"), "{out}");
    assert!(
        out.contains(".deciduous/update-templates/CLAUDE.md"),
        "the report says where the new section is:\n{out}"
    );
    assert!(!out.contains("appended deciduous block"), "{out}");
    // The templates it would have written are there to diff against, and
    // they are the remote ones.
    let left = p.read(".deciduous/update-templates/.claude/commands/recover.md");
    assert!(left.contains("deciduous remote pull"));
    assert!(!left.contains("deciduous sync  # Do this frequently!"));
    assert!(p
        .read(".deciduous/update-templates/CLAUDE.md")
        .contains("### The Graph Server"));

    // Files still as init wrote them become the remote-aware templates.
    let decision = p.read(".claude/commands/decision.md");
    assert!(decision.contains("## The Graph Server"), "{decision}");
    assert!(!decision.contains("SYNC AFTER PULL"));
    let sync = p.read(".claude/commands/sync.md");
    assert!(sync.contains("deciduous remote status"));
    assert!(!sync.contains("git pull --rebase"));
    for rel in [".claude/commands/work.md", ".claude/commands/document.md"] {
        assert!(
            !p.read(rel).contains("\ndeciduous sync\n"),
            "{rel} still tells agents to run deciduous sync"
        );
    }

    let before = p.snapshot();
    let again = p.update();
    assert_eq!(
        p.snapshot(),
        before,
        "second update changed files:\n{again}"
    );
    assert!(!again.contains("Updated .claude"), "{again}");
    assert!(!again.contains("section replaced"), "{again}");
}

#[test]
fn a_section_init_wrote_becomes_the_remote_section_once() {
    let p = Project::remote();
    let original = p.read("CLAUDE.md");
    assert!(original.contains("deciduous sync    # Reconcile .deciduous/graph.json"));

    let out = p.update();
    let body = p.read("CLAUDE.md");
    assert!(
        out.contains("Updated CLAUDE.md (section replaced)"),
        "{out}"
    );
    assert_eq!(body.matches(START).count(), 1, "{body}");
    assert_eq!(body.matches(END).count(), 1);
    assert!(body.starts_with("# Project Instructions\n"));
    assert!(body.contains("deciduous remote status   # What this copy and the graph server"));
    assert!(body.contains("### Audit Checklist (Before Ending a Session)"));
    assert!(!body.contains("Before Every Sync"));
    assert!(!body.contains("git add .deciduous/graph.json"));

    let again = p.update();
    assert_eq!(p.read("CLAUDE.md"), body);
    assert!(again.contains("Unchanged CLAUDE.md"), "{again}");
}

#[test]
fn a_start_marker_with_no_end_and_hand_written_text_is_not_doubled() {
    // The shape that produced two start markers: the hand-written section
    // kept its start marker and lost its end marker.
    let p = Project::remote();
    let claude = format!("# Project\n\n{START}\n{HAND_SECTION}\n## Build\n\nmake test\n");
    p.write("CLAUDE.md", &claude);

    p.update();
    let body = p.read("CLAUDE.md");
    assert_eq!(body, claude);
    assert_eq!(body.matches(START).count(), 1);
    p.update();
    assert_eq!(p.read("CLAUDE.md"), claude);
}

#[test]
fn a_file_1_0_10_left_with_two_start_markers_is_repaired() {
    // 1.0.10 kept the old start marker and wrote a whole section after it.
    let p = Project::remote();
    let init_section = {
        let s = p.read("CLAUDE.md");
        let a = s.find(START).unwrap();
        let b = s.find(END).unwrap() + END.len();
        s[a..b].to_string()
    };
    p.write(
        "CLAUDE.md",
        &format!("# Project\n\n{START}\n{init_section}\n## Build\n\nmake test\n"),
    );

    p.update();
    let body = p.read("CLAUDE.md");
    assert_eq!(body.matches(START).count(), 1, "{body}");
    assert_eq!(body.matches(END).count(), 1);
    assert!(body.starts_with(&format!("# Project\n\n{START}\n## Decision Graph Workflow")));
    assert!(body.ends_with(&format!("{END}\n## Build\n\nmake test\n")));
    assert!(body.contains("### The Graph Server"));
    p.update();
    assert_eq!(p.read("CLAUDE.md"), body);
}

#[test]
fn a_command_file_1_0_10_appended_to_gets_the_remote_block() {
    // The appended block is deciduous's text, so it is replaced; the hand
    // written part above it stays byte for byte.
    let p = Project::remote();
    let git_template = p.read(".claude/commands/recover.md");
    let file = format!(
        "{}\n\n{START}\n{}\n{END}\n",
        HAND_RECOVER.trim_end(),
        git_template.trim()
    );
    p.write(".claude/commands/recover.md", &file);

    let out = p.update();
    let body = p.read(".claude/commands/recover.md");
    assert!(
        out.contains("Updated (deciduous block) .claude/commands/recover.md"),
        "{out}"
    );
    assert!(body.starts_with(HAND_RECOVER.trim_end()));
    assert_eq!(body.matches(START).count(), 1);
    assert!(!body.contains("deciduous sync  # Do this frequently!"));
    assert!(body.contains("BEFORE ENDING -> deciduous remote status"));
    p.update();
    assert_eq!(p.read(".claude/commands/recover.md"), body);
}
