//! Black-box and SDK acceptance for native create transactions.

use assert_cmd::Command;
use pm_rust::{CreateItem, PmRustError, Workspace};
use predicates::str::contains;
use serde_json::Value;
use std::fs;
#[cfg(target_os = "linux")]
#[path = "support/published_cli.rs"]
mod published_cli;
use std::process::{Command as ProcessCommand, Stdio};
#[cfg(target_os = "linux")]
use std::thread;
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
    r#"{"hash_algorithm":"sha256","ts":"2026-08-07T10:06:30.183Z","author":"fixture-agent","author_source":"asserted","agent_provenance":{"role":{"value":"implementer","source":"argv"}},"op":"create","patch":[{"op":"replace","path":"/body","value":"0"},{"op":"add","path":"/metadata/id","value":"sample-native"},{"op":"add","path":"/metadata/title","value":"Native create"},{"op":"add","path":"/metadata/description","value":"-A"},{"op":"add","path":"/metadata/type","value":"Task"},{"op":"add","path":"/metadata/status","value":"open"},{"op":"add","path":"/metadata/priority","value":2},{"op":"add","path":"/metadata/tags","value":["0","safe","true"]},{"op":"add","path":"/metadata/created_at","value":"2026-08-07T10:06:30.183Z"},{"op":"add","path":"/metadata/updated_at","value":"2026-08-07T10:06:30.183Z"},{"op":"add","path":"/metadata/author","value":"fixture-agent"}],"before_hash":"3cc22dff72be7b14824654a7a64ea62b04799939b2fee54c1b5f52ca60bf6df0","after_hash":"6eb97257a863250fafbcc2d460f0b9a08a1b864bebf2c725632258e3f72db01c","item_hash_version":3,"message":"create fixture","event_class":"substantive","record_hash_version":1,"record_hash":"edccc33297c6df932310784ab556317e3e1f9e07fb24d7c626c2104a3991d64a"}"#,
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
/// Proves the Rust transaction matches the published pm SDK byte for byte.
///
/// The recorded line came from pm 2026.8.7 and carried `item_hash_version: 2`.
/// The published CLI moved that epoch to 3 in 2026.8.24; pm CLI 2026.9.11
/// added the top-level `hash_algorithm: "sha256"` field (pm-bbk8we), which
/// also changes `record_hash` because the digest input includes the new field.
/// Every other byte, including `before_hash` and `after_hash`, is unchanged,
/// which is what makes editing the marker in place the honest update rather
/// than a re-record.
fn sdk_create_matches_the_published_sdk_fixture_exactly() -> Result<(), Box<dyn std::error::Error>>
{
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

/// Replaces owner bytes only after the recovery reader proves initialization finished.
#[cfg(target_os = "linux")]
fn replace_lock_after_recovery_opens(
    lock_path: &std::path::Path,
    journal_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // A FIFO writer opens only once recovery has opened its reader, which
    // occurs after the owner token was fully written and synced. Hold that
    // rendezvous open until the replacement bytes are safely installed.
    let mut journal_writer = fs::OpenOptions::new().write(true).open(journal_path)?;
    let payload: Value = serde_json::from_slice(&fs::read(lock_path)?)?;
    assert_eq!(payload["id"], "sample-replaced");
    assert!(
        payload["token"]
            .as_str()
            .is_some_and(|token| !token.is_empty())
    );
    fs::write(lock_path, "replacement owner\n")?;
    std::io::Write::write_all(&mut journal_writer, b"not json")?;
    drop(journal_writer);
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
    replace_lock_after_recovery_opens(&lock_path, &journal_path)?;
    assert!(matches!(
        creator.join().map_err(|_| "creator panicked")?,
        Err(PmRustError::RecoveryConflict { .. })
    ));
    let replacement = fs::read_to_string(&lock_path)?;
    assert_eq!(replacement, "replacement owner\n");
    fs::remove_file(&lock_path)?;
    if let Some(published) = published_cli::published_cli_or_skip("Create replacement differential")
    {
        let scratch = tempfile::tempdir()?;
        let driver = scratch.path().join("lock-replacement.mjs");
        fs::write(
            &driver,
            format!(
                "import fs from 'node:fs/promises'; import path from 'node:path'; import {{pathToFileURL}} from 'node:url'; const {{acquireLock}} = await import(pathToFileURL({})); const root=process.argv[2]; const release=await acquireLock(root,'sample-replaced',1800,'fixture-agent'); const lock=path.join(root,'locks/sample-replaced.lock'); const initial=JSON.parse(await fs.readFile(lock,'utf8')); if(!initial.token)throw new Error('owner token incomplete'); await fs.writeFile(lock,'replacement owner\\n'); await release(); process.stdout.write(await fs.readFile(lock,'utf8'));",
                serde_json::to_string(&published.package_root.join("dist/core/lock/lock.js"))?
            ),
        )?;
        let output = std::process::Command::new(
            std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned()),
        )
        .arg(driver)
        .arg(&pm_root)
        .env("DO_NOT_TRACK", "1")
        .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, replacement.as_bytes());
        fs::remove_file(lock_path)?;
    }
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

/// A deliberately paused owner exposes existence polling before token initialization.
#[cfg(target_os = "linux")]
#[test]
fn replacement_fixture_waits_for_initialized_owner() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Read as _;
    let (_directory, workspace) = tracker()?;
    let lock_path = workspace.pm_root().join("sample-delayed.lock");
    let journal = workspace.pm_root().join("delayed-recovery.fifo");
    fs::write(&lock_path, "")?;
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        &journal,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )?;
    let (sender, receiver) = std::sync::mpsc::channel();
    let writer_lock = lock_path.clone();
    let writer_journal = journal.clone();
    let replacer = thread::spawn(move || -> Result<(), String> {
        sender
            .send(rustix::thread::gettid().as_raw_nonzero().get())
            .map_err(|error| error.to_string())?;
        replace_lock_after_recovery_opens(&writer_lock, &writer_journal)
            .map_err(|error| error.to_string())
    });
    let tid = receiver.recv()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let wait = fs::read_to_string(format!("/proc/self/task/{tid}/wchan"))?;
        if matches!(wait.trim(), "wait_for_partner" | "pipe_wait") {
            break;
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!("replacer never entered FIFO rendezvous: {wait}").into());
        }
        // Back off between /proc reads; the rendezvous itself is still detected by state, not by timing.
        thread::sleep(std::time::Duration::from_millis(1));
    }
    let before_initialization = fs::read(&lock_path)?;
    fs::write(
        &lock_path,
        r#"{"id":"sample-replaced","token":"initialized-owner"}"#,
    )?;
    let mut bytes = String::new();
    fs::File::open(&journal)?.read_to_string(&mut bytes)?;
    replacer
        .join()
        .map_err(|_| "replacer panicked")?
        .map_err(std::io::Error::other)?;
    assert_eq!(
        before_initialization, b"",
        "fixture replaced the lock before its owner initialized it"
    );
    assert_eq!(bytes, "not json");
    assert_eq!(fs::read_to_string(lock_path)?, "replacement owner\n");
    Ok(())
}
