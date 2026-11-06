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

#[cfg(windows)]
#[path = "support/windows_fs.rs"]
mod windows_fs;

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

/// Claim lineage retains full-corpus duplicate and payload validation through the public SDK.
#[test]
fn ownership_parent_index_keeps_unrelated_validation() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, root) = tracker()?;
    let workspace = Workspace::discover(directory.path())?;
    let request = pm_rust::OwnershipItem {
        id: "demo-a".to_owned(),
        author: "fixture-agent".to_owned(),
        timestamp: Some("2026-08-22T10:00:00.000Z".to_owned()),
        message: None,
        force: false,
        if_available: false,
        provenance_role: None,
    };
    let original_item = fs::read(root.join("tasks/demo-a.toon"))?;
    write(root.join("tasks/unrelated.toon"), ITEM_B)?;
    assert!(matches!(
        workspace.claim_with_options(&request, &pm_rust::OwnershipOptions::default()),
        Err(PmRustError::DuplicateItemId { .. })
    ));
    assert_eq!(fs::read(root.join("tasks/demo-a.toon"))?, original_item);
    assert!(!root.join("history/demo-a.jsonl").exists());
    assert!(!root.join("runtime/session.json").exists());
    write(
        root.join("tasks/unrelated.toon"),
        "invalid unrelated payload",
    )?;
    assert!(matches!(
        workspace.claim_with_options(&request, &pm_rust::OwnershipOptions::default()),
        Err(PmRustError::InvalidItemDocument { .. })
    ));
    assert_eq!(fs::read(root.join("tasks/demo-a.toon"))?, original_item);
    assert!(!root.join("history/demo-a.jsonl").exists());
    assert!(!root.join("runtime/session.json").exists());
    fs::remove_file(root.join("tasks/unrelated.toon"))?;
    workspace.claim_with_options(&request, &pm_rust::OwnershipOptions::default())?;
    let state: Value = serde_json::from_slice(&fs::read(root.join("runtime/session.json"))?)?;
    assert!(state["semantic_attribution"].is_object());
    let claimed_item = fs::read(root.join("tasks/demo-a.toon"))?;
    let claimed_history = fs::read(root.join("history/demo-a.jsonl"))?;
    let claimed_session = fs::read(root.join("runtime/session.json"))?;
    for (payload, duplicate) in [(ITEM_B, true), ("invalid unrelated payload", false)] {
        write(root.join("tasks/unrelated.toon"), payload)?;
        let Err(error) =
            workspace.release_with_options(&request, &pm_rust::OwnershipOptions::default())
        else {
            return Err("invalid corpus must refuse before release".into());
        };
        if duplicate {
            assert!(matches!(error, PmRustError::DuplicateItemId { .. }));
        } else {
            assert!(matches!(error, PmRustError::InvalidItemDocument { .. }));
        }
        assert_eq!(fs::read(root.join("tasks/demo-a.toon"))?, claimed_item);
        assert_eq!(
            fs::read(root.join("history/demo-a.jsonl"))?,
            claimed_history
        );
        assert_eq!(
            fs::read(root.join("runtime/session.json"))?,
            claimed_session
        );
    }
    fs::remove_file(root.join("tasks/unrelated.toon"))?;
    workspace.release_with_options(&request, &pm_rust::OwnershipOptions::default())?;
    Ok(())
}

