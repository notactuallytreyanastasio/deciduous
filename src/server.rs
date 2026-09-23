//! The shared graph server every project writes to, set up by `init` and
//! checked by `update`.
//!
//! Since 1.0.3 agents write through the HTTP MCP server, and a project with
//! no server is a project whose agents have nowhere to write. So `init` and
//! `update` do not finish until the project points at a server that answers:
//!
//! - a project with `[remote] url` is checked: the server must answer and a
//!   token must be stored;
//! - a project without one gets this machine's local server, set up once
//!   with Docker from the release's `deciduous-mcp-docker.tar.gz` (the same
//!   version as this binary, verified against the release's `checksums.txt`)
//!   and its `scripts/setup.sh`: PostgreSQL 17 and the server on
//!   `127.0.0.1:4000`, credentials in `~/.config/deciduous/server/.env`.
//!
//! No Docker is an error, not a skip. `DECIDUOUS_NO_SERVER=1` skips the step
//! for tests and CI, which run `init` in throwaway directories.

use crate::config::Config;
use crate::remote::{self, Remote};
use colored::Colorize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const SKIP_ENV: &str = "DECIDUOUS_NO_SERVER";
/// A local bundle to use instead of downloading one (for unreleased builds).
pub const BUNDLE_ENV: &str = "DECIDUOUS_SERVER_BUNDLE";
const RELEASES: &str = "https://github.com/notactuallytreyanastasio/deciduous/releases/download";
const BUNDLE: &str = "deciduous-mcp-docker.tar.gz";

/// Which command is asking. `init` verifies a configured remote in full;
/// `update` only checks it answers and a token is stored, because the full
/// check exports the whole workspace and `update --all` runs it per project.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Caller {
    Init,
    Update,
}

/// `~/.config/deciduous/server`, next to the credentials file.
pub fn server_home() -> Option<PathBuf> {
    remote::credentials_path().and_then(|p| p.parent().map(|d| d.join("server")))
}

fn env_file() -> Option<PathBuf> {
    server_home().map(|h| h.join(".env"))
}

/// A `KEY=value` or `KEY='value'` line from the setup script's settings file.
/// The file is read, never executed.
fn env_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| {
        l.strip_prefix(&format!("{key}="))
            .map(|v| v.trim().trim_matches('\'').to_string())
    })
}

fn local_url(env_text: &str) -> String {
    let port = env_value(env_text, "DECIDUOUS_PORT").unwrap_or_else(|| "4000".into());
    format!("http://127.0.0.1:{port}")
}

fn answers(url: &str) -> bool {
    ureq::get(&format!("{url}/health"))
        .timeout(std::time::Duration::from_secs(5))
        .call()
        .is_ok()
}

/// The step `init` and `update` run after writing their files.
pub fn ensure(project: &Path, caller: Caller) -> Result<(), String> {
    if std::env::var(SKIP_ENV).is_ok_and(|v| !v.is_empty() && v != "0") {
        println!(
            "   {} shared graph server ({SKIP_ENV} is set)",
            "Skipped".yellow()
        );
        return Ok(());
    }
    println!("\n{}", "Shared graph server".cyan().bold());

    match remote::read_remote_url(project) {
        Some(url) => check_configured(project, &url, caller),
        None => {
            let (url, token) = local_server()?;
            if remote::token().ok().as_deref() != Some(token.as_str()) {
                let path = remote::store_token(&token)?;
                println!("   {} token in {}", "Stored".green(), path.display());
            }
            remote::write_remote_url(project, &url)?;
            println!(
                "   {} .deciduous/config.toml [remote] url = {url}",
                "Wrote".green()
            );
            check_configured(project, &url, Caller::Init)?;
            register_claude_code(&url, &token);
            Ok(())
        }
    }
}

fn check_configured(project: &Path, url: &str, caller: Caller) -> Result<(), String> {
    let mut cfg = Config::load();
    cfg.remote.url = Some(url.to_string());
    let fix = format!(
        "\n\nThe project points at {url}. Start that server, or store its token with\n\n    \
         deciduous remote login --url {url}\n\nthen rerun this command."
    );
    let r = Remote::resolve(&cfg, project).map_err(|e| format!("{e}{fix}"))?;
    match caller {
        Caller::Update => r.health().map_err(|e| format!("{e}{fix}"))?,
        Caller::Init => {
            let c = r.check().map_err(|e| format!("{e}{fix}"))?;
            println!(
                "   {} {url} holds {} nodes, {} edges in workspace {}",
                "Connected".green(),
                c.nodes,
                c.edges,
                r.workspace.cyan()
            );
            return Ok(());
        }
    }
    println!(
        "   {} {url} (workspace {})",
        "Connected".green(),
        r.workspace.cyan()
    );
    Ok(())
}

