//! A project whose `.deciduous/config.toml` has no `[remote]` takes the
//! machine's default from `~/.deciduous/config.toml`, and `remote init --user`
//! writes that file. Both run against a URL nothing listens on, so what is
//! asserted is where the CLI says it is going, not that it got there.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

struct Sandbox {
    root: TempDir,
}

impl Sandbox {
    fn new() -> Self {
        let root = TempDir::new().unwrap();
        std::fs::create_dir_all(root.path().join("home")).unwrap();
        Sandbox { root }
    }
    fn home(&self) -> PathBuf {
        self.root.path().join("home")
    }
    fn git(&self, dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("HOME", self.home())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }
    fn dx(&self, dir: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_deciduous"))
            .args(args)
            .current_dir(dir)
            .env("HOME", self.home())
            .env("XDG_CONFIG_HOME", self.home().join(".config"))
            .env("DECIDUOUS_MCP_TOKEN", "test-token-test-token-test-token-1234")
            .env("DECIDUOUS_NO_SERVER", "1")
            .env_remove("DECIDUOUS_DB_PATH")
            .env("NO_COLOR", "1")
            .output()
            .unwrap()
    }
    fn repo(&self, rel: &str) -> PathBuf {
        let dir = self.root.path().join(rel);
        std::fs::create_dir_all(&dir).unwrap();
        self.git(&dir, &["init", "-q", "-b", "main"]);
        self.git(&dir, &["commit", "-q", "--allow-empty", "-m", &format!("root of {rel}")]);
        let out = self.dx(&dir, &["init"]);
        assert!(out.status.success(), "deciduous init: {}", String::from_utf8_lossy(&out.stderr));
        dir
    }
}

fn text(out: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

// Port 9 (discard) is assigned and almost never bound; a connection is refused
// immediately, so the CLI reports the URL it tried without waiting.
const DEAD: &str = "http://127.0.0.1:9";

#[test]
fn a_project_without_a_remote_uses_the_machine_default() {
    let sb = Sandbox::new();
    let repo = sb.repo("proj");
    let cfg = std::fs::read_to_string(repo.join(".deciduous/config.toml")).unwrap_or_default();
    assert!(!cfg.contains("[remote]"), "init wrote a remote: {cfg}");

    // No default yet: local only, and status says so.
    let out = text(&sb.dx(&repo, &["remote", "status"]));
    assert!(out.contains("Local only"), "{out}");

    // The machine default, written by hand the way a user would.
    std::fs::create_dir_all(sb.home().join(".deciduous")).unwrap();
    std::fs::write(
        sb.home().join(".deciduous/config.toml"),
        format!("[remote]\nurl = \"{DEAD}/\"\nworkspace = \"never-used\"\n"),
    )
    .unwrap();

    let out = text(&sb.dx(&repo, &["remote", "status"]));
    assert!(out.contains(&format!("Remote: {DEAD}")), "{out}");
    assert!(out.contains("machine default from"), "{out}");
    assert!(out.contains(".deciduous/config.toml"), "{out}");
    // The project's own workspace name, never the per-user file's.
    assert!(out.contains("Workspace: proj"), "{out}");
    assert!(!out.contains("never-used"), "{out}");

    // The project's file is untouched: the fallback is read, never written.
    let after = std::fs::read_to_string(repo.join(".deciduous/config.toml")).unwrap_or_default();
    assert_eq!(after, cfg);

    // A project remote still wins over the machine default.
    std::fs::write(
        repo.join(".deciduous/config.toml"),
        format!("{cfg}\n[remote]\nurl = \"http://127.0.0.1:19\"\nworkspace = \"own\"\n"),
    )
    .unwrap();
    let out = text(&sb.dx(&repo, &["remote", "status"]));
    assert!(out.contains("Remote: http://127.0.0.1:19"), "{out}");
    assert!(!out.contains("machine default"), "{out}");
    assert!(out.contains("Workspace: own"), "{out}");
}

#[test]
fn remote_init_user_writes_the_machine_default_and_keeps_the_rest_of_the_file() {
    let sb = Sandbox::new();
    let repo = sb.repo("proj");
    let user_cfg = sb.home().join(".deciduous/config.toml");
    std::fs::create_dir_all(user_cfg.parent().unwrap()).unwrap();
    std::fs::write(&user_cfg, "# mine\n[branch]\nmain_branches = [\"trunk\"]\n").unwrap();

    let out = sb.dx(&repo, &["remote", "init", "--user", &format!("{DEAD}/")]);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("Machine default:"), "{t}");
    assert!(t.contains("did not answer"), "an unreachable URL is a warning, not a refusal: {t}");

    let written = std::fs::read_to_string(&user_cfg).unwrap();
    assert!(written.contains("# mine"), "comment dropped: {written}");
    assert!(written.contains("main_branches = [\"trunk\"]"), "{written}");
    assert!(written.contains(&format!("url = \"{DEAD}\"")), "trailing slash kept or url missing: {written}");
    assert!(!written.contains("workspace"), "{written}");

    // The project's own config gained nothing.
    let cfg = std::fs::read_to_string(repo.join(".deciduous/config.toml")).unwrap_or_default();
    assert!(!cfg.contains("[remote]"), "{cfg}");

    // And the project now resolves to it.
    let out = text(&sb.dx(&repo, &["remote", "status"]));
    assert!(out.contains(&format!("Remote: {DEAD}")), "{out}");
    assert!(out.contains("machine default from"), "{out}");
}

#[test]
fn a_broken_user_file_contributes_nothing() {
    let sb = Sandbox::new();
    let repo = sb.repo("proj");
    std::fs::create_dir_all(sb.home().join(".deciduous")).unwrap();
    std::fs::write(sb.home().join(".deciduous/config.toml"), "this = = is not toml").unwrap();
    let out = sb.dx(&repo, &["remote", "status"]);
    let t = text(&out);
    assert!(t.contains("Local only"), "{t}");
    assert!(out.status.success(), "{t}");
}