/// A filename alias absent from the validated ID index cannot leave a partial claim or release.
#[test]
fn ownership_missing_index_target_refuses_without_writes() -> Result<(), Box<dyn std::error::Error>>
{
    for release in [false, true] {
        let (directory, root) = tracker()?;
        let mut original = ITEM_A.replace("id: demo-a", "id: demo-real");
        if release {
            original.push_str("assignee: fixture-agent\nclaim_principal: fixture-agent\n");
        }
        write(root.join("tasks/demo-a.toon"), &original)?;
        let session = "{\"retained\":true}\n";
        write(root.join("runtime/session.json"), session)?;
        let workspace = Workspace::discover(directory.path())?;
        let request = pm_rust::OwnershipItem {
            id: "demo-a".to_owned(),
            author: "fixture-agent".to_owned(),
            timestamp: Some("2026-08-22T10:00:00.000Z".to_owned()),
            message: None,
            force: false,
            if_available: false,
            provenance_role: None,
        };
        let options = pm_rust::OwnershipOptions::default();
        let result = if release {
            workspace.release_with_options(&request, &options)
        } else {
            workspace.claim_with_options(&request, &options)
        };
        assert!(matches!(result, Err(PmRustError::ItemNotFound { .. })));
        assert_eq!(
            fs::read_to_string(root.join("tasks/demo-a.toon"))?,
            original
        );
        assert!(!root.join("history/demo-a.jsonl").exists());
        assert_eq!(
            fs::read_to_string(root.join("runtime/session.json"))?,
            session
        );
    }
    Ok(())
}

/// Every public read refuses a listing fault instead of fabricating an empty selection.
fn queries_refuse_broken_storage(workspace: &Workspace) -> Result<(), Box<dyn std::error::Error>> {
    let results = [
        workspace.get("demo-a").map(|_| ()),
        workspace.list(ItemFilter::default()).map(|_| ()),
        workspace
            .list_page(
                &ItemFilter::default(),
                &pm_rust::ListOptions::default(),
                "2026-08-22T10:00:00.000Z",
            )
            .map(|_| ()),
        workspace
            .context(
                &pm_rust::ContextOptions {
                    no_extension_health: true,
                    ..Default::default()
                },
                "2026-08-22T10:00:00.000Z",
            )
            .map(|_| ()),
        workspace
            .search("needle", &pm_rust::SearchOptions::default())
            .map(|_| ()),
    ];
    for result in results {
        if !matches!(result, Err(PmRustError::Io { .. })) {
            return Err("query did not preserve the real listing refusal".into());
        }
    }
    Ok(())
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
        nested.join(".."),
        root.clone(),
    ] {
        assert_eq!(Workspace::discover(&start)?.pm_root(), root);
    }
    Ok(())
}

#[test]
fn discovery_errors_are_typed() -> Result<(), Box<dyn std::error::Error>> {
    assert!(matches!(
        Workspace::discover(std::path::Path::new("")),
        Err(PmRustError::Io { source, .. }) if source.kind() == std::io::ErrorKind::InvalidInput
    ));
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
    #[cfg(windows)]
    {
        // Ordinary Windows paths resolve away `..` in absolute(). A verbatim
        // spelling retains it through discovery's fold, then Win32 refuses it
        // at canonicalize. Exercise the library and CLI builds, too.
        let mut spelling = fs::canonicalize(directory.path())?.into_os_string();
        spelling.push(r"\..");
        let parent = PathBuf::from(spelling);
        assert!(
            std::path::absolute(&parent)?
                .components()
                .any(|component| component == std::path::Component::ParentDir)
        );
        assert!(matches!(
            Workspace::discover(&parent),
            Err(PmRustError::Io { path, source })
                if path == parent && source.raw_os_error() == Some(123)
        ));
        Command::new(env!("CARGO_BIN_EXE_pm-rust"))
            .arg("--workspace")
            .arg(&parent)
            .arg("list")
            .assert()
            .code(2)
            .stderr(contains("filesystem operation failed"));
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn discovery_rejects_a_symlinked_tracker_root() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir()?;
    let workspace = directory.path().join("workspace");
    let external = directory.path().join("external-tracker");
    write(external.join("settings.json"), "{}\n")?;
    fs::create_dir_all(workspace.join(".agents"))?;
    symlink(&external, workspace.join(".agents/pm"))?;
    assert!(matches!(
        Workspace::discover(&workspace),
        Err(PmRustError::TrackerNotFound { .. })
    ));
    Ok(())
}

#[cfg(windows)]
#[test]
fn discovery_rejects_a_windows_symlinked_tracker_root() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::windows::fs::symlink_dir;

    let directory = tempfile::tempdir()?;
    let workspace = directory.path().join("workspace");
    let external = directory.path().join("external-tracker");
    write(external.join("settings.json"), "{}\n")?;
    fs::create_dir_all(workspace.join(".agents"))?;
    if !windows_fs::symlink_created(symlink_dir(&external, workspace.join(".agents/pm")))? {
        return Ok(());
    }
    assert!(matches!(
        Workspace::discover(&workspace),
        Err(PmRustError::TrackerNotFound { .. })
    ));
    Ok(())
}

