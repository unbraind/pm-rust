//! Black-box and SDK acceptance for native update, comment, and close
//! mutations against fixtures recorded from the published pm 2026.8.21 CLI
//! under its reproducible workspace recipe (fixed clock, zero tick).

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use pm_rust::{CloseItem, CommentItem, CreateItem, PmRustError, UpdateItem, Workspace};
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
    r#"{"ts":"2026-08-22T10:00:00.000Z","author":"fixture-agent","author_source":"asserted","agent_provenance":{"role":{"value":"implementer","source":"argv"}},"op":"create","patch":[{"op":"replace","path":"/body","value":"Original body"},{"op":"add","path":"/metadata/id","value":"sample-conv"},{"op":"add","path":"/metadata/title","value":"Conformance item"},{"op":"add","path":"/metadata/description","value":"First desc"},{"op":"add","path":"/metadata/type","value":"Task"},{"op":"add","path":"/metadata/status","value":"open"},{"op":"add","path":"/metadata/priority","value":2},{"op":"add","path":"/metadata/tags","value":["alpha","beta"]},{"op":"add","path":"/metadata/created_at","value":"2026-08-22T10:00:00.000Z"},{"op":"add","path":"/metadata/updated_at","value":"2026-08-22T10:00:00.000Z"},{"op":"add","path":"/metadata/author","value":"fixture-agent"}],"before_hash":"3cc22dff72be7b14824654a7a64ea62b04799939b2fee54c1b5f52ca60bf6df0","after_hash":"ce63b69e6445b50ae43919f31607098a4e414350c8ca52003bd84ea609f979bf","item_hash_version":2,"message":""}"#,
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
    r#"{"ts":"2026-08-22T10:00:00.000Z","author":"fixture-agent","author_source":"asserted","agent_provenance":{"role":{"value":"implementer","source":"argv"}},"op":"update","patch":[{"op":"replace","path":"/metadata/priority","value":3},{"op":"replace","path":"/metadata/title","value":"Renamed item"}],"before_hash":"ce63b69e6445b50ae43919f31607098a4e414350c8ca52003bd84ea609f979bf","after_hash":"dc48d8c5971803ef643ec17734542814d3064982d1bc1b8d585d539d2266459c","item_hash_version":2,"message":"rename and reprioritize"}"#,
    "\n",
);

/// Canonical history line recorded after the recorded comment step.
const COMMENT_HISTORY_LINE: &str = concat!(
    r#"{"ts":"2026-08-22T10:00:00.000Z","author":"fixture-agent","author_source":"asserted","op":"comment_add","patch":[{"op":"add","path":"/metadata/comments","value":[{"created_at":"2026-08-22T10:00:00.000Z","author":"fixture-agent","text":"First native note"}]}],"before_hash":"dc48d8c5971803ef643ec17734542814d3064982d1bc1b8d585d539d2266459c","after_hash":"2f2d4a1680bcbb9dbf4e570ea50ceedbd7807a5f7d3c514bf8a4187b99fa37c0","item_hash_version":2,"message":"note recorded"}"#,
    "\n",
);

/// Canonical history line recorded after the recorded status update step.
const STATUS_UPDATE_HISTORY_LINE: &str = concat!(
    r#"{"ts":"2026-08-22T10:00:00.000Z","author":"fixture-agent","author_source":"asserted","agent_provenance":{"role":{"value":"implementer","source":"argv"}},"op":"update","patch":[{"op":"replace","path":"/metadata/status","value":"in_progress"}],"before_hash":"2f2d4a1680bcbb9dbf4e570ea50ceedbd7807a5f7d3c514bf8a4187b99fa37c0","after_hash":"e7f7fb53e6330e7ffacbf15a870ad604a2e8d5236e67ac6c9e373b8ea4d6a0d0","item_hash_version":2}"#,
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
    r#"{"ts":"2026-08-22T10:00:00.000Z","author":"fixture-agent","author_source":"asserted","agent_provenance":{"role":{"value":"implementer","source":"argv"}},"op":"close","patch":[{"op":"replace","path":"/metadata/status","value":"closed"},{"op":"add","path":"/metadata/closed_at","value":"2026-08-22T10:00:00.000Z"},{"op":"add","path":"/metadata/completed_at","value":"2026-08-22T10:00:00.000Z"},{"op":"add","path":"/metadata/close_reason","value":"conformance complete"}],"before_hash":"e7f7fb53e6330e7ffacbf15a870ad604a2e8d5236e67ac6c9e373b8ea4d6a0d0","after_hash":"2319c2cdf7e8164348a3235c87426c55dbf49aaeb286cc4893494b2b3b6eb6a8","item_hash_version":2}"#,
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
fn native_create_matches_the_published_2026_8_21_v2_history_bytes()
-> Result<(), Box<dyn std::error::Error>> {
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
fn native_update_matches_the_published_2026_8_21_bytes_exactly()
-> Result<(), Box<dyn std::error::Error>> {
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
fn native_comment_matches_the_published_2026_8_21_bytes_exactly()
-> Result<(), Box<dyn std::error::Error>> {
    let (directory, workspace) = tracker()?;
    workspace.create(create_request())?;
    workspace.update(update_request())?;
    let result = workspace.comment(&CommentItem {
        id: "sample-conv".to_owned(),
        text: "First native note".to_owned(),
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        message: Some("note recorded".to_owned()),
        provenance_role: None,
        force_stale_lock: false,
    })?;
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
    workspace.create(create_request())?;
    workspace.update(update_request())?;
    workspace.comment(&CommentItem {
        id: "sample-conv".to_owned(),
        text: "First native note".to_owned(),
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        message: Some("note recorded".to_owned()),
        provenance_role: None,
        force_stale_lock: false,
    })?;
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
        provenance_role: Some("implementer".to_owned()),
        force_stale_lock: false,
    })?;
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
fn native_close_matches_the_published_2026_8_21_bytes_exactly()
-> Result<(), Box<dyn std::error::Error>> {
    let (directory, workspace) = tracker()?;
    workspace.create(create_request())?;
    workspace.update(update_request())?;
    workspace.comment(&CommentItem {
        id: "sample-conv".to_owned(),
        text: "First native note".to_owned(),
        author: "fixture-agent".to_owned(),
        timestamp: Some(TIMESTAMP.to_owned()),
        message: Some("note recorded".to_owned()),
        provenance_role: None,
        force_stale_lock: false,
    })?;
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
        provenance_role: Some("implementer".to_owned()),
        force_stale_lock: false,
    })?;
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
    assert!(matches!(
        bad_timestamp,
        Err(PmRustError::InvalidCreateRequest { .. })
    ));
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
