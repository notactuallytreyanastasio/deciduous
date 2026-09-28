//! The pieces of "a server on this machine" that are decisions rather than
//! side effects: which database, which way to run the server, what the
//! settings file says, and what the service definitions contain. `server.rs`
//! does the downloading, installing and starting; everything here is pure so
//! it can be tested without a service manager or a database.
//!
//! Before 1.0.11 this machine's server always ran in Docker, with PostgreSQL
//! in a second container. Most machines that run deciduous already have a
//! PostgreSQL (Homebrew, Postgres.app, a distribution package), and Docker
//! Desktop is a large thing to require for one small service. So the default
//! is now the release's single-file server executable, run as a user service
//! (launchd on macOS, systemd --user on Linux) against PostgreSQL on
//! localhost:5432. Docker stays available: `--docker` runs the server in a
//! container, `--docker-postgres` runs PostgreSQL in one too (the old path).

use std::fmt;
use std::path::Path;

/// The database the user named, or asked for by default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Database {
    /// PostgreSQL on localhost:5432 as the current OS user, no password.
    System,
    /// A URL the user gave (`--database-url`, or typed into the wizard).
    Url(String),
    /// A new PostgreSQL container managed by setup.sh (the pre-1.0.11 path).
    DockerPostgres,
}

/// How the server itself runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerMode {
    /// The release's executable, as a launchd/systemd user service.
    Native,
    /// The server container from the release's Docker bundle.
    Docker,
}

impl fmt::Display for ServerMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ServerMode::Native => "native",
            ServerMode::Docker => "docker",
        })
    }
}

/// What `remote setup` was told about this machine's server on the command
/// line. Every field is optional: `None` means "not said", which the wizard
/// asks about in a terminal and which defaults otherwise.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalRequest {
    pub port: Option<u16>,
    pub database: Option<Database>,
    /// `--docker`: the server in a container.
    pub docker: bool,
}

/// A request resolved into what will be installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub server: ServerMode,
    pub database: Database,
}

/// Flags to a plan. A PostgreSQL container is only reachable from the
/// server's container network, so `--docker-postgres` implies the server in
/// Docker too. The environment is never consulted: a DATABASE_URL exported
/// for some other project must not become this machine's graph database
/// without anyone saying so; `--database-url "$DATABASE_URL"` says so.
pub fn resolve(req: &LocalRequest) -> Plan {
    let database = req.database.clone().unwrap_or(Database::System);
    let server = if req.docker || database == Database::DockerPostgres {
        ServerMode::Docker
    } else {
        ServerMode::Native
    };
    Plan { server, database }
}

/// A PostgreSQL URL, split so it can be shown without its password,
/// rewritten for a container, and written as the `ecto://` form the server
/// reads.
#[derive(Clone, PartialEq, Eq)]
pub struct DbUrl {
    pub user: String,
    password: Option<String>,
    pub host: String,
    pub port: u16,
    pub database: String,
    /// libpq's sslmode, if the URL carried one.
    pub sslmode: Option<String>,
}

/// Never prints the password, even through `{:?}`.
impl fmt::Debug for DbUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.redacted())
    }
}

impl DbUrl {
    /// PostgreSQL on this machine as the current user, database `deciduous`.
    /// No password: Homebrew and Postgres.app create a superuser named after
    /// the OS user and trust local connections, and Debian-style packages
    /// authenticate local users by name. Anything else is what "configure it
    /// yourself" is for.
    pub fn system_default(user: &str) -> DbUrl {
        DbUrl {
            user: user.to_string(),
            password: None,
            host: "localhost".into(),
            port: 5432,
            database: "deciduous".into(),
            sslmode: None,
        }
    }