#[test]
fn reads_sorts_filters_and_preserves_extension_fields() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, _) = tracker()?;
    let workspace = Workspace::discover(directory.path())?;
    let items = workspace.read_items()?;
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].metadata.id, "demo-a");
    assert_eq!(items[0].metadata.tags, Vec::<String>::new());
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
    assert_eq!(excluded.items, Vec::<pm_rust::ItemSummary>::new());
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
    assert_eq!(item.metadata.description, "");
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
    write(root.join("chores/demo-a.toon"), ITEM_A)?;
    assert!(matches!(
        Workspace::discover(directory.path())?.read_items(),
        Err(PmRustError::DuplicateItemId { id, .. }) if id == "demo-a"
    ));
    let workspace = Workspace::discover(directory.path())?;
    for index in 0..70 {
        let id = format!("tail-{index:03}");
        write(
            root.join(format!("tasks/{id}.toon")),
            &ITEM_A.replace("id: demo-a", &format!("id: {id}")),
        )?;
    }
    // A later worker read failure must not overtake the earlier duplicate refusal.
    fs::write(root.join("tasks/zzz-invalid.toon"), [0xff])?;
    assert!(matches!(
        workspace.list(ItemFilter::default()),
        Err(PmRustError::DuplicateItemId { id, .. }) if id == "demo-a"
    ));
    assert!(matches!(
        workspace.search("alpha", &pm_rust::SearchOptions::default()),
        Err(PmRustError::DuplicateItemId { id, .. }) if id == "demo-a"
    ));
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
    for result in [
        workspace
            .list_page(
                &ItemFilter::default(),
                &pm_rust::ListOptions::default(),
                "2026-08-22T10:00:00.000Z",
            )
            .map(|_| ()),
        workspace
            .context(
                &pm_rust::ContextOptions {
                    no_extension_health: true,
                    ..Default::default()
                },
                "2026-08-22T10:00:00.000Z",
            )
            .map(|_| ()),
        workspace
            .search("needle", &pm_rust::SearchOptions::default())
            .map(|_| ()),
    ] {
        assert!(matches!(result, Err(PmRustError::Io { .. })));
    }
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
    Command::new(env!("CARGO_BIN_EXE_pm-rust"))
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

#[cfg(windows)]
#[test]
fn reports_a_windows_exclusively_held_item_path() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::windows::fs::OpenOptionsExt;

    let (directory, root) = tracker()?;
    let path = root.join("tasks/demo-a.toon");
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&path)?;
    let result = Workspace::discover(directory.path())?.read_items();
    Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .arg("--workspace")
        .arg(directory.path())
        .arg("list")
        .assert()
        .code(2)
        .stderr(contains("filesystem operation failed"));
    assert!(matches!(result, Err(PmRustError::Io { path: failed, .. }) if failed == path));
    drop(held);
    assert_eq!(
        Workspace::discover(directory.path())?.read_items()?.len(),
        2
    );
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
    let workspace = Workspace::discover(directory.path())?;
    let result = workspace.read_items();
    let queries = queries_refuse_broken_storage(&workspace);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    queries?;
    assert!(matches!(
        result,
        Err(PmRustError::Io { path: failed, .. }) if failed == path
    ));
    Ok(())
}

