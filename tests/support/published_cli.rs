//! Shared published-CLI locator for the integration contracts.
//!
//! Both `tests/branch_merge_contract.rs` and `tests/conformance_differential.rs`
//! need to locate the published Node CLI. The two copies had already diverged:
//! the branch-merge copy accepted a `PM_NODE_CLI` entry-script fallback without
//! validating `dist/cli.js` exists or that the path is a file, so a
//! misconfigured variable produced a confusing downstream assertion instead of
//! the intended skip notice. One locator lives here now, and it validates the
//! entry script before returning.

use std::fs;
use std::path::{Path, PathBuf};

/// Describes one located published CLI installation.
pub struct PublishedCli {
    /// Path to the package root holding `dist/`.
    pub package_root: PathBuf,
    /// Path to the validated `dist/cli.js` entry script.
    ///
    /// Some consumers (the branch-merge contract) locate the CLI only to read
    /// `package_root`, so the field is allowed to be dead in those crates.
    #[allow(dead_code)]
    pub entry: PathBuf,
}

/// Locates the published Node CLI through the environment or common prefixes.
///
/// `PM_NODE_CLI` may point at the package root or directly at an entry script;
/// otherwise each directory on `PATH` is probed for a `pm` launcher resolving
/// inside an `@unbrained/pm-cli` installation. The entry script is validated
/// before returning so a misconfigured `PM_NODE_CLI` produces an explicit skip
/// notice rather than a confusing downstream assertion failure.
#[allow(clippy::module_name_repetitions)]
pub fn locate_published_cli() -> Option<PublishedCli> {
    if let Ok(value) = std::env::var("PM_NODE_CLI") {
        let path = PathBuf::from(value);
        let entry = path.join("dist/cli.js");
        if path.is_dir() && entry.is_file() {
            return Some(PublishedCli {
                package_root: path,
                entry,
            });
        }
        // An explicit entry script implies its package root two levels up.
        if path.is_file()
            && let Some(root) = path.parent().and_then(Path::parent)
            && root.join("dist/cli.js").is_file()
        {
            return Some(PublishedCli {
                package_root: root.to_path_buf(),
                entry: path,
            });
        }
        return None;
    }
    let path_variable = std::env::var("PATH").ok()?;
    for directory in std::env::split_paths(&path_variable) {
        let candidate = directory.join("pm");
        if !candidate.is_file() {
            continue;
        }
        let Ok(resolved) = fs::canonicalize(&candidate) else {
            continue;
        };
        let mut current = resolved.parent().map(Path::to_path_buf);
        while let Some(ancestor) = current {
            current = ancestor.parent().map(Path::to_path_buf);
            if ancestor.file_name().is_some_and(|name| name == "pm-cli")
                && ancestor
                    .parent()
                    .and_then(Path::file_name)
                    .is_some_and(|name| name == "@unbrained")
            {
                let entry = ancestor.join("dist/cli.js");
                if entry.is_file() {
                    return Some(PublishedCli {
                        package_root: ancestor.clone(),
                        entry,
                    });
                }
            }
        }
    }
    None
}
