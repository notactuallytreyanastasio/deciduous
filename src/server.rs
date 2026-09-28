//! The shared graph server every project writes to, set up by `init` and
//! checked by `update`.
//!
//! Since 1.0.3 agents write through the HTTP MCP server, and a project with
//! no server is a project whose agents have nowhere to write. So `init` and
//! `update` do not finish until the project points at a server that answers:
//!
//! - a project with `[remote] url` is checked: the server must answer and a
//!   token must be stored;
//! - a project without one is asked where its graph lives (`setup_wizard`),
//!   in a terminal; with no terminal the command stops and names
//!   `remote setup --local` / `--url`.
//!
//! "This machine" is, by default, the release's single-file server
//! executable (`deciduous-mcp-<os>-<arch>`, the same version as this binary,
//! verified against the release's `checksums.txt`) installed under
//! `~/.config/deciduous/server/` and run as a user service — a launchd
//! LaunchAgent on macOS, a systemd user unit on Linux — against PostgreSQL on
//! localhost:5432, or any PostgreSQL URL the user gives. The executable
//! creates its database if it is missing and migrates on every start.
//! Docker is the alternative, not the requirement: `--docker` runs the server
//! from the release's `deciduous-mcp-docker.tar.gz` via its
//! `scripts/setup.sh`, and `--docker-postgres` adds a PostgreSQL container
//! (before 1.0.11 that was the only way). Settings and credentials are in
//! `~/.config/deciduous/server/.env` either way; `DECIDUOUS_SERVER_MODE` in it
//! says which, and a file without it predates native servers and is Docker.
//! See `local_server` for the decisions and file formats.
//!
//! The port is asked, never assumed. A fixed default squats a port the machine
//! may already want — 4000 is Phoenix's, and a server sitting on it breaks the
//! dev server of the very project being logged. So first setup suggests a
//! random free port (`suggested_port`) and the user can accept or replace it;
//! `--port` and `DECIDUOUS_PORT` answer for scripts.
//!
//! `DECIDUOUS_NO_SERVER=1` skips the step for tests and CI, which run `init`
//! in throwaway directories.

use crate::config::Config;
use crate::local_server::{
    self as local, env_value, Database, DbUrl, LocalRequest, ServerMode, ServicePaths, Settings,
};
use crate::remote::{self, Remote};
use colored::Colorize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const SKIP_ENV: &str = "DECIDUOUS_NO_SERVER";
/// A local bundle to use instead of downloading one (for unreleased builds).
pub const BUNDLE_ENV: &str = "DECIDUOUS_SERVER_BUNDLE";
/// A local server executable to install instead of downloading one (for
/// unreleased builds: `BURRITO_TARGET=darwin_arm64 mix release` in
/// deciduous_mcp puts one in burrito_out/).
pub const BINARY_ENV: &str = "DECIDUOUS_SERVER_BINARY";
const RELEASES: &str = "https://github.com/notactuallytreyanastasio/deciduous/releases/download";
const BUNDLE: &str = "deciduous-mcp-docker.tar.gz";
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Why setting up this machine's server stopped. A `Retry` is a database
/// that does not answer or refuses the URL: in the wizard the user can pick
/// another; anywhere else it is an error like any other.
enum LocalError {
    Retry(String),
    Fatal(String),
}

impl From<String> for LocalError {
    fn from(e: String) -> Self {
        LocalError::Fatal(e)
    }
}

impl From<&str> for LocalError {
    fn from(e: &str) -> Self {
        LocalError::Fatal(e.to_string())
    }
}

impl LocalError {
    fn message(self) -> String {
        match self {
            LocalError::Retry(m) | LocalError::Fatal(m) => m,
        }
    }
}

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

/// The range a suggested port is drawn from: above the registered-services
/// range, and below the ephemeral range macOS (49152+) and Linux (32768+) hand
/// out for outgoing connections, so a suggestion cannot collide with a client
/// socket the machine opens later.
const PORT_FLOOR: u16 = 20000;
const PORT_CEILING: u16 = 32767;

/// A random free port to suggest for this machine's server. Four draws, each
/// checked by binding it; the last draw is returned unchecked, since a port
/// that is free now can be taken before Docker publishes it anyway — setup
/// fails loudly in that case rather than silently picking something else.
fn suggested_port() -> u16 {
    let span = u32::from(PORT_CEILING - PORT_FLOOR) + 1;
    let draw = || {
        let bytes = *uuid::Uuid::new_v4().as_bytes();
        let n = u32::from_be_bytes([0, 0, bytes[0], bytes[1]]);
        PORT_FLOOR + (n % span) as u16
    };
    let free = |port: u16| std::net::TcpListener::bind(("127.0.0.1", port)).is_ok();
    (0..3)
        .map(|_| draw())
        .find(|&p| free(p))
        .unwrap_or_else(draw)
}

/// The URL of the server described by a settings file. A settings file always
/// carries `DECIDUOUS_PORT` (setup.sh writes it when it creates the file), so a
/// missing one means a hand-edited file: say so rather than guess a port and
/// report a healthy server that is really someone else's.
fn local_url(env_text: &str) -> Result<String, String> {
    let port = env_value(env_text, "DECIDUOUS_PORT").ok_or(
        "the server settings file has no DECIDUOUS_PORT, so the server's port is unknown. \
         Add the line the server was started on, or delete the file and set the server up again.",
    )?;
    Ok(format!("http://127.0.0.1:{port}"))
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
        // Where the graph lives is the user's choice, not a default: ask in a
        // terminal, and without one say how to answer on the command line.
        None => {
            use std::io::IsTerminal;
            if std::io::stdin().is_terminal() {
                println!(
                    "   This project has no server yet ([remote] in .deciduous/config.toml).\n"
                );
                setup_wizard(project, SetupChoice::Ask)
            } else {
                Err(format!(
                    "{} has no server configured ([remote] in .deciduous/config.toml), and \
                     there is no terminal to ask where its graph should live. Choose one:\n\n{}\n\
                     then rerun this command. Tests and CI in a throwaway directory can set {SKIP_ENV}=1.",
                    project.display(),
                    SCRIPTED_CHOICES
                ))
            }
        }
    }
}

