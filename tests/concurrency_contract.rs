//! Multi-process no-lost-update acceptance for native comment mutations.
//!
//! The test spawns many concurrent native CLI processes that each append a
//! comment to one shared item, relying on the per-item lock and its configured
//! wait budget rather than any test-side coordination. It then proves that
//! every accepted mutation survived: the item's comment rows, the history
//! stream length, and the history hash chain all agree with the set of
//! processes that reported success.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::thread;

use serde_json::Value;
#[path = "support/process_environment.rs"]
mod process_environment;
#[path = "support/published_cli.rs"]
#[allow(dead_code)]
mod published_cli;

const PROCESSES: usize = 8;
const MUTATIONS_PER_PROCESS: usize = 3;

/// Builds the fixture workspace and seeds one shared item for contention.
fn seeded_workspace() -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    let published = published_cli::locate_published_cli()
        .ok_or("published CLI required for no-git initialization")?;
    let init = oracle_command()
        .arg(published.entry)
        .current_dir(directory.path())
        .env(
            "GIT_CEILING_DIRECTORIES",
            directory.path().parent().ok_or("temp parent")?,
        )
        .args(["init", "sample-", "--defaults", "--agent-guidance", "skip"])
        .output()?;
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    fs::write(
        root.join("settings.json"),
        r#"{"id_prefix":"sample-","item_format":"toon","locks":{"ttl_seconds":1800,"wait_ms":180000}}"#,
    )?;
    fs::create_dir(directory.path().join("empty-path"))?;
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
    Ok(directory)
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
        .env("PATH", workspace.join("empty-path"))
        .env(
            "GIT_CEILING_DIRECTORIES",
            workspace.parent().unwrap_or(workspace),
        )
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

#[test]
/// Proves concurrent native processes never lose an accepted mutation.
fn concurrent_comment_processes_preserve_every_accepted_mutation()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = seeded_workspace()?;
    let workspace = Arc::new(directory.path().to_path_buf());

    let mut handles = Vec::new();
    for process in 0..PROCESSES {
        let workspace = Arc::clone(&workspace);
        handles.push(thread::spawn(move || {
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
        "every submitted write must succeed: {rejections:?}"
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

/// Starts the published CLI without ambient tracker or agent context.
fn oracle_command() -> Command {
    let mut command = published_cli::oracle_command("node");
    process_environment::isolate_environment(&mut command);
    command
}

/// Starts a native fixture process without ambient tracker or agent context.
fn native_command(root: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pm-rust"));
    process_environment::isolate_environment(&mut command);
    command.current_dir(root);
    command
}
