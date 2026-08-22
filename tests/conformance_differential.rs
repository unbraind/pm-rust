//! Live differential conformance against the published Node `pm` CLI.
//!
//! The suite drives the real published CLI and the native Rust binary over two
//! identical fixture workspaces with identical inputs, then asserts the stored
//! `.toon` items and `.jsonl` history streams are byte-for-byte identical after
//! every operation. The published CLI executes inside its own reproducible
//! workspace-recipe facility (fixed clock, zero tick) with the wall-clock
//! `Date` pinned to the same instant, so every timestamp it writes is
//! deterministic and matchable by the native binary's explicit `--timestamp`.
//!
//! When no Node `pm` installation can be located the suite prints an explicit
//! skip notice and passes; it never simulates the published side.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const CLOCK: &str = "2026-08-22T10:00:00.000Z";

/// Describes one located published CLI installation.
struct PublishedCli {
    /// Path to the package root holding `dist/`.
    package_root: PathBuf,
    /// Entry script used to boot the CLI.
    entry: PathBuf,
}

/// Locates the published Node CLI through the environment or common prefixes.
///
/// `PM_NODE_CLI` may point at the package root or directly at an entry script;
/// otherwise each directory on `PATH` is probed for a `pm` launcher resolving
/// inside an `@unbrained/pm-cli` installation.
fn locate_published_cli() -> Option<PublishedCli> {
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
        if let (true, Some(root)) = (path.is_file(), path.parent().and_then(Path::parent)) {
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

/// Renders the deterministic recipe driver used to execute the published CLI.
///
/// The driver pins the recipe clock at [`CLOCK`] with zero tick and replaces
/// the global `Date` constructor with one returning the same fixed instant, so
/// even code paths that bypass the recipe clock write reproducible values.
fn driver_script(sdk: &Path, entry: &Path) -> String {
    let template = r#"
// Differential-conformance driver: runs one real published-pm CLI invocation
// under a reproducible workspace recipe (fixed clock, zero tick) with the
// wall-clock Date pinned to the same instant.
import { runWithWorkspaceRecipe } from "@SDK@";
import { pathToFileURL } from "node:url";

const fixed = Date.parse(process.env.FIXED_CLOCK);
class pinnedDate extends Date {
  constructor(...args) {
    args.length === 0 ? super(fixed) : super(...args);
  }
  static now() {
    return fixed;
  }
}
globalThis.Date = pinnedDate;
process.argv = [process.argv[0], "pm", ...process.argv.slice(2)];
const recipe = {
  schema: "https://schema.unbrained.dev/pm/workspace-recipe/v1",
  clock: process.env.FIXED_CLOCK,
  tickMs: 0,
  seed: "conformance-seed",
  operations: [],
};
try {
  await runWithWorkspaceRecipe(recipe, async () => {
    await import(pathToFileURL("@ENTRY@"));
  });
} catch (error) {
  if (error && error.name !== "CommanderError") {
    console.error(error);
    process.exitCode = 1;
  }
}
"#;
    template
        .replace("@SDK@", &sdk.to_string_lossy())
        .replace("@ENTRY@", &entry.to_string_lossy())
}

/// Writes the driver script into the scratch directory and returns its path.
fn write_driver(
    directory: &Path,
    published: &PublishedCli,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let sdk = published.package_root.join("dist/cli-bundle/sdk.js");
    if !sdk.is_file() {
        return Err(format!("published SDK bundle not found at {}", sdk.display()).into());
    }
    let path = directory.join("conformance-driver.mjs");
    fs::write(&path, driver_script(&sdk, &published.entry))?;
    Ok(path)
}

/// Runs one command with a minimal deterministic environment.
fn run_minimal(
    program: &Path,
    arguments: &[String],
    working_directory: &Path,
) -> Result<std::process::Output, Box<dyn std::error::Error>> {
    let mut command = Command::new(program);
    command.current_dir(working_directory);
    command.env_clear();
    command.env("PATH", std::env::var("PATH").unwrap_or_default());
    command.env("HOME", std::env::var("HOME").unwrap_or_default());
    command.env("FIXED_CLOCK", CLOCK);
    command.args(arguments).output().map_err(Into::into)
}

/// Recursively copies one directory's regular files and subdirectories.
fn copy_directory(source: &Path, destination: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.path().is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// One recorded operation executed identically on both implementations.
struct Step {
    /// Human-readable label used in failure messages.
    label: &'static str,
    /// Arguments passed to the native binary after its workspace flag.
    native: &'static [&'static str],
    /// Arguments passed to the published Node CLI.
    node: &'static [&'static str],
}

/// Returns the full recorded mutation sequence exercised on both sides.
#[allow(clippy::too_many_lines)]
fn steps() -> Vec<Step> {
    vec![
        Step {
            label: "create",
            native: &[
                "create",
                "--id",
                "sample-diff",
                "--title",
                "Conformance item",
                "--type",
                "Task",
                "--author",
                "fixture-agent",
                "--description",
                "First desc",
                "--tags",
                "alpha,beta",
                "--body",
                "Original body",
            ],
            node: &[
                "create",
                "--id",
                "sample-diff",
                "--title",
                "Conformance item",
                "--type",
                "Task",
                "--author",
                "fixture-agent",
                "--description",
                "First desc",
                "--tags",
                "alpha,beta",
                "--body",
                "Original body",
            ],
        },
        Step {
            label: "update title and priority",
            native: &[
                "update",
                "sample-diff",
                "--title",
                "Renamed item",
                "--priority",
                "3",
                "--message",
                "rename and reprioritize",
                "--author",
                "fixture-agent",
            ],
            node: &[
                "update",
                "sample-diff",
                "--title",
                "Renamed item",
                "--priority",
                "3",
                "--message",
                "rename and reprioritize",
                "--author",
                "fixture-agent",
            ],
        },
        Step {
            label: "comment append",
            native: &[
                "comment",
                "sample-diff",
                "First native note",
                "--message",
                "note recorded",
                "--author",
                "fixture-agent",
            ],
            node: &[
                "comments",
                "sample-diff",
                "First native note",
                "--message",
                "note recorded",
                "--author",
                "fixture-agent",
            ],
        },
        Step {
            label: "status transition",
            native: &[
                "update",
                "sample-diff",
                "--status",
                "in_progress",
                "--author",
                "fixture-agent",
            ],
            node: &[
                "update",
                "sample-diff",
                "--status",
                "in_progress",
                "--author",
                "fixture-agent",
            ],
        },
        Step {
            label: "close",
            native: &[
                "close",
                "sample-diff",
                "--reason",
                "conformance complete",
                "--author",
                "fixture-agent",
            ],
            node: &[
                "close",
                "sample-diff",
                "--reason",
                "conformance complete",
                "--author",
                "fixture-agent",
            ],
        },
    ]
}

#[test]
/// Proves the native binary matches the live published CLI byte for byte.
fn rust_and_published_cli_produce_identical_bytes_over_the_same_sequence()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(published) = locate_published_cli() else {
        println!("skip: no published Node pm CLI found (set PM_NODE_CLI to enable)");
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;

    let node_workspace = tempfile::tempdir()?;
    let interpreter =
        PathBuf::from(std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned()));
    let initialized = run_minimal(
        &interpreter,
        &[
            driver.to_string_lossy().into_owned(),
            "init".to_owned(),
            "sample-".to_owned(),
            "--defaults".to_owned(),
        ],
        node_workspace.path(),
    )?;
    assert!(
        initialized.status.success(),
        "published pm init failed: {}",
        String::from_utf8_lossy(&initialized.stderr)
    );
    let rust_workspace = tempfile::tempdir()?;
    copy_directory(
        &node_workspace.path().join(".agents"),
        &rust_workspace.path().join(".agents"),
    )?;

    for step in steps() {
        let mut node_arguments: Vec<String> = vec![driver.to_string_lossy().into_owned()];
        node_arguments.extend(step.node.iter().map(ToString::to_string));
        let node_output = run_minimal(&interpreter, &node_arguments, node_workspace.path())?;
        assert!(
            node_output.status.success(),
            "published CLI failed at {}: {}",
            step.label,
            String::from_utf8_lossy(&node_output.stderr)
        );

        let mut rust_arguments: Vec<String> =
            vec![format!("--workspace={}", rust_workspace.path().display())];
        rust_arguments.extend(step.native.iter().map(ToString::to_string));
        rust_arguments.push(format!("--timestamp={CLOCK}"));
        let rust_output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
            .args(&rust_arguments)
            .current_dir(rust_workspace.path())
            .output()?;
        assert!(
            rust_output.status.success(),
            "native binary failed at {}: {} {}",
            step.label,
            String::from_utf8_lossy(&rust_output.stdout),
            String::from_utf8_lossy(&rust_output.stderr)
        );

        for artifact in [
            ".agents/pm/tasks/sample-diff.toon",
            ".agents/pm/history/sample-diff.jsonl",
        ] {
            let node_bytes = fs::read(node_workspace.path().join(artifact)).map_err(|error| {
                format!(
                    "published side missing {artifact} after {}: {error}",
                    step.label
                )
            })?;
            let rust_bytes = fs::read(rust_workspace.path().join(artifact)).map_err(|error| {
                format!(
                    "native side missing {artifact} after {}: {error}",
                    step.label
                )
            })?;
            assert_eq!(
                rust_bytes, node_bytes,
                "{} diverges in {artifact}: native vs published",
                step.label
            );
        }
    }
    Ok(())
}
