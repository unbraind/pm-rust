//! Black-box acceptance tests for the first read-only compatibility slice.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use assert_cmd::Command;
use pm_rust::{ItemFilter, PmRustError, Workspace};
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use proptest::prelude::*;
use serde_json::Value;
use tempfile::TempDir;

const ITEM_A: &str = r#"id: demo-a
title: Alpha
description: ""
type: Task
status: open
priority: 1
tags: []
created_at: "2026-08-06T00:00:00.000Z"
updated_at: "2026-08-06T00:00:00.000Z"
extension_value: retained
body: "alpha body"
"#;

const ITEM_B: &str = r#"id: demo-b
title: Beta
description: second
type: Issue
status: closed
priority: 2
tags[1]: beta
created_at: "2026-08-06T00:00:00.000Z"
updated_at: "2026-08-06T01:00:00.000Z"
parent: demo-a
body: ""
"#;

fn write(path: impl AsRef<Path>, content: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.as_ref().parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(())
}

fn tracker() -> Result<(TempDir, PathBuf), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    write(root.join("settings.json"), "{}\n")?;
    write(root.join("tasks/demo-a.toon"), ITEM_A)?;
    write(root.join("issues/nested/demo-b.toon"), ITEM_B)?;
    write(root.join("history/ignored.toon"), "not: an item\n")?;
    write(root.join("tasks/ignored.txt"), "not toon\n")?;
    Ok((directory, fs::canonicalize(root)?))
}

#[test]
fn discovers_workspace_tracker_nested_path_and_file() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, root) = tracker()?;
    let nested = directory.path().join("src/nested");
    write(nested.join("input.txt"), "fixture")?;
    for start in [
        directory.path().to_path_buf(),
        nested.clone(),
        nested.join("input.txt"),
        root.clone(),
    ] {
        assert_eq!(Workspace::discover(&start)?.pm_root(), root);
    }
    Ok(())
}

#[test]
fn discovery_errors_are_typed() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let missing = directory.path().join("missing");
    assert!(matches!(
        Workspace::discover(&missing),
        Err(PmRustError::Io { path, .. }) if path == missing
    ));
    assert!(matches!(
        Workspace::discover(directory.path()),
        Err(PmRustError::TrackerNotFound { .. })
    ));
    for candidate in [
        directory.path().join("pm"),
        directory.path().join("tracker"),
    ] {
        write(candidate.join("settings.json"), "{}")?;
        assert!(matches!(
            Workspace::discover(&candidate),
            Err(PmRustError::TrackerNotFound { .. })
        ));
    }
    Ok(())
}

#[test]
fn reads_sorts_filters_and_preserves_extension_fields() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, _) = tracker()?;
    let workspace = Workspace::discover(directory.path())?;
    let items = workspace.read_items()?;
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].metadata.id, "demo-a");
    assert!(items[0].metadata.tags.is_empty());
    assert_eq!(items[0].metadata.extra["extension_value"], "retained");
    assert_eq!(items[1].metadata.parent.as_deref(), Some("demo-a"));

    let result = workspace.list(ItemFilter {
        status: Some("open".to_owned()),
        item_type: Some("task".to_owned()),
        id: Some("demo-a".to_owned()),
    })?;
    assert_eq!((result.count, result.total), (1, 2));
    assert_eq!(result.items[0].id, "demo-a");
    let excluded = workspace.list(ItemFilter {
        status: Some("closed".to_owned()),
        item_type: Some("Task".to_owned()),
        id: None,
    })?;
    assert!(excluded.items.is_empty());
    assert_eq!(workspace.get("demo-b")?.body, "");
    assert!(matches!(
        workspace.get("unknown"),
        Err(PmRustError::ItemNotFound { id }) if id == "unknown"
    ));
    Ok(())
}

