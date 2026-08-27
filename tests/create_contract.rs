//! Black-box and SDK acceptance for native create transactions.

use std::fs;
use std::process::{Command as ProcessCommand, Stdio};
#[cfg(target_os = "linux")]
use std::thread;
#[cfg(target_os = "linux")]
use std::time::Duration;

use assert_cmd::Command;
use pm_rust::{CreateItem, PmRustError, Workspace};
use predicates::str::contains;
use serde_json::Value;
use tempfile::TempDir;

const TIMESTAMP: &str = "2026-08-07T10:06:30.183Z";
const ITEM_BYTES: &str = r#"id: sample-native
title: Native create
description: "-A"
type: Task
status: open
priority: 2
tags[3]: "0",safe,"true"
created_at: "2026-08-07T10:06:30.183Z"
updated_at: "2026-08-07T10:06:30.183Z"
author: fixture-agent
body: "0"
"#;
const HISTORY_BYTES: &str = concat!(
    r#"{"ts":"2026-08-07T10:06:30.183Z","author":"fixture-agent","author_source":"asserted","agent_provenance":{"role":{"value":"implementer","source":"argv"}},"op":"create","patch":[{"op":"replace","path":"/body","value":"0"},{"op":"add","path":"/metadata/id","value":"sample-native"},{"op":"add","path":"/metadata/title","value":"Native create"},{"op":"add","path":"/metadata/description","value":"-A"},{"op":"add","path":"/metadata/type","value":"Task"},{"op":"add","path":"/metadata/status","value":"open"},{"op":"add","path":"/metadata/priority","value":2},{"op":"add","path":"/metadata/tags","value":["0","safe","true"]},{"op":"add","path":"/metadata/created_at","value":"2026-08-07T10:06:30.183Z"},{"op":"add","path":"/metadata/updated_at","value":"2026-08-07T10:06:30.183Z"},{"op":"add","path":"/metadata/author","value":"fixture-agent"}],"before_hash":"3cc22dff72be7b14824654a7a64ea62b04799939b2fee54c1b5f52ca60bf6df0","after_hash":"6eb97257a863250fafbcc2d460f0b9a08a1b864bebf2c725632258e3f72db01c","item_hash_version":2,"message":"create fixture"}"#,
    "\n"
);

fn tracker() -> Result<(TempDir, Workspace), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(&root)?;
    fs::write(
        root.join("settings.json"),
        r#"{"id_prefix":"sample-","item_format":"toon","locks":{"ttl_seconds":1800}}"#,
    )?;
    let workspace = Workspace::discover(directory.path())?;
    Ok((directory, workspace))
}

/// Builds the canonical SDK request used across native create acceptance cases.
fn request(id: &str) -> CreateItem {
    CreateItem {
        id: id.to_owned(),
        title: "Native create".to_owned(),
        description: "-A".to_owned(),
        item_type: "Task".to_owned(),
        status: "open".to_owned(),
        priority: 2,
        tags: ["0", "safe", "true"].map(str::to_owned).to_vec(),
        body: "0".to_owned(),
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        message: Some("create fixture".to_owned()),
        provenance_role: Some("implementer".to_owned()),
        force_stale_lock: false,
    }
}

#[test]
/// Proves the Rust transaction matches the official pm 2026.8.7 SDK byte for byte.
fn sdk_create_matches_the_published_pm_2026_8_7_fixture_exactly()
-> Result<(), Box<dyn std::error::Error>> {
    let (_directory, workspace) = tracker()?;
    let result = workspace.create(request("sample-native"))?;
    assert_eq!(
        result.item_path,
        std::path::Path::new("tasks/sample-native.toon")
    );
    assert_eq!(
        result.history_path,
        std::path::Path::new("history/sample-native.jsonl")
    );
    assert_eq!(
        result.after_hash,
        "6eb97257a863250fafbcc2d460f0b9a08a1b864bebf2c725632258e3f72db01c"
    );
    assert_eq!(
        fs::read_to_string(workspace.pm_root().join(&result.item_path))?,
        ITEM_BYTES
    );
    assert_eq!(
        fs::read_to_string(workspace.pm_root().join(&result.history_path))?,
        HISTORY_BYTES
    );
    assert!(
        !workspace
            .pm_root()
            .join("locks/sample-native.lock")
            .exists()
    );
    assert!(
        !workspace
            .pm_root()
            .join("runtime/transactions/create-sample-native.json")
            .exists()
    );
    assert_eq!(workspace.get("sample-native")?, result.item);
    assert!(matches!(
        workspace.create(request("sample-native")),
        Err(PmRustError::ItemAlreadyExists { id }) if id == "sample-native"
    ));
    let mut ambiguous = request("sample-ambiguous");
    ambiguous.title = "\"".to_owned();
    ambiguous.description = "-A".to_owned();
    ambiguous.status = "A\"B".to_owned();
    ambiguous.tags = ["0", "safe", "true"].map(str::to_owned).to_vec();
    ambiguous.body = "0".to_owned();
    let ambiguous_result = workspace.create(ambiguous)?;
    let ambiguous_bytes =
        fs::read_to_string(workspace.pm_root().join("tasks/sample-ambiguous.toon"))?;
    assert!(
        ambiguous_bytes.contains("title: \"\\\"\"\n"),
        "{ambiguous_bytes}"
    );
    assert!(
        ambiguous_bytes.contains("status: \"A\\\"B\"\n"),
        "{ambiguous_bytes}"
    );
    assert_eq!(ambiguous_result.item, workspace.get("sample-ambiguous")?);
    Ok(())
}

