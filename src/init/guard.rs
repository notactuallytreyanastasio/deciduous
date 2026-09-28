//! How `deciduous update` writes the harness files it owns (commands, skills,
//! hook scripts, agents.toml, OpenCode and Windsurf files) without destroying
//! anything someone else put there.
//!
//! The update used to overwrite each of them unconditionally. Upgrading the 86
//! projects on one machine to 1.0.2 found 59 such files that deciduous had not
//! written: a project's own `build-test` command, hand-edited hook scripts, a
//! staged edit. Each would have been replaced by the generic template.
//!
//! The rule now, per file:
//!
//! | on disk | action |
//! |---|---|
//! | absent | write the template |
//! | identical | nothing |
//! | carries a `<!-- deciduous:start -->` block holding text deciduous shipped | replace only the block |
//! | carries a block someone edited | keep it untouched |
//! | written by deciduous (hash recorded in `.deciduous/harness.json`, or any template text deciduous ever shipped) | replace |
//! | anything else, Markdown, no `[remote]` | keep it, append the template in a marked block |
//! | anything else, Markdown, project has a `[remote]` | keep it untouched |
//! | anything else, script / TOML / other | keep it untouched |
//!
//! Every file about to change is first copied to
//! `.deciduous/update-backups/<unix-seconds>/<path>`. Every file kept
//! untouched gets the template it would have had written beside it, at
//! `.deciduous/update-templates/<path>`, so `diff` shows what it is missing.
//!
//! Why a `[remote]` project gets no appended block: its graph lives on a
//! server, so a Markdown file someone wrote there is usually the server
//! workflow written by hand, because the shipped templates described the git
//! one. Appending a template under it gives agents two sets of instructions
//! for one graph. In a git project the appended block describes the model the
//! user's own text describes, and 1.0.3 found 59 such files worth keeping
//! both halves of, so that rule stays.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const BLOCK_START: &str = "<!-- deciduous:start -->";
pub const BLOCK_END: &str = "<!-- deciduous:end -->";
const MANIFEST: &str = ".deciduous/harness.json";

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Outcome {
    Created,
    Unchanged,
    Updated,
    BlockReplaced,
    Appended,
    KeptYours,
    Removed,
}

impl Outcome {
    pub fn label(self) -> &'static str {
        match self {
            Outcome::Created => "Creating",
            Outcome::Unchanged => "Unchanged",
            Outcome::Updated => "Updated",
            Outcome::BlockReplaced => "Updated (deciduous block)",
            Outcome::Appended => "Kept yours, appended deciduous block",
            Outcome::KeptYours => "Kept yours",
            Outcome::Removed => "Removed",
        }
    }
}

