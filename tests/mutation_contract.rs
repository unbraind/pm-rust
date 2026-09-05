//! Black-box and SDK acceptance for native mutations against published pm.
//!
//! Item and patch fixtures retain their original deterministic workspace
//! recipe. Their event classification and record-integrity envelopes were
//! sealed using the published PM CLI 2026.9.4 implementation. The independent
//! live differential suite additionally compares complete persisted bytes.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use pm_rust::{CloseItem, CommentItem, CreateItem, PmRustError, UpdateItem, Workspace};
use predicates::str::contains;
use tempfile::TempDir;

const TIMESTAMP: &str = "2026-08-22T10:00:00.000Z";

/// Canonical item bytes recorded after the recorded create step.
const CREATE_ITEM_BYTES: &str = concat!(
    "id: sample-conv\n",
    "title: Conformance item\n",
    "description: First desc\n",
    "type: Task\n",
    "status: open\n",
    "priority: 2\n",
    "tags[2]: alpha,beta\n",
    "created_at: \"2026-08-22T10:00:00.000Z\"\n",
    "updated_at: \"2026-08-22T10:00:00.000Z\"\n",
    "author: fixture-agent\n",
    "body: Original body\n",
);

/// Canonical history line recorded after the recorded create step.
const CREATE_HISTORY_LINE: &str = concat!(
    r#"{"ts":"2026-08-22T10:00:00.000Z","author":"fixture-agent","author_source":"asserted","agent_provenance":{"role":{"value":"implementer","source":"argv"}},"op":"create","patch":[{"op":"replace","path":"/body","value":"Original body"},{"op":"add","path":"/metadata/id","value":"sample-conv"},{"op":"add","path":"/metadata/title","value":"Conformance item"},{"op":"add","path":"/metadata/description","value":"First desc"},{"op":"add","path":"/metadata/type","value":"Task"},{"op":"add","path":"/metadata/status","value":"open"},{"op":"add","path":"/metadata/priority","value":2},{"op":"add","path":"/metadata/tags","value":["alpha","beta"]},{"op":"add","path":"/metadata/created_at","value":"2026-08-22T10:00:00.000Z"},{"op":"add","path":"/metadata/updated_at","value":"2026-08-22T10:00:00.000Z"},{"op":"add","path":"/metadata/author","value":"fixture-agent"}],"before_hash":"3cc22dff72be7b14824654a7a64ea62b04799939b2fee54c1b5f52ca60bf6df0","after_hash":"ce63b69e6445b50ae43919f31607098a4e414350c8ca52003bd84ea609f979bf","item_hash_version":3,"message":"","event_class":"substantive","record_hash_version":1,"record_hash":"e750549a13e46de26f4946c140a9c23055e956e609d0c38d9d0b67019dc152b2"}"#,
    "\n",
);

/// Canonical item bytes recorded after the recorded update step.
const UPDATE_ITEM_BYTES: &str = concat!(
    "id: sample-conv\n",
    "title: Renamed item\n",
    "description: First desc\n",
    "type: Task\n",
    "status: open\n",
    "priority: 3\n",
    "tags[2]: alpha,beta\n",
    "created_at: \"2026-08-22T10:00:00.000Z\"\n",
    "updated_at: \"2026-08-22T10:00:00.000Z\"\n",
    "author: fixture-agent\n",
    "body: Original body\n",
);

/// Canonical history line recorded after the recorded update step.
const UPDATE_HISTORY_LINE: &str = concat!(
    r#"{"ts":"2026-08-22T10:00:00.000Z","author":"fixture-agent","author_source":"asserted","agent_provenance":{"role":{"value":"implementer","source":"argv"}},"op":"update","patch":[{"op":"replace","path":"/metadata/priority","value":3},{"op":"replace","path":"/metadata/title","value":"Renamed item"}],"before_hash":"ce63b69e6445b50ae43919f31607098a4e414350c8ca52003bd84ea609f979bf","after_hash":"dc48d8c5971803ef643ec17734542814d3064982d1bc1b8d585d539d2266459c","item_hash_version":3,"message":"rename and reprioritize","event_class":"substantive","record_hash_version":1,"record_hash":"69a7a2e9b4c7de6326deaac238ddfe6b3fee6df1cfefc3f5fabf18a6c5b3a1c0"}"#,
    "\n",
);

/// Canonical history line recorded after the recorded comment step.
const COMMENT_HISTORY_LINE: &str = concat!(
    r#"{"ts":"2026-08-22T10:00:00.000Z","author":"fixture-agent","author_source":"asserted","op":"comment_add","patch":[{"op":"add","path":"/metadata/comments","value":[{"created_at":"2026-08-22T10:00:00.000Z","author":"fixture-agent","text":"First native note"}]}],"before_hash":"dc48d8c5971803ef643ec17734542814d3064982d1bc1b8d585d539d2266459c","after_hash":"2f2d4a1680bcbb9dbf4e570ea50ceedbd7807a5f7d3c514bf8a4187b99fa37c0","item_hash_version":3,"message":"note recorded","event_class":"substantive","record_hash_version":1,"record_hash":"3158c03422db29673bfcdcb0c1765fb3acba048ed11e2ead4ed9f55931379dd0"}"#,
    "\n",
);

/// Canonical history line recorded after the recorded status update step.
const STATUS_UPDATE_HISTORY_LINE: &str = concat!(
    r#"{"ts":"2026-08-22T10:00:00.000Z","author":"fixture-agent","author_source":"asserted","agent_provenance":{"role":{"value":"implementer","source":"argv"}},"op":"update","patch":[{"op":"replace","path":"/metadata/status","value":"in_progress"}],"before_hash":"2f2d4a1680bcbb9dbf4e570ea50ceedbd7807a5f7d3c514bf8a4187b99fa37c0","after_hash":"e7f7fb53e6330e7ffacbf15a870ad604a2e8d5236e67ac6c9e373b8ea4d6a0d0","item_hash_version":3,"event_class":"substantive","record_hash_version":1,"record_hash":"e5df5d617800b1e89f5a808a647372fdb90c4030121c166eb4588b93515b19cb"}"#,
    "\n",
);