/// This machine's local server: the running one, or a new one from the
/// release bundle. Returns its URL and token.
fn local_server() -> Result<(String, String), String> {
    let env_path = env_file().ok_or("cannot determine a config directory (is HOME set?)")?;
    if let Ok(text) = std::fs::read_to_string(&env_path) {
        let url = local_url(&text);
        let token = env_value(&text, "DECIDUOUS_MCP_TOKEN")
            .ok_or_else(|| format!("{} has no DECIDUOUS_MCP_TOKEN", env_path.display()))?;
        if answers(&url) {
            println!("   {} local server at {url}", "Found".green());
            return Ok((url, token));
        }
    }
    run_setup(&env_path)?;
    let text = std::fs::read_to_string(&env_path).map_err(|e| {
        format!(
            "setup finished but {} is unreadable: {e}",
            env_path.display()
        )
    })?;
    let url = local_url(&text);
    let token = env_value(&text, "DECIDUOUS_MCP_TOKEN")
        .ok_or_else(|| format!("{} has no DECIDUOUS_MCP_TOKEN", env_path.display()))?;
    if !answers(&url) {
        return Err(format!("setup finished but {url}/health does not answer"));
    }
    Ok((url, token))
}

fn docker_ready() -> Result<(), String> {
    let installed = Command::new("docker").arg("--version").output().is_ok();
    if !installed {
        return Err(
            "Docker is required: deciduous keeps every project's graph in a \
             PostgreSQL server, and without a remote configured `init` sets one up \
             on this machine with Docker.\n\n\
             Install Docker Desktop (https://docs.docker.com/get-docker/) and rerun, or \
             point this project at an existing server:\n\n    \
             deciduous remote login --url <url>\n    deciduous remote init <url>"
                .into(),
        );
    }
    let running = Command::new("docker")
        .arg("info")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !running {
        return Err("Docker is installed but not running. Start Docker and rerun.".into());
    }
    Ok(())
}

/// Downloads (or takes from `DECIDUOUS_SERVER_BUNDLE`) this version's bundle,
/// verifies it, and runs its setup script against the machine's settings file.
fn run_setup(env_path: &Path) -> Result<(), String> {
    docker_ready()?;
    let home = env_path.parent().ok_or("settings path has no parent")?;
    let version = env!("CARGO_PKG_VERSION");
    let dir = home.join(format!("bundle-{version}"));
    let script = dir.join("deciduous-mcp-docker/scripts/setup.sh");

    if !script.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
        let archive = dir.join(BUNDLE);
        match std::env::var_os(BUNDLE_ENV) {
            Some(local) => {
                std::fs::copy(&local, &archive)
                    .map_err(|e| format!("copying {}: {e}", PathBuf::from(&local).display()))?;
                println!("   {} {}", "Using".green(), PathBuf::from(local).display());
            }
            None => download_verified(version, &archive)?,
        }
        let ok = Command::new("tar")
            .arg("-xzf")
            .arg(&archive)
            .arg("-C")
            .arg(&dir)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok || !script.exists() {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(format!("could not unpack {BUNDLE}"));
        }
    }

    println!(
        "   {} PostgreSQL and the server with Docker (the first build takes a few minutes)",
        "Setting up".green()
    );
    let mut cmd = Command::new("sh");
    cmd.arg(&script)
        .env("DECIDUOUS_ENV_FILE", env_path)
        .env(
            "COMPOSE_PROJECT_NAME",
            std::env::var("COMPOSE_PROJECT_NAME").unwrap_or_else(|_| "deciduous".into()),
        )
        .env_remove("DATABASE_URL");
    // A token this machine already uses becomes the local server's token, so
    // one machine never has two. setup.sh reads it on first setup only.
    match remote::token() {
        Ok(t) if t.len() >= 32 => {
            cmd.env(remote::TOKEN_ENV, t);
        }
        _ => {
            cmd.env_remove(remote::TOKEN_ENV);
        }
    }
    let status = cmd
        .status()
        .map_err(|e| format!("running {}: {e}", script.display()))?;
    if !status.success() {
        return Err(format!(
            "the server setup failed (see its output above). Rerun this command once it is fixed; \
             settings and data in {} are kept.",
            home.display()
        ));
    }
    Ok(())
}