    /// Accepts `postgres://`, `postgresql://` and `ecto://`, with or without
    /// a password and port. The only query parameter understood is libpq's
    /// `sslmode`; any other is refused rather than silently dropped.
    pub fn parse(input: &str) -> Result<DbUrl, String> {
        let input = input.trim();
        if input.contains(['\'', '\n', '\r']) {
            return Err(
                "the database URL cannot contain quotes or line breaks; percent-encode them \
                 (' is %27)"
                    .into(),
            );
        }
        let rest = ["postgres://", "postgresql://", "ecto://"]
            .iter()
            .find_map(|s| input.strip_prefix(s))
            .ok_or("a PostgreSQL URL starts with postgres://, postgresql:// or ecto://")?;
        let (rest, query) = match rest.split_once('?') {
            Some((r, q)) => (r, Some(q)),
            None => (rest, None),
        };
        let (authority, database) = rest
            .split_once('/')
            .ok_or("the URL names no database (…/deciduous at the end)")?;
        if database.is_empty() || database.contains('/') {
            return Err(format!(
                "{database:?} is not a database name; end the URL with /<database>"
            ));
        }
        // The last @ separates credentials: a percent-decoded password may
        // not contain one, but a careless one might.
        let (userinfo, hostport) = match authority.rsplit_once('@') {
            Some((u, h)) => (Some(u), h),
            None => (None, authority),
        };
        let (user, password) = match userinfo {
            Some(u) => match u.split_once(':') {
                Some((user, pw)) => (user.to_string(), Some(pw.to_string())),
                None => (u.to_string(), None),
            },
            None => (String::new(), None),
        };
        let (host, port) = if let Some(h) = hostport.strip_prefix('[') {
            // [::1]:5432
            let (h, after) = h.split_once(']').ok_or("unclosed [ in the host")?;
            (h.to_string(), after.strip_prefix(':'))
        } else {
            match hostport.rsplit_once(':') {
                Some((h, p)) => (h.to_string(), Some(p)),
                None => (hostport.to_string(), None),
            }
        };
        if host.is_empty() {
            return Err("the URL names no host".into());
        }
        let port = match port {
            None => 5432,
            Some(p) => p
                .parse::<u16>()
                .ok()
                .filter(|&p| p != 0)
                .ok_or_else(|| format!("{p:?} is not a port number"))?,
        };
        let mut sslmode = None;
        for pair in query.unwrap_or("").split('&').filter(|p| !p.is_empty()) {
            match pair.split_once('=') {
                Some(("sslmode", v)) => match v {
                    "disable" | "allow" | "prefer" | "require" | "verify-ca" | "verify-full" => {
                        sslmode = Some(v.to_string())
                    }
                    other => return Err(format!("sslmode={other} is not one libpq knows")),
                },
                _ => {
                    return Err(format!(
                        "unsupported URL parameter {pair:?}: only sslmode is understood \
                         (other settings go in ~/.config/deciduous/server/.env)"
                    ))
                }
            }
        }
        if user.is_empty() {
            return Err(
                "the URL names no user (postgres://USER@host/db); the server runs as a \
                 service, where there is no login name to fall back on"
                    .into(),
            );
        }
        Ok(DbUrl {
            user,
            password,
            host,
            port,
            database: database.to_string(),
            sslmode,
        })
    }

    pub fn has_password(&self) -> bool {
        self.password.is_some()
    }