#[cfg(windows)]
#[test]
fn reports_a_windows_exclusively_held_nested_directory() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, root) = tracker()?;
    let nested = root.join("tasks/nested/locked");
    fs::create_dir_all(&nested)?;
    let sentinel = nested.join("sentinel.txt");
    fs::write(&sentinel, "directory enumeration fixture")?;
    let held = windows_fs::exclusive_directory_handle(&nested)?;
    assert!(nested.is_dir());
    let error = fs::read_dir(&nested)
        .err()
        .ok_or("exclusively held directory listing succeeded")?;
    assert_eq!(error.raw_os_error(), Some(32), "{error:?}");
    let workspace = Workspace::discover(directory.path())?;
    let result = workspace.read_items();
    queries_refuse_broken_storage(&workspace)?;
    drop(held);
    assert!(matches!(result, Err(PmRustError::Io { path, .. }) if path == nested));
    assert_eq!(
        fs::read_to_string(&sentinel)?,
        "directory enumeration fixture"
    );
    assert_eq!(
        Workspace::discover(directory.path())?.read_items()?.len(),
        2
    );
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
    assert_eq!(
        Workspace::discover(directory.path())?
            .list(ItemFilter::default())?
            .total,
        2
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn ignores_windows_symlinked_item_directories() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::windows::fs::{symlink_dir, symlink_file};

    let (directory, root) = tracker()?;
    let external = directory.path().join("external");
    write(external.join("secret.toon"), ITEM_A)?;
    if !windows_fs::symlink_created(symlink_dir(&external, root.join("tasks/link")))? {
        return Ok(());
    }
    if !windows_fs::symlink_created(symlink_dir(&external, root.join("linked-items")))? {
        return Ok(());
    }
    if !windows_fs::symlink_created(symlink_file(
        external.join("secret.toon"),
        root.join("tasks/linked.toon"),
    ))? {
        return Ok(());
    }
    assert_eq!(
        Workspace::discover(directory.path())?.read_items()?.len(),
        2
    );
    assert_eq!(
        Workspace::discover(directory.path())?
            .list(ItemFilter::default())?
            .total,
        2
    );
    Ok(())
}

#[test]
fn cli_lists_gets_and_reports_failures() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, _) = tracker()?;
    let workspace = directory.path().to_string_lossy();
    let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .args([
            "--workspace",
            &workspace,
            "list",
            "--json",
            "--status",
            "open",
        ])
        .output()?;
    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(payload["count"], 1);
    assert_eq!(payload["items"][0]["id"], "demo-a");
    Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .args(["--workspace", &workspace, "get", "demo-b"])
        .assert()
        .success()
        .stdout(contains("\"id\": \"demo-b\"").and(contains("\"parent\": \"demo-a\"")));
    Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .args(["--workspace", &workspace, "get", "missing"])
        .assert()
        .code(2)
        .stderr(contains("pm item not found: missing"));
    Command::new(env!("CARGO_BIN_EXE_pm-rust"))
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

/// The production SDK preserves decoded-ID ordering and filters within a streamed corpus.
#[test]
fn sdk_streaming_sorts_filters_and_preserves_total() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, root) = tracker()?;
    let workspace = Workspace::discover(directory.path())?;
    let all = workspace.list(ItemFilter::default())?;
    assert_eq!(
        all.items
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        ["demo-a", "demo-b"]
    );
    let closed = workspace.list(ItemFilter {
        status: Some("closed".to_owned()),
        ..Default::default()
    })?;
    assert_eq!(closed.items.len(), 1);
    assert_eq!(closed.total, 2);
    let complete = workspace.list_page(
        &ItemFilter::default(),
        &pm_rust::ListOptions {
            all: true,
            ..Default::default()
        },
        "2026-08-22T10:00:00.000Z",
    )?;
    assert_eq!(complete["count"], 2);
    write(
        root.join("issues/nested/demo-b.toon"),
        &(ITEM_B
            .replace("parent: demo-a\n", "")
            .replace("status: closed", "status: open")
            + "dependencies[1]{id,kind}:\n  demo-a,child_of\n"),
    )?;
    let children = workspace.list_page(
        &ItemFilter::default(),
        &pm_rust::ListOptions {
            parent: Some("demo-a".to_owned()),
            ..Default::default()
        },
        "2026-08-22T10:00:00.000Z",
    )?;
    assert_eq!(children["count"], 1);
    assert_eq!(children["items"][0]["id"], "demo-b");
    write(
        root.join("issues/nested/demo-b.toon"),
        &ITEM_B.replace("status: closed", "status: open"),
    )?;
    let page = workspace.list_page(
        &ItemFilter {
            id: Some("demo-b".to_owned()),
            ..Default::default()
        },
        &pm_rust::ListOptions {
            tag: Some("beta".to_owned()),
            ..Default::default()
        },
        "2026-08-22T10:00:00.000Z",
    )?;
    assert_eq!(page["count"], 1);
    assert_eq!(page["items"][0]["id"], "demo-b");
    for offset in [None, Some("0".to_owned())] {
        assert!(matches!(
            workspace.list_page(
                &ItemFilter::default(),
                &pm_rust::ListOptions {
                    limit: Some("bad".to_owned()),
                    offset,
                    ..Default::default()
                },
                "2026-08-22T10:00:00.000Z",
            ),
            Err(PmRustError::InvalidReadRequest { .. })
        ));
    }
    Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .arg("--workspace")
        .arg(directory.path())
        .args(["list", "--limit", "bad", "--offset", "0"])
        .assert()
        .code(2)
        .stderr(contains("output control"));
    Ok(())
}

