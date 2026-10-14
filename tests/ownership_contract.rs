//! Public native SDK acceptance for lifecycle, selection and durable failure boundaries.

use std::{collections::BTreeMap, fs};

use pm_rust::{
    AuthorIdentity, CreateItem, OwnershipItem, OwnershipOptions, PmRustError, SelectionOptions,
    Workspace,
};
use serde_json::{Value, json};

type TestResult = Result<(), Box<dyn std::error::Error>>;
const CLOCK: &str = "2026-08-22T10:00:00Z";

/// Creates one real canonical tracker and item through the native public API.
fn fixture() -> Result<(tempfile::TempDir, Workspace), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(&root)?;
    fs::write(
        root.join("settings.json"),
        r#"{"governance":{"preset":"strict"},"locks":{"wait_ms":0}}"#,
    )?;
    let workspace = Workspace::discover(&root)?;
    workspace.create(CreateItem {
        id: "sample-one".to_owned(),
        title: "One".to_owned(),
        description: String::new(),
        item_type: "Task".to_owned(),
        status: "open".to_owned(),
        priority: 2,
        tags: Vec::new(),
        body: String::new(),
        author: "alpha".to_owned(),
        timestamp: Some(CLOCK.to_owned()),
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    })?;
    Ok((directory, workspace))
}

/// Supplies a deterministic ownership request without ambient identity resolution.
fn request(author: &str) -> OwnershipItem {
    OwnershipItem {
        id: "sample-one".to_owned(),
        author: author.to_owned(),
        timestamp: Some(CLOCK.to_owned()),
        message: None,
        force: false,
        if_available: false,
        provenance_role: None,
    }
}

/// An object-state publication fault cannot undo committed ownership metadata.
#[test]
fn session_publication_faults_preserve_completed_ownership() -> TestResult {
    let (_directory, workspace) = fixture()?;
    let root = workspace.pm_root();
    let session = root.join("runtime/session.json");
    fs::create_dir(&session)?;
    let alpha = request("alpha");
    assert!(matches!(
        workspace.claim(&alpha),
        Err(PmRustError::Io { .. })
    ));
    assert_eq!(
        workspace.get("sample-one")?.metadata.extra["assignee"],
        "alpha"
    );
    assert!(matches!(
        workspace.release(&alpha),
        Err(PmRustError::Io { .. })
    ));
    assert!(
        !workspace
            .get("sample-one")?
            .metadata
            .extra
            .contains_key("assignee")
    );
    fs::remove_dir(&session)?;
    let lock = root.join("locks/session-state.lock");
    let held = serde_json::to_vec(
        &json!({"created_at":pm_rust::current_timestamp(), "owner":"fixture-holder"}),
    )?;
    fs::write(&lock, &held)?;
    assert!(
        matches!(workspace.claim(&alpha), Err(PmRustError::LockConflict {id}) if id=="session-state")
    );
    assert_eq!(fs::read(&lock)?, held);
    assert_eq!(
        workspace.get("sample-one")?.metadata.extra["assignee"],
        "alpha"
    );
    assert_eq!(
        fs::read_to_string(root.join("history/sample-one.jsonl"))?
            .lines()
            .count(),
        4
    );
    Ok(())
}

