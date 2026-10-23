//! Multi-process no-lost-update acceptance for native comment mutations.
//!
//! The test spawns many concurrent native CLI processes that each append a
//! comment to one shared item. A start barrier releases the writer threads
//! together; each process relies on the per-item lock and its configured wait
//! budget. The test then proves that
//! every accepted mutation survived: the item's comment rows, the history
//! stream length, and the history hash chain all agree with the set of
//! processes that reported success.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Barrier};
use std::thread;

use serde_json::Value;
#[path = "support/process_environment.rs"]
mod process_environment;
#[path = "support/published_cli.rs"]
#[allow(dead_code)]
mod published_cli;

const PROCESSES: usize = 50;
const MUTATIONS_PER_PROCESS: usize = 3;

/// Run the native executable in a plain tracker with no Git executable available.
fn native_command(workspace: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pm-rust"));
    process_environment::isolate_environment(&mut command);
    command
        .current_dir(workspace)
        .env("HOME", workspace.join("isolated-home"))
        .env("USERPROFILE", workspace.join("isolated-home"))
        .env(
            "GIT_CEILING_DIRECTORIES",
            workspace.parent().unwrap_or(workspace),
        )
        .env("PATH", workspace.join("no-tools"));
    command
}

/// Builds the fixture workspace and seeds one shared item for contention.
fn seeded_workspace() -> Result<Option<tempfile::TempDir>, Box<dyn std::error::Error>> {
    let Some(published) = published_cli::published_cli_or_skip("50-writer non-git contract") else {
        return Ok(None);
    };
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    let interpreter = std::env::var_os("PM_NODE_INTERPRETER").unwrap_or_else(|| "node".into());
    let mut init = published_cli::oracle_command(interpreter);
    process_environment::isolate_environment(&mut init);
    let output = init
        .arg(published.entry)
        .args(["init", "sample-", "--defaults", "--agent-guidance", "skip"])
        .current_dir(directory.path())
        .env("HOME", directory.path().join("isolated-home"))
        .env("USERPROFILE", directory.path().join("isolated-home"))
        .env(
            "GIT_CEILING_DIRECTORIES",
            directory.path().parent().ok_or("temp parent")?,
        )
        .output()?;
    assert!(
        output.status.success(),
        "oracle init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut settings: Value = serde_json::from_slice(&fs::read(root.join("settings.json"))?)?;
    settings["locks"]["wait_ms"] = serde_json::json!(60000);
    settings["agent_identity"]["probes_enabled"] = serde_json::json!(false);
    fs::write(root.join("settings.json"), serde_json::to_vec(&settings)?)?;
    assert!(!directory.path().join(".git").exists());
    let output = native_command(directory.path())
        .args([
            "--workspace=.",
            "create",
            "--id=sample-load",
            "--title=Contention target",
            "--type=Task",
            "--author=load-agent",
        ])
        .output()?;
    assert!(output.status.success());
    Ok(Some(directory))
}

/// Appends one comment from one dedicated OS process.
///
/// Returns `Ok(())` when the mutation was accepted, and `Err(reason)` carrying
/// the process's own diagnostic when it was not. The reason is kept rather than
/// collapsed to a boolean because the two ways a writer can be turned away are
/// not equivalent: exhausting the configured lock wait budget is admission
/// control and depends on how fast the host is, while any other failure is a
/// product defect. A test that only counts successes cannot tell them apart and
/// would pass through a real crash on a slow platform.
fn append_comment(workspace: &Path, marker: &str) -> Result<(), String> {
    let text = format!("concurrent note {marker}");
    let output = native_command(workspace)
        .args([
            "--workspace=.",
            "comment",
            "sample-load",
            text.as_str(),
            "--author=load-agent",
        ])
        .output()
        .map_err(|error| format!("could not spawn the native binary: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "exit {status}: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr).trim()
    ))
}

/// Reads the stored item document bytes for the contention target.
fn stored_item(workspace: &Path) -> Result<String, Box<dyn std::error::Error>> {
    Ok(fs::read_to_string(
        workspace.join(".agents/pm/tasks/sample-load.toon"),
    )?)
}

/// Reads every history record for the contention target.
fn history_records(workspace: &Path) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let raw = fs::read_to_string(workspace.join(".agents/pm/history/sample-load.jsonl"))?;
    Ok(raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<Vec<_>, _>>()?)
}

/// Enumerates every writer and mutation marker in the complete contention workload.
fn expected_markers() -> std::collections::BTreeSet<String> {
    (0..PROCESSES)
        .flat_map(|process| {
            (0..MUTATIONS_PER_PROCESS)
                .map(move |mutation| format!("concurrent note {process}-{mutation}"))
        })
        .collect()
}

