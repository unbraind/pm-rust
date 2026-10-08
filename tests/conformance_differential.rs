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
use std::path::PathBuf;
use std::process::Command;

const CLOCK: &str = "2026-08-22T10:00:00.000Z";

#[path = "support/published_cli.rs"]
mod published_cli;

use published_cli::published_cli_or_skip;

#[path = "support/mutation_differential.rs"]
mod mutation_differential;
use mutation_differential::{copy_directory, run_minimal, write_driver};

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