/// A failed start update retains its claim and a journal for interrupted history.
#[test]
fn lifecycle_storage_faults_preserve_claim_and_recovery_journal() -> TestResult {
    let (_directory, workspace) = fixture()?;
    let root = workspace.pm_root();
    let alpha = request("alpha");
    let options = OwnershipOptions::default();
    let before = fs::read(root.join("tasks/sample-one.toon"))?;
    let mut invalid = alpha.clone();
    invalid.timestamp = Some("2999-01-01T00:00:00Z".to_owned());
    assert!(matches!(
        workspace.pause_task(&invalid, &options),
        Err(PmRustError::InvalidMutationRequest { .. })
    ));
    invalid = request("alpha");
    invalid.id = "sample-absent".to_owned();
    assert!(workspace.pause_task(&invalid, &options).is_err());
    fs::write(root.join("settings.json"), "invalid")?;
    assert!(workspace.pause_task(&alpha, &options).is_err());
    fs::write(
        root.join("settings.json"),
        r#"{"schema":{"files":{"workflows":"missing.json"}}}"#,
    )?;
    assert!(matches!(
        workspace.pause_task(&alpha, &options),
        Err(PmRustError::Io { .. })
    ));
    assert_eq!(fs::read(root.join("tasks/sample-one.toon"))?, before);
    fs::write(root.join("settings.json"), "{}")?;
    workspace.claim(&alpha)?;
    let history = root.join("history/sample-one.jsonl");
    fs::remove_file(&history)?;
    fs::create_dir(&history)?;
    let outcome = workspace.start_task(&alpha, &options);
    assert!(
        matches!(outcome, Err(PmRustError::CommandRefusal {exit_code:1, context,..}) if context["why"].as_str().is_some_and(|why| why.starts_with("claim completed for sample-one")))
    );
    assert_eq!(workspace.get("sample-one")?.metadata.status, "in_progress");
    assert!(
        root.join("runtime/transactions/update-sample-one.json")
            .is_file()
    );
    Ok(())
}

/// Missing type constraints and repeated starts retain normal lifecycle semantics.
#[test]
fn optional_workflows_and_sdk_identity_controls_compose() -> TestResult {
    let (_directory, workspace) = fixture()?;
    let alpha = request("alpha");
    let default = OwnershipOptions::default();
    for (index, schema) in [
        json!({}),
        json!({"type_workflows":[]}),
        json!({"type_workflows":[{"type":"Task"}]}),
        json!({"type_workflows":[{"type":"Task","allowed_transitions":[["open","in_progress"]]}]}),
    ]
    .into_iter()
    .enumerate()
    {
        fs::write(
            workspace.pm_root().join("settings.json"),
            serde_json::to_vec(
                &json!({"governance":{"preset":"strict", "workflow_enforcement":"strict"}, "schema":schema}),
            )?,
        )?;
        assert_eq!(
            workspace.start_task(&alpha, &default)?["changed_field_count"],
            if index == 0 { 3 } else { 0 }
        );
    }
    fs::write(
        workspace.pm_root().join("settings.json"),
        r#"{"governance":{"preset":"strict","workflow_enforcement":"strict"},"schema":{"type_workflows":[{"type":"Task","allowed_transitions":[["draft","open"]]}]}}"#,
    )?;
    assert!(
        matches!(workspace.pause_task(&alpha, &default), Err(PmRustError::CommandRefusal {detail,..}) if detail.contains("draft -> open"))
    );
    fs::write(
        workspace.pm_root().join("settings.json"),
        r#"{"governance":{"preset":"strict"}}"#,
    )?;
    let explicit = OwnershipOptions {
        identity: Some(AuthorIdentity::resolve(
            Some("alpha"),
            None,
            &BTreeMap::new(),
        )),
        release_bypass: false,
    };
    assert_eq!(
        workspace.start_task(&alpha, &explicit)?["changed_field_count"],
        0
    );
    assert!(matches!(
        workspace.start_task(&request("beta"), &default),
        Err(PmRustError::OwnershipRefusal { .. })
    ));
    assert!(matches!(
        workspace.pause_task(&request("beta"), &default),
        Err(PmRustError::OwnershipRefusal { .. })
    ));
    assert_eq!(
        workspace.pause_task(&alpha, &explicit)?["changed_field_count"],
        3
    );
    fs::write(
        workspace.pm_root().join("settings.json"),
        r#"{"governance":{"preset":"strict","workflow_enforcement":"strict"},"schema":{"type_workflows":[{"type":"Task","allowed_transitions":[["open","closed"],["draft","open"],["open","in_progress"]]}]}}"#,
    )?;
    assert_eq!(
        workspace.start_task(&alpha, &explicit)?["status"],
        "in_progress"
    );
    fs::write(
        workspace.pm_root().join("settings.json"),
        r#"{"governance":{"preset":"strict"}}"#,
    )?;
    assert_eq!(workspace.pause_task(&alpha, &default)?["status"], "open");
    Ok(())
}