/// Canonical item bytes recorded after the recorded close step.
const CLOSE_ITEM_BYTES: &str = concat!(
    "id: sample-conv\n",
    "title: Renamed item\n",
    "description: First desc\n",
    "type: Task\n",
    "status: closed\n",
    "priority: 3\n",
    "tags[2]: alpha,beta\n",
    "created_at: \"2026-08-22T10:00:00.000Z\"\n",
    "updated_at: \"2026-08-22T10:00:00.000Z\"\n",
    "closed_at: \"2026-08-22T10:00:00.000Z\"\n",
    "completed_at: \"2026-08-22T10:00:00.000Z\"\n",
    "author: fixture-agent\n",
    "comments[1]{created_at,author,text}:\n",
    "  \"2026-08-22T10:00:00.000Z\",fixture-agent,First native note\n",
    "close_reason: conformance complete\n",
    "body: Original body\n",
);

/// Canonical history line recorded after the recorded close step.
const CLOSE_HISTORY_LINE: &str = concat!(
    r#"{"ts":"2026-08-22T10:00:00.000Z","author":"fixture-agent","author_source":"asserted","agent_provenance":{"role":{"value":"implementer","source":"argv"}},"op":"close","patch":[{"op":"replace","path":"/metadata/status","value":"closed"},{"op":"add","path":"/metadata/closed_at","value":"2026-08-22T10:00:00.000Z"},{"op":"add","path":"/metadata/completed_at","value":"2026-08-22T10:00:00.000Z"},{"op":"add","path":"/metadata/close_reason","value":"conformance complete"}],"before_hash":"e7f7fb53e6330e7ffacbf15a870ad604a2e8d5236e67ac6c9e373b8ea4d6a0d0","after_hash":"2319c2cdf7e8164348a3235c87426c55dbf49aaeb286cc4893494b2b3b6eb6a8","item_hash_version":3,"event_class":"substantive","record_hash_version":1,"record_hash":"145aef732674f6a4f9fd28245c94ed6314957f4945c5eae95c860cc55a2f44fe"}"#,
    "\n",
);

/// Builds one fresh tracker fixture with the canonical sample settings.
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

/// Builds the canonical SDK create request used by the recorded sequence.
fn create_request() -> CreateItem {
    CreateItem {
        id: "sample-conv".to_owned(),
        title: "Conformance item".to_owned(),
        description: "First desc".to_owned(),
        item_type: "Task".to_owned(),
        status: "open".to_owned(),
        priority: 2,
        tags: ["alpha", "beta"].map(str::to_owned).to_vec(),
        body: "Original body".to_owned(),
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        message: None,
        provenance_role: Some("implementer".to_owned()),
        force_stale_lock: false,
    }
}

/// Builds the canonical SDK update request used by the recorded sequence.
fn update_request() -> UpdateItem {
    UpdateItem {
        id: "sample-conv".to_owned(),
        title: Some("Renamed item".to_owned()),
        description: None,
        status: None,
        priority: Some(3),
        tags: None,
        body: None,
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        message: Some("rename and reprioritize".to_owned()),
        provenance_role: Some("implementer".to_owned()),
        force_stale_lock: false,
    }
}

/// Builds the canonical SDK comment request used by the recorded sequence.
fn comment_request() -> CommentItem {
    CommentItem {
        id: "sample-conv".to_owned(),
        text: "First native note".to_owned(),
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        message: Some("note recorded".to_owned()),
        provenance_role: None,
        force_stale_lock: false,
    }
}

/// Builds the canonical SDK status-only update used by the recorded sequence.
fn status_update_request() -> UpdateItem {
    UpdateItem {
        id: "sample-conv".to_owned(),
        title: None,
        description: None,
        status: Some("in_progress".to_owned()),
        priority: None,
        tags: None,
        body: None,
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        message: None,
        provenance_role: Some("implementer".to_owned()),
        force_stale_lock: false,
    }
}

/// Replays the recorded sequence prefix through `step` so every byte-exact
/// test starts from the same durable state. `step` selects the last
/// operation to apply: `create`, `update`, `comment`, or `status-update`.
fn advance_to(workspace: &Workspace, step: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Reject an unknown step rather than falling through to the full sequence.
    // A typo such as "comment " previously replayed four mutations instead of
    // three, and the byte-exact assertion that followed compared against the
    // wrong durable state without naming the cause.
    const STEPS: [&str; 4] = ["create", "update", "comment", "status-update"];
    let Some(target) = STEPS.iter().position(|candidate| *candidate == step) else {
        return Err(format!("unknown advance_to step {step:?}; expected one of {STEPS:?}").into());
    };

    workspace.create(create_request())?;
    if target == 0 {
        return Ok(());
    }
    workspace.update(update_request())?;
    if target == 1 {
        return Ok(());
    }
    workspace.comment(&comment_request())?;
    if target == 2 {
        return Ok(());
    }
    workspace.update(status_update_request())?;
    Ok(())
}

/// Reads the stored history stream for the sample item.
fn history(root: &Path) -> String {
    fs::read_to_string(root.join(".agents/pm/history/sample-conv.jsonl")).unwrap_or_default()
}

/// Reads the stored item document for the sample item.
fn item(root: &Path) -> String {
    fs::read_to_string(root.join(".agents/pm/tasks/sample-conv.toon")).unwrap_or_default()
}

#[test]
/// Proves the native create transaction emits the recorded v2 history bytes.
fn native_create_matches_the_published_history_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, workspace) = tracker()?;
    let result = workspace.create(create_request())?;
    assert_eq!(
        result.after_hash,
        "ce63b69e6445b50ae43919f31607098a4e414350c8ca52003bd84ea609f979bf"
    );
    assert_eq!(item(directory.path()), CREATE_ITEM_BYTES);
    assert_eq!(history(directory.path()), CREATE_HISTORY_LINE);
    Ok(())
}

