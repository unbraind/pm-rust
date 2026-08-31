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

#[path = "support/published_cli.rs"]
mod published_cli;

use published_cli::{PublishedCli, published_cli_or_skip};

/// Encodes one path as a JSON string literal.
///
/// A Windows path contains backslashes, which are escape introducers inside a
/// JavaScript double-quoted string. JSON-encoding the path keeps the driver
/// script syntactically valid on every platform.
fn json_string(value: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Renders the deterministic recipe driver used to execute the published CLI.
///
/// The driver pins the recipe clock at [`CLOCK`] with zero tick and replaces
/// the global `Date` constructor with one returning the same fixed instant, so
/// even code paths that bypass the recipe clock write reproducible values.
///
/// `sdk` is the SDK entry the driver imports and `entry` the published CLI
/// entry script; both are JSON-encoded before interpolation.
fn driver_script(sdk: &Path, entry: &Path) -> String {
    let sdk_json = json_string(&sdk.to_string_lossy());
    let entry_json = json_string(&entry.to_string_lossy());
    let template = r#"
// Differential-conformance driver: runs one real published-pm CLI invocation
// under a reproducible workspace recipe (fixed clock, zero tick) with the
// wall-clock Date pinned to the same instant. Both interpolated paths are
// JSON-encoded so Windows backslashes do not break the string literals, and
// both imports go through `pathToFileURL` so the specifiers are valid on every
// platform.
import { pathToFileURL } from "node:url";
const { runWithWorkspaceRecipe } = await import(pathToFileURL(__SDK_JSON__));

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
    await import(pathToFileURL(__ENTRY_JSON__));
  });
} catch (error) {
  if (error && error.name !== "CommanderError") {
    console.error(error);
    process.exitCode = 1;
  }
}
"#;
    template
        .replace("__SDK_JSON__", &sdk_json)
        .replace("__ENTRY_JSON__", &entry_json)
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
    // The clear is deliberate - the point is a reproducible environment - but a
    // few variables are load-bearing for the interpreter itself rather than for
    // the program under test. On Windows, node.exe fails to start without
    // SystemRoot, so clearing it breaks the launch before the published CLI
    // ever runs. Restore those from the parent, then apply the deterministic
    // overrides on top so they still win.
    for name in [
        "PATH",
        "HOME",
        "SystemRoot",
        "SystemDrive",
        "windir",
        "TEMP",
        "TMP",
        "USERPROFILE",
        "COMSPEC",
        "PATHEXT",
        "NUMBER_OF_PROCESSORS",
        "PROCESSOR_ARCHITECTURE",
    ] {
        if let Ok(value) = std::env::var(name) {
            command.env(name, value);
        }
    }
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
    let Some(published) = published_cli_or_skip("The differential conformance suite") else {
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
