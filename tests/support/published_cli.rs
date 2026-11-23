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

/// Panics when a required published CLI is absent, otherwise returns `None`.
///
/// Both differential suites degrade to a printed skip when no published CLI is
/// reachable, so a run without one passes while proving nothing. That is the
/// shape a gate fails in silently: present, green, and never executed. Setting
/// `PM_RUST_REQUIRE_PUBLISHED_CLI=1` — which CI does — turns the absence into a
/// failure, so the differential can only pass by actually running.
pub fn published_cli_or_skip(context: &str) -> Option<PublishedCli> {
    if let Some(cli) = locate_published_cli() {
        return Some(cli);
    }
    assert!(
        std::env::var("PM_RUST_REQUIRE_PUBLISHED_CLI").is_err(),
        "{context} requires the published Node pm CLI, but none was found and \
         PM_RUST_REQUIRE_PUBLISHED_CLI is set. Run npm ci --prefix tests/oracle --ignore-scripts \
         or point PM_NODE_CLI at a matching installation.",
    );
    println!(
        "skip: no published Node pm CLI found at the pinned version (run npm ci --prefix tests/oracle --ignore-scripts)"
    );
    None
}

/// Accepts a package only when its entry and version match the locked oracle.
fn pinned_cli(package_root: PathBuf) -> Option<PublishedCli> {
    let entry = package_root.join("dist/cli.js");
    if !entry.is_file() {
        return None;
    }
    let pin: serde_json::Value =
        serde_json::from_str(include_str!("../oracle/package.json")).ok()?;
    let installed: serde_json::Value =
        serde_json::from_slice(&fs::read(package_root.join("package.json")).ok()?).ok()?;
    let version = pin["dependencies"]["@unbrained/pm-cli"].as_str()?;
    if installed["version"] != version {
        eprintln!(
            "Ignoring published Node pm CLI version {} (expected {version})",
            installed["version"]
        );
        return None;
    }
    Some(PublishedCli {
        package_root,
        entry,
    })
}

/// Locates the locked published Node CLI through an explicit override, local install or PATH.
#[allow(clippy::module_name_repetitions)]
pub fn locate_published_cli() -> Option<PublishedCli> {
    if let Ok(value) = std::env::var("PM_NODE_CLI") {
        // Canonicalize before consumers use a different working directory.
        let path = PathBuf::from(value).canonicalize().ok()?;
        if path.is_dir() {
            return pinned_cli(path);
        }
        // Only dist/cli.js is the executable entry, even if other dist files exist.
        if path.ends_with("dist/cli.js") {
            return pinned_cli(path.parent()?.parent()?.to_path_buf());
        }
        return None;
    }
    let package_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/oracle/node_modules/@unbrained/pm-cli");
    if let Some(cli) = pinned_cli(package_root) {
        return Some(cli);
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
                && let Some(cli) = pinned_cli(ancestor)
            {
                return Some(cli);
            }
        }
    }
    None
}

/// Builds an oracle process through the shared telemetry-isolating launcher.
#[allow(dead_code)]
pub fn oracle_command(interpreter: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    let mut command = std::process::Command::new(interpreter);
    command.arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle/launch.mts"));
    command.arg("--entry");
    command
}

/// Launches a tool whose descendants execute oracle code, such as Git merge drivers.
#[allow(dead_code)]
pub fn oracle_tool(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    let interpreter = std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned());
    let mut command = std::process::Command::new(interpreter);
    command.arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle/launch.mts"));
    command.arg("--command").arg(program);
    command
}