#[test]
/// Proves a native update writes the recorded item and history bytes exactly.
fn native_update_matches_the_published_bytes_exactly() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, workspace) = tracker()?;
    workspace.create(create_request())?;
    let result = workspace.update(update_request())?;
    assert_eq!(
        result.after_hash,
        "dc48d8c5971803ef643ec17734542814d3064982d1bc1b8d585d539d2266459c"
    );
    assert_eq!(item(directory.path()), UPDATE_ITEM_BYTES);
    assert_eq!(
        history(directory.path()),
        CREATE_HISTORY_LINE.to_owned() + UPDATE_HISTORY_LINE
    );
    Ok(())
}

#[test]
/// Proves a native comment append writes the recorded history bytes exactly.
fn native_comment_matches_the_published_bytes_exactly() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, workspace) = tracker()?;
    advance_to(&workspace, "update")?;
    let result = workspace.comment(&comment_request())?;
    assert_eq!(
        result.after_hash,
        "2f2d4a1680bcbb9dbf4e570ea50ceedbd7807a5f7d3c514bf8a4187b99fa37c0"
    );
    assert_eq!(
        history(directory.path()),
        CREATE_HISTORY_LINE.to_owned() + UPDATE_HISTORY_LINE + COMMENT_HISTORY_LINE
    );
    Ok(())
}

#[test]
/// Proves a status-only update omits the history message key like pm does.
fn native_status_update_omits_an_absent_message_key() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, workspace) = tracker()?;
    advance_to(&workspace, "comment")?;
    workspace.update(status_update_request())?;
    assert_eq!(
        history(directory.path()),
        CREATE_HISTORY_LINE.to_owned()
            + UPDATE_HISTORY_LINE
            + COMMENT_HISTORY_LINE
            + STATUS_UPDATE_HISTORY_LINE
    );
    Ok(())
}

#[test]
/// Proves a native close writes the recorded item and history bytes exactly.
fn native_close_matches_the_published_bytes_exactly() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, workspace) = tracker()?;
    advance_to(&workspace, "status-update")?;
    let result = workspace.close(CloseItem {
        id: "sample-conv".to_owned(),
        reason: "conformance complete".to_owned(),
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        provenance_role: Some("implementer".to_owned()),
        force_stale_lock: false,
    })?;
    assert_eq!(
        result.after_hash,
        "2319c2cdf7e8164348a3235c87426c55dbf49aaeb286cc4893494b2b3b6eb6a8"
    );
    assert_eq!(item(directory.path()), CLOSE_ITEM_BYTES);
    assert_eq!(
        history(directory.path()),
        CREATE_HISTORY_LINE.to_owned()
            + UPDATE_HISTORY_LINE
            + COMMENT_HISTORY_LINE
            + STATUS_UPDATE_HISTORY_LINE
            + CLOSE_HISTORY_LINE
    );
    Ok(())
}

#[test]
/// Proves the CLI exposes update, comment, and close as black-box commands.
fn cli_mutations_drive_the_same_native_transactions() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, _workspace) = tracker()?;
    let workspace_arg = format!("--workspace={}", directory.path().display());
    Command::cargo_bin("pm-rust")?
        .args([
            workspace_arg.as_str(),
            "create",
            "--id",
            "sample-conv",
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
            "--timestamp",
            TIMESTAMP,
        ])
        .assert()
        .success();
    Command::cargo_bin("pm-rust")?
        .args([
            workspace_arg.as_str(),
            "update",
            "sample-conv",
            "--title",
            "Renamed item",
            "--priority",
            "3",
            "--author",
            "fixture-agent",
            "--message",
            "rename and reprioritize",
            "--timestamp",
            TIMESTAMP,
        ])
        .assert()
        .success();
    Command::cargo_bin("pm-rust")?
        .args([
            workspace_arg.as_str(),
            "comment",
            "sample-conv",
            "First native note",
            "--author",
            "fixture-agent",
            "--message",
            "note recorded",
            "--timestamp",
            TIMESTAMP,
        ])
        .assert()
        .success();
    Command::cargo_bin("pm-rust")?
        .args([
            workspace_arg.as_str(),
            "update",
            "sample-conv",
            "--status",
            "in_progress",
            "--author",
            "fixture-agent",
            "--timestamp",
            TIMESTAMP,
        ])
        .assert()
        .success();
    Command::cargo_bin("pm-rust")?
        .args([
            workspace_arg.as_str(),
            "close",
            "sample-conv",
            "--reason",
            "conformance complete",
            "--author",
            "fixture-agent",
            "--timestamp",
            TIMESTAMP,
        ])
        .assert()
        .success();
    assert_eq!(item(directory.path()), CLOSE_ITEM_BYTES);
    assert_eq!(
        history(directory.path()),
        CREATE_HISTORY_LINE.to_owned()
            + UPDATE_HISTORY_LINE
            + COMMENT_HISTORY_LINE
            + STATUS_UPDATE_HISTORY_LINE
            + CLOSE_HISTORY_LINE
    );
    Ok(())
}