const SCRIPTED_CHOICES: &str = "    \
    deciduous remote setup --local                  # this machine: a background service, PostgreSQL on localhost:5432\n    \
    deciduous remote setup --database-url <url>     # this machine's server, a PostgreSQL you name\n    \
    deciduous remote setup --docker-postgres        # this machine: PostgreSQL and the server in Docker\n    \
    deciduous remote setup --url <url>              # a server someone else runs\n";

/// Points the project at this machine's server, setting it up if needed.
fn connect_local(project: &Path, req: &LocalRequest, interactive: bool) -> Result<(), LocalError> {
    let (url, token) = local_server(req, interactive)?;
    connect(project, &url, &token).map_err(LocalError::Fatal)
}

/// Stores the token, writes `[remote]`, verifies, registers with Claude Code.
fn connect(project: &Path, url: &str, token: &str) -> Result<(), String> {
    if remote::token().ok().as_deref() != Some(token) {
        let path = remote::store_token(token)?;
        println!("   {} token in {}", "Stored".green(), path.display());
    }
    remote::write_remote_url(project, url)?;
    println!(
        "   {} .deciduous/config.toml [remote] url = {url}",
        "Wrote".green()
    );
    check_configured(project, url, Caller::Init)?;
    register_claude_code(url, token);
    Ok(())
}

/// What `deciduous remote setup` was told on the command line, if anything.
/// `Local` carries what was said about this machine's server; anything not
/// said is asked in a terminal or defaulted (see `local_server::resolve`).
pub enum SetupChoice {
    Ask,
    Local(LocalRequest),
    Url(String),
}

/// `deciduous remote setup`: the same end state as `init`'s server step,
/// chosen interactively (or by flags in a script).
pub fn setup_wizard(project: &Path, choice: SetupChoice) -> Result<(), String> {
    use std::io::IsTerminal;
    let interactive = std::io::stdin().is_terminal();

    println!("{}", "deciduous remote setup".cyan().bold());
    println!(
        "Every project writes its decision graph to a shared server. This connects {}\n\
         to one: a server on this machine, or one someone else runs.\n",
        project.display()
    );

    let choice = match choice {
        SetupChoice::Ask if !interactive => {
            return Err(format!(
                "stdin is not a terminal, so there is no one to ask. Use\n\n{SCRIPTED_CHOICES}\n\
                 (--url takes its token from DECIDUOUS_MCP_TOKEN or `remote login`.)"
            ))
        }
        SetupChoice::Ask => {
            if let Some(current) = remote::read_remote_url(project) {
                println!("This project already points at {}", current.cyan());
                if ask_yes("Keep it?", true)? {
                    check_configured(project, &current, Caller::Init)?;
                    let token = remote::token()?;
                    register_claude_code(&current, &token);
                    return done(&current);
                }
            }
            // A local choice that fails on its database comes back here, so
            // the user can pick another instead of starting over.
            loop {
                match ask_where()? {
                    SetupChoice::Local(req) => {
                        println!();
                        match connect_local(project, &req, interactive) {
                            Ok(()) => {
                                let url = remote::read_remote_url(project).unwrap_or_default();
                                return done(&url);
                            }
                            Err(LocalError::Retry(why)) => {
                                println!("\n{} {why}\n", "Not usable:".yellow().bold());
                            }
                            Err(LocalError::Fatal(e)) => return Err(e),
                        }
                    }
                    other => break other,
                }
            }
        }
        c => c,
    };

    match choice {
        SetupChoice::Local(req) => {
            println!();
            connect_local(project, &req, interactive).map_err(LocalError::message)?;
            let url = remote::read_remote_url(project).unwrap_or_default();
            done(&url)
        }
        SetupChoice::Url(url) => {
            let url = url.trim().trim_end_matches('/').to_string();
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return Err(format!("{url:?} is not an http(s) URL"));
            }
            let token = match remote::token() {
                Ok(t)
                    if !interactive
                        || ask_yes("Use the token already stored on this machine?", true)? =>
                {
                    t
                }
                Ok(_) | Err(_) if interactive => ask_secret("Token (not shown)")?,
                _ => {
                    return Err(format!(
                        "no token: set {} or run `deciduous remote login --url {url}` first",
                        remote::TOKEN_ENV
                    ))
                }
            };
            // Checked against the server before anything is stored or written.
            std::env::set_var(remote::TOKEN_ENV, &token);
            let mut cfg = Config::load();
            cfg.remote.url = Some(url.clone());
            Remote::resolve(&cfg, project)
                .and_then(|r| r.check())
                .map_err(|e| format!("{e}\n\nNothing was stored or written."))?;
            println!();
            connect(project, &url, &token)?;
            done(&url)
        }
        SetupChoice::Ask => unreachable!("resolved above"),
    }
}

fn done(url: &str) -> Result<(), String> {
    println!(
        "\n{} this project writes to {url}. Restart Claude Code so it picks up the server;\n\
         it sends the logging instructions when it connects.",
        "Done:".green().bold()
    );
    Ok(())
}