/// Public selection retries, skips and propagates validation and live-lock failures.
#[test]
fn sdk_selection_preserves_identity_and_refusal_boundaries() -> TestResult {
    let (_directory, workspace) = fixture()?;
    let alpha = request("alpha");
    let default = OwnershipOptions::default();
    let selection = SelectionOptions::default();
    assert_eq!(
        workspace.claim_next(&alpha, &default, &selection)?["id"],
        "sample-one"
    );
    let explicit = OwnershipOptions {
        identity: Some(AuthorIdentity::resolve(
            Some("alpha"),
            None,
            &BTreeMap::new(),
        )),
        release_bypass: false,
    };
    assert_eq!(
        workspace.claim_next(&alpha, &explicit, &selection)?["changed_field_count"],
        0
    );
    assert!(matches!(
        workspace.claim_next(
            &alpha,
            &default,
            &SelectionOptions {
                assignee_filter: Some("mine".to_owned()),
                ..SelectionOptions::default()
            }
        ),
        Err(PmRustError::CommandRefusal { exit_code: 2, .. })
    ));
    let mut beta = request("beta");
    assert!(matches!(
        workspace.claim_next(&beta, &default, &selection),
        Err(PmRustError::CommandRefusal { exit_code: 3, .. })
    ));
    beta.if_available = true;
    assert_eq!(
        workspace.claim_next(&beta, &default, &selection)?["available"],
        false
    );
    workspace.release(&alpha)?;
    let lock = workspace.pm_root().join("locks/sample-one.lock");
    fs::write(&lock, [0xff])?;
    assert!(matches!(
        workspace.claim_next(&alpha, &default, &selection),
        Err(PmRustError::Io { .. })
    ));
    fs::write(
        &lock,
        serde_json::to_vec(
            &json!({"created_at":pm_rust::current_timestamp(), "owner":"fixture-holder"}),
        )?,
    )?;
    assert!(
        matches!(workspace.claim_next(&alpha, &default, &selection), Err(PmRustError::OwnershipRefusal {context,..}) if context["code"]=="lock_conflict")
    );
    fs::remove_file(lock)?;
    let identity = AuthorIdentity::resolve(
        None,
        None,
        &BTreeMap::from([
            ("PI_AGENT".to_owned(), "1".to_owned()),
            ("PI_SESSION_ID".to_owned(), "fixture-session".to_owned()),
        ]),
    );
    let mut detected = request(&identity.author);
    let options = OwnershipOptions {
        identity: Some(identity),
        release_bypass: false,
    };
    workspace.claim_with_options(&detected, &options)?;
    assert!(matches!(
        workspace.claim_next(&detected, &options, &selection),
        Err(PmRustError::CommandRefusal { exit_code: 4, .. })
    ));
    detected.if_available = true;
    let receipt: Value = workspace.claim_next(&detected, &options, &selection)?;
    assert_eq!(receipt["available"], false);
    assert_eq!(receipt["attempts"], 1);
    let session_path = workspace.pm_root().join("runtime/session.json");
    let mut state: Value = serde_json::from_slice(&fs::read(&session_path)?)?;
    let instance = options
        .identity
        .as_ref()
        .and_then(|identity| identity.instance.as_deref())
        .ok_or("missing session instance")?;
    state["semantic_attribution"][instance] = Value::Null;
    fs::write(session_path, serde_json::to_vec(&state)?)?;
    assert_eq!(
        workspace
            .claim_with_options(&detected, &options)?
            .changed_field_count,
        0
    );
    Ok(())
}