#[test]
#[allow(clippy::too_many_lines)]
/// Proves mutation guards refuse unknown items and empty required inputs.
fn mutation_guards_reject_invalid_requests() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, workspace) = tracker()?;
    let missing = workspace.update(UpdateItem {
        id: "sample-missing".to_owned(),
        title: Some("New title".to_owned()),
        description: None,
        status: None,
        priority: None,
        tags: None,
        body: None,
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    });
    assert!(matches!(missing, Err(PmRustError::ItemNotFound { .. })));

    let missing_comment = workspace.comment(&CommentItem {
        id: "sample-missing".to_owned(),
        text: "Orphan note".to_owned(),
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    });
    assert!(matches!(
        missing_comment,
        Err(PmRustError::ItemNotFound { .. })
    ));

    let missing_close = workspace.close(CloseItem {
        id: "sample-missing".to_owned(),
        reason: "orphan close".to_owned(),
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        provenance_role: None,
        force_stale_lock: false,
    });
    assert!(matches!(
        missing_close,
        Err(PmRustError::ItemNotFound { .. })
    ));

    workspace.create(create_request())?;
    let empty = workspace.update(UpdateItem {
        id: "sample-conv".to_owned(),
        title: None,
        description: None,
        status: None,
        priority: None,
        tags: None,
        body: None,
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    });
    assert!(matches!(
        empty,
        Err(PmRustError::InvalidMutationRequest { .. })
    ));

    let no_reason = workspace.close(CloseItem {
        id: "sample-conv".to_owned(),
        reason: "  ".to_owned(),
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        provenance_role: None,
        force_stale_lock: false,
    });
    assert!(matches!(
        no_reason,
        Err(PmRustError::InvalidMutationRequest { .. })
    ));

    let empty_comment = workspace.comment(&CommentItem {
        id: "sample-conv".to_owned(),
        text: String::new(),
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    });
    assert!(matches!(
        empty_comment,
        Err(PmRustError::InvalidMutationRequest { .. })
    ));

    let bad_timestamp = workspace.update(UpdateItem {
        id: "sample-conv".to_owned(),
        title: Some("Later".to_owned()),
        description: None,
        status: None,
        priority: None,
        tags: None,
        body: None,
        author: "fixture-agent".to_owned(),
        timestamp: Some("not-a-timestamp".to_owned()),
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    });
    // A malformed timestamp on an in-place mutation reports the mutation
    // variant, not the create variant, so the failure reads as `invalid
    // mutation request`.
    assert!(matches!(
        bad_timestamp,
        Err(PmRustError::InvalidMutationRequest { .. })
    ));

    // An out-of-range priority is refused before any durable state moves.
    let bad_priority = workspace.update(UpdateItem {
        id: "sample-conv".to_owned(),
        title: None,
        description: None,
        status: None,
        priority: Some(5),
        tags: None,
        body: None,
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    });
    assert!(matches!(
        bad_priority,
        Err(PmRustError::InvalidMutationRequest { .. })
    ));

    // Whitespace-only required fields are refused like absent ones.
    let blank_title = workspace.update(UpdateItem {
        id: "sample-conv".to_owned(),
        title: Some("   ".to_owned()),
        description: None,
        status: None,
        priority: None,
        tags: None,
        body: None,
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    });
    assert!(matches!(
        blank_title,
        Err(PmRustError::InvalidMutationRequest { .. })
    ));

    let blank_status = workspace.update(UpdateItem {
        id: "sample-conv".to_owned(),
        title: None,
        description: None,
        status: Some(String::new()),
        priority: None,
        tags: None,
        body: None,
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    });
    assert!(matches!(
        blank_status,
        Err(PmRustError::InvalidMutationRequest { .. })
    ));

    // A blank author is refused before any durable state moves.
    let blank_author = workspace.update(UpdateItem {
        id: "sample-conv".to_owned(),
        title: Some("Author test".to_owned()),
        description: None,
        status: None,
        priority: None,
        tags: None,
        body: None,
        author: "   ".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    });
    assert!(matches!(
        blank_author,
        Err(PmRustError::InvalidMutationRequest { .. })
    ));
    Ok(())
}

#[test]
/// Proves a conflicting durable journal refuses every in-place mutation entry point.
fn a_conflicting_journal_refuses_update_comment_and_close() -> Result<(), Box<dyn std::error::Error>>
{
    let (directory, workspace) = tracker()?;
    let root = directory.path();
    workspace.create(create_request())?;
    let item_bytes = item(root);

    // Plant one diverged journal per operation: the journalled bytes differ
    // from the durable item, so recovery must refuse instead of guessing.
    let transactions = root.join(".agents/pm/runtime/transactions");
    fs::create_dir_all(&transactions)?;
    for operation in ["update", "comment", "close"] {
        fs::write(
            transactions.join(format!("{operation}-sample-conv.json")),
            serde_json::json!({
                "version": 1,
                "id": "sample-conv",
                "item_type": "Task",
                "item_bytes": format!("{item_bytes}foreign"),
                "history_bytes": "\"stub\": true\n",
                "before_item_hash": "foreign-before-hash",
            })
            .to_string(),
        )?;
    }

    let refused = workspace.update(update_request());
    assert!(matches!(refused, Err(PmRustError::RecoveryConflict { .. })));
    let refused = workspace.comment(&CommentItem {
        id: "sample-conv".to_owned(),
        text: "Conflicting note".to_owned(),
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    });
    assert!(matches!(refused, Err(PmRustError::RecoveryConflict { .. })));
    let refused = workspace.close(CloseItem {
        id: "sample-conv".to_owned(),
        reason: "conflicting close".to_owned(),
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        provenance_role: None,
        force_stale_lock: false,
    });
    assert!(matches!(refused, Err(PmRustError::RecoveryConflict { .. })));
    Ok(())
}

#[test]
/// Proves closed stdout pipes turn completed CLI mutations into exit code 2.
fn cli_mutations_report_a_closed_stdout_pipe() -> Result<(), Box<dyn std::error::Error>> {
    use std::process::{Command as ProcessCommand, Stdio};

    let (directory, workspace) = tracker()?;
    let binary = env!("CARGO_BIN_EXE_pm-rust");
    workspace.create(create_request())?;

    // Each mutation completes its durable transaction and only then fails to
    // write the JSON response onto the closed pipe, so every dispatch arm's
    // write failure must surface as exit code 2.
    let sequences: [&[&str]; 3] = [
        &[
            "update",
            "sample-conv",
            "--title",
            "Piped title",
            "--author",
            "pipe-agent",
        ],
        &[
            "comment",
            "sample-conv",
            "piped note",
            "--author",
            "pipe-agent",
        ],
        &[
            "close",
            "sample-conv",
            "--reason",
            "piped close",
            "--author",
            "pipe-agent",
        ],
    ];
    for arguments in sequences {
        let mut command = ProcessCommand::new(binary);
        command
            .args(["--workspace", directory.path().to_string_lossy().as_ref()])
            .args(arguments)
            .stdout(Stdio::piped());
        let mut child = command.spawn()?;
        drop(child.stdout.take());
        assert_eq!(child.wait()?.code(), Some(2));
    }
    Ok(())
}