    /// Sets a password typed at a prompt, percent-encoding everything but
    /// RFC 3986's unreserved characters; the server URL-decodes it.
    pub fn set_password(&mut self, plain: &str) {
        let encoded = plain
            .bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                    (b as char).to_string()
                }
                _ => format!("%{b:02X}"),
            })
            .collect();
        self.password = Some(encoded);
    }

    fn host_for_url(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        }
    }

    /// The URL the server reads, with the password. Only ever written to the
    /// settings file (mode 600) or a child process's environment.
    pub fn ecto_url(&self) -> String {
        let creds = match &self.password {
            Some(p) => format!("{}:{p}", self.user),
            None => self.user.clone(),
        };
        format!(
            "ecto://{creds}@{}:{}/{}",
            self.host_for_url(),
            self.port,
            self.database
        )
    }

    /// The URL for people: the password, if any, as `***`.
    pub fn redacted(&self) -> String {
        let creds = match &self.password {
            Some(_) => format!("{}:***", self.user),
            None => self.user.clone(),
        };
        let q = self
            .sslmode
            .as_ref()
            .map(|m| format!("?sslmode={m}"))
            .unwrap_or_default();
        format!(
            "postgres://{creds}@{}:{}/{}{q}",
            self.host_for_url(),
            self.port,
            self.database
        )
    }

    pub fn is_loopback(&self) -> bool {
        matches!(self.host.as_str(), "localhost" | "127.0.0.1" | "::1")
    }

    /// DB_SSL and DB_SSL_VERIFY for the server, in the terms of
    /// config/runtime.exs. `prefer` and `allow` are plain connections: the
    /// server does not fall back between TLS and no TLS, and on a machine's
    /// own PostgreSQL there is nothing to protect in transit.
    pub fn ssl_settings(&self) -> (&'static str, &'static str) {
        match self.sslmode.as_deref() {
            Some("require") => ("true", "none"),
            Some("verify-ca") => ("true", "ca"),
            Some("verify-full") => ("true", "full"),
            _ => ("false", "full"),
        }
    }

    /// The same database seen from inside a container, where localhost is
    /// the container itself. Docker Desktop (macOS, Windows) routes
    /// host.docker.internal to the host's loopback; on Linux, compose.yaml
    /// maps that name to the host gateway, which PostgreSQL only answers on
    /// if it listens there. Returns the URL and a sentence saying which.
    pub fn for_container(&self, os: &str) -> (DbUrl, Option<String>) {
        if !self.is_loopback() {
            return (self.clone(), None);
        }
        let mut url = self.clone();
        url.host = "host.docker.internal".into();
        let note = if os == "linux" {
            format!(
                "The server container reaches PostgreSQL on this machine at host.docker.internal, \
                 the Docker host gateway (compose.yaml maps it). PostgreSQL must listen there \
                 (listen_addresses in postgresql.conf) and pg_hba.conf must admit the Docker \
                 networks (172.16.0.0/12) for user {}; out of the box it listens on localhost \
                 only.",
                self.user
            )
        } else {
            format!(
                "The server container reaches PostgreSQL on this machine at host.docker.internal \
                 (Docker Desktop routes it to this machine's localhost:{}).",
                self.port
            )
        };
        (url, Some(note))
    }
}

/// The settings file (`~/.config/deciduous/server/.env`) as far as the CLI
/// needs to understand it. It is read, never executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub mode: ServerMode,
    pub port: Option<u16>,
    pub token: Option<String>,
    /// The server version installed, for a native server. Docker installs
    /// build from the bundle of the CLI that set them up and do not record it.
    pub version: Option<String>,
}

/// A `KEY=value` or `KEY='value'` line.
pub fn env_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| {
        l.strip_prefix(&format!("{key}="))
            .map(|v| v.trim().trim_matches('\'').to_string())
    })
}

impl Settings {
    /// A file from any deciduous version. Before 1.0.11 there was only Docker
    /// and no DECIDUOUS_SERVER_MODE; such a file (it has
    /// DECIDUOUS_DATABASE_MODE=local or external) is a Docker install and
    /// stays one.
    pub fn parse(text: &str) -> Result<Settings, String> {
        let mode = match env_value(text, "DECIDUOUS_SERVER_MODE").as_deref() {
            Some("native") => ServerMode::Native,
            Some("docker") | None => ServerMode::Docker,
            Some(other) => {
                return Err(format!(
                    "DECIDUOUS_SERVER_MODE={other} in the server settings file is neither native \
                     nor docker"
                ))
            }
        };
        let port = env_value(text, "DECIDUOUS_PORT").and_then(|p| p.parse().ok());
        Ok(Settings {
            mode,
            port,
            token: env_value(text, "DECIDUOUS_MCP_TOKEN"),
            version: env_value(text, "DECIDUOUS_SERVER_VERSION"),
        })
    }
}

/// The settings file for a native server. Every key the server reads is
/// written, so nothing depends on the environment of whoever starts it.
pub fn native_settings(version: &str, port: u16, token: &str, db: &DbUrl) -> String {
    let (ssl, verify) = db.ssl_settings();
    format!(
        "DECIDUOUS_SERVER_MODE=native\n\
         DECIDUOUS_SERVER_VERSION={version}\n\
         DECIDUOUS_BIND_ADDRESS=127.0.0.1\n\
         DECIDUOUS_PORT={port}\n\
         DECIDUOUS_MCP_TOKEN='{token}'\n\
         DATABASE_URL='{url}'\n\
         DB_SSL={ssl}\n\
         DB_SSL_VERIFY={verify}\n\
         DB_SSL_CA_FILE=\n\
         POOL_SIZE=10\n\
         DECIDUOUS_PREPARE_DATABASE=true\n",
        url = db.ecto_url()
    )
}

