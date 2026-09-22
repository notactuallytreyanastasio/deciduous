//! Explicit scaffolding for a standalone PostgreSQL Docker project.
//!
//! Templates are shared with the Python fallback for older installed CLIs.
//! Generating files does not start Docker, initialize a local graph, or change
//! any existing Deciduous configuration.

use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub const DEFAULT_POSTGRES_OUTPUT: &str = "deciduous-postgres";
pub const DEFAULT_POSTGRES_PORT: u16 = 55432;

const DOCKERFILE: &str = include_str!("../scripts/team-memory/postgres-only/Dockerfile");
const COMPOSE: &str = include_str!("../scripts/team-memory/postgres-only/compose.yaml");
const README: &str = include_str!("../scripts/team-memory/postgres-only/README.md");
const GITIGNORE: &str = include_str!("../scripts/team-memory/postgres-only/.gitignore");

/// Generate a new Postgres-only project without touching existing paths.
///
/// The output's parent must already exist. Existing output paths, including
/// dangling symlinks, are refused. On Unix the new directory is mode 0700 and
/// the generated credential file is created as mode 0600 before writing bytes.
pub fn scaffold_postgres(output: &Path, port: u16) -> Result<PathBuf, String> {
    scaffold_with_writer(output, port, |file, bytes| file.write_all(bytes))
}

fn scaffold_with_writer(
    output: &Path,
    port: u16,
    mut write: impl FnMut(&mut File, &[u8]) -> io::Result<()>,
) -> Result<PathBuf, String> {
    if port < 1024 {
        return Err("Postgres host port must be between 1024 and 65535".to_string());
    }

    let output = checked_output(output)?;
    let directory = DirBuilder::new();
    #[cfg(unix)]
    let directory = {
        use std::os::unix::fs::DirBuilderExt;
        let mut directory = directory;
        directory.mode(0o700);
        directory
    };
    directory
        .create(&output)
        .map_err(|error| format!("cannot create new directory {}: {error}", output.display()))?;

    // Uuid v4 uses the existing OS-backed random source. Two UUIDs provide
    // 244 random bits after their fixed version/variant bits, encoded as hex.
    let password = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let env = format!(
        "POSTGRES_DB=deciduous\nPOSTGRES_USER=deciduous\nPOSTGRES_PASSWORD={password}\nPOSTGRES_PORT={port}\n"
    );
    let files = [
        ("Dockerfile", DOCKERFILE),
        ("compose.yaml", COMPOSE),
        ("README.md", README),
        (".gitignore", GITIGNORE),
        (".env", env.as_str()),
    ];
    let mut created = Vec::new();

    let result = (|| -> io::Result<()> {
        for (name, contents) in files {
            let path = output.join(name);
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }

            let mut file = options.open(&path)?;
            // Track a file as soon as this process creates it, so a short or
            // failed write is cleaned up too. Never delete unowned entries.
            created.push(path);
            write(&mut file, contents.as_bytes())?;
        }
        Ok(())
    })();

    if let Err(error) = result {
        let mut cleanup_failed = false;
        for path in created.iter().rev() {
            cleanup_failed |= fs::remove_file(path).is_err();
        }
        cleanup_failed |= fs::remove_dir(&output).is_err();
        let detail = if cleanup_failed {
            " Cleanup was incomplete; inspect the output directory before retrying."
        } else {
            " Removed the incomplete files and directory created by this command."
        };
        return Err(format!(
            "could not write Postgres scaffold: {error}.{detail}"
        ));
    }

    Ok(output)
}

fn checked_output(output: &Path) -> Result<PathBuf, String> {
    if output.as_os_str().is_empty() || output.file_name().is_none() {
        return Err("output must name a new directory, not the current directory or a root".into());
    }
    match fs::symlink_metadata(output) {
        Ok(_) => {
            return Err(format!(
                "refusing existing output path {}; choose a new directory",
                output.display()
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("cannot inspect {}: {error}", output.display())),
    }

    let parent = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    // Resolve the parent once and return an absolute path. Refuse a symlink
    // parent rather than silently creating files in a different directory.
    let metadata = fs::symlink_metadata(parent).map_err(|error| {
        format!(
            "output parent {} must already exist: {error}",
            parent.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "output parent {} must be a directory, not a file or symlink",
            parent.display()
        ));
    }
    let parent = fs::canonicalize(parent)
        .map_err(|error| format!("cannot resolve output parent: {error}"))?;
    Ok(parent.join(output.file_name().expect("checked file name")))
}

/// Print usable next commands without displaying the generated password.
pub fn print_postgres_next_steps(directory: &Path) {
    println!("\nPOSTGRES FILES GENERATED (nothing has been started)");
    println!("  Directory: {}", directory.display());
    println!("  Dockerfile, compose.yaml, README.md, .gitignore, and private .env");
    println!("  The generated password is in .env; keep that file out of Git and logs.");
    println!("\nStart PostgreSQL when ready:");
    println!("  cd {}", shell_quote(&directory.to_string_lossy()));
    println!("  docker compose --env-file .env up -d --build --wait");
    println!("  docker compose --env-file .env ps");
    println!("\nPostgres only: configure the Deciduous MCP service to connect separately.");
    println!("  Follow README.md for connection settings, backups, and shutdown.");
    println!("  https://deciduous.dev/tutorial/local-postgres.html");
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn failed_write_removes_only_the_new_scaffold() {
        let parent = TempDir::new().unwrap();
        let output = parent.path().join("new-postgres");
        let sentinel = parent.path().join("keep.txt");
        fs::write(&sentinel, "unrelated data").unwrap();
        for fail_after in [1, 3, 5] {
            let mut writes = 0;
            let result = scaffold_with_writer(&output, 55432, |file, contents| {
                writes += 1;
                file.write_all(contents)?;
                if writes == fail_after {
                    Err(io::Error::other("injected write failure"))
                } else {
                    Ok(())
                }
            });

            assert!(result.is_err());
            assert!(!output.exists());
            assert_eq!(fs::read_to_string(&sentinel).unwrap(), "unrelated data");
            assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 1);
        }
    }

    #[test]
    fn privileged_ports_are_rejected_without_creating_a_directory() {
        let parent = TempDir::new().unwrap();
        let output = parent.path().join("new-postgres");
        for port in [0, 80, 1023] {
            assert!(scaffold_postgres(&output, port).is_err());
            assert!(!output.exists());
        }
    }

    #[test]
    fn shell_paths_are_quoted() {
        assert_eq!(
            shell_quote("/a space/it's postgres"),
            "'/a space/it'\\''s postgres'"
        );
    }
}