#[test]
/// Proves CLI mutations reject invalid inputs with the same guards as the SDK.
fn cli_mutations_reject_invalid_inputs() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, _workspace) = tracker()?;
    let workspace_arg = format!("--workspace={}", directory.path().display());
    Command::cargo_bin("pm-rust")?
        .args([
            workspace_arg.as_str(),
            "create",
            "--id",
            "sample-conv",
            "--title",
            "Conformance item",
            "--type",
            "Task",
            "--author",
            "fixture-agent",
            "--description",
            "First desc",
            "--timestamp",
            TIMESTAMP,
        ])
        .assert()
        .success();

    // A blank author is refused.
    Command::cargo_bin("pm-rust")?
        .args([
            workspace_arg.as_str(),
            "update",
            "sample-conv",
            "--title",
            "Bad author",
            "--author",
            "   ",
            "--timestamp",
            TIMESTAMP,
        ])
        .assert()
        .code(2)
        .stderr(contains("invalid mutation request"));

    // A bad timestamp is refused.
    Command::cargo_bin("pm-rust")?
        .args([
            workspace_arg.as_str(),
            "update",
            "sample-conv",
            "--title",
            "Bad timestamp",
            "--author",
            "fixture-agent",
            "--timestamp",
            "not-a-timestamp",
        ])
        .assert()
        .code(2)
        .stderr(contains("invalid mutation request"));

    // A blank title is refused.
    Command::cargo_bin("pm-rust")?
        .args([
            workspace_arg.as_str(),
            "update",
            "sample-conv",
            "--title",
            "   ",
            "--author",
            "fixture-agent",
            "--timestamp",
            TIMESTAMP,
        ])
        .assert()
        .code(2)
        .stderr(contains("invalid mutation request"));

    // A blank close reason is refused.
    Command::cargo_bin("pm-rust")?
        .args([
            workspace_arg.as_str(),
            "close",
            "sample-conv",
            "--reason",
            "  ",
            "--author",
            "fixture-agent",
            "--timestamp",
            TIMESTAMP,
        ])
        .assert()
        .code(2)
        .stderr(contains("invalid mutation request"));

    Ok(())
}

#[test]
/// Proves closing an already terminal item is refused.
fn close_refuses_an_already_terminal_item() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, workspace) = tracker()?;
    workspace.create(create_request())?;
    workspace.close(CloseItem {
        id: "sample-conv".to_owned(),
        reason: "first close".to_owned(),
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        provenance_role: None,
        force_stale_lock: false,
    })?;
    let second = workspace.close(CloseItem {
        id: "sample-conv".to_owned(),
        reason: "second close".to_owned(),
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        provenance_role: None,
        force_stale_lock: false,
    });
    assert!(matches!(
        second,
        Err(PmRustError::InvalidMutationRequest { .. })
    ));
    Ok(())
}

#[test]
/// Proves each updateable field can be changed individually.
///
/// The `changed` guard uses `||` short-circuiting, so updating only `title`
/// never evaluates `description.is_some()`, `priority.is_some()`, etc. This
/// test exercises each field in isolation so every `||` operand has its true
/// direction taken in the integration-test binary.
fn single_field_updates_cover_every_change_arm() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, workspace) = tracker()?;
    workspace.create(create_request())?;

    // Description only.
    workspace.update(UpdateItem {
        id: "sample-conv".to_owned(),
        title: None,
        description: Some("Updated desc".to_owned()),
        status: None,
        priority: None,
        tags: None,
        body: None,
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    })?;

    // Status only.
    workspace.update(UpdateItem {
        id: "sample-conv".to_owned(),
        title: None,
        description: None,
        status: Some("in_progress".to_owned()),
        priority: None,
        tags: None,
        body: None,
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    })?;

    // Priority only.
    workspace.update(UpdateItem {
        id: "sample-conv".to_owned(),
        title: None,
        description: None,
        status: None,
        priority: Some(3),
        tags: None,
        body: None,
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    })?;

    // Tags only.
    workspace.update(UpdateItem {
        id: "sample-conv".to_owned(),
        title: None,
        description: None,
        status: None,
        priority: None,
        tags: Some(vec!["solo".to_owned()]),
        body: None,
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    })?;

    // Body only.
    workspace.update(UpdateItem {
        id: "sample-conv".to_owned(),
        title: None,
        description: None,
        status: None,
        priority: None,
        tags: None,
        body: Some("Updated body".to_owned()),
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    })?;

    Ok(())
}

#[test]
/// Proves a crash between journal write and durable publish is replayed, not lost.
///
/// The recovery path exists for the window where a transaction journal is on
/// disk but the item document and its history line were never published. It is
/// exercised here by reconstructing that exact on-disk state — journal present,
/// item removed, history truncated — and then driving an ordinary update, which
/// must replay the journal before applying its own change.
fn a_durable_journal_replays_a_missing_item_and_history_before_the_next_mutation()
-> Result<(), Box<dyn std::error::Error>> {
    let (directory, workspace) = tracker()?;
    let root = directory.path();
    workspace.create(create_request())?;

    let item_bytes = item(root);
    let history_bytes = history(root);
    assert!(!item_bytes.is_empty(), "the create must publish an item");
    assert!(!history_bytes.is_empty(), "the create must publish history");

    // Reconstruct the post-crash state: the journal survived, the durable
    // publish did not. Both files are REMOVED rather than truncated, which is
    // the distinction the recovery guard draws — a present-but-divergent stream
    // is a conflict and is refused, while an absent one is replayable.
    let transactions = root.join(".agents/pm/runtime/transactions");
    fs::create_dir_all(&transactions)?;
    fs::write(
        transactions.join("update-sample-conv.json"),
        serde_json::json!({
            "version": 1,
            "id": "sample-conv",
            "item_type": "Task",
            "item_bytes": item_bytes,
            "history_bytes": history_bytes,
            "before_item_hash": "0000000000000000000000000000000000000000000000000000000000000000",
        })
        .to_string(),
    )?;
    fs::remove_file(root.join(".agents/pm/tasks/sample-conv.toon"))?;
    fs::remove_file(root.join(".agents/pm/history/sample-conv.jsonl"))?;

    workspace.update(update_request())?;

    let recovered_history = history(root);
    assert!(
        recovered_history.contains(&history_bytes),
        "the journalled history line must be replayed, not discarded"
    );
    assert!(
        item(root).contains("Renamed item"),
        "the update must apply on top of the replayed item"
    );
    Ok(())
}