#[test]
fn cli_create_emits_json_and_reports_validation_and_conflicts()
-> Result<(), Box<dyn std::error::Error>> {
    let (directory, _) = tracker()?;
    let workspace = directory.path().to_string_lossy();
    let arguments = [
        "--workspace",
        workspace.as_ref(),
        "create",
        "--id",
        "sample-cli",
        "--title",
        "CLI create",
        "--description",
        "CLI fixture",
        "--type",
        "Issue",
        "--status",
        "open",
        "--priority",
        "1",
        "--author",
        "cli-agent",
        "--body",
        "CLI body",
        "--timestamp",
        TIMESTAMP,
        "--tags",
        "native,rust",
        "--message",
        "CLI history",
        "--force-stale-lock",
    ];
    let output = Command::cargo_bin("pm-rust")?.args(arguments).output()?;
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(json["item"]["id"], "sample-cli");
    assert_eq!(json["item"]["priority"], 1);
    assert_eq!(json["item_path"], "issues/sample-cli.toon");
    Command::cargo_bin("pm-rust")?
        .args(arguments)
        .assert()
        .code(2)
        .stderr(contains("pm item already exists: sample-cli"));
    Command::cargo_bin("pm-rust")?
        .args([
            "--workspace",
            workspace.as_ref(),
            "create",
            "--id",
            "../escape",
            "--title",
            "unsafe",
            "--type",
            "Task",
            "--author",
            "cli-agent",
        ])
        .assert()
        .code(2)
        .stderr(contains("invalid create request"));
    Command::cargo_bin("pm-rust")?
        .args([
            "--workspace",
            workspace.as_ref(),
            "create",
            "--id",
            "sample-custom",
            "--title",
            "Unsupported type",
            "--type",
            "Custom",
            "--author",
            "cli-agent",
        ])
        .assert()
        .code(2)
        .stderr(contains("canonical built-in item types only"));
    Command::cargo_bin("pm-rust")?
        .args([
            "--workspace",
            workspace.as_ref(),
            "create",
            "--id",
            "sample-priority",
            "--title",
            "Invalid priority",
            "--type",
            "Task",
            "--priority",
            "5",
            "--author",
            "cli-agent",
        ])
        .assert()
        .code(2)
        .stderr(contains("0..=4"));
    Ok(())
}

#[test]
fn cli_create_reports_a_closed_stdout_pipe() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, _) = tracker()?;
    let mut child = ProcessCommand::new(env!("CARGO_BIN_EXE_pm-rust"))
        .args([
            "--workspace",
            &directory.path().to_string_lossy(),
            "create",
            "--id",
            "sample-pipe",
            "--title",
            "Pipe create",
            "--type",
            "Task",
            "--author",
            "pipe-agent",
            "--body",
            &"x".repeat(16_000),
            "--timestamp",
            TIMESTAMP,
        ])
        .stdout(Stdio::piped())
        .spawn()?;
    drop(child.stdout.take());
    assert_eq!(child.wait()?.code(), Some(2));
    Ok(())
}

