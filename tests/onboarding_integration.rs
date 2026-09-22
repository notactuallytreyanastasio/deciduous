//! First-use and upgrade guidance should recommend shared memory without
//! provisioning infrastructure or changing the user's storage configuration.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

const SETUP_HEADING: &str = "DECIDUOUS 1.0: SET UP SHARED POSTGRES MEMORY";
const SETUP_URL: &str = "https://deciduous.dev/tutorial/local-postgres.html";

fn command(project: &Path, config_home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_deciduous"));
    command
        .current_dir(project)
        .env("DECIDUOUS_DB_PATH", project.join(".deciduous/deciduous.db"))
        .env("XDG_CONFIG_HOME", config_home)
        .env_remove("DECIDUOUS_MCP_TOKEN")
        .env("NO_COLOR", "1");
    command
}

fn run(project: &Path, config_home: &Path, args: &[&str]) -> Output {
    command(project, config_home)
        .args(args)
        .output()
        .expect("run deciduous in an isolated project")
}

fn successful_stdout(output: &Output) -> String {
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone()).expect("CLI output is UTF-8")
}

fn assert_setup_guidance(output: &str) {
    assert_eq!(output.matches(SETUP_HEADING).count(), 1);
    for text in [
        SETUP_URL,
        "======================================================================",
        "UPGRADING AN EXISTING INSTALL?",
        "https://deciduous.dev/tutorial/upgrading.html",
        "Before launching a team of agents",
        "From a cloned Deciduous source repository",
        "Python 3 and Docker Compose",
        "python3 scripts/team-memory/local-stack.py configure --apply",
        "python3 scripts/team-memory/local-stack.py up --apply",
        "deciduous setup --postgres --output ./deciduous-postgres --port 55432",
        "That command creates files only",
        "For an existing shared server",
        "deciduous remote init <server-base-url> --workspace <project-name>",
        "shared HTTP MCP endpoint",
        "remote init does not redirect plain CLI writes",
        "solo/offline",
        "No Postgres server was started or remote credentials configured",
    ] {
        assert!(output.contains(text), "missing onboarding guidance: {text}");
    }
}

fn assert_no_remote_setup(project: &Path, config_home: &Path) {
    let config = fs::read_to_string(project.join(".deciduous/config.toml"))
        .expect("local configuration exists");
    let parsed: toml::Value = toml::from_str(&config).expect("valid local configuration");
    assert!(parsed.get("remote").is_none(), "remote config was created");
    assert!(!config_home.join("deciduous/credentials").exists());
    assert!(!config_home.join("deciduous/team-memory/local.env").exists());
    assert!(!project.join(".env").exists());
    assert!(!project.join(".mcp.json").exists());
}

#[test]
fn setup_prints_to_stdout_outside_a_project_without_creating_files() {
    let project = TempDir::new().expect("empty directory");
    let config_home = TempDir::new().expect("isolated config directory");

    for _ in 0..2 {
        let output = run(project.path(), config_home.path(), &["setup"]);
        assert_setup_guidance(&successful_stdout(&output));
        assert!(output.stderr.is_empty(), "setup guide belongs on stdout");
        assert_eq!(fs::read_dir(project.path()).unwrap().count(), 0);
        assert_eq!(fs::read_dir(config_home.path()).unwrap().count(), 0);
    }
}

#[test]
fn init_recommends_postgres_and_repeated_init_preserves_solo_data() {
    let project = TempDir::new().expect("project directory");
    let config_home = TempDir::new().expect("isolated config directory");

    let first = successful_stdout(&run(
        project.path(),
        config_home.path(),
        &["init", "--claude"],
    ));
    assert_setup_guidance(&first);
    assert!(first.find(SETUP_HEADING).unwrap() < first.find("Local tools").unwrap());
    assert_no_remote_setup(project.path(), config_home.path());

    successful_stdout(&run(
        project.path(),
        config_home.path(),
        &["add", "goal", "Preserve this solo goal"],
    ));
    let config_path = project.path().join(".deciduous/config.toml");
    let original_config = fs::read(&config_path).expect("read configuration");

    let second = successful_stdout(&run(
        project.path(),
        config_home.path(),
        &["init", "--claude"],
    ));
    assert_setup_guidance(&second);
    assert_eq!(fs::read(&config_path).unwrap(), original_config);
    assert_no_remote_setup(project.path(), config_home.path());
    let nodes = successful_stdout(&run(project.path(), config_home.path(), &["nodes"]));
    assert!(nodes.contains("Preserve this solo goal"));
}

#[test]
fn update_repeats_guidance_without_configuring_a_remote() {
    let project = TempDir::new().expect("project directory");
    let config_home = TempDir::new().expect("isolated config directory");
    successful_stdout(&run(
        project.path(),
        config_home.path(),
        &["init", "--claude"],
    ));
    let config_path = project.path().join(".deciduous/config.toml");
    let original_config = fs::read(&config_path).expect("read configuration");

    for _ in 0..2 {
        let output = successful_stdout(&run(project.path(), config_home.path(), &["update"]));
        assert_setup_guidance(&output);
        assert_eq!(fs::read(&config_path).unwrap(), original_config);
        assert_no_remote_setup(project.path(), config_home.path());
    }
}

