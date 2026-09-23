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
//! | carries a `<!-- deciduous:start -->` block | replace only the block |
//! | written by deciduous (hash recorded in `.deciduous/harness.json`, or any template text deciduous ever shipped) | replace |
//! | anything else, Markdown | keep it, append the template in a marked block |
//! | anything else, script / TOML / other | keep it untouched |
//!
//! Every file about to change is first copied to
//! `.deciduous/update-backups/<unix-seconds>/<path>`.

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
                    match end {
                        Some(end) => {
                            let block = format!("{BLOCK_START}\n{}\n{BLOCK_END}", template.trim());
                            let body = format!("{}{}{}", &s[..start], block, &s[end..]);
                            if body == *s {
                                Outcome::Unchanged
                            } else {
                                backup(root, &rel)?;
                                fs::write(path, &body)
                                    .map_err(|e| format!("Could not write {rel}: {e}"))?;
                                Outcome::BlockReplaced
                            }
                        }
                        None => Outcome::KeptYours,
                    }
                }
                Ok(ref s) if written_by_us || shipped_by_deciduous(s) => {
                    backup(root, &rel)?;
                    put(&mut manifest, template)?;
                    Outcome::Updated
                }
                Ok(ref s) if rel.ends_with(".md") => {
                    backup(root, &rel)?;
                    let body = format!(
                        "{}\n\n{BLOCK_START}\n{}\n{BLOCK_END}\n",
                        s.trim_end(),
                        template.trim()
                    );
                    fs::write(path, body).map_err(|e| format!("Could not write {rel}: {e}"))?;
                    manifest.remove(&rel);
                    Outcome::Appended
                }
                _ => Outcome::KeptYours,
            }
        }
    };
    write_manifest(root, &manifest);
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
        use crate::init::templates as t;
        use crate::opencode as o;
        let current = [
            ("DECISION_MD", t::DECISION_MD),
            ("RECOVER_MD", t::RECOVER_MD),
            ("DOCUMENT_MD", t::DOCUMENT_MD),
            ("BUILD_TEST_MD", t::BUILD_TEST_MD),
            ("SERVE_UI_MD", t::SERVE_UI_MD),
            ("SYNC_GRAPH_MD", t::SYNC_GRAPH_MD),
            ("DECISION_GRAPH_MD", t::DECISION_GRAPH_MD),
            ("SYNC_MD", t::SYNC_MD),
            ("WORK_MD", t::WORK_MD),
            ("DEMO_SWARM_MD", t::DEMO_SWARM_MD),
            ("HOOK_VERSION_CHECK", t::HOOK_VERSION_CHECK),
            ("CLAUDE_AGENTS_TOML", t::CLAUDE_AGENTS_TOML),
            ("SKILL_PULSE", t::SKILL_PULSE),
            ("SKILL_NARRATIVES", t::SKILL_NARRATIVES),
            ("SKILL_ARCHAEOLOGY", t::SKILL_ARCHAEOLOGY),
            ("WINDSURF_HOOKS_JSON", t::WINDSURF_HOOKS_JSON),
            ("WINDSURF_RULES_DECIDUOUS", t::WINDSURF_RULES_DECIDUOUS),
            ("PLUGIN_VERSION_CHECK", o::PLUGIN_VERSION_CHECK),
            ("AGENT_DECIDUOUS", o::AGENT_DECIDUOUS),
            ("TOOL_DECIDUOUS", o::TOOL_DECIDUOUS),
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
