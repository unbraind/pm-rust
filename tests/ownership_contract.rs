//! Public native SDK acceptance for lifecycle, selection and durable failure boundaries.

use std::{collections::BTreeMap, fs};

use pm_rust::{
    AuthorIdentity, CreateItem, OwnershipItem, OwnershipOptions, PmRustError, SelectionOptions,
    Workspace, admission::AdmissionBatch,
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
        matches!(workspace.claim(&alpha), Err(PmRustError::CommandRefusal { detail, exit_code:4, .. }) if detail.contains("session-state lock is stale (lock_info_invalid_shape)"))
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

#[path = "support/process_environment.rs"]
mod process_environment;

/// Executes the actual ownership CLI in the same disposable non-git tracker.
fn ownership_cli(
    workspace: &Workspace,
    args: &[&str],
) -> Result<std::process::Output, std::io::Error> {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_pm-rust"));
    process_environment::isolate_environment(&mut command);
    command
        .current_dir(
            workspace
                .pm_root()
                .parent()
                .and_then(std::path::Path::parent)
                .unwrap_or(workspace.pm_root()),
        )
        .env("HOME", workspace.pm_root().join("runtime/test-home"))
        .env("USERPROFILE", workspace.pm_root().join("runtime/test-home"))
        .args(args)
        .output()
}

/// A held item's if-available receipt takes precedence over unrelated malformed payloads.
#[test]
fn held_claim_skip_precedes_unrelated_corpus_validation() -> TestResult {
    let (_directory, workspace) = fixture()?;
    workspace.claim(&request("alpha"))?;
    let target = workspace.pm_root().join("tasks/sample-one.toon");
    let before = fs::read(&target)?;
    fs::write(
        workspace.pm_root().join("tasks/unrelated.toon"),
        "invalid payload",
    )?;
    let mut beta = request("beta");
    beta.if_available = true;
    assert_eq!(workspace.claim(&beta)?.skipped, Some(true));
    let output = ownership_cli(
        &workspace,
        &[
            "claim",
            "sample-one",
            "--author",
            "beta",
            "--if-available",
            "--json",
        ],
    )?;
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(receipt["skipped"], true);
    assert_eq!(fs::read(target)?, before);
    Ok(())
}

/// A held item's hard refusal retains `already_claimed_by` and exit four despite unrelated damage.
#[test]
fn held_claim_refusal_precedes_unrelated_corpus_validation() -> TestResult {
    let (_directory, workspace) = fixture()?;
    workspace.claim(&request("alpha"))?;
    fs::write(
        workspace.pm_root().join("tasks/unrelated.toon"),
        "invalid payload",
    )?;
    assert!(
        matches!(workspace.claim(&request("beta")), Err(PmRustError::OwnershipRefusal {context,..}) if context["code"]=="already_claimed_by")
    );
    let output = ownership_cli(
        &workspace,
        &["claim", "sample-one", "--author", "beta", "--json"],
    )?;
    assert_eq!(output.status.code(), Some(4));
    let refusal: Value = serde_json::from_slice(&output.stderr)?;
    assert_eq!(refusal["code"], "already_claimed_by");
    Ok(())
}

/// Argument errors precede missing-ID lookup for both ownership operations.
#[test]
fn ownership_argument_refusals_precede_missing_id() -> TestResult {
    let (_directory, workspace) = fixture()?;
    for operation in ["claim", "release"] {
        for (author, timestamp, reason) in [
            ("", CLOCK, "author must not be empty"),
            (
                "alpha",
                "2999-01-01T00:00:00.000Z",
                "timestamp must not be in the future",
            ),
        ] {
            let mut invalid = request(author);
            invalid.id = "sample-missing".to_owned();
            invalid.timestamp = Some(timestamp.to_owned());
            let result = if operation == "claim" {
                workspace.claim(&invalid)
            } else {
                workspace.release(&invalid)
            };
            assert!(
                matches!(result, Err(PmRustError::InvalidMutationRequest {reason: actual}) if actual==reason)
            );
            let output = ownership_cli(
                &workspace,
                &[
                    operation,
                    "sample-missing",
                    "--author",
                    author,
                    "--timestamp",
                    timestamp,
                ],
            )?;
            assert_eq!(output.status.code(), Some(2));
            assert!(
                String::from_utf8_lossy(&output.stderr).contains(reason),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    Ok(())
}

/// Lost item bytes roll forward under the lock before the lineage snapshot is captured.
#[test]
fn ownership_recovers_missing_item_before_lineage_validation() -> TestResult {
    for operation in ["claim", "release"] {
        let (_directory, workspace) = fixture()?;
        let root = workspace.pm_root();
        let alpha = request("alpha");
        if operation == "release" {
            workspace.claim(&alpha)?;
        }
        let history = root.join("history/sample-one.jsonl");
        let before_history = fs::read_to_string(&history)?;
        if operation == "claim" {
            workspace.claim(&alpha)?;
        } else {
            workspace.release(&alpha)?;
        }
        let target = root.join("tasks/sample-one.toon");
        let item_bytes = fs::read_to_string(&target)?;
        let committed_history = fs::read_to_string(&history)?;
        let history_bytes = committed_history
            .strip_prefix(&before_history)
            .ok_or("history prefix changed")?;
        assert_eq!(history_bytes.lines().count(), 1);
        fs::remove_file(&target)?;
        fs::write(&history, before_history)?;
        let journal = root.join(format!("runtime/transactions/{operation}-sample-one.json"));
        fs::write(
            &journal,
            serde_json::to_vec(
                &json!({"version":1,"id":"sample-one","item_type":"Task","item_bytes":item_bytes,"history_bytes":history_bytes,"before_item_hash":""}),
            )?,
        )?;
        if operation == "claim" {
            workspace.claim(&alpha)?;
        } else {
            workspace.release(&alpha)?;
        }
        assert_eq!(fs::read_to_string(target)?, item_bytes);
        assert!(fs::read_to_string(history)?.starts_with(&committed_history));
        assert!(!journal.exists());
    }
    Ok(())
}

/// An actual busy item lock wins over unrelated corpus errors before any snapshot read.
#[test]
fn ownership_lock_refusal_precedes_corpus_validation() -> TestResult {
    let (_directory, workspace) = fixture()?;
    fs::write(
        workspace.pm_root().join("tasks/unrelated.toon"),
        "invalid payload",
    )?;
    fs::write(
        workspace.pm_root().join("locks/sample-one.lock"),
        serde_json::to_vec(
            &json!({"created_at":pm_rust::current_timestamp(),"author":"competing-agent"}),
        )?,
    )?;
    assert!(
        matches!(workspace.claim(&request("alpha")), Err(PmRustError::OwnershipRefusal {context,..}) if context["code"]=="lock_conflict")
    );
    Ok(())
}

/// Previews judge ownership by the batch author alone; an ambient SDK identity is never inherited.
#[test]
fn preview_ownership_ignores_an_ambient_scoped_identity() -> TestResult {
    let (_directory, workspace) = fixture()?;
    let owner = AuthorIdentity::resolve(Some("alpha"), None, &BTreeMap::new());
    owner.with(|| workspace.start_task(&request("alpha"), &OwnershipOptions::default()))?;
    let batch = |author: &str| {
        AdmissionBatch::from_json(&serde_json::to_vec(&json!({
            "author": author,
            "mutations": [{"op": "update", "id": "sample-one", "options": {"title": "Preview"}}],
        }))?)
        .map_err(Box::<dyn std::error::Error>::from)
    };
    let intruder = batch("intruder")?;
    let foreign = owner.with(|| workspace.preview_item_mutations(&intruder))?;
    assert!(!foreign.ok);
    assert_eq!(foreign.diagnostics[0].code, "ownership_conflict");
    let admitted = workspace.preview_item_mutations(&batch("alpha")?)?;
    assert!(admitted.ok, "{:?}", admitted.diagnostics);
    Ok(())
}

/// Scoped SDK identity is consistent across claim, collection, lifecycle and release.
#[test]
fn scoped_detected_principal_covers_all_ownership_comparisons() -> TestResult {
    let (directory, workspace) = fixture()?;
    let identity = AuthorIdentity::resolve(
        None,
        None,
        &BTreeMap::from([("CODEX_THREAD_ID".into(), "fixture-session-one".into())]),
    );
    let foreign = AuthorIdentity::resolve(
        None,
        None,
        &BTreeMap::from([("CODEX_THREAD_ID".into(), "fixture-session-two".into())]),
    );
    let request = request(&identity.author);
    let options = OwnershipOptions::default();
    identity.with(|| workspace.start_task(&request, &options))?;
    assert_eq!(
        workspace.get("sample-one")?.metadata.extra["claim_principal"],
        identity.principal
    );
    let collection = pm_rust::CollectionOptions {
        author: identity.author.clone(),
        add: vec!["README.md".into()],
        ..pm_rust::CollectionOptions::default()
    };
    assert!(foreign.with(|| workspace.claim(&request)).is_err());
    assert!(
        foreign
            .with(|| workspace.collection("sample-one", "files", &collection, directory.path()))
            .is_err()
    );
    assert!(
        foreign
            .with(|| workspace.pause_task(&request, &options))
            .is_err()
    );
    identity.with(|| workspace.collection("sample-one", "files", &collection, directory.path()))?;
    identity.with(|| workspace.pause_task(&request, &options))?;
    assert!(
        !workspace
            .get("sample-one")?
            .metadata
            .extra
            .contains_key("claim_principal")
    );
    Ok(())
}