/// `text` with `KEY=value` replaced (or appended when absent).
pub fn with_value(text: &str, key: &str, value: &str) -> String {
    let prefix = format!("{key}=");
    let mut found = false;
    let mut out: Vec<String> = text
        .lines()
        .map(|l| {
            if l.starts_with(&prefix) {
                found = true;
                format!("{prefix}{value}")
            } else {
                l.to_string()
            }
        })
        .collect();
    if !found {
        out.push(format!("{prefix}{value}"));
    }
    out.join("\n") + "\n"
}

/// `a.b.c` ordering, for "is the installed server older than this CLI".
/// Anything unparsable counts as older, so it gets replaced.
pub fn older_than(installed: Option<&str>, current: &str) -> bool {
    fn parse(v: &str) -> Option<(u64, u64, u64)> {
        let mut it = v.trim().trim_start_matches('v').split('.');
        let t = (
            it.next()?.parse().ok()?,
            it.next()?.parse().ok()?,
            it.next()?.split(['-', '+']).next()?.parse().ok()?,
        );
        Some(t)
    }
    match (installed.and_then(parse), parse(current)) {
        (Some(i), Some(c)) => i < c,
        (None, Some(_)) => true,
        _ => false,
    }
}

/// The release asset holding the server executable for this OS and CPU,
/// named as release.yml publishes it.
pub fn server_asset(os: &str, arch: &str) -> Option<String> {
    let os = match os {
        "macos" => "darwin",
        "linux" => "linux",
        "windows" => "windows",
        _ => return None,
    };
    let arch = match arch {
        "aarch64" => "arm64",
        "x86_64" => "amd64",
        _ => return None,
    };
    if os == "windows" && arch == "arm64" {
        return None;
    }
    let ext = if os == "windows" { ".exe" } else { "" };
    Some(format!("deciduous-mcp-{os}-{arch}{ext}"))
}

pub const LAUNCHD_LABEL: &str = "dev.deciduous.server";
pub const SYSTEMD_UNIT: &str = "deciduous-server.service";

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Where the service finds things. The settings file carries the secrets;
/// the service definition carries only paths.
pub struct ServicePaths<'a> {
    pub binary: &'a Path,
    pub env_file: &'a Path,
    /// Where the executable unpacks its embedded runtime (Burrito's
    /// DECIDUOUS_MCP_INSTALL_DIR), kept next to the settings.
    pub runtime_dir: &'a Path,
    pub home: &'a Path,
    pub log: &'a Path,
}

/// A LaunchAgent: started at login and on load, restarted whenever it exits
/// (KeepAlive), at most every 10 seconds, with output in the log file.
pub fn launchd_plist(p: &ServicePaths) -> String {
    let s = |path: &Path| xml_escape(&path.display().to_string());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{LAUNCHD_LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{bin}</string>
  </array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>DECIDUOUS_ENV_FILE</key>
    <string>{env}</string>
    <key>DECIDUOUS_MCP_INSTALL_DIR</key>
    <string>{rt}</string>
  </dict>
  <key>WorkingDirectory</key>
  <string>{home}</string>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>ThrottleInterval</key>
  <integer>10</integer>
  <key>StandardOutPath</key>
  <string>{log}</string>
  <key>StandardErrorPath</key>
  <string>{log}</string>
</dict>
</plist>
"#,
        bin = s(p.binary),
        env = s(p.env_file),
        rt = s(p.runtime_dir),
        home = s(p.home),
        log = s(p.log),
    )
}

