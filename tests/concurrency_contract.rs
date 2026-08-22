//! Multi-process no-lost-update acceptance for native comment mutations.
//!
//! The test spawns many concurrent native CLI processes that each append a
//! comment to one shared item, relying on the per-item lock and its configured
//! wait budget rather than any test-side coordination. It then proves that
//! every accepted mutation survived: the item's comment rows, the history
//! stream length, and the history hash chain all agree with the set of
//! processes that reported success.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::thread;

use serde_json::Value;

const PROCESSES: usize = 8;
const MUTATIONS_PER_PROCESS: usize = 3;

/// Builds the fixture workspace and seeds one shared item for contention.
fn seeded_workspace() -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(&root)?;
    fs::write(
        root.join("settings.json"),
        r#"{"id_prefix":"sample-","item_format":"toon","locks":{"ttl_seconds":1800,"wait_ms":60000}}"#,
    )?;
    let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .current_dir(directory.path())
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
fn append_comment(workspace: &PathBuf, marker: &str) -> bool {
    let text = format!("concurrent note {marker}");
    let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .current_dir(workspace)
        .args([
            "--workspace=.",
            "comment",
            "sample-load",
            text.as_str(),
            "--author=load-agent",
        ])
        .output();
    output.is_ok_and(|output| output.status.success())
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
            for mutation in 0..MUTATIONS_PER_PROCESS {
                let marker = format!("{process}-{mutation}");
                if append_comment(&workspace, &marker) {
                    accepted += 1;
                }
            }
            accepted
        }));
    }
    let mut accepted_total = 0;
    for handle in handles {
        let accepted = handle
            .join()
            .map_err(|panic| format!("worker thread panicked: {panic:?}"))?;
        accepted_total += accepted;
    }
    assert_eq!(
        accepted_total,
        PROCESSES * MUTATIONS_PER_PROCESS,
        "the configured lock wait budget must admit every writer"
    );

    // Every accepted mutation is present as one comment row.
    let item = stored_item(directory.path())?;
    let row_count = item
        .lines()
        .filter(|line| line.starts_with("  \"") && line.contains("concurrent note"))
        .count();
    assert_eq!(row_count, accepted_total, "lost comment rows in the item");

    // Every accepted mutation is present as exactly one history record, and
    // the hash chain proves each update built on the previous durable state.
    let records = history_records(directory.path())?;
    assert_eq!(
        records.len(),
        accepted_total + 1,
        "history stream length diverges from accepted mutations"
    );
    for pair in records.windows(2) {
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
        assert_eq!(record["item_hash_version"], 2, "record {index} is not v2");
    }
    Ok(())
}