fn normalise(s: &str) -> String {
    s.replace("\r\n", "\n")
        .trim()
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

fn sha(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}

/// Whether some release, beta or dev build of deciduous produced this text.
pub fn shipped_by_deciduous(text: &str) -> bool {
    super::known_templates::KNOWN_TEMPLATE_SHA256
        .binary_search(&sha(&normalise(text)).as_str())
        .is_ok()
}

/// Whether a marked section (a `CLAUDE.md` deciduous section, or the block
/// `write` appends) holds text deciduous shipped. Marker lines are ignored:
/// sections were shipped both with and without them, and a file damaged by
/// 1.0.10's update carries a start marker twice.
pub fn shipped_section(text: &str) -> bool {
    let body = normalise(&strip_markers(text));
    shipped_by_deciduous(&body)
        || shipped_by_deciduous(&format!("{BLOCK_START}\n{body}\n{BLOCK_END}"))
}

fn strip_markers(text: &str) -> String {
    text.replace("\r\n", "\n")
        .lines()
        .filter(|l| !matches!(l.trim(), BLOCK_START | BLOCK_END))
        .collect::<Vec<_>>()
        .join("\n")
}

fn block_sha(template: &str) -> String {
    sha(&normalise(template))
}

/// A marked block is deciduous's to replace when it holds a shipped text, or
/// the text this project's last update put there (recorded in the manifest
/// under `<path>#block`). Anything else in it was written by someone.
fn block_is_ours(block: &str, recorded: Option<&String>) -> bool {
    shipped_section(block) || recorded.is_some_and(|h| *h == block_sha(&strip_markers(block)))
}

/// Whether the project at `root` keeps its graph on a server.
pub fn remote_configured(root: &Path) -> bool {
    crate::remote::config_at(&root.join(".deciduous"))
        .map(|c| c.remote.is_configured())
        .unwrap_or(false)
}

/// Where `write` leaves the template for a file it kept untouched.
pub const TEMPLATE_DIR: &str = ".deciduous/update-templates";

/// Puts `template` at `.deciduous/update-templates/<rel>` and returns that
/// path, relative to `root`, for the report. `None` outside a project.
pub fn leave_template(root: &Path, rel: &str, template: &str) -> Option<String> {
    if !root.join(".deciduous").is_dir() {
        return None;
    }
    let out = format!("{TEMPLATE_DIR}/{rel}");
    let dst = root.join(&out);
    fs::create_dir_all(dst.parent()?).ok()?;
    fs::write(&dst, template).ok()?;
    Some(out)
}

fn read_manifest(root: &Path) -> BTreeMap<String, String> {
    fs::read_to_string(root.join(MANIFEST))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_manifest(root: &Path, m: &BTreeMap<String, String>) {
    if root.join(".deciduous").is_dir() {
        if let Ok(s) = serde_json::to_string_pretty(m) {
            let _ = fs::write(root.join(MANIFEST), s + "\n");
        }
    }
}

/// One backup directory per process, so a whole `update` (or `update --all`)
/// run shares a timestamp.
fn backup_stamp() -> &'static str {
    static STAMP: OnceLock<String> = OnceLock::new();
    STAMP.get_or_init(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_else(|_| "0".into())
    })
}

/// Copies `path` (relative to `root`) into this run's backup directory.
pub fn backup(root: &Path, rel: &str) -> Result<(), String> {
    let src = root.join(rel);
    if !src.is_file() || !root.join(".deciduous").is_dir() {
        return Ok(());
    }
    let dst = root
        .join(".deciduous")
        .join("update-backups")
        .join(backup_stamp())
        .join(rel);
    if dst.exists() {
        return Ok(());
    }
    if let Some(p) = dst.parent() {
        fs::create_dir_all(p).map_err(|e| format!("Could not create backup dir: {e}"))?;
    }
    fs::copy(&src, &dst).map_err(|e| format!("Could not back up {rel}: {e}"))?;
    Ok(())
}

/// The path of this run's backups, if any were taken.
pub fn backup_dir(root: &Path) -> Option<PathBuf> {
    let d = root
        .join(".deciduous")
        .join("update-backups")
        .join(backup_stamp());
    d.is_dir().then_some(d)
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path).map_err(|e| e.to_string())?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).map_err(|e| e.to_string())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