#[test]
fn update_keeps_existing_remote_config_and_credentials_untouched() {
    let project = TempDir::new().expect("project directory");
    let config_home = TempDir::new().expect("isolated config directory");
    successful_stdout(&run(
        project.path(),
        config_home.path(),
        &["init", "--claude"],
    ));

    let config_path = project.path().join(".deciduous/config.toml");
    let mut configured = fs::read_to_string(&config_path).unwrap();
    configured.push_str(
        "\n# Existing team server; update must not replace this.\n\
         [remote]\nurl = \"https://memory.example.invalid\"\nworkspace = \"existing-team\"\n",
    );
    fs::write(&config_path, &configured).unwrap();
    let credential_path = config_home.path().join("deciduous/credentials");
    fs::create_dir_all(credential_path.parent().unwrap()).unwrap();
    fs::write(
        &credential_path,
        "synthetic-test-credential-do-not-connect\n",
    )
    .unwrap();

    let output = successful_stdout(&run(project.path(), config_home.path(), &["update"]));
    assert_setup_guidance(&output);
    assert_eq!(fs::read_to_string(&config_path).unwrap(), configured);
    assert_eq!(
        fs::read_to_string(&credential_path).unwrap(),
        "synthetic-test-credential-do-not-connect\n"
    );
    assert!(!output.contains("synthetic-test-credential"));
    assert!(!config_home
        .path()
        .join("deciduous/team-memory/local.env")
        .exists());
}

#[test]
fn graph_json_has_no_onboarding_banner() {
    let project = TempDir::new().expect("project directory");
    let config_home = TempDir::new().expect("isolated config directory");
    successful_stdout(&run(
        project.path(),
        config_home.path(),
        &["init", "--claude"],
    ));

    let output = successful_stdout(&run(project.path(), config_home.path(), &["graph"]));
    let graph: serde_json::Value = serde_json::from_str(&output).expect("graph remains plain JSON");
    assert!(graph["nodes"].is_array());
    assert!(!output.contains(SETUP_HEADING));
}

#[test]
fn stdio_mcp_has_no_onboarding_banner() {
    let project = TempDir::new().expect("project directory");
    let config_home = TempDir::new().expect("isolated config directory");
    let mut child = command(project.path(), config_home.path())
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start local MCP server");
    let message = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"onboarding-test","version":"1.0"}}}"#;
    {
        let mut stdin = child.stdin.take().expect("MCP stdin");
        writeln!(stdin, "{message}").expect("write initialize request");
    }
    let output = successful_stdout(&child.wait_with_output().expect("finish MCP request"));
    let response: serde_json::Value =
        serde_json::from_str(output.trim()).expect("MCP stdout remains a JSON-RPC response");
    assert_eq!(response["id"], 1);
    assert!(response["result"]["capabilities"].is_object());
    assert!(!output.contains(SETUP_HEADING));
}

fn env_value(contents: &str, key: &str) -> String {
    contents
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("missing generated .env key {key}"))
        .to_string()
}