/// Raw reads and strict selected-document failures propagate through SDK iterators.
#[test]
fn sdk_streaming_preserves_invalid_utf8_refusals() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, root) = tracker()?;
    fs::write(root.join("tasks/demo-a.toon"), [0xff])?;
    let workspace = Workspace::discover(directory.path())?;
    for result in [
        workspace.get("demo-a").map(|_| ()),
        workspace.list(ItemFilter::default()).map(|_| ()),
        workspace
            .list_page(
                &ItemFilter::default(),
                &pm_rust::ListOptions::default(),
                "2026-08-22T10:00:00.000Z",
            )
            .map(|_| ()),
        workspace
            .context(
                &pm_rust::ContextOptions {
                    no_extension_health: true,
                    ..Default::default()
                },
                "2026-08-22T10:00:00.000Z",
            )
            .map(|_| ()),
        workspace
            .search("needle", &pm_rust::SearchOptions::default())
            .map(|_| ()),
    ] {
        assert!(
            matches!(result, Err(PmRustError::Io { source, .. }) if source.kind() == std::io::ErrorKind::InvalidData)
        );
    }
    write(root.join("tasks/demo-a.toon"), "<<<<<<< damaged\n")?;
    assert!(matches!(
        workspace.list_page(
            &ItemFilter {
                id: Some("demo-a".to_owned()),
                ..Default::default()
            },
            &pm_rust::ListOptions::default(),
            "2026-08-22T10:00:00.000Z",
        ),
        Err(PmRustError::InvalidItemDocument { .. })
    ));
    Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .arg("--workspace")
        .arg(directory.path())
        .args(["list", "--id", "demo-a"])
        .assert()
        .code(2)
        .stderr(contains("merge conflict markers detected"));
    assert!(matches!(
        workspace.list(ItemFilter::default()),
        Err(PmRustError::InvalidItemDocument { .. })
    ));
    assert!(matches!(
        workspace.search("needle", &pm_rust::SearchOptions::default()),
        Err(PmRustError::InvalidItemDocument { .. })
    ));
    Ok(())
}

/// Selected search payloads retain strict decoding after a successful metadata filter.
#[test]
fn sdk_search_strict_payload_failure_after_filter() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, root) = tracker()?;
    write(
        root.join("tasks/demo-a.toon"),
        &format!("{ITEM_A}extra_invalid[2]: value\n"),
    )?;
    let workspace = Workspace::discover(directory.path())?;
    let options = pm_rust::SearchOptions {
        status: Some("open".to_owned()),
        ..Default::default()
    };
    assert!(matches!(
        workspace.search("alpha", &options),
        Err(PmRustError::InvalidItemDocument { .. })
    ));
    Ok(())
}
