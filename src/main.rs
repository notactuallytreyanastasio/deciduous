//! Deciduous - Decision Graph Tooling
//!
//! This is a thin Rust wrapper that extracts and delegates to the Elixir CLI.
//! The Elixir binary is embedded at build time via include_bytes!().

use std::env;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, exit};

const VERSION: &str = env!("CARGO_PKG_VERSION");

// Platform-specific embedded binary
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const EMBEDDED_BINARY: &[u8] = include_bytes!("../burrito_out/deciduex_darwin_arm64");

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const EMBEDDED_BINARY: &[u8] = include_bytes!("../burrito_out/deciduex_darwin_amd64");

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const EMBEDDED_BINARY: &[u8] = include_bytes!("../burrito_out/deciduex_linux_amd64");

#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
)))]
const EMBEDDED_BINARY: &[u8] = &[];

fn get_cache_dir() -> PathBuf {
    let base = env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = env::var("HOME").expect("HOME not set");
            PathBuf::from(home).join(".cache")
        });
    base.join("deciduous")
}

fn get_binary_path() -> PathBuf {
    get_cache_dir().join(format!("deciduex-{}", VERSION))
}

fn ensure_binary() -> PathBuf {
    let binary_path = get_binary_path();
    
    if binary_path.exists() {
        return binary_path;
    }
    
    if EMBEDDED_BINARY.is_empty() {
        eprintln!("Error: No embedded binary for this platform.");
        eprintln!("Supported: macOS (arm64, x86_64), Linux (x86_64)");
        exit(1);
    }
    
    // Create cache directory
    let cache_dir = get_cache_dir();
    fs::create_dir_all(&cache_dir).expect("Failed to create cache directory");
    
    // Extract binary
    let mut file = fs::File::create(&binary_path).expect("Failed to create binary file");
    file.write_all(EMBEDDED_BINARY).expect("Failed to write binary");
    
    // Make executable
    let mut perms = file.metadata().expect("Failed to get metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&binary_path, perms).expect("Failed to set permissions");
    
    eprintln!("Extracted deciduex v{} to {}", VERSION, binary_path.display());
    
    binary_path
}

fn main() {
    let binary = ensure_binary();
    let args: Vec<String> = env::args().skip(1).collect();
    
    let status = Command::new(&binary)
        .args(&args)
        .status()
        .expect("Failed to execute deciduex");
    
    exit(status.code().unwrap_or(1));
}