/// The wizard's question. If this machine already has a server, choices 1-3
/// all mean "use it" and the menu says so, rather than offering installs that
/// `local_server` would refuse.
fn ask_where() -> Result<SetupChoice, String> {
    let existing = env_file()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| Settings::parse(&t).ok());
    println!("Where should this project's graph live?\n");
    if let Some(s) = &existing {
        let how = match s.mode {
            ServerMode::Native => "a background service",
            ServerMode::Docker => "Docker",
        };
        println!(
            "  1) This machine: the server already set up here ({how}, port {})",
            s.port.map_or("?".into(), |p| p.to_string())
        );
        println!("  2) A server someone else runs: you need its URL and token\n");
        return match ask("Choose 1 or 2", Some("1"))?.as_str() {
            "1" => Ok(SetupChoice::Local(LocalRequest::default())),
            "2" => Ok(SetupChoice::Url(ask(
                "Server URL (e.g. https://example.com/deciduous-mcp)",
                None,
            )?)),
            other => Err(format!("expected 1 or 2, got {other:?}")),
        };
    }
    let system = DbUrl::system_default(&current_user());
    let status = match tcp_answers(&system) {
        Ok(()) => "answering now".green().to_string(),
        Err(_) => "nothing answers there now".yellow().to_string(),
    };
    println!(
        "  1) This machine: the deciduous server as a background service, using PostgreSQL on\n     \
         localhost:5432 as {} ({status})",
        system.user
    );
    println!("  2) This machine: the server as a background service, using a PostgreSQL you name");
    println!("     (any host, port, user, password)");
    println!("  3) This machine, in Docker: a new PostgreSQL and the server in containers");
    println!("  4) A server someone else runs: you need its URL and token\n");
    println!(
        "  1 and 2 install the release's server executable in {}\n  \
         and start it at login. --docker runs that server in Docker instead.\n",
        server_home().map_or("~/.config/deciduous/server".into(), |h| h
            .display()
            .to_string())
    );
    let local = |database| {
        Ok(SetupChoice::Local(LocalRequest {
            database: Some(database),
            ..LocalRequest::default()
        }))
    };
    match ask("Choose 1-4", Some("1"))?.as_str() {
        "1" => local(Database::System),
        "2" => {
            println!(
                "\nA PostgreSQL URL: postgres://USER@HOST:PORT/DATABASE (add ?sslmode=require or\n\
                 verify-full for TLS). Leave the password out; it is asked next, not shown."
            );
            let url = ask("URL", Some("postgres://postgres@localhost:5432/deciduous"))?;
            let mut db = DbUrl::parse(&url)?;
            if !db.has_password() {
                let pw = ask_secret_optional("Password (not shown; empty for none)")?;
                if !pw.is_empty() {
                    db.set_password(&pw);
                }
            }
            local(Database::Url(db.ecto_url()))
        }
        "3" => local(Database::DockerPostgres),
        "4" => Ok(SetupChoice::Url(ask(
            "Server URL (e.g. https://example.com/deciduous-mcp)",
            None,
        )?)),
        other => Err(format!("expected 1, 2, 3 or 4, got {other:?}")),
    }
}

/// The OS user, who owns the default database role on a Homebrew or
/// Postgres.app install.
fn current_user() -> String {
    ["USER", "LOGNAME"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
        .or_else(|| {
            Command::new("id")
                .arg("-un")
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|u| !u.is_empty())
        })
        .unwrap_or_else(|| "postgres".into())
}

/// Something accepts TCP connections at the database's host and port. Not
/// proof it is PostgreSQL or that the URL works — `prepare_database` is —
/// but it answers "is PostgreSQL even running" in a second, before anything
/// is downloaded.
fn tcp_answers(db: &DbUrl) -> Result<(), String> {
    use std::net::ToSocketAddrs;
    let addrs: Vec<_> = (db.host.as_str(), db.port)
        .to_socket_addrs()
        .map_err(|e| format!("{}: {e}", db.host))?
        .collect();
    let mut last = format!("{} has no address", db.host);
    for addr in addrs {
        match std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_secs(2)) {
            Ok(_) => return Ok(()),
            Err(e) => last = e.to_string(),
        }
    }
    Err(last)
}

fn ask(question: &str, default: Option<&str>) -> Result<String, String> {
    use std::io::Write;
    match default {
        Some(d) => print!("{question} [{d}]: "),
        None => print!("{question}: "),
    }
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| format!("reading the answer: {e}"))?;
    let line = line.trim();
    match (line.is_empty(), default) {
        (true, Some(d)) => Ok(d.to_string()),
        (true, None) => Err(format!("{question}: no answer given")),
        _ => Ok(line.to_string()),
    }
}

fn ask_yes(question: &str, default: bool) -> Result<bool, String> {
    let a = ask(question, Some(if default { "Y/n" } else { "y/N" }))?;
    Ok(match a.to_lowercase().as_str() {
        "y/n" | "y/n " => default,
        "y" | "yes" => true,
        "n" | "no" => false,
        _ => default,
    })
}

/// Reads a line with terminal echo off, so the token is not shown or left on
/// screen. Echo is restored even when reading fails.
fn ask_secret(question: &str) -> Result<String, String> {
    let answer = ask_secret_optional(question)?;
    if answer.is_empty() {
        return Err(format!("{question}: no answer given"));
    }
    Ok(answer)
}

