//! `deciduous demo-swarm`: an easter egg. One Opus boss and four Sonnet
//! workers build one Tetris together in iTerm2 or Ghostty panes, sharing one
//! decision graph, and every pane is recorded for a later replay.
//!
//! The work is a zsh script embedded in the binary. zsh, not bash: macOS
//! ships bash 3.2, whose `read -t` takes whole seconds, and a tour you can
//! fast-forward with a keypress needs fractional timeouts.

use std::io::Write;
use std::process::Command;

/// The script, as shipped. It copies itself into the arena it builds, and
/// every pane runs that copy.
pub const SCRIPT: &str = include_str!("demo-swarm.zsh");

/// Run the launcher with the user's arguments; returns the exit code.
pub fn run(args: &[String]) -> i32 {
    let path =
        std::env::temp_dir().join(format!("deciduous-demo-swarm-{}.zsh", std::process::id()));
    let written = std::fs::File::create(&path).and_then(|mut f| f.write_all(SCRIPT.as_bytes()));
    if let Err(e) = written {
        eprintln!("demo-swarm: cannot write {}: {e}", path.display());
        return 1;
    }
    let status = Command::new("zsh")
        .arg(&path)
        .arg("launch")
        .args(args)
        .status();
    let _ = std::fs::remove_file(&path);
    match status {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) => {
            eprintln!("demo-swarm: cannot run zsh: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_parses() {
        let Ok(out) = Command::new("zsh")
            .arg("-n")
            .arg("/dev/stdin")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut c| {
                c.stdin.take().unwrap().write_all(SCRIPT.as_bytes())?;
                c.wait_with_output()
            })
        else {
            eprintln!("zsh not installed; skipping");
            return;
        };
        assert!(
            out.status.success(),
            "zsh -n failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn refuses_outside_iterm_and_ghostty() {
        if Command::new("zsh").arg("--version").output().is_err() {
            return;
        }
        let path = std::env::temp_dir().join(format!("demo-swarm-test-{}.zsh", std::process::id()));
        std::fs::write(&path, SCRIPT).unwrap();
        let out = Command::new("zsh")
            .arg(&path)
            .arg("launch")
            .env("TERM_PROGRAM", "Apple_Terminal")
            .output()
            .unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(out.status.code(), Some(1));
        let err = String::from_utf8_lossy(&out.stderr);
        // On macOS the terminal check answers; elsewhere the platform check does first.
        assert!(
            err.contains("iTerm2 and Ghostty") || err.contains("needs macOS"),
            "{err}"
        );
    }
}