/// Writes one harness file under `root` by the rules in the module docs.
pub fn write(
    root: &Path,
    path: &Path,
    template: &str,
    executable: bool,
) -> Result<Outcome, String> {
    let rel = relative(root, path);
    let mut manifest = read_manifest(root);
    let put = |manifest: &mut BTreeMap<String, String>, body: &str| -> Result<(), String> {
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).map_err(|e| format!("Could not create {}: {e}", p.display()))?;
        }
        fs::write(path, body).map_err(|e| format!("Could not write {rel}: {e}"))?;
        if executable {
            make_executable(path)?;
        }
        manifest.insert(rel.clone(), sha(body));
        Ok(())
    };

    let outcome = match fs::read(path) {
        Err(_) => {
            put(&mut manifest, template)?;
            Outcome::Created
        }
        Ok(bytes) => {
            let existing = String::from_utf8(bytes.clone());
            let written_by_us = manifest
                .get(&rel)
                .is_some_and(|h| existing.as_ref().map(|s| sha(s) == *h).unwrap_or(false));
            match existing {
                Ok(ref s) if s == template => {
                    manifest.insert(rel.clone(), sha(s));
                    Outcome::Unchanged
                }
                Ok(ref s)
                    if rel.ends_with(".md") && s.contains(BLOCK_START) && s.contains(BLOCK_END) =>
                {
                    let start = s.find(BLOCK_START).unwrap();
                    let end = s[start..]
                        .find(BLOCK_END)
                        .map(|i| start + i + BLOCK_END.len());
                    let block_key = format!("{rel}#block");
                    match end {
                        Some(end) if block_is_ours(&s[start..end], manifest.get(&block_key)) => {
                            let block = format!("{BLOCK_START}\n{}\n{BLOCK_END}", template.trim());
                            let body = format!("{}{}{}", &s[..start], block, &s[end..]);
                            manifest.insert(block_key, block_sha(template));
                            if body == *s {
                                Outcome::Unchanged
                            } else {
                                backup(root, &rel)?;
                                fs::write(path, &body)
                                    .map_err(|e| format!("Could not write {rel}: {e}"))?;
                                Outcome::BlockReplaced
                            }
                        }
                        _ => Outcome::KeptYours,
                    }
                }
                Ok(ref s) if written_by_us || shipped_by_deciduous(s) => {
                    backup(root, &rel)?;
                    put(&mut manifest, template)?;
                    Outcome::Updated
                }
                Ok(ref s) if rel.ends_with(".md") && !remote_configured(root) => {
                    backup(root, &rel)?;
                    let body = format!(
                        "{}\n\n{BLOCK_START}\n{}\n{BLOCK_END}\n",
                        s.trim_end(),
                        template.trim()
                    );
                    fs::write(path, body).map_err(|e| format!("Could not write {rel}: {e}"))?;
                    manifest.remove(&rel);
                    manifest.insert(format!("{rel}#block"), block_sha(template));
                    Outcome::Appended
                }
                _ => Outcome::KeptYours,
            }
        }
    };
    write_manifest(root, &manifest);
    if outcome == Outcome::KeptYours {
        leave_template(root, &rel, template);
    }
    Ok(outcome)
}