#[test]
fn defaults_an_omitted_description_and_preserves_literal_array_text()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    write(root.join("settings.json"), "{}\n")?;
    write(
        root.join("tasks/demo-a.toon"),
        &ITEM_A.replacen("description: \"\"\n", "", 1).replacen(
            "body: \"alpha body\"",
            "body: \"literal: []\"",
            1,
        ),
    )?;
    let item = Workspace::discover(directory.path())?.get("demo-a")?;
    assert!(item.metadata.description.is_empty());
    assert_eq!(item.body, "literal: []");
    Ok(())
}

proptest! {
    #[test]
    fn exact_id_filter_never_returns_a_different_item(candidate in "[a-z0-9-]{1,24}") {
        let (directory, _) = tracker().map_err(|error| TestCaseError::fail(error.to_string()))?;
        let workspace = Workspace::discover(directory.path())
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        let result = workspace.list(ItemFilter {
            id: Some(candidate.clone()),
            ..ItemFilter::default()
        }).map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert!(result.items.iter().all(|item| item.id == candidate));
    }
}

#[test]
fn rejects_invalid_documents() -> Result<(), Box<dyn std::error::Error>> {
    let owned_cases = [
        (
            ITEM_A.replacen("id: demo-a", "id: \"\"", 1),
            "required field id is empty",
        ),
        (
            ITEM_A.replacen("title: Alpha", "title: \"\"", 1),
            "required field title is empty",
        ),
        (
            ITEM_A.replacen("type: Task", "type: \"\"", 1),
            "required field type is empty",
        ),
        (
            ITEM_A.replacen("status: open", "status: \"\"", 1),
            "required field status is empty",
        ),
        (
            ITEM_A.replacen(
                "created_at: \"2026-08-06T00:00:00.000Z\"",
                "created_at: \"\"",
                1,
            ),
            "required field created_at is empty",
        ),
        (
            ITEM_A.replacen(
                "updated_at: \"2026-08-06T00:00:00.000Z\"",
                "updated_at: \"\"",
                1,
            ),
            "required field updated_at is empty",
        ),
        (
            ITEM_A.replacen("priority: 1", "priority: 5", 1),
            "priority must be between 0 and 4",
        ),
    ];
    let mut cases = vec![
        (
            "<<<<<<< ours\n".to_owned(),
            "merge conflict markers detected",
        ),
        ("=======\n".to_owned(), "merge conflict markers detected"),
        (
            ">>>>>>> theirs\n".to_owned(),
            "merge conflict markers detected",
        ),
        (
            "=======not-a-conflict-marker\n".to_owned(),
            "TOON decode failed",
        ),
        ("  <<<<<<< indented\n".to_owned(), "TOON decode failed"),
        ("not valid toon [\n".to_owned(), "TOON decode failed"),
    ];
    cases.extend(owned_cases);
    for (index, (content, expected)) in cases.iter().enumerate() {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join(".agents/pm");
        write(root.join("settings.json"), "{}")?;
        write(root.join(format!("tasks/case-{index}.toon")), content)?;
        let error = Workspace::discover(directory.path())?
            .read_items()
            .err()
            .ok_or("invalid fixture unexpectedly passed")?;
        assert!(error.to_string().contains(expected), "{error}");
    }
    Ok(())
}

#[test]
fn rejects_duplicate_ids_and_deleted_tracker() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, root) = tracker()?;
    write(root.join("chores/duplicate.toon"), ITEM_A)?;
    assert!(matches!(
        Workspace::discover(directory.path())?.read_items(),
        Err(PmRustError::DuplicateItemId { id, .. }) if id == "demo-a"
    ));
    let workspace = Workspace::discover(directory.path())?;
    fs::remove_dir_all(&root)?;
    assert!(matches!(
        workspace.read_items(),
        Err(PmRustError::Io { .. })
    ));
    assert!(matches!(
        workspace.list(ItemFilter::default()),
        Err(PmRustError::Io { .. })
    ));
    assert!(matches!(
        workspace.get("demo-a"),
        Err(PmRustError::Io { .. })
    ));
    Ok(())
}

