//! Embedded trace interceptor for Node.js
//!
//! The trace interceptor is a Node.js module that intercepts fetch() calls
//! to the Anthropic API and records them to deciduous via the CLI.
//!
//! This module embeds the compiled JavaScript bundle and extracts it to
//! `~/.deciduous/trace-interceptor/` on first use.

use std::path::PathBuf;

/// Embedded trace interceptor JavaScript bundle
/// Built from trace-interceptor/src/ using esbuild
const INTERCEPTOR_JS: &str = include_str!("../trace-interceptor/dist/bundle.js");

/// Version marker to detect when the embedded JS changes
/// Format: Hash of ENTIRE bundle content (not just first N bytes!)
fn bundle_version() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    // Hash the ENTIRE bundle - changes anywhere will trigger re-extraction
    INTERCEPTOR_JS.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Ensure the trace interceptor is installed and return its path
///
/// The interceptor is extracted to `~/.deciduous/trace-interceptor/interceptor.js`
/// and only re-extracted if the embedded bundle has changed.
pub fn ensure_interceptor_installed() -> std::io::Result<PathBuf> {
    let home = std::env::var("HOME").map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "HOME environment variable not set",
        )
    })?;

    let interceptor_dir = PathBuf::from(home)
        .join(".deciduous")
        .join("trace-interceptor");
    let interceptor_path = interceptor_dir.join("interceptor.js");
    let version_path = interceptor_dir.join(".version");

    let current_version = bundle_version();

    // Check if already installed with correct version
    if interceptor_path.exists() && version_path.exists() {
        if let Ok(installed_version) = std::fs::read_to_string(&version_path) {
            if installed_version.trim() == current_version {
                return Ok(interceptor_path);
            }
        }
    }

    // Extract the interceptor
    std::fs::create_dir_all(&interceptor_dir)?;
    std::fs::write(&interceptor_path, INTERCEPTOR_JS)?;
    std::fs::write(&version_path, &current_version)?;

    Ok(interceptor_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interceptor_embedded() {
        // The embedded JS should be non-empty
        assert!(!INTERCEPTOR_JS.is_empty());
        // Should contain expected content markers
        assert!(INTERCEPTOR_JS.contains("fetch") || INTERCEPTOR_JS.contains("globalThis"));
    }

    #[test]
    fn test_bundle_version_deterministic() {
        let v1 = bundle_version();
        let v2 = bundle_version();
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_ensure_interceptor_installed() {
        // This test actually installs to ~/.deciduous/trace-interceptor/
        // which is fine for testing
        let result = ensure_interceptor_installed();
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.exists());
        assert!(path.to_string_lossy().contains("interceptor.js"));
    }
}