#[test]
fn concurrent_processes_preserve_one_complete_create() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, workspace) = tracker()?;
    let binary = env!("CARGO_BIN_EXE_pm-rust");
    let mut children = Vec::new();
    for worker in 0..8 {
        children.push(
            ProcessCommand::new(binary)
                .args([
                    "--workspace",
                    &directory.path().to_string_lossy(),
                    "create",
                    "--id",
                    "sample-race",
                    "--title",
                    "Concurrent create",
                    "--type",
                    "Task",
                    "--author",
                    &format!("worker-{worker}"),
                    "--timestamp",
                    TIMESTAMP,
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?,
        );
    }
    let children = children
        .into_iter()
        .map(std::process::Child::wait_with_output)
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(
        children
            .iter()
            .filter(|output| output.status.success())
            .count(),
        1
    );
    let item = workspace.get("sample-race")?;
    assert!(
        item.metadata.extra["author"]
            .as_str()
            .is_some_and(|author| author.starts_with("worker-"))
    );
    let history = fs::read_to_string(workspace.pm_root().join("history/sample-race.jsonl"))?;
    assert_eq!(history.lines().count(), 1);
    assert!(serde_json::from_str::<Value>(history.trim()).is_ok());
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn create_never_removes_a_lock_replaced_by_another_owner() -> Result<(), Box<dyn std::error::Error>>
{
    let (_directory, workspace) = tracker()?;
    let pm_root = workspace.pm_root().to_path_buf();
    let journal_root = pm_root.join("runtime/transactions");
    fs::create_dir_all(&journal_root)?;
    let journal_path = journal_root.join("create-sample-replaced.json");
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        &journal_path,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )?;
    let creator = thread::spawn(move || workspace.create(request("sample-replaced")));
    let lock_path = pm_root.join("locks/sample-replaced.lock");
    for _ in 0..5_000 {
        if lock_path.exists() {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert!(lock_path.exists());
    fs::write(&lock_path, "replacement owner\n")?;
    fs::write(&journal_path, "not json")?;
    assert!(matches!(
        creator.join().map_err(|_| "creator panicked")?,
        Err(PmRustError::RecoveryConflict { .. })
    ));
    assert_eq!(fs::read_to_string(&lock_path)?, "replacement owner\n");
    fs::remove_file(lock_path)?;
    Ok(())
}

#[test]
fn all_builtin_type_folders_and_current_timestamp_are_supported()
-> Result<(), Box<dyn std::error::Error>> {
    let (_directory, workspace) = tracker()?;
    let cases = [
        ("Epic", "epics"),
        ("Feature", "features"),
        ("Task", "tasks"),
        ("Chore", "chores"),
        ("Issue", "issues"),
        ("Decision", "decisions"),
        ("Event", "events"),
        ("Reminder", "reminders"),
        ("Milestone", "milestones"),
        ("Meeting", "meetings"),
        ("Plan", "plans"),
    ];
    for (index, (item_type, folder)) in cases.into_iter().enumerate() {
        let mut candidate = request(&format!("sample-type-{index}"));
        candidate.item_type = item_type.to_owned();
        candidate.timestamp = None;
        candidate.tags.clear();
        candidate.author = "true".to_owned();
        let result = workspace.create(candidate)?;
        assert_eq!(
            result.item_path,
            std::path::Path::new(folder).join(format!("sample-type-{index}.toon"))
        );
        assert!(result.item.metadata.created_at.ends_with('Z'));
        let raw = fs::read_to_string(workspace.pm_root().join(result.item_path))?;
        assert!(raw.contains("tags: []\n"));
        assert!(raw.contains("author: \"true\"\n"));
    }
    Ok(())
}

#[test]
/// Proves each validation rule refuses on its own.
///
/// Every case overrides exactly one field of the canonical fixture, so the
/// refusal it asserts can only come from the rule it names. Building the cases
/// from `request` rather than repeating a full literal also means a new
/// `CreateItem` field is one edit rather than seven.
fn sdk_create_rejects_invalid_ids_and_fields() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, workspace) = tracker()?;

    // `sample-bad-`, not `bad-`: an id that also violates the configured
    // `sample-` prefix would let the refusal come from the prefix rule instead
    // of the trailing-hyphen rule this case exists to pin.
    //
    // The leading-hyphen case cannot be isolated the same way - an id that
    // starts with a hyphen cannot also start with `sample-` - so it asserts the
    // refusal without claiming which of the two rules produced it.
    let cases: Vec<(&str, CreateItem)> = vec![
        (
            "an id starting with a hyphen",
            CreateItem {
                id: "-bad".to_owned(),
                ..request("sample-unused")
            },
        ),
        (
            "an id ending with a hyphen",
            CreateItem {
                id: "sample-bad-".to_owned(),
                ..request("sample-unused")
            },
        ),
        (
            "an empty id",
            CreateItem {
                id: String::new(),
                ..request("sample-unused")
            },
        ),
        (
            "a blank title",
            CreateItem {
                title: "  ".to_owned(),
                ..request("sample-empty")
            },
        ),
        (
            "an unsupported item type",
            CreateItem {
                item_type: "Custom".to_owned(),
                ..request("sample-custom")
            },
        ),
        (
            "an out-of-range priority",
            CreateItem {
                priority: 5,
                ..request("sample-bad-prio")
            },
        ),
        (
            "a malformed timestamp",
            CreateItem {
                timestamp: Some("not-a-timestamp".to_owned()),
                ..request("sample-bad-ts")
            },
        ),
    ];

    for (rule, invalid) in cases {
        let result = workspace.create(invalid);
        assert!(
            matches!(result, Err(PmRustError::InvalidCreateRequest { .. })),
            "{rule} must be refused with InvalidCreateRequest, got {result:?}"
        );
    }

    Ok(())
}