fn download_verified(version: &str, archive: &Path) -> Result<(), String> {
    let base = format!("{RELEASES}/v{version}");
    println!("   {} {base}/{BUNDLE}", "Downloading".green());
    let sums = ureq::get(&format!("{base}/checksums.txt"))
        .timeout(std::time::Duration::from_secs(60))
        .call()
        .map_err(|e| {
            format!(
                "fetching checksums.txt for v{version}: {e}\n\n\
                 A build that is not a release has no bundle to download. Point \
                 {BUNDLE_ENV} at a {BUNDLE} (deciduous_mcp/scripts/package-docker.sh \
                 makes one) and rerun."
            )
        })?
        .into_string()
        .map_err(|e| format!("reading checksums.txt: {e}"))?;
    let want = sums
        .lines()
        .find_map(|l| {
            let mut it = l.split_whitespace();
            let (h, f) = (it.next()?, it.next()?);
            (f.trim_start_matches('*') == BUNDLE).then(|| h.to_lowercase())
        })
        .ok_or_else(|| format!("checksums.txt for v{version} does not list {BUNDLE}"))?;

    let mut bytes = Vec::new();
    ureq::get(&format!("{base}/{BUNDLE}"))
        .timeout(std::time::Duration::from_secs(600))
        .call()
        .map_err(|e| format!("downloading {BUNDLE}: {e}"))?
        .into_reader()
        .read_to_end(&mut bytes)
        .map_err(|e| format!("downloading {BUNDLE}: {e}"))?;
    let got = format!("{:x}", Sha256::digest(&bytes));
    if got != want {
        return Err(format!(
            "{BUNDLE} does not match checksums.txt (expected {want}, got {got}); nothing was run"
        ));
    }
    std::fs::write(archive, &bytes).map_err(|e| format!("writing {}: {e}", archive.display()))
}

/// Registers the server with Claude Code at user scope, unless a `deciduous`
/// server is already registered there (which is left as it is).
fn register_claude_code(url: &str, token: &str) {
    let Ok(out) = Command::new("claude")
        .args(["mcp", "get", "deciduous"])
        .output()
    else {
        println!(
            "   {} Claude Code not found; register the server in your client: {url}/mcp",
            "Note".yellow()
        );
        return;
    };
    if out.status.success() {
        println!(
            "   {} Claude Code already has a deciduous MCP server",
            "Unchanged".dimmed()
        );
        return;
    }
    let added = Command::new("claude")
        .args([
            "mcp",
            "add",
            "--scope",
            "user",
            "--transport",
            "http",
            "deciduous",
            &format!("{url}/mcp"),
            "--header",
            &format!("Authorization: Bearer {token}"),
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if added {
        println!(
            "   {} Claude Code: deciduous MCP at {url}/mcp (user scope; restart Claude Code)",
            "Registered".green()
        );
    } else {
        println!(
            "   {} could not register with Claude Code; run: claude mcp add --scope user --transport http deciduous {url}/mcp --header \"Authorization: Bearer $DECIDUOUS_MCP_TOKEN\"",
            "Note".yellow()
        );
    }
}

use std::io::Read as _;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_quoted_and_bare_values_without_executing_anything() {
        let env = "COMPOSE_PROJECT_NAME=deciduous\nDECIDUOUS_PORT=4010\nDECIDUOUS_MCP_TOKEN='abc$(rm -rf /)'\n";
        assert_eq!(env_value(env, "DECIDUOUS_PORT").as_deref(), Some("4010"));
        assert_eq!(
            env_value(env, "DECIDUOUS_MCP_TOKEN").as_deref(),
            Some("abc$(rm -rf /)")
        );
        assert_eq!(local_url(env), "http://127.0.0.1:4010");
        assert_eq!(local_url(""), "http://127.0.0.1:4000");
    }

    #[test]
    fn the_skip_variable_skips() {
        std::env::set_var(SKIP_ENV, "1");
        let t = tempfile::TempDir::new().unwrap();
        assert!(ensure(t.path(), Caller::Init).is_ok());
        std::env::remove_var(SKIP_ENV);
    }
}