/// Deletes a harness file deciduous no longer ships, by the same test
/// `write` uses for "written by deciduous": its hash is in the manifest, or
/// it is a template text some release shipped. Anything else is the user's
/// and stays. `None` when there was nothing at `path`.
pub fn remove(root: &Path, path: &Path) -> Result<Option<Outcome>, String> {
    let rel = relative(root, path);
    let Ok(existing) = fs::read_to_string(path) else {
        return Ok(None);
    };
    let mut manifest = read_manifest(root);
    let ours =
        manifest.get(&rel).is_some_and(|h| sha(&existing) == *h) || shipped_by_deciduous(&existing);
    if !ours {
        return Ok(Some(Outcome::KeptYours));
    }
    backup(root, &rel)?;
    fs::remove_file(path).map_err(|e| format!("Could not remove {rel}: {e}"))?;
    manifest.remove(&rel);
    write_manifest(root, &manifest);
    Ok(Some(Outcome::Removed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn project() -> TempDir {
        let t = TempDir::new().unwrap();
        fs::create_dir_all(t.path().join(".deciduous")).unwrap();
        fs::create_dir_all(t.path().join(".claude/commands")).unwrap();
        t
    }

    #[test]
    fn a_missing_file_is_written_and_recorded() {
        let t = project();
        let p = t.path().join(".claude/commands/x.md");
        assert_eq!(
            write(t.path(), &p, "# X\n", false).unwrap(),
            Outcome::Created
        );
        assert_eq!(fs::read_to_string(&p).unwrap(), "# X\n");
        assert!(fs::read_to_string(t.path().join(MANIFEST))
            .unwrap()
            .contains(".claude/commands/x.md"));
    }

    #[test]
    fn a_file_deciduous_wrote_is_replaced_and_backed_up() {
        let t = project();
        let p = t.path().join(".claude/commands/x.md");
        write(t.path(), &p, "# X v1\n", false).unwrap();
        assert_eq!(
            write(t.path(), &p, "# X v2\n", false).unwrap(),
            Outcome::Updated
        );
        assert_eq!(fs::read_to_string(&p).unwrap(), "# X v2\n");
        let b = backup_dir(t.path()).unwrap().join(".claude/commands/x.md");
        assert_eq!(fs::read_to_string(b).unwrap(), "# X v1\n");
    }

    #[test]
    fn a_users_markdown_is_kept_and_the_template_appended_once() {
        let t = project();
        let p = t.path().join(".claude/commands/build-test.md");
        fs::write(&p, "# Build and Test\n\nTest the generator in /tmp.\n").unwrap();
        assert_eq!(
            write(t.path(), &p, "# Build and Test\n\nGeneric.\n", false).unwrap(),
            Outcome::Appended
        );
        let body = fs::read_to_string(&p).unwrap();
        assert!(body.starts_with("# Build and Test\n\nTest the generator in /tmp.\n"));
        assert!(body.contains(&format!(
            "{BLOCK_START}\n# Build and Test\n\nGeneric.\n{BLOCK_END}"
        )));
        // The next update replaces only the block.
        assert_eq!(
            write(t.path(), &p, "# Build and Test\n\nGeneric v2.\n", false).unwrap(),
            Outcome::BlockReplaced
        );
        let body = fs::read_to_string(&p).unwrap();
        assert!(body.starts_with("# Build and Test\n\nTest the generator in /tmp.\n"));
        assert!(body.contains("Generic v2.") && !body.contains("Generic.\n"));
        assert_eq!(body.matches(BLOCK_START).count(), 1);
    }

    #[test]
    fn a_users_script_or_toml_is_left_alone() {
        let t = project();
        fs::create_dir_all(t.path().join(".claude/hooks")).unwrap();
        let s = t.path().join(".claude/hooks/require-action-node.sh");
        fs::write(&s, "#!/bin/sh\n# mine\nexit 0\n").unwrap();
        assert_eq!(
            write(
                t.path(),
                &s,
                "#!/bin/sh\nexec deciduous log-loop pre\n",
                true
            )
            .unwrap(),
            Outcome::KeptYours
        );
        assert_eq!(
            fs::read_to_string(&s).unwrap(),
            "#!/bin/sh\n# mine\nexit 0\n"
        );
        let a = t.path().join(".claude/agents.toml");
        fs::write(&a, "[mine]\nx = 1\n").unwrap();
        assert_eq!(
            write(t.path(), &a, "[agents]\n", false).unwrap(),
            Outcome::KeptYours
        );
    }

    #[test]
    fn remove_deletes_what_deciduous_wrote_and_keeps_the_rest() {
        let t = project();
        fs::create_dir_all(t.path().join(".claude/hooks")).unwrap();
        let ours = t.path().join(".claude/hooks/require-action-node.sh");
        write(
            t.path(),
            &ours,
            "#!/bin/sh\nexec deciduous log-loop pre\n",
            true,
        )
        .unwrap();
        assert_eq!(remove(t.path(), &ours).unwrap(), Some(Outcome::Removed));
        assert!(!ours.exists());
        assert!(backup_dir(t.path())
            .is_some_and(|d| d.join(".claude/hooks/require-action-node.sh").exists()));

        let theirs = t.path().join(".claude/hooks/post-commit-reminder.sh");
        fs::write(&theirs, "#!/bin/sh\n# mine\nexit 0\n").unwrap();
        assert_eq!(remove(t.path(), &theirs).unwrap(), Some(Outcome::KeptYours));
        assert!(theirs.exists());

        assert_eq!(remove(t.path(), &ours).unwrap(), None);
    }

    #[test]
    fn every_current_harness_template_is_in_the_known_list() {
        // Fails when a template changes without running
        // `python3 scripts/gen_known_templates.py > src/init/known_templates.rs`.
        // Without the regeneration, installs made from the old text would read
        // as the user's own on the next update and get appended to, not replaced.
        use crate::init::remote_templates as r;
        use crate::init::templates as t;
        use crate::opencode as o;
        let current = [
            ("DECISION_MD", t::DECISION_MD),
            ("RECOVER_MD", t::RECOVER_MD),
            ("DOCUMENT_MD", t::DOCUMENT_MD),
            ("BUILD_TEST_MD", t::BUILD_TEST_MD),
            ("SERVE_UI_MD", t::SERVE_UI_MD),
            ("DECISION_GRAPH_MD", t::DECISION_GRAPH_MD),
            ("SYNC_MD", t::SYNC_MD),
            ("WORK_MD", t::WORK_MD),
            ("DEMO_SWARM_MD", t::DEMO_SWARM_MD),
            ("HOOK_VERSION_CHECK", t::HOOK_VERSION_CHECK),
            ("HOOK_BOARD_MENTIONS", t::HOOK_BOARD_MENTIONS),
            ("CLAUDE_AGENTS_TOML", t::CLAUDE_AGENTS_TOML),
            ("SKILL_PULSE", t::SKILL_PULSE),
            ("SKILL_NARRATIVES", t::SKILL_NARRATIVES),
            ("SKILL_ARCHAEOLOGY", t::SKILL_ARCHAEOLOGY),
            ("WINDSURF_HOOKS_JSON", t::WINDSURF_HOOKS_JSON),
            ("WINDSURF_RULES_DECIDUOUS", t::WINDSURF_RULES_DECIDUOUS),
            ("PLUGIN_VERSION_CHECK", o::PLUGIN_VERSION_CHECK),
            ("AGENT_DECIDUOUS", o::AGENT_DECIDUOUS),
            ("TOOL_DECIDUOUS", o::TOOL_DECIDUOUS),
            ("CLAUDE_MD_SECTION", t::CLAUDE_MD_SECTION),
            ("DECISION_MD_REMOTE", r::DECISION_MD_REMOTE),
            ("RECOVER_MD_REMOTE", r::RECOVER_MD_REMOTE),
            ("WORK_MD_REMOTE", r::WORK_MD_REMOTE),
            ("DOCUMENT_MD_REMOTE", r::DOCUMENT_MD_REMOTE),
            ("SYNC_MD_REMOTE", r::SYNC_MD_REMOTE),
            ("CLAUDE_MD_SECTION_REMOTE", r::CLAUDE_MD_SECTION_REMOTE),
        ];
        let missing: Vec<&str> = current
            .iter()
            .filter(|(_, v)| !shipped_by_deciduous(v))
            .map(|(n, _)| *n)
            .collect();
        assert!(
            missing.is_empty(),
            "regenerate src/init/known_templates.rs; not listed: {missing:?}"
        );
    }

    fn remote_project() -> TempDir {
        let t = project();
        fs::write(
            t.path().join(".deciduous/config.toml"),
            "[remote]\nurl = \"https://graph.invalid/mcp\"\nworkspace = \"p\"\n",
        )
        .unwrap();
        t
    }

    #[test]
    fn a_users_markdown_on_a_remote_project_is_left_alone() {
        let t = remote_project();
        let p = t.path().join(".claude/commands/recover.md");
        let mine = "# Recover\n\nRun `deciduous remote pull`.\n";
        fs::write(&p, mine).unwrap();
        assert_eq!(
            write(t.path(), &p, "# Recover\n\nGeneric.\n", false).unwrap(),
            Outcome::KeptYours
        );
        assert_eq!(fs::read_to_string(&p).unwrap(), mine);
        assert_eq!(
            fs::read_to_string(
                t.path()
                    .join(TEMPLATE_DIR)
                    .join(".claude/commands/recover.md")
            )
            .unwrap(),
            "# Recover\n\nGeneric.\n"
        );
    }

    #[test]
    fn a_block_someone_edited_is_kept() {
        let t = project();
        let p = t.path().join(".claude/commands/build-test.md");
        fs::write(&p, "# Mine\n").unwrap();
        write(t.path(), &p, "# Generic\n", false).unwrap();
        let edited = fs::read_to_string(&p)
            .unwrap()
            .replace("# Generic", "# Generic, but run it in /tmp");
        fs::write(&p, &edited).unwrap();
        assert_eq!(
            write(t.path(), &p, "# Generic v2\n", false).unwrap(),
            Outcome::KeptYours
        );
        assert_eq!(fs::read_to_string(&p).unwrap(), edited);
    }

    #[test]
    fn a_shipped_block_on_a_remote_project_becomes_the_remote_template() {
        // A file 1.0.10 appended the git-model block to: the block is ours.
        let t = remote_project();
        let p = t.path().join(".claude/commands/recover.md");
        let mine = "# Recover\n\nRun `deciduous remote pull`.";
        fs::write(
            &p,
            format!(
                "{mine}\n\n{BLOCK_START}\n{}\n{BLOCK_END}\n",
                crate::init::templates::RECOVER_MD.trim()
            ),
        )
        .unwrap();
        let remote = crate::init::remote_templates::RECOVER_MD_REMOTE;
        assert_eq!(
            write(t.path(), &p, remote, false).unwrap(),
            Outcome::BlockReplaced
        );
        let body = fs::read_to_string(&p).unwrap();
        assert!(body.starts_with(mine));
        assert!(body.contains("deciduous remote pull\ndeciduous remote status"));
        assert!(!body.contains("deciduous sync  # Do this frequently!"));
        assert_eq!(
            write(t.path(), &p, remote, false).unwrap(),
            Outcome::Unchanged
        );
    }

    #[test]
    fn remote_templates_track_the_git_ones() {
        // The remote variants are the git ones with their sync passages
        // rewritten. A shared line edited in one and not the other fails here.
        use crate::init::remote_templates as r;
        use crate::init::templates as t;
        let git_model = [
            "sync",
            "graph.json",
            "git pull",
            "merge",
            "push",
            "daily workflow",
            "0.17",
            "teammate",
            "live graph",
            "ordinary git",
            "work normally",
        ];
        for (name, git, remote) in [
            ("DECISION_MD", t::DECISION_MD, r::DECISION_MD_REMOTE),
            ("RECOVER_MD", t::RECOVER_MD, r::RECOVER_MD_REMOTE),
            ("WORK_MD", t::WORK_MD, r::WORK_MD_REMOTE),
            ("DOCUMENT_MD", t::DOCUMENT_MD, r::DOCUMENT_MD_REMOTE),
            (
                "CLAUDE_MD_SECTION",
                t::CLAUDE_MD_SECTION,
                r::CLAUDE_MD_SECTION_REMOTE,
            ),
        ] {
            let have: std::collections::HashSet<&str> = remote.lines().collect();
            let missing: Vec<&str> = git
                .lines()
                .filter(|l| {
                    let l2 = l.to_lowercase();
                    !git_model.iter().any(|w| l2.contains(w)) && !have.contains(l)
                })
                .collect();
            assert!(missing.is_empty(), "{name}_REMOTE lacks {missing:#?}");
        }
    }

    #[test]
    fn remote_templates_never_send_agents_to_the_git_workflow() {
        use crate::init::remote_templates as r;
        for (name, text) in [
            ("DECISION_MD_REMOTE", r::DECISION_MD_REMOTE),
            ("RECOVER_MD_REMOTE", r::RECOVER_MD_REMOTE),
            ("WORK_MD_REMOTE", r::WORK_MD_REMOTE),
            ("DOCUMENT_MD_REMOTE", r::DOCUMENT_MD_REMOTE),
            ("SYNC_MD_REMOTE", r::SYNC_MD_REMOTE),
            ("CLAUDE_MD_SECTION_REMOTE", r::CLAUDE_MD_SECTION_REMOTE),
        ] {
            for bad in [
                "\ndeciduous sync",
                "git pull --rebase",
                "git add .deciduous/graph.json",
                "Before Every Sync",
                "-> deciduous sync",
            ] {
                assert!(!text.contains(bad), "{name} still says {bad:?}");
            }
        }
    }

    #[test]
    fn any_template_deciduous_ever_shipped_counts_as_ours() {
        // The 1.0.2 build-test template, as an old install would have it.
        let t = project();
        let p = t.path().join(".claude/commands/build-test.md");
        fs::write(&p, crate::init::templates::BUILD_TEST_MD).unwrap();
        assert!(shipped_by_deciduous(crate::init::templates::BUILD_TEST_MD));
        assert_eq!(
            write(t.path(), &p, "# new\n", false).unwrap(),
            Outcome::Updated
        );
    }
}