/// A systemd user unit. Paths are quoted, so a home directory with a space
/// in it still works.
pub fn systemd_unit(p: &ServicePaths) -> String {
    let q = |path: &Path| {
        format!(
            "\"{}\"",
            path.display()
                .to_string()
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
        )
    };
    let env = |k: &str, path: &Path| {
        format!(
            "\"{k}={}\"",
            path.display()
                .to_string()
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
        )
    };
    format!(
        "[Unit]\n\
         Description=deciduous graph server\n\
         After=network.target\n\
         \n\
         [Service]\n\
         Environment={envfile}\n\
         Environment={rt}\n\
         WorkingDirectory={home}\n\
         ExecStart={bin}\n\
         Restart=always\n\
         RestartSec=5\n\
         StandardOutput=append:{log}\n\
         StandardError=append:{log}\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        envfile = env("DECIDUOUS_ENV_FILE", p.env_file),
        rt = env("DECIDUOUS_MCP_INSTALL_DIR", p.runtime_dir),
        home = q(p.home),
        bin = q(p.binary),
        // systemd takes append: paths verbatim, up to the end of the line.
        log = p.log.display(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn the_default_database_is_this_users_postgres_on_5432_with_no_password() {
        let db = DbUrl::system_default("bg");
        assert_eq!(db.ecto_url(), "ecto://bg@localhost:5432/deciduous");
        assert_eq!(db.redacted(), "postgres://bg@localhost:5432/deciduous");
        assert!(db.is_loopback());
    }

    #[test]
    fn postgres_postgresql_and_ecto_urls_all_normalise_to_ecto() {
        for u in [
            "postgres://ann:pw@db.internal:6543/graphs",
            "postgresql://ann:pw@db.internal:6543/graphs",
            "ecto://ann:pw@db.internal:6543/graphs",
            "  postgres://ann:pw@db.internal:6543/graphs\n",
        ] {
            assert_eq!(
                DbUrl::parse(u).unwrap().ecto_url(),
                "ecto://ann:pw@db.internal:6543/graphs",
                "{u}"
            );
        }
        assert_eq!(
            DbUrl::parse("postgres://ann@h/g").unwrap().ecto_url(),
            "ecto://ann@h:5432/g"
        );
        assert_eq!(
            DbUrl::parse("postgres://ann@[::1]:5433/g")
                .unwrap()
                .ecto_url(),
            "ecto://ann@[::1]:5433/g"
        );
    }

    #[test]
    fn the_password_is_never_shown_even_through_debug() {
        let db = DbUrl::parse("postgres://ann:s3cr%40t@db:5432/g?sslmode=require").unwrap();
        assert_eq!(
            db.redacted(),
            "postgres://ann:***@db:5432/g?sslmode=require"
        );
        assert!(!format!("{db:?}").contains("s3cr"));
        assert!(db.ecto_url().contains("s3cr%40t"), "the server does get it");
    }

    #[test]
    fn a_typed_password_is_encoded_into_the_url_and_still_not_shown() {
        let mut db = DbUrl::parse("postgres://ann@db:5432/g").unwrap();
        assert!(!db.has_password());
        db.set_password("p@ss:w/rd's 1");
        assert_eq!(
            db.ecto_url(),
            "ecto://ann:p%40ss%3Aw%2Frd%27s%201@db:5432/g"
        );
        assert_eq!(db.redacted(), "postgres://ann:***@db:5432/g");
        // What the wizard stores round-trips through parse unchanged.
        assert_eq!(DbUrl::parse(&db.ecto_url()).unwrap(), db);
    }

    #[test]
    fn sslmode_becomes_the_servers_tls_settings() {
        let ssl = |q: &str| {
            DbUrl::parse(&format!("postgres://a@h/d{q}"))
                .unwrap()
                .ssl_settings()
        };
        assert_eq!(ssl(""), ("false", "full"));
        assert_eq!(ssl("?sslmode=disable"), ("false", "full"));
        assert_eq!(ssl("?sslmode=require"), ("true", "none"));
        assert_eq!(ssl("?sslmode=verify-ca"), ("true", "ca"));
        assert_eq!(ssl("?sslmode=verify-full"), ("true", "full"));
        assert!(DbUrl::parse("postgres://a@h/d?sslmode=bogus").is_err());
    }

    #[test]
    fn urls_the_server_could_not_use_are_refused_with_a_reason() {
        for (u, why) in [
            ("mysql://a@h/d", "starts with postgres://"),
            ("postgres://a@h", "no database"),
            ("postgres://a@h/", "not a database name"),
            ("postgres://a@h:port/d", "not a port"),
            ("postgres://a@h:0/d", "not a port"),
            ("postgres://h/d", "no user"),
            ("postgres://a@/d", "no host"),
            ("postgres://a:it's@h/d", "quotes"),
            ("postgres://a@h/d?connect_timeout=5", "only sslmode"),
        ] {
            let e = DbUrl::parse(u).unwrap_err();
            assert!(e.contains(why), "{u}: {e}");
        }
    }

    #[test]
    fn inside_a_container_localhost_becomes_host_docker_internal() {
        let db = DbUrl::parse("postgres://bg:pw@127.0.0.1:5432/deciduous").unwrap();
        let (mac, note) = db.for_container("macos");
        assert_eq!(
            mac.ecto_url(),
            "ecto://bg:pw@host.docker.internal:5432/deciduous"
        );
        assert!(note.unwrap().contains("Docker Desktop"));
        let (_, note) = db.for_container("linux");
        let note = note.unwrap();
        assert!(note.contains("host gateway") && note.contains("pg_hba.conf"));
        assert!(!note.contains("pw"));

        let remote = DbUrl::parse("postgres://a@db.example.com/d").unwrap();
        assert_eq!(remote.for_container("linux"), (remote.clone(), None));
    }

    #[test]
    fn with_no_flags_the_plan_is_the_native_server_on_system_postgres() {
        assert_eq!(
            resolve(&LocalRequest::default()),
            Plan {
                server: ServerMode::Native,
                database: Database::System
            }
        );
    }

    #[test]
    fn flags_choose_the_database_and_how_the_server_runs() {
        let url = Database::Url("postgres://a@h/d".into());
        let plan = |database: Option<Database>, docker| {
            resolve(&LocalRequest {
                port: None,
                database,
                docker,
            })
        };
        assert_eq!(
            plan(Some(url.clone()), false),
            Plan {
                server: ServerMode::Native,
                database: url.clone()
            }
        );
        assert_eq!(
            plan(Some(url.clone()), true),
            Plan {
                server: ServerMode::Docker,
                database: url
            }
        );
        assert_eq!(plan(None, true).database, Database::System);
        // A PostgreSQL container is only reachable from the container network.
        assert_eq!(
            plan(Some(Database::DockerPostgres), false).server,
            ServerMode::Docker
        );
    }

    #[test]
    fn a_settings_file_from_before_native_servers_is_a_docker_install() {
        let old = "COMPOSE_PROJECT_NAME=deciduous\nDECIDUOUS_DATABASE_MODE=local\n\
                   DECIDUOUS_BIND_ADDRESS=127.0.0.1\nDECIDUOUS_PORT=24555\n\
                   DECIDUOUS_MCP_TOKEN='abc'\nPOSTGRES_PASSWORD='p'\n\
                   DATABASE_URL='ecto://deciduous:p@db:5432/deciduous'\nDB_SSL=false\n";
        let s = Settings::parse(old).unwrap();
        assert_eq!(s.mode, ServerMode::Docker);
        assert_eq!(s.port, Some(24555));
        assert_eq!(s.token.as_deref(), Some("abc"));
        assert_eq!(s.version, None);
        let external = old.replace("=local", "=external");
        assert_eq!(Settings::parse(&external).unwrap().mode, ServerMode::Docker);
        assert!(Settings::parse("DECIDUOUS_SERVER_MODE=podman\n").is_err());
    }

    #[test]
    fn a_native_settings_file_says_everything_the_server_reads() {
        let db = DbUrl::parse("postgres://bg:pw@localhost:5432/deciduous?sslmode=require").unwrap();
        let text = native_settings("1.0.11", 24123, "t0ken", &db);
        let s = Settings::parse(&text).unwrap();
        assert_eq!(s.mode, ServerMode::Native);
        assert_eq!(s.port, Some(24123));
        assert_eq!(s.version.as_deref(), Some("1.0.11"));
        for line in [
            "DECIDUOUS_BIND_ADDRESS=127.0.0.1",
            "DATABASE_URL='ecto://bg:pw@localhost:5432/deciduous'",
            "DB_SSL=true",
            "DB_SSL_VERIFY=none",
            "DECIDUOUS_PREPARE_DATABASE=true",
            "DECIDUOUS_MCP_TOKEN='t0ken'",
        ] {
            assert!(text.lines().any(|l| l == line), "missing {line}:\n{text}");
        }
    }

    #[test]
    fn a_value_is_replaced_in_place_or_appended() {
        let t = "A=1\nDECIDUOUS_SERVER_VERSION=1.0.10\nB=2\n";
        assert_eq!(
            with_value(t, "DECIDUOUS_SERVER_VERSION", "1.0.11"),
            "A=1\nDECIDUOUS_SERVER_VERSION=1.0.11\nB=2\n"
        );
        assert_eq!(with_value("A=1\n", "B", "2"), "A=1\nB=2\n");
    }

    #[test]
    fn an_older_server_is_one_with_a_lower_version_or_none_recorded() {
        assert!(older_than(Some("1.0.10"), "1.0.11"));
        assert!(older_than(Some("0.9.99"), "1.0.0"));
        assert!(!older_than(Some("1.0.11"), "1.0.11"));
        assert!(!older_than(Some("1.1.0"), "1.0.11"));
        assert!(older_than(None, "1.0.11"));
        assert!(older_than(Some("garbage"), "1.0.11"));
    }

    #[test]
    fn the_asset_name_matches_what_the_release_publishes() {
        assert_eq!(
            server_asset("macos", "aarch64").as_deref(),
            Some("deciduous-mcp-darwin-arm64")
        );
        assert_eq!(
            server_asset("linux", "x86_64").as_deref(),
            Some("deciduous-mcp-linux-amd64")
        );
        assert_eq!(
            server_asset("windows", "x86_64").as_deref(),
            Some("deciduous-mcp-windows-amd64.exe")
        );
        assert_eq!(server_asset("freebsd", "x86_64"), None);
        assert_eq!(server_asset("linux", "riscv64"), None);
        // Every name here is one release.yml uploads and checksums.
        let workflow = include_str!("../.github/workflows/release.yml");
        for (os, arch) in [
            ("macos", "aarch64"),
            ("macos", "x86_64"),
            ("linux", "aarch64"),
            ("linux", "x86_64"),
            ("windows", "x86_64"),
        ] {
            let name = server_asset(os, arch).unwrap();
            assert!(
                workflow.contains(&format!("artifacts/{0}/{0}", name.trim_end_matches(".exe")))
                    || workflow.contains(&format!("/{name}")),
                "release.yml does not publish {name}"
            );
        }
    }

    fn paths(root: &str) -> (PathBuf, PathBuf, PathBuf, PathBuf, PathBuf) {
        let h = PathBuf::from(root);
        (
            h.join("bin/deciduous-mcp"),
            h.join(".env"),
            h.join("runtime"),
            h.clone(),
            h.join("logs/server.log"),
        )
    }

    #[test]
    fn the_launch_agent_runs_the_binary_with_only_paths_and_keeps_it_alive() {
        let (bin, env, rt, home, log) = paths("/Users/a & b/.config/deciduous/server");
        let plist = launchd_plist(&ServicePaths {
            binary: &bin,
            env_file: &env,
            runtime_dir: &rt,
            home: &home,
            log: &log,
        });
        assert!(plist.contains("<string>dev.deciduous.server</string>"));
        assert!(plist.contains(
            "<string>/Users/a &amp; b/.config/deciduous/server/bin/deciduous-mcp</string>"
        ));
        assert!(plist.contains("<key>DECIDUOUS_ENV_FILE</key>"));
        assert!(plist.contains("<key>KeepAlive</key>\n  <true/>"));
        assert!(plist.contains("<key>RunAtLoad</key>\n  <true/>"));
        assert!(plist.contains("logs/server.log</string>"));
        assert!(!plist.contains("DATABASE_URL") && !plist.contains("TOKEN"));
    }

    #[test]
    fn the_systemd_unit_quotes_paths_and_restarts_the_server() {
        let (bin, env, rt, home, log) = paths("/home/a b/.config/deciduous/server");
        let unit = systemd_unit(&ServicePaths {
            binary: &bin,
            env_file: &env,
            runtime_dir: &rt,
            home: &home,
            log: &log,
        });
        assert!(unit.contains("ExecStart=\"/home/a b/.config/deciduous/server/bin/deciduous-mcp\""));
        assert!(unit.contains(
            "Environment=\"DECIDUOUS_ENV_FILE=/home/a b/.config/deciduous/server/.env\""
        ));
        assert!(unit.contains("Restart=always"));
        assert!(unit.contains("WantedBy=default.target"));
        assert!(!unit.contains("DATABASE_URL") && !unit.contains("TOKEN"));
    }
}