#[test]
// POSIX-shaped end to end: the second phase backdates a stale-cleanup GATE,
// which is a directory, and `File::open` on a directory succeeds on unix but
// not on Windows. Ageing only the lock file made the first phase work there and
// the second fail, so the gate stayed fresh, `acquire_lock_attempt` returned
// LockConflict, and the reclaim assertion failed. Gating the whole test is the
// honest option: it does not run on Windows rather than running and asserting
// nothing. Coverage is measured on Linux, where it does run.
#[cfg(unix)]
/// Proves a stale lock is reclaimed when `force_stale_lock` is set.
///
/// This exercises the `acquire_lock_attempt` stale-cleanup path through the
/// public API, covering the `!stale || !force_stale`, gate-creation, and
/// abandoned-gate branches in the integration-test compilation unit.
fn a_stale_lock_is_reclaimed_when_force_stale_lock_is_set() -> Result<(), Box<dyn std::error::Error>>
{
    let (directory, workspace) = tracker()?;
    let root = directory.path();
    workspace.create(create_request())?;

    // Plant a stale lock: write a valid-looking payload and set its mtime far
    // enough in the past that the ttl_seconds=1800 threshold is exceeded.
    let lock_path = root.join(".agents/pm/locks/sample-conv.lock");
    let Some(parent) = lock_path.parent() else {
        return Err("lock path has no parent".into());
    };
    fs::create_dir_all(parent)?;
    fs::write(
        &lock_path,
        r#"{"id":"sample-conv","pid":1,"owner":"stale-owner","created_at":"2020-01-01T00:00:00.000Z","ttl_seconds":1800,"token":"stale"}"#,
    )?;
    // NOT cfg(unix): `File::set_times` and `FileTimes` are cross-platform. Under
    // a unix-only guard the mtime stayed at "now" on Windows, so the planted
    // lock was never stale, `force_stale_lock` had nothing to reclaim, and the
    // test failed there while passing on Linux - a guard that made the test
    // vacuous on one platform and red on the other.
    let file = std::fs::File::options().write(true).open(&lock_path)?;
    file.set_times(std::fs::FileTimes::new().set_modified(std::time::SystemTime::UNIX_EPOCH))?;

    // The update with `force_stale_lock` must reclaim the stale lock and apply.
    let result = workspace.update(UpdateItem {
        force_stale_lock: true,
        ..update_request()
    });
    assert!(result.is_ok(), "a stale lock should be reclaimed, not held");
    assert!(item(root).contains("Renamed item"));

    // Plant a stale lock again, this time beside an abandoned stale-cleanup
    // gate directory. The gate's mtime is set to the epoch so it reads as
    // abandoned; the reclaim path removes the gate and recreates it before
    // proceeding.
    fs::write(
        &lock_path,
        r#"{"id":"sample-conv","pid":1,"owner":"stale-owner","created_at":"2020-01-01T00:00:00.000Z","ttl_seconds":1800,"token":"stale"}"#,
    )?;
    let gate = root.join(".agents/pm/locks/sample-conv.lock.stale-cleanup");
    fs::create_dir_all(&gate)?;
    // The lock file is aged on every platform: without it the planted lock is
    // not stale on Windows and the test asserts nothing there.
    std::fs::File::options()
        .write(true)
        .open(&lock_path)?
        .set_times(std::fs::FileTimes::new().set_modified(std::time::SystemTime::UNIX_EPOCH))?;
    // The GATE is a directory, and `File::open` on a directory succeeds on unix
    // but not on Windows, so only this half is guarded.
    #[cfg(unix)]
    {
        std::fs::File::open(&gate)?
            .set_times(std::fs::FileTimes::new().set_modified(std::time::SystemTime::UNIX_EPOCH))?;
    }
    let result = workspace.update(UpdateItem {
        id: "sample-conv".to_owned(),
        title: Some("Twice renamed".to_owned()),
        description: None,
        status: None,
        priority: None,
        tags: None,
        body: None,
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        message: None,
        provenance_role: Some("implementer".to_owned()),
        force_stale_lock: true,
    });
    assert!(result.is_ok(), "an abandoned gate should be reclaimed");
    assert!(item(root).contains("Twice renamed"));
    Ok(())
}

#[test]
/// Proves create recovery refuses a journal whose item bytes differ.
///
/// This exercises the create `recover` conflict branches through the public
/// API: a foreign durable item beside a create journal must refuse rather than
/// overwrite.
fn create_recovery_refuses_foreign_durable_item() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, workspace) = tracker()?;
    let root = directory.path();
    workspace.create(create_request())?;

    // Plant a create journal whose item bytes differ from the durable item.
    let transactions = root.join(".agents/pm/runtime/transactions");
    let item_bytes = item(root);
    fs::write(
        transactions.join("create-sample-conv.json"),
        serde_json::json!({
            "version": 1,
            "id": "sample-conv",
            "item_type": "Task",
            "item_bytes": format!("{item_bytes}foreign"),
            "history_bytes": "\"stub\": true\n",
            "before_item_hash": "0000000000000000000000000000000000000000000000000000000000000000",
        })
        .to_string(),
    )?;

    // A second create on the same id fails because the item exists; but the
    // recovery check fires first and refuses the foreign journal.
    let refused = workspace.create(create_request());
    assert!(matches!(
        refused,
        Err(PmRustError::ItemAlreadyExists { .. } | PmRustError::RecoveryConflict { .. })
    ));
    Ok(())
}

#[test]
/// Proves create recovery refuses a journal with a version mismatch.
fn create_recovery_refuses_a_mismatched_journal() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, workspace) = tracker()?;
    let root = directory.path();
    workspace.create(create_request())?;

    let transactions = root.join(".agents/pm/runtime/transactions");
    fs::write(
        transactions.join("create-sample-conv.json"),
        serde_json::json!({
            "version": 2,
            "id": "sample-conv",
            "item_type": "Task",
            "item_bytes": item(root),
            "history_bytes": history(root),
            "before_item_hash": "0000000000000000000000000000000000000000000000000000000000000000",
        })
        .to_string(),
    )?;

    let refused = workspace.create(create_request());
    assert!(matches!(
        refused,
        Err(PmRustError::RecoveryConflict { .. } | PmRustError::ItemAlreadyExists { .. })
    ));
    Ok(())
}