#[test]
fn postgres_scaffold_has_only_the_database_templates_and_private_env() {
    let project = TempDir::new().unwrap();
    let config_home = TempDir::new().unwrap();
    let output = run(project.path(), config_home.path(), &["setup", "--postgres"]);
    let stdout = successful_stdout(&output);
    let directory = project.path().join("deciduous-postgres");
    let mut names: Vec<_> = fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            ".env",
            ".gitignore",
            "Dockerfile",
            "README.md",
            "compose.yaml"
        ]
    );

    for (name, template) in [
        (
            "Dockerfile",
            include_str!("../scripts/team-memory/postgres-only/Dockerfile"),
        ),
        (
            "compose.yaml",
            include_str!("../scripts/team-memory/postgres-only/compose.yaml"),
        ),
        (
            "README.md",
            include_str!("../scripts/team-memory/postgres-only/README.md"),
        ),
        (
            ".gitignore",
            include_str!("../scripts/team-memory/postgres-only/.gitignore"),
        ),
    ] {
        assert_eq!(fs::read_to_string(directory.join(name)).unwrap(), template);
    }
    let compose: serde_yaml::Value =
        serde_yaml::from_str(&fs::read_to_string(directory.join("compose.yaml")).unwrap()).unwrap();
    let services = compose["services"].as_mapping().unwrap();
    assert_eq!(services.len(), 1, "scaffold must contain only Postgres");
    assert!(services.contains_key(serde_yaml::Value::String("db".into())));
    assert_eq!(
        compose["services"]["db"]["ports"][0].as_str(),
        Some("127.0.0.1:${POSTGRES_PORT:-55432}:5432")
    );
    assert!(compose["volumes"]["postgres-data"].is_null());
    let env = fs::read_to_string(directory.join(".env")).unwrap();
    let password = env_value(&env, "POSTGRES_PASSWORD");
    assert_eq!(env_value(&env, "POSTGRES_PORT"), "55432");
    assert_eq!(env_value(&env, "POSTGRES_DB"), "deciduous");
    assert_eq!(env_value(&env, "POSTGRES_USER"), "deciduous");
    assert!(password.len() == 64 && password.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert!(
        !stdout.contains(&password),
        "generated secret must never reach stdout"
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains(&password));
    assert!(stdout.contains("nothing has been started"));
    assert!(stdout.contains("up -d --build --wait"));
    assert!(stdout.contains("configure the Deciduous MCP service to connect separately"));
    assert!(!project.path().join(".deciduous").exists());
    assert_eq!(fs::read_dir(config_home.path()).unwrap().count(), 0);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(directory.join(".env"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn postgres_scaffold_uses_requested_port_and_unique_passwords() {
    let project = TempDir::new().unwrap();
    let config_home = TempDir::new().unwrap();
    let mut passwords = Vec::new();
    for directory in ["first database", "second database"] {
        let output = command(project.path(), config_home.path())
            .env("POSTGRES_PORT", "65432")
            .args([
                "setup",
                "--postgres",
                "--output",
                directory,
                "--port",
                "55433",
            ])
            .output()
            .unwrap();
        let stdout = successful_stdout(&output);
        let env = fs::read_to_string(project.path().join(directory).join(".env")).unwrap();
        assert_eq!(env_value(&env, "POSTGRES_PORT"), "55433");
        let password = env_value(&env, "POSTGRES_PASSWORD");
        assert!(!stdout.contains(&password));
        passwords.push(password);
    }
    assert!(
        passwords[0] != passwords[1],
        "scaffolds must get independent credentials"
    );
    assert!(!project.path().join(".env").exists());
}

#[test]
fn postgres_scaffold_refuses_existing_files_and_directories_without_changes() {
    let project = TempDir::new().unwrap();
    let config_home = TempDir::new().unwrap();
    successful_stdout(&run(
        project.path(),
        config_home.path(),
        &["setup", "--postgres"],
    ));
    let directory = project.path().join("deciduous-postgres");
    let original_env = fs::read(directory.join(".env")).unwrap();
    let retry = run(project.path(), config_home.path(), &["setup", "--postgres"]);
    assert!(!retry.status.success());
    assert!(String::from_utf8_lossy(&retry.stderr).contains("refusing existing output path"));
    assert!(fs::read(directory.join(".env")).unwrap() == original_env);
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 5);

    fs::write(project.path().join("existing-file"), "keep this file").unwrap();
    let file = run(
        project.path(),
        config_home.path(),
        &["setup", "--postgres", "--output", "existing-file"],
    );
    assert!(!file.status.success());
    assert_eq!(
        fs::read_to_string(project.path().join("existing-file")).unwrap(),
        "keep this file"
    );
}

#[test]
fn postgres_scaffold_validates_flags_and_port_before_writing() {
    let project = TempDir::new().unwrap();
    let config_home = TempDir::new().unwrap();
    for args in [
        vec!["setup", "--port", "55432"],
        vec!["setup", "--output", "must-not-exist"],
        vec!["setup", "--postgres", "--port", "0"],
        vec!["setup", "--postgres", "--port", "80"],
        vec!["setup", "--postgres", "--port", "1023"],
        vec!["setup", "--postgres", "--port", "65536"],
        vec!["setup", "--postgres", "--port", "not-a-port"],
        vec!["setup", "--postgres", "--output", "missing-parent/child"],
    ] {
        let output = run(project.path(), config_home.path(), &args);
        assert!(!output.status.success(), "arguments should fail: {args:?}");
        assert_eq!(fs::read_dir(project.path()).unwrap().count(), 0);
        assert_eq!(fs::read_dir(config_home.path()).unwrap().count(), 0);
    }
}

#[cfg(unix)]
#[test]
fn postgres_scaffold_refuses_symlink_outputs_and_parents() {
    use std::os::unix::fs::symlink;
    let project = TempDir::new().unwrap();
    let config_home = TempDir::new().unwrap();
    let existing = project.path().join("existing");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("keep.txt"), "untouched").unwrap();
    symlink(&existing, project.path().join("linked-output")).unwrap();
    symlink(
        project.path().join("absent"),
        project.path().join("dangling-output"),
    )
    .unwrap();
    symlink(&existing, project.path().join("linked-parent")).unwrap();

    for target in [
        "linked-output",
        "dangling-output",
        "linked-parent/new-postgres",
    ] {
        let output = run(
            project.path(),
            config_home.path(),
            &["setup", "--postgres", "--output", target],
        );
        assert!(
            !output.status.success(),
            "must refuse symlink path {target}"
        );
    }
    assert_eq!(fs::read_dir(&existing).unwrap().count(), 1);
    assert_eq!(
        fs::read_to_string(existing.join("keep.txt")).unwrap(),
        "untouched"
    );
    assert!(!project.path().join("absent").exists());
}