/// As `ask_secret`, where an empty answer is an answer.
fn ask_secret_optional(question: &str) -> Result<String, String> {
    let stty = |arg: &str| {
        Command::new("stty")
            .arg(arg)
            .stdin(
                std::fs::File::open("/dev/tty").map_or(std::process::Stdio::inherit(), Into::into),
            )
            .status()
    };
    let read = || {
        use std::io::Write;
        print!("{question}: ");
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .map(|_| line.trim().to_string())
            .map_err(|e| format!("reading the answer: {e}"))
    };
    let _ = stty("-echo");
    let answer = read();
    let _ = stty("echo");
    println!();
    answer
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

/// This machine's local server: the running one, the one set up here before
/// (started again if it is down), or a new one. Returns its URL and token.
fn local_server(req: &LocalRequest, interactive: bool) -> Result<(String, String), LocalError> {
    let env_path = env_file().ok_or("cannot determine a config directory (is HOME set?)")?;
    if let Ok(text) = std::fs::read_to_string(&env_path) {
        let settings = Settings::parse(&text)?;
        // The settings file is authoritative once written: a server set up
        // one way is not quietly replaced by a flag asking for another. Its
        // database holds every project's graph.
        let plan = local::resolve(req);
        let asked = req.database.is_some() || req.docker;
        if asked && plan.server != settings.mode {
            return Err(LocalError::Fatal(format!(
                "this machine's server is already set up to run {} ({}), and it is kept that \
                 way. To replace it, stop it and move that file away first; its database is \
                 not touched.",
                match settings.mode {
                    ServerMode::Native => "as a background service",
                    ServerMode::Docker => "in Docker",
                },
                env_path.display()
            )));
        }
        if asked || req.port.is_some() {
            println!(
                "   {} the server already set up in {}; its database and port stand",
                "Using".green(),
                env_path.display()
            );
        }
        let url = local_url(&text)?;
        let token = env_value(&text, "DECIDUOUS_MCP_TOKEN")
            .ok_or_else(|| format!("{} has no DECIDUOUS_MCP_TOKEN", env_path.display()))?;
        if answers(&url) {
            println!("   {} local server at {url}", "Found".green());
            return Ok((url, token));
        }
        match settings.mode {
            ServerMode::Native => start_native(&env_path, &settings, &url)?,
            // setup.sh treats its settings file as authoritative on reruns.
            ServerMode::Docker => run_setup(&env_path, None, DockerDatabase::Saved)?,
        }
        if !answers(&url) {
            return Err(LocalError::Fatal(format!(
                "setup finished but {url}/health does not answer"
            )));
        }
        return Ok((url, token));
    }

    let plan = local::resolve(req);
    match (plan.server, plan.database) {
        (ServerMode::Docker, Database::DockerPostgres) => {
            let port = resolve_port(req.port)?;
            run_setup(&env_path, Some(port), DockerDatabase::Container)?;
        }
        (ServerMode::Docker, database) => {
            let db = database_url(&database)?;
            let port = resolve_port(req.port)?;
            run_setup(&env_path, Some(port), DockerDatabase::Host(db))?;
        }
        (ServerMode::Native, database) => {
            let db = database_url(&database)?;
            install_native(&env_path, &db, req.port, interactive)?;
        }
    }
    let text = std::fs::read_to_string(&env_path).map_err(|e| {
        format!(
            "setup finished but {} is unreadable: {e}",
            env_path.display()
        )
    })?;
    let url = local_url(&text)?;
    let token = env_value(&text, "DECIDUOUS_MCP_TOKEN")
        .ok_or_else(|| format!("{} has no DECIDUOUS_MCP_TOKEN", env_path.display()))?;
    if !answers(&url) {
        return Err(LocalError::Fatal(format!(
            "setup finished but {url}/health does not answer"
        )));
    }
    Ok((url, token))
}

/// The database a choice names, checked for "is anything listening" before
/// anything is downloaded or written.
fn database_url(database: &Database) -> Result<DbUrl, LocalError> {
    let db = match database {
        Database::System => DbUrl::system_default(&current_user()),
        Database::Url(u) => DbUrl::parse(u).map_err(LocalError::Retry)?,
        Database::DockerPostgres => unreachable!("a PostgreSQL container is setup.sh's"),
    };
    if let Err(e) = tcp_answers(&db) {
        let hint = if *database == Database::System {
            "\n\nIs PostgreSQL installed and running? (brew services list; systemctl status \
             postgresql). Or name another PostgreSQL with --database-url, or run one in Docker \
             with --docker-postgres."
        } else {
            ""
        };
        return Err(LocalError::Retry(format!(
            "nothing answers at {}:{} ({e}), the database in {}.{hint}",
            db.host,
            db.port,
            db.redacted()
        )));
    }
    Ok(db)
}

/// The token this machine's server accepts. A token this machine already
/// uses becomes it, so one machine never has two; otherwise 64 hex digits
/// from the OS random source (two v4 UUIDs, 244 random bits).
fn machine_token() -> String {
    match remote::token() {
        Ok(t) if t.len() >= 32 => t,
        _ => format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        ),
    }
}

/// The service managers this can install a user service with.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ServiceManager {
    Launchd,
    Systemd,
}