#[cfg(unix)]
#[test]
fn reports_an_unreadable_item_path() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    if rustix::process::geteuid().is_root() {
        return Ok(());
    }
    let (directory, root) = tracker()?;
    let path = root.join("tasks/demo-a.toon");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000))?;
    let result = Workspace::discover(directory.path())?.read_items();
    Command::cargo_bin("pm-rust")?
        .args(["--workspace", &directory.path().to_string_lossy(), "list"])
        .assert()
        .code(2)
        .stderr(contains("filesystem operation failed"));
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    assert!(matches!(
        result,
        Err(PmRustError::Io { path: failed, .. }) if failed == path
    ));
    Ok(())
}

#[cfg(unix)]
#[test]
fn reports_an_unreadable_nested_item_directory() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    if rustix::process::geteuid().is_root() {
        return Ok(());
    }
    let (directory, root) = tracker()?;
    let path = root.join("tasks/nested/locked");
    fs::create_dir_all(&path)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000))?;
    let result = Workspace::discover(directory.path())?.read_items();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    assert!(matches!(
        result,
        Err(PmRustError::Io { path: failed, .. }) if failed == path
    ));
    Ok(())
}

#[cfg(unix)]
#[test]
fn ignores_symlinked_item_directories() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;
    use std::os::unix::net::UnixListener;
    let (directory, root) = tracker()?;
    let external = directory.path().join("external");
    write(external.join("secret.toon"), ITEM_A)?;
    symlink(&external, root.join("tasks/link"))?;
    symlink(&external, root.join("linked-items"))?;
    let _listener = UnixListener::bind(root.join("tasks/read-side.sock"))?;
    assert_eq!(
        Workspace::discover(directory.path())?.read_items()?.len(),
        2
    );
    Ok(())
}

#[test]
fn cli_lists_gets_and_reports_failures() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, _) = tracker()?;
    let workspace = directory.path().to_string_lossy();
    let output = Command::cargo_bin("pm-rust")?
        .args(["--workspace", &workspace, "list", "--status", "open"])
        .output()?;
    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(payload["count"], 1);
    assert_eq!(payload["items"][0]["id"], "demo-a");
    Command::cargo_bin("pm-rust")?
        .args(["--workspace", &workspace, "get", "demo-b"])
        .assert()
        .success()
        .stdout(contains("\"id\": \"demo-b\"").and(contains("\"parent\": \"demo-a\"")));
    Command::cargo_bin("pm-rust")?
        .args(["--workspace", &workspace, "get", "missing"])
        .assert()
        .code(2)
        .stderr(contains("pm item not found: missing"));
    Command::cargo_bin("pm-rust")?
        .args(["--workspace", "/path/that/does/not/exist", "list"])
        .assert()
        .code(2)
        .stderr(contains("filesystem operation failed"));
    Ok(())
}

#[test]
fn cli_reports_a_closed_stdout_pipe() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, root) = tracker()?;
    for index in 0..2_000 {
        write(
            root.join(format!("tasks/generated-{index}.toon")),
            &ITEM_A.replacen("id: demo-a", &format!("id: generated-{index}"), 1),
        )?;
    }
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .args(["--workspace", &directory.path().to_string_lossy(), "list"])
        .stdout(Stdio::piped())
        .spawn()?;
    drop(child.stdout.take());
    assert_eq!(child.wait()?.code(), Some(2));

    write(
        root.join("tasks/demo-a.toon"),
        &ITEM_A.replacen(
            "body: \"alpha body\"",
            &format!("body: \"{}\"", "x".repeat(200_000)),
            1,
        ),
    )?;
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .args([
            "--workspace",
            &directory.path().to_string_lossy(),
            "get",
            "demo-a",
        ])
        .stdout(Stdio::piped())
        .spawn()?;
    drop(child.stdout.take());
    assert_eq!(child.wait()?.code(), Some(2));
    Ok(())
}