#[test]
/// Proves a comment refuses when the stored `comments` value is not an array.
///
/// A scalar, object, or null `comments` field is not a compatible append
/// target. The mutation must refuse instead of silently overwriting the stored
/// content with a one-element array.
fn a_comment_refuses_a_non_array_comments_value() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, workspace) = tracker()?;
    let root = directory.path();
    workspace.create(create_request())?;

    // Plant a non-array `comments` value directly in the stored item.
    let item_path = root.join(".agents/pm/tasks/sample-conv.toon");
    let raw = fs::read_to_string(&item_path)?;
    let polluted = format!("{raw}comments: \"not-an-array\"\n");
    fs::write(&item_path, polluted)?;

    let refused = workspace.comment(&CommentItem {
        id: "sample-conv".to_owned(),
        text: "note".to_owned(),
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    });
    assert!(matches!(
        refused,
        Err(PmRustError::InvalidMutationRequest { .. })
    ));
    // The stored content must be unchanged.
    assert!(fs::read_to_string(&item_path)?.contains("not-an-array"));
    Ok(())
}

#[test]
/// Proves create recovery refuses when the durable history differs from the
/// journal even though the item matches.
fn create_recovery_refuses_a_diverged_history() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, workspace) = tracker()?;
    let root = directory.path();
    workspace.create(create_request())?;
    let item_bytes = item(root);
    let transactions = root.join(".agents/pm/runtime/transactions");
    fs::write(
        transactions.join("create-sample-conv.json"),
        serde_json::json!({
            "version": 1,
            "id": "sample-conv",
            "item_type": "Task",
            "item_bytes": item_bytes,
            "history_bytes": "{\"stub\": true}\n",
            "before_item_hash": "0000000000000000000000000000000000000000000000000000000000000000",
        })
        .to_string(),
    )?;
    // The item matches the journal but the history stream diverged.
    fs::write(
        root.join(".agents/pm/history/sample-conv.jsonl"),
        "diverged\n",
    )?;
    let refused = workspace.create(create_request());
    assert!(matches!(
        refused,
        Err(PmRustError::RecoveryConflict { .. } | PmRustError::ItemAlreadyExists { .. })
    ));
    Ok(())
}

#[test]
/// Proves `recover_mutation` completes the history half when the item already
/// holds the post-mutation image (crash after item replace, before history
/// append).
fn a_journal_completes_the_history_half_when_the_item_is_already_replaced()
-> Result<(), Box<dyn std::error::Error>> {
    let (directory, workspace) = tracker()?;
    let root = directory.path();
    workspace.create(create_request())?;
    let original_item = item(root);
    let updated_item = original_item.replace("Conformance item", "Recovered title");
    let original_history = history(root);
    let appended_history = format!("{original_history}{{\"ts\":\"stub\",\"op\":\"update\"}}\n");

    // Plant an update journal whose item_bytes match the durable item (the
    // item half already committed) but whose history line has not been
    // appended yet.
    let transactions = root.join(".agents/pm/runtime/transactions");
    fs::write(
        transactions.join("update-sample-conv.json"),
        serde_json::json!({
            "version": 1,
            "id": "sample-conv",
            "item_type": "Task",
            "item_bytes": updated_item,
            "history_bytes": appended_history,
            "before_item_hash": "0000000000000000000000000000000000000000000000000000000000000000",
        })
        .to_string(),
    )?;
    fs::write(
        root.join(".agents/pm/tasks/sample-conv.toon"),
        &updated_item,
    )?;

    // The next update must recover: recognise the after-image, append the
    // missing history line, remove the journal, then apply the update.
    workspace.update(update_request())?;
    assert!(
        history(root).contains(&appended_history),
        "the journalled history line must be replayed during recovery"
    );
    assert!(!transactions.join("update-sample-conv.json").exists());
    Ok(())
}

#[cfg(unix)]
#[test]
/// Proves a failed roll-forward surfaces its IO error rather than reporting success.
///
/// `recover_mutation`'s roll-forward arm runs when the durable item still holds
/// the before-image or is absent: it replays the journalled item and history.
/// Every existing test drives that arm with a writable tracker, so the `?` on
/// `atomic_replace` was never taken and a recovery that could not write would
/// have been indistinguishable from one that did.
///
/// The item directory is made unwritable so `atomic_replace` fails creating its
/// private temporary, which is the first fallible step of the replay.
fn a_roll_forward_that_cannot_write_the_item_fails_instead_of_reporting_success()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    let (directory, workspace) = tracker()?;
    let root = directory.path();
    workspace.create(create_request())?;

    let item_bytes = item(root);
    let history_bytes = history(root);

    // Post-crash state: journal survived, durable publish did not.
    let transactions = root.join(".agents/pm/runtime/transactions");
    fs::create_dir_all(&transactions)?;
    fs::write(
        transactions.join("update-sample-conv.json"),
        serde_json::json!({
            "version": 1,
            "id": "sample-conv",
            "item_type": "Task",
            "item_bytes": item_bytes,
            "history_bytes": history_bytes,
            "before_item_hash": "0000000000000000000000000000000000000000000000000000000000000000",
        })
        .to_string(),
    )?;
    fs::remove_file(root.join(".agents/pm/tasks/sample-conv.toon"))?;
    fs::remove_file(root.join(".agents/pm/history/sample-conv.jsonl"))?;

    // Deny writes to the directory the replay must create its temporary in.
    let tasks = root.join(".agents/pm/tasks");
    fs::set_permissions(&tasks, fs::Permissions::from_mode(0o555))?;
    let outcome = workspace.update(update_request());
    // Restore before asserting so a failed assertion cannot leave the temporary
    // directory undeletable.
    fs::set_permissions(&tasks, fs::Permissions::from_mode(0o755))?;

    assert!(
        matches!(outcome, Err(PmRustError::Io { .. })),
        "an unwritable item directory must surface as an IO error, got {outcome:?}"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
/// Proves an unreadable item subdirectory fails the lookup rather than reporting the item missing.
///
/// `locate_item_recursive` walks every directory that is not excluded at the
/// root. Its `read_directory(dir)?` had no coverage because the walk always
/// succeeded in tests, so a directory the process cannot read would have been
/// silently skipped and the item reported absent — a wrong answer rather than
/// an error.
fn an_unreadable_item_directory_fails_the_lookup_rather_than_hiding_the_item()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    let (directory, workspace) = tracker()?;
    let root = directory.path();
    workspace.create(create_request())?;

    let tasks = root.join(".agents/pm/tasks");
    fs::set_permissions(&tasks, fs::Permissions::from_mode(0o000))?;
    let outcome = workspace.update(update_request());
    fs::set_permissions(&tasks, fs::Permissions::from_mode(0o755))?;

    assert!(
        outcome.is_err(),
        "an unreadable item directory must fail the lookup, got {outcome:?}"
    );
    Ok(())
}