fn service_manager() -> Result<ServiceManager, String> {
    match std::env::consts::OS {
        "macos" => Command::new("launchctl")
            .arg("version")
            .output()
            .map(|_| ServiceManager::Launchd)
            .map_err(|e| format!("launchctl cannot be run ({e})")),
        "linux" => {
            let ok = Command::new("systemctl")
                .args(["--user", "show-environment"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            if ok {
                Ok(ServiceManager::Systemd)
            } else {
                Err("systemd user services are not available here (`systemctl --user` has no \
                     session: a container, WSL without systemd, or a login without a user bus)"
                    .into())
            }
        }
        os => Err(format!(
            "deciduous installs the server as a launchd or systemd user service, and {os} has neither"
        )),
    }
}

/// Where a native install keeps its pieces, under the settings file's
/// directory.
struct NativeLayout {
    home: PathBuf,
    binary: PathBuf,
    runtime: PathBuf,
    log: PathBuf,
}

impl NativeLayout {
    fn new(env_path: &Path) -> Result<NativeLayout, String> {
        let home = env_path
            .parent()
            .ok_or("settings path has no parent")?
            .to_path_buf();
        let exe = if cfg!(windows) {
            "deciduous-mcp.exe"
        } else {
            "deciduous-mcp"
        };
        Ok(NativeLayout {
            binary: home.join("bin").join(exe),
            runtime: home.join("runtime"),
            log: home.join("logs").join("server.log"),
            home,
        })
    }

    fn service_paths<'a>(&'a self, env_path: &'a Path) -> ServicePaths<'a> {
        ServicePaths {
            binary: &self.binary,
            env_file: env_path,
            runtime_dir: &self.runtime,
            home: &self.home,
            log: &self.log,
        }
    }

    fn foreground_command(&self, env_path: &Path) -> String {
        format!(
            "DECIDUOUS_ENV_FILE='{}' DECIDUOUS_MCP_INSTALL_DIR='{}' '{}'",
            env_path.display(),
            self.runtime.display(),
            self.binary.display()
        )
    }
}

/// A new native server: the executable, a database it has prepared, the
/// settings file, and a user service that keeps it running.
fn install_native(
    env_path: &Path,
    db: &DbUrl,
    port: Option<u16>,
    interactive: bool,
) -> Result<(), LocalError> {
    let layout = NativeLayout::new(env_path)?;
    // Before downloading anything: without a service manager there is
    // nothing to install into, and the user may prefer Docker.
    let manager = match service_manager() {
        Ok(m) => Some(m),
        Err(why) => {
            println!("   {} {why}.", "Note".yellow());
            if interactive && ask_yes("Run the server in Docker instead?", true)? {
                let port = resolve_port(port)?;
                return Ok(run_setup(
                    env_path,
                    Some(port),
                    DockerDatabase::Host(db.clone()),
                )?);
            }
            None
        }
    };

    install_binary(&layout, true)?;
    let token = machine_token();
    prepare_database(&layout, db, &token).map_err(LocalError::Retry)?;

    let port = resolve_port(port)?;
    if std::net::TcpListener::bind(("127.0.0.1", port)).is_err() {
        return Err(LocalError::Fatal(format!(
            "port {port} on 127.0.0.1 is already in use. Choose another with --port."
        )));
    }
    write_new_settings(env_path, &local::native_settings(VERSION, port, &token, db))?;
    println!(
        "   {} {} (mode 600: database URL, token, port {port})",
        "Wrote".green(),
        env_path.display()
    );

    let url = format!("http://127.0.0.1:{port}");
    match manager {
        Some(m) => {
            install_service(m, &layout, env_path)?;
            wait_ready(&url, &layout.log)?;
            Ok(())
        }
        None => Err(LocalError::Fatal(format!(
            "the server is installed but nothing here can run it as a service. Start it in a \
             terminal and leave it running:\n\n    {}\n\nthen rerun this command. Or set it up \
             in Docker: move {} away and rerun with --docker.",
            layout.foreground_command(env_path),
            env_path.display()
        ))),
    }
}

/// A native server set up before that is not answering: put back whatever
/// is missing (the executable, the service definition) and start it.
fn start_native(env_path: &Path, settings: &Settings, url: &str) -> Result<(), LocalError> {
    let layout = NativeLayout::new(env_path)?;
    println!(
        "   {} the server at {url}, which is not answering",
        "Starting".yellow()
    );
    let manager = service_manager().map_err(|why| {
        format!(
            "{why}. Start the server in a terminal:\n\n    {}\n",
            layout.foreground_command(env_path)
        )
    })?;
    let stale = local::older_than(settings.version.as_deref(), VERSION);
    if !layout.binary.exists() || stale {
        install_binary(&layout, true)?;
        if stale {
            set_settings_value(env_path, "DECIDUOUS_SERVER_VERSION", VERSION)?;
        }
    }
    install_service(manager, &layout, env_path)?;
    wait_ready(url, &layout.log)?;
    Ok(())
}

/// Puts this version's server executable at `layout.binary`: from
/// `DECIDUOUS_SERVER_BINARY` when set, otherwise downloaded from this
/// version's release and checked against its checksums.txt. Written beside
/// the target and renamed over it, so a running server keeps its file and
/// macOS never sees a signed executable modified in place.
fn install_binary(layout: &NativeLayout, force: bool) -> Result<(), String> {
    if layout.binary.exists() && !force {
        return Ok(());
    }
    let dir = layout.binary.parent().ok_or("binary path has no parent")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    let tmp = dir.join(format!(".deciduous-mcp.{}.tmp", std::process::id()));
    let result = (|| {
        match std::env::var_os(BINARY_ENV) {
            Some(local) => {
                std::fs::copy(&local, &tmp)
                    .map_err(|e| format!("copying {}: {e}", PathBuf::from(&local).display()))?;
                println!(
                    "   {} {} ({BINARY_ENV})",
                    "Using".green(),
                    PathBuf::from(local).display()
                );
            }
            None => {
                let asset = local::server_asset(std::env::consts::OS, std::env::consts::ARCH)
                    .ok_or_else(|| {
                        format!(
                            "no server executable is released for {}-{}. Use Docker \
                                 (--docker) or build one and point {BINARY_ENV} at it.",
                            std::env::consts::OS,
                            std::env::consts::ARCH
                        )
                    })?;
                download_verified(VERSION, &asset, &tmp, BINARY_ENV)?;
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| format!("chmod {}: {e}", tmp.display()))?;
        }
        std::fs::rename(&tmp, &layout.binary)
            .map_err(|e| format!("installing {}: {e}", layout.binary.display()))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result?;
    println!("   {} {}", "Installed".green(), layout.binary.display());
    Ok(())
}

/// Runs the executable once with `DECIDUOUS_MCP_COMMAND=prepare-database`:
/// it connects with the URL, creates the database if PostgreSQL says it is
/// missing, migrates, and exits. A failure comes back in PostgreSQL's words
/// ("role ... does not exist", "password authentication failed", "permission
/// denied to create database"), which says more than anything the CLI could
/// guess from a port probe.
fn prepare_database(layout: &NativeLayout, db: &DbUrl, token: &str) -> Result<(), String> {
    let (ssl, verify) = db.ssl_settings();
    println!(
        "   {} {} (created if missing, then migrated; the first run unpacks the server)",
        "Preparing".green(),
        db.redacted()
    );
    let out = Command::new(&layout.binary)
        .current_dir(&layout.home)
        .env("DECIDUOUS_MCP_COMMAND", "prepare-database")
        .env("DATABASE_URL", db.ecto_url())
        .env("DB_SSL", ssl)
        .env("DB_SSL_VERIFY", verify)
        .env("DECIDUOUS_MCP_TOKEN", token)
        .env("DECIDUOUS_MCP_INSTALL_DIR", &layout.runtime)
        .env("POOL_SIZE", "2")
        .env("PORT", "0")
        .env_remove("DECIDUOUS_ENV_FILE")
        .env_remove("DB_SSL_CA_FILE")
        .env_remove("DECIDUOUS_PREPARE_DATABASE")
        .output()
        .map_err(|e| format!("running {}: {e}", layout.binary.display()))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    if out.status.success() {
        if stdout.contains("created database") {
            println!("   {} database {}", "Created".green(), db.database);
        }
        return Ok(());
    }
    // The command's own message is on stderr, after the log lines; the
    // password is never in it, and is scrubbed anyway in case a driver
    // message ever echoes the URL.
    let stderr = String::from_utf8_lossy(&out.stderr);
    let message: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !is_log_line(l))
        .collect();
    let message = if message.is_empty() {
        format!(
            "the server exited with {} and said nothing (stdout: {})",
            out.status,
            stdout.trim()
        )
    } else {
        message.join("\n")
    };
    Err(scrub(&message, db))
}

/// Logger and launcher output, as opposed to the command's own message.
fn is_log_line(line: &str) -> bool {
    let plain = strip_ansi(line);
    let b = plain.as_bytes();
    plain.starts_with("[i]")
        || plain.starts_with("** (")
        || plain.starts_with("(")
        || (b.len() > 13 && b[2] == b':' && b[5] == b':' && b[8] == b'.')
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn scrub(text: &str, db: &DbUrl) -> String {
    let full = db.ecto_url();
    let mut out = text.replace(&full, &db.redacted());
    if let Some(pw) = full
        .strip_prefix("ecto://")
        .and_then(|r| r.split_once('@'))
        .and_then(|(creds, _)| creds.split_once(':'))
        .map(|(_, pw)| pw.to_string())
    {
        if !pw.is_empty() {
            out = out.replace(&pw, "***");
        }
    }
    out
}

/// Writes a settings file that must not exist yet, owner-only from the first
/// byte: the temporary file is created with mode 600 and hard-linked into
/// place, which fails rather than replacing a file another setup wrote.
fn write_new_settings(env_path: &Path, text: &str) -> Result<(), String> {
    let dir = env_path.parent().ok_or("settings path has no parent")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    let tmp = dir.join(format!(".env.{}.tmp", std::process::id()));
    write_private(&tmp, text)?;
    let linked = std::fs::hard_link(&tmp, env_path);
    let _ = std::fs::remove_file(&tmp);
    linked.map_err(|e| format!("creating {}: {e}", env_path.display()))
}

/// Replaces one value in an existing settings file, keeping mode 600.
fn set_settings_value(env_path: &Path, key: &str, value: &str) -> Result<(), String> {
    let text = std::fs::read_to_string(env_path)
        .map_err(|e| format!("reading {}: {e}", env_path.display()))?;
    let dir = env_path.parent().ok_or("settings path has no parent")?;
    let tmp = dir.join(format!(".env.{}.tmp", std::process::id()));
    write_private(&tmp, &local::with_value(&text, key, value))?;
    std::fs::rename(&tmp, env_path).map_err(|e| format!("updating {}: {e}", env_path.display()))
}

fn write_private(path: &Path, text: &str) -> Result<(), String> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts
        .open(path)
        .map_err(|e| format!("creating {}: {e}", path.display()))?;
    f.write_all(text.as_bytes())
        .map_err(|e| format!("writing {}: {e}", path.display()))
}

fn user_id() -> String {
    // SAFETY: getuid has no preconditions and cannot fail.
    unsafe { libc::getuid() }.to_string()
}

fn launch_agent_path() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
    Ok(PathBuf::from(home)
        .join("Library/LaunchAgents")
        .join(format!("{}.plist", local::LAUNCHD_LABEL)))
}

fn systemd_unit_path() -> Result<PathBuf, String> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .ok_or("HOME is not set")?;
    Ok(base.join("systemd/user").join(local::SYSTEMD_UNIT))
}

fn run_checked(program: &str, args: &[&str]) -> Result<(), String> {
    let out = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("running {program}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "`{program} {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// Writes the service definition and (re)starts the service from it.
fn install_service(
    manager: ServiceManager,
    layout: &NativeLayout,
    env_path: &Path,
) -> Result<(), String> {
    let logs = layout.log.parent().ok_or("log path has no parent")?;
    std::fs::create_dir_all(logs).map_err(|e| format!("creating {}: {e}", logs.display()))?;
    let paths = layout.service_paths(env_path);
    match manager {
        ServiceManager::Launchd => {
            let plist = launch_agent_path()?;
            let dir = plist.parent().ok_or("plist path has no parent")?;
            std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
            std::fs::write(&plist, local::launchd_plist(&paths))
                .map_err(|e| format!("writing {}: {e}", plist.display()))?;
            let domain = format!("gui/{}", user_id());
            let target = format!("{domain}/{}", local::LAUNCHD_LABEL);
            // bootstrap refuses a label that is already loaded; bootout of
            // one that is not is harmless. bootout returns while the old
            // server is still shutting down, and a restart that polled /ready
            // then would hear the old process answer and report the new one
            // ready (it did, on the first upgrade run). So wait until launchd
            // no longer knows the label, which is when the process has gone.
            let loaded = |t: &str| {
                Command::new("launchctl")
                    .args(["print", t])
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false)
            };
            if loaded(&target) {
                let _ = Command::new("launchctl")
                    .args(["bootout", &target])
                    .output();
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
                while loaded(&target) {
                    if std::time::Instant::now() > deadline {
                        return Err(format!(
                            "launchd did not stop {} within 30 seconds",
                            local::LAUNCHD_LABEL
                        ));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
            }
            let plist_s = plist.display().to_string();
            run_checked("launchctl", &["bootstrap", &domain, &plist_s])?;
            println!(
                "   {} launchd agent {} ({}), logs in {}",
                "Started".green(),
                local::LAUNCHD_LABEL,
                plist.display(),
                layout.log.display()
            );
        }
        ServiceManager::Systemd => {
            let unit = systemd_unit_path()?;
            let dir = unit.parent().ok_or("unit path has no parent")?;
            std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
            std::fs::write(&unit, local::systemd_unit(&paths))
                .map_err(|e| format!("writing {}: {e}", unit.display()))?;
            run_checked("systemctl", &["--user", "daemon-reload"])?;
            run_checked("systemctl", &["--user", "enable", local::SYSTEMD_UNIT])?;
            run_checked("systemctl", &["--user", "restart", local::SYSTEMD_UNIT])?;
            println!(
                "   {} systemd user unit {} ({}), logs in {}",
                "Started".green(),
                local::SYSTEMD_UNIT,
                unit.display(),
                layout.log.display()
            );
            println!(
                "   {} user services stop at logout unless lingering is on: loginctl enable-linger {}",
                "Note".yellow(),
                current_user()
            );
        }
    }
    Ok(())
}

/// Polls /ready (which checks the database and migrations) until it answers
/// 200. The first start unpacks the embedded runtime, so allow two minutes.
fn wait_ready(url: &str, log: &Path) -> Result<(), String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    while std::time::Instant::now() < deadline {
        let ok = ureq::get(&format!("{url}/ready"))
            .timeout(std::time::Duration::from_secs(3))
            .call()
            .is_ok();
        if ok {
            println!("   {} {url}/ready", "Ready".green());
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    let tail = std::fs::read_to_string(log)
        .map(|t| {
            let lines: Vec<&str> = t.lines().collect();
            lines[lines.len().saturating_sub(15)..].join("\n")
        })
        .unwrap_or_else(|e| format!("({} unreadable: {e})", log.display()));
    Err(format!(
        "the server did not become ready at {url}/ready within two minutes. The end of {}:\n\n{tail}",
        log.display()
    ))
}

/// The port for a server being set up for the first time: what the caller was
/// told (`--port`), else `DECIDUOUS_PORT`, else asked in a terminal with a
/// random free port suggested, else that suggestion unasked.
fn resolve_port(requested: Option<u16>) -> Result<u16, String> {
    use std::io::IsTerminal;
    if let Some(port) = requested {
        return Ok(port);
    }
    if let Ok(text) = std::env::var("DECIDUOUS_PORT") {
        return parse_port(&text).map_err(|e| format!("DECIDUOUS_PORT: {e}"));
    }
    let suggested = suggested_port();
    if !std::io::stdin().is_terminal() {
        println!("   {} port {suggested}", "Chose".green());
        return Ok(suggested);
    }
    println!(
        "\nThe server needs a port on this machine. {} is free right now; anything\n\
         already using the port you pick (a dev server on 4000, say) would stop this\n\
         server from starting.\n",
        suggested.to_string().cyan()
    );
    let answer = ask("Port", Some(&suggested.to_string()))?;
    parse_port(&answer)
}

/// A port number an answer or the environment supplied. Port 0 is refused
/// rather than passed through: the kernel would choose, and the URL written to
/// the project's config would point at nothing after a restart.
fn parse_port(text: &str) -> Result<u16, String> {
    let text = text.trim();
    match text.parse::<u16>() {
        Ok(0) => Err(
            "port 0 lets the kernel choose a port, and the URL written to the \
                      project's config would not survive a restart. Pick a fixed port."
                .into(),
        ),
        Ok(port) => Ok(port),
        Err(_) => Err(format!("{text:?} is not a port number (1-65535)")),
    }
}

/// Docker is required only on the paths that use it; this runs there and
/// nowhere else.
fn docker_ready() -> Result<(), String> {
    let installed = Command::new("docker").arg("--version").output().is_ok();
    if !installed {
        return Err(
            "Docker is not installed, and the choice made needs it (--docker or \
             --docker-postgres, or a Docker install set up here before).\n\n\
             Install Docker Desktop (https://docs.docker.com/get-docker/) and rerun, or run \
             the server without Docker, as a background service against PostgreSQL on this \
             machine:\n\n    deciduous remote setup --local\n    \
             deciduous remote setup --database-url postgres://USER@HOST:5432/deciduous"
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

/// Which database setup.sh points the server container at.
enum DockerDatabase {
    /// A new PostgreSQL container (setup.sh's default, local mode).
    Container,
    /// A PostgreSQL the user named, possibly on this machine's localhost
    /// (setup.sh --external-database).
    Host(DbUrl),
    /// Whatever the existing settings file says; setup.sh ignores the
    /// environment once that file exists.
    Saved,
}

/// Downloads (or takes from `DECIDUOUS_SERVER_BUNDLE`) this version's bundle,
/// verifies it, and runs its setup script against the machine's settings file.
fn run_setup(env_path: &Path, port: Option<u16>, database: DockerDatabase) -> Result<(), String> {
    docker_ready()?;
    let home = env_path.parent().ok_or("settings path has no parent")?;
    let dir = home.join(format!("bundle-{VERSION}"));
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
            None => download_verified(VERSION, BUNDLE, &archive, BUNDLE_ENV)?,
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

    let mut cmd = Command::new("sh");
    cmd.arg(&script)
        .env("DECIDUOUS_ENV_FILE", env_path)
        .env(
            "COMPOSE_PROJECT_NAME",
            std::env::var("COMPOSE_PROJECT_NAME").unwrap_or_else(|_| "deciduous".into()),
        )
        .env_remove("DATABASE_URL")
        .env_remove("DB_SSL")
        .env_remove("DB_SSL_VERIFY");
    match &database {
        DockerDatabase::Container => println!(
            "   {} PostgreSQL and the server with Docker (the first build takes a few minutes)",
            "Setting up".green()
        ),
        DockerDatabase::Saved => println!(
            "   {} the Docker install in {}",
            "Starting".green(),
            env_path.display()
        ),
        DockerDatabase::Host(db) => {
            let (inside, note) = db.for_container(std::env::consts::OS);
            println!(
                "   {} the server with Docker, using {} (the first build takes a few minutes)",
                "Setting up".green(),
                db.redacted()
            );
            if let Some(note) = note {
                println!("   {} {note}", "Note".yellow());
            }
            let (ssl, verify) = inside.ssl_settings();
            cmd.arg("--external-database")
                .env("DATABASE_URL", inside.ecto_url())
                .env("DB_SSL", ssl)
                .env("DB_SSL_VERIFY", verify);
        }
    }
    // Only on first setup: setup.sh ignores the environment once its settings
    // file exists, and `local_server` passes None in that case.
    match port {
        Some(p) => {
            cmd.env("DECIDUOUS_PORT", p.to_string());
        }
        None => {
            cmd.env_remove("DECIDUOUS_PORT");
        }
    }
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

/// Downloads `asset` from this version's release into `dest`, refusing it
/// unless its SHA-256 is the one the release's checksums.txt lists.
/// `override_env` names the variable that points at a local file instead,
/// for builds that are not releases.
fn download_verified(
    version: &str,
    asset: &str,
    dest: &Path,
    override_env: &str,
) -> Result<(), String> {
    let base = format!("{RELEASES}/v{version}");
    println!("   {} {base}/{asset}", "Downloading".green());
    let sums = ureq::get(&format!("{base}/checksums.txt"))
        .timeout(std::time::Duration::from_secs(60))
        .call()
        .map_err(|e| {
            format!(
                "fetching checksums.txt for v{version}: {e}\n\n\
                 A build that is not a release has nothing to download. Point \
                 {override_env} at a local {asset} and rerun."
            )
        })?
        .into_string()
        .map_err(|e| format!("reading checksums.txt: {e}"))?;
    let want = checksum_for(&sums, asset)
        .ok_or_else(|| format!("checksums.txt for v{version} does not list {asset}"))?;

    let mut bytes = Vec::new();
    ureq::get(&format!("{base}/{asset}"))
        .timeout(std::time::Duration::from_secs(600))
        .call()
        .map_err(|e| format!("downloading {asset}: {e}"))?
        .into_reader()
        .read_to_end(&mut bytes)
        .map_err(|e| format!("downloading {asset}: {e}"))?;
    let got = format!("{:x}", Sha256::digest(&bytes));
    if got != want {
        return Err(format!(
            "{asset} does not match checksums.txt (expected {want}, got {got}); nothing was run"
        ));
    }
    std::fs::write(dest, &bytes).map_err(|e| format!("writing {}: {e}", dest.display()))
}

/// The SHA-256 `sha256sum` output lists for `asset` (`<hex>  <name>` or
/// `<hex> *<name>`).
fn checksum_for(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let (h, f) = (it.next()?, it.next()?);
        (f.trim_start_matches('*') == asset).then(|| h.to_lowercase())
    })
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
        assert_eq!(local_url(env).unwrap(), "http://127.0.0.1:4010");
    }

    #[test]
    fn a_settings_file_without_a_port_is_an_error_not_a_guessed_port() {
        let e = local_url("DECIDUOUS_MCP_TOKEN='x'\n").unwrap_err();
        assert!(e.contains("DECIDUOUS_PORT"), "{e}");
    }

    #[test]
    fn a_suggested_port_is_free_and_outside_the_ephemeral_range() {
        for _ in 0..20 {
            let port = suggested_port();
            assert!(
                (PORT_FLOOR..=PORT_CEILING).contains(&port),
                "{port} is outside {PORT_FLOOR}-{PORT_CEILING}"
            );
        }
    }

    #[test]
    fn two_suggestions_are_not_the_same_port_every_time() {
        let first = suggested_port();
        assert!(
            (0..20).any(|_| suggested_port() != first),
            "suggested_port returned {first} twenty times, so it is not random"
        );
    }

    #[test]
    fn an_explicit_port_is_taken_over_asking() {
        assert_eq!(resolve_port(Some(4010)).unwrap(), 4010);
    }

    #[test]
    fn a_port_that_is_not_a_number_is_refused() {
        assert!(parse_port("eighty").unwrap_err().contains("not a port"));
        assert!(parse_port("70000").unwrap_err().contains("not a port"));
        assert!(parse_port("").unwrap_err().contains("not a port"));
    }

    #[test]
    fn port_zero_is_refused_so_the_written_url_keeps_working() {
        assert!(parse_port("0").unwrap_err().contains("survive a restart"));
    }

    #[test]
    fn surrounding_whitespace_in_an_answer_is_ignored() {
        assert_eq!(parse_port(" 4010\n").unwrap(), 4010);
    }

    #[test]
    fn the_checksum_is_found_for_the_named_asset_only() {
        let sums = "aa11  deciduous-mcp-darwin-arm64\nBB22 *deciduous-mcp-docker.tar.gz\n\
                    cc33  deciduous-mcp-darwin-arm64.sig\n";
        assert_eq!(
            checksum_for(sums, "deciduous-mcp-darwin-arm64").as_deref(),
            Some("aa11")
        );
        assert_eq!(
            checksum_for(sums, "deciduous-mcp-docker.tar.gz").as_deref(),
            Some("bb22")
        );
        assert_eq!(checksum_for(sums, "deciduous-mcp-linux-amd64"), None);
    }

    #[test]
    fn the_prepare_commands_message_is_kept_and_its_log_lines_dropped() {
        // As the executable writes them: Burrito's launcher notes, a coloured
        // Logger line, then the command's own message.
        let stderr = "[i] Install path is being overridden using `DECIDUOUS_MCP_INSTALL_DIR`\n\
             \u{1b}[31m14:09:57.881 [error] #PID<0.208.0> (Postgrex.Notifications) failed\n\
             \u{1b}[0mcannot connect to nosuchrole@localhost:5432/d: FATAL 28000 role \"nosuchrole\" does not exist\n";
        let kept: Vec<&str> = stderr
            .lines()
            .filter(|l| !l.trim().is_empty() && !is_log_line(l.trim()))
            .collect();
        assert_eq!(kept.len(), 1, "{kept:?}");
        assert!(strip_ansi(kept[0]).starts_with("cannot connect to nosuchrole@"));
    }

    #[test]
    fn a_password_is_scrubbed_from_anything_the_server_says() {
        let db = DbUrl::parse("postgres://ann:hunter2@db:5432/g").unwrap();
        let said = format!("failed for {} (password hunter2)", db.ecto_url());
        let shown = scrub(&said, &db);
        assert!(!shown.contains("hunter2"), "{shown}");
        assert!(shown.contains("postgres://ann:***@db:5432/g"));
    }

    #[test]
    fn a_settings_file_is_created_private_and_never_replaced() {
        let t = tempfile::TempDir::new().unwrap();
        let path = t.path().join("server/.env");
        write_new_settings(&path, "DECIDUOUS_PORT=1\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        assert!(write_new_settings(&path, "DECIDUOUS_PORT=2\n").is_err());
        set_settings_value(&path, "DECIDUOUS_SERVER_VERSION", "9.9.9").unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "DECIDUOUS_PORT=1\nDECIDUOUS_SERVER_VERSION=9.9.9\n"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "rewriting keeps it private");
        }
    }

    #[test]
    fn the_skip_variable_skips() {
        std::env::set_var(SKIP_ENV, "1");
        let t = tempfile::TempDir::new().unwrap();
        assert!(ensure(t.path(), Caller::Init).is_ok());
        std::env::remove_var(SKIP_ENV);
    }
}