#[test]
/// Proves concurrent native processes never lose an accepted mutation.
fn concurrent_comment_processes_preserve_every_accepted_mutation()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(directory) = seeded_workspace()? else {
        return Ok(());
    };
    let workspace = Arc::new(directory.path().to_path_buf());
    let start = Arc::new(Barrier::new(PROCESSES));

    let mut handles = Vec::new();
    for process in 0..PROCESSES {
        let workspace = Arc::clone(&workspace);
        let start = Arc::clone(&start);
        handles.push(thread::spawn(move || {
            start.wait();
            let mut accepted = 0;
            let mut rejections = Vec::new();
            for mutation in 0..MUTATIONS_PER_PROCESS {
                let marker = format!("{process}-{mutation}");
                match append_comment(workspace.as_path(), &marker) {
                    Ok(()) => accepted += 1,
                    Err(reason) => rejections.push(reason),
                }
            }
            (accepted, rejections)
        }));
    }
    let mut accepted_total = 0;
    let mut rejections = Vec::new();
    for handle in handles {
        let (accepted, mut reasons) = handle
            .join()
            .map_err(|panic| format!("worker thread panicked: {panic:?}"))?;
        accepted_total += accepted;
        rejections.append(&mut reasons);
    }
    assert!(
        rejections.is_empty(),
        "every writer must succeed: {rejections:?}"
    );
    assert_eq!(accepted_total, PROCESSES * MUTATIONS_PER_PROCESS);

    // Every accepted mutation is present as one comment row.
    let item = stored_item(directory.path())?;
    let row_count = item
        .lines()
        .filter(|line| line.starts_with("  \"") && line.contains("concurrent note"))
        .count();
    assert_eq!(row_count, accepted_total, "lost comment rows in the item");
    let document = pm_rust::Workspace::discover(directory.path())?.get("sample-load")?;
    let actual: std::collections::BTreeSet<_> = document.metadata.extra["comments"]
        .as_array()
        .ok_or("missing comment array")?
        .iter()
        .filter_map(|comment| comment["text"].as_str())
        .collect();
    let expected = expected_markers();
    assert_eq!(actual, expected.iter().map(String::as_str).collect());

    let document = pm_rust::Workspace::discover(directory.path())?.get("sample-load")?;
    let comments = document.metadata.extra["comments"]
        .as_array()
        .ok_or("comment array")?;
    for process in 0..PROCESSES {
        for mutation in 0..MUTATIONS_PER_PROCESS {
            let marker = format!("concurrent note {process}-{mutation}");
            assert_eq!(
                comments.iter().filter(|row| row["text"] == marker).count(),
                1
            );
        }
    }

    // Every accepted mutation is present as exactly one history record, and
    // the hash chain proves each update built on the previous durable state.
    let records = history_records(directory.path())?;
    assert_eq!(
        records.len(),
        accepted_total + 1,
        "history stream length diverges from accepted mutations"
    );
    for (index, pair) in records.windows(2).enumerate() {
        // Presence first. `Value::Null` is what indexing returns for a missing
        // key, so if the record schema ever renamed or dropped either field
        // both sides would be Null, the equality would hold, and this test
        // would keep passing while proving nothing about the chain.
        assert!(
            pair[0]["after_hash"].is_string(),
            "record {index} has no after_hash; the chain check would compare Null to Null"
        );
        assert!(
            pair[1]["before_hash"].is_string(),
            "record {} has no before_hash; the chain check would compare Null to Null",
            index + 1
        );
        assert_eq!(
            pair[0]["after_hash"], pair[1]["before_hash"],
            "history hash chain broke under concurrency"
        );
    }
    for (index, record) in records.iter().enumerate().skip(1) {
        assert_eq!(
            record["op"], "comment_add",
            "record {index} is not a comment"
        );
        assert_eq!(
            record["item_hash_version"], 3,
            "record {index} is not stamped with the published epoch"
        );
    }
    Ok(())
}

/// Optional oracle absence skips the real concurrency test; required absence fails closed.
#[test]
fn absent_concurrency_oracle_obeys_optional_and_required_modes()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    for required in [false, true] {
        let mut command = Command::new(std::env::current_exe()?);
        command
            .args([
                "--exact",
                "concurrent_comment_processes_preserve_every_accepted_mutation",
                "--nocapture",
            ])
            .env("PM_NODE_CLI", directory.path().join("absent-oracle"));
        if required {
            command.env("PM_RUST_REQUIRE_PUBLISHED_CLI", "1");
        } else {
            command.env_remove("PM_RUST_REQUIRE_PUBLISHED_CLI");
        }
        let output = command.output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.success(), !required, "{stdout}{stderr}");
        if required {
            assert!(
                stderr.contains("requires the published Node pm CLI"),
                "{stdout}{stderr}"
            );
        } else {
            assert!(
                stdout.contains("skip: no published Node pm CLI found"),
                "{stdout}{stderr}"
            );
        }
    }
    Ok(())
}