#[test]
/// Proves a roll-forward recovery surfaces a history append failure.
///
/// When the item is absent and the history directory is gone, `atomic_replace`
/// succeeds (the tasks directory exists) but `append_history_line` fails
/// because it cannot create the history file. The error must surface rather
/// than being silently swallowed.
fn a_roll_forward_surfaces_a_history_append_failure() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, workspace) = tracker()?;
    let root = directory.path();
    workspace.create(create_request())?;
    let item_bytes = item(root);
    let history_bytes = history(root);

    let transactions = root.join(".agents/pm/runtime/transactions");
    fs::create_dir_all(&transactions)?;
    fs::write(
        transactions.join("update-sample-conv.json"),
        serde_json::json!({
            "version": 1,
            "id": "sample-conv",
            "item_type": "Task",
            "item_bytes": item_bytes,
            "history_bytes": history_bytes,
            "before_item_hash": "0000000000000000000000000000000000000000000000000000000000000000",
        })
        .to_string(),
    )?;
    fs::remove_file(root.join(".agents/pm/tasks/sample-conv.toon"))?;
    fs::remove_dir_all(root.join(".agents/pm/history"))?;

    let outcome = workspace.update(update_request());
    assert!(
        matches!(outcome, Err(PmRustError::Io { .. })),
        "a missing history directory must surface as an IO error, got {outcome:?}"
    );
    Ok(())
}

#[test]
/// Proves a journal written before `before_item_hash` existed still parses.
///
/// The field was added as a required one, which meant a journal written by the
/// previous build failed `serde_json::from_str`; `recover` and `recover_mutation`
/// then returned `RecoveryConflict` for every later operation on that identifier
/// until an operator deleted the file by hand. The exposure window is a crash
/// followed by an upgrade, which is exactly when recovery has to work.
fn a_journal_without_the_before_hash_still_recovers() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, workspace) = tracker()?;
    let root = directory.path();
    workspace.create(create_request())?;

    // A legacy journal: valid in every respect except that it predates the
    // before-image hash, so the field is absent rather than empty.
    let journal_dir = root.join(".agents/pm/runtime/transactions");
    fs::create_dir_all(&journal_dir)?;
    let item_path = root.join(".agents/pm/tasks/sample-conv.toon");
    let after_bytes = fs::read_to_string(&item_path)?;
    fs::write(
        journal_dir.join("update-sample-conv.json"),
        serde_json::to_string(&serde_json::json!({
            "version": 1,
            "id": "sample-conv",
            "item_type": "Task",
            "item_bytes": after_bytes,
            "history_bytes": "{\"op\":\"legacy\"}\n",
        }))?,
    )?;

    // Recovery must actually succeed: a journal predating before_item_hash
    // still deserializes (the field is `#[serde(default)]`) and the mutation
    // completes. Asserting only that the failure is not one specific failure
    // ("invalid durable journal") would let any other `RecoveryConflict` pass —
    // a test named "still recovers" that passes when recovery fails is worse
    // than no test.
    let result = workspace.update(UpdateItem {
        title: Some("Parsed a legacy journal".to_owned()),
        ..update_request()
    });
    assert!(
        result.is_ok(),
        "a journal predating before_item_hash must still recover, but recovery failed: {}",
        result
            .as_ref()
            .err()
            .map_or("no error".to_owned(), std::string::ToString::to_string)
    );
    Ok(())
}

#[test]
/// Proves a mutation that replays a journal still re-reads the durable item.
///
/// `recover_mutation` hands back the document it decoded on its common
/// no-journal path so the tracker is not walked twice. When a journal IS
/// replayed it returns `None` instead, because the replay has just rewritten
/// the item on disk and any copy read before that is stale. This drives that
/// second path: without the re-read, the mutation would build its result from
/// a document that no longer matches the bytes on disk.
fn a_replayed_journal_forces_the_item_to_be_read_again() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, workspace) = tracker()?;
    let root = directory.path();
    workspace.create(create_request())?;

    let item_path = root.join(".agents/pm/tasks/sample-conv.toon");
    let after_bytes = fs::read_to_string(&item_path)?;
    let history_line = "{\"op\":\"replayed\",\"id\":\"sample-conv\"}\n";

    // A journal whose after-image matches the item already on disk: the item
    // half is complete, so recovery replays only the history half and then
    // reports that the caller must re-read.
    let journal_dir = root.join(".agents/pm/runtime/transactions");
    fs::create_dir_all(&journal_dir)?;
    fs::write(
        journal_dir.join("update-sample-conv.json"),
        serde_json::to_string(&serde_json::json!({
            "version": 1,
            "id": "sample-conv",
            "item_type": "Task",
            "item_bytes": after_bytes,
            "history_bytes": history_line,
            "before_item_hash": "",
        }))?,
    )?;

    workspace.update(UpdateItem {
        title: Some("Read again after replay".to_owned()),
        ..update_request()
    })?;

    let history = fs::read_to_string(root.join(".agents/pm/history/sample-conv.jsonl"))?;
    assert!(
        history.contains("\"op\":\"replayed\""),
        "the journalled history line must be replayed before the mutation appends its own"
    );
    assert!(
        fs::read_to_string(&item_path)?.contains("Read again after replay"),
        "the mutation must apply on top of the re-read document"
    );
    Ok(())
}
