use std::fs;
use std::thread;
use std::time::Duration;

use proptest::prelude::*;
use tempfile::TempDir;

use crate::item::decode_item;

use super::*;

#[cfg(windows)]
#[path = "windows_fs.rs"]
mod windows_fs;

#[cfg(windows)]
#[path = "windows_acl.rs"]
mod windows_acl;

const TS: &str = "2026-08-07T10:06:30.183Z";

/// Lifecycle storage faults stop before mutation or retain a durable recovery journal.
#[test]
fn lifecycle_storage_faults_preserve_recovery_evidence() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root("{}")?;
    create_item(&pm_root, request())?;
    let mut mutation = OwnershipItem {
        id: "sample-unit".to_owned(),
        author: "unit-agent".to_owned(),
        timestamp: Some(TS.to_owned()),
        message: None,
        force: false,
        if_available: false,
        provenance_role: None,
    };
    let options = crate::OwnershipOptions::default();
    let settings_path = pm_root.join("settings.json");
    let item_path = pm_root.join("tasks/sample-unit.toon");
    let before = fs::read(&item_path)?;
    fs::write(&settings_path, "invalid")?;
    assert!(lifecycle_status(&pm_root, &mutation, &options, true).is_err());
    fs::write(&settings_path, "{}")?;
    mutation.id = "sample-absent".to_owned();
    assert!(lifecycle_status(&pm_root, &mutation, &options, true).is_err());
    mutation.id = "sample-unit".to_owned();
    fs::write(
        &settings_path,
        r#"{"schema":{"files":{"workflows":"missing.json"}}}"#,
    )?;
    assert!(matches!(
        lifecycle_status(&pm_root, &mutation, &options, true),
        Err(PmRustError::Io { .. })
    ));
    assert_eq!(fs::read(&item_path)?, before);
    fs::write(&settings_path, "{}")?;
    let history = pm_root.join("history/sample-unit.jsonl");
    fs::remove_file(&history)?;
    fs::create_dir(&history)?;
    assert!(matches!(
        lifecycle_status(&pm_root, &mutation, &options, true),
        Err(PmRustError::Io { .. })
    ));
    assert_eq!(
        decode_item(&item_path, &fs::read_to_string(&item_path)?)?
            .metadata
            .status,
        "in_progress"
    );
    assert!(
        pm_root
            .join("runtime/transactions/update-sample-unit.json")
            .is_file()
    );
    Ok(())
}

/// Session publication failures never silently undo a completed ownership transaction.
#[test]
fn ownership_session_faults_and_contention_preserve_durable_steps()
-> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root("{}")?;
    create_item(&pm_root, request())?;
    let workspace = crate::Workspace::discover(&pm_root)?;
    let mut mutation = OwnershipItem {
        id: "sample-unit".to_owned(),
        author: "unit-agent".to_owned(),
        timestamp: Some(TS.to_owned()),
        message: None,
        force: false,
        if_available: false,
        provenance_role: None,
    };
    let options = crate::OwnershipOptions::default();
    let session = pm_root.join("runtime/session.json");
    fs::create_dir(&session)?;
    assert!(matches!(
        workspace.claim_with_options(&mutation, &options),
        Err(PmRustError::Io { .. })
    ));
    assert_eq!(
        workspace.get("sample-unit")?.metadata.extra["assignee"],
        "unit-agent"
    );
    mutation.author = "other".to_owned();
    assert!(matches!(
        workspace.start_task(&mutation, &options),
        Err(PmRustError::OwnershipRefusal { .. })
    ));
    mutation.author = "unit-agent".to_owned();
    assert!(matches!(
        workspace.release_with_options(&mutation, &options),
        Err(PmRustError::Io { .. })
    ));
    assert!(
        !workspace
            .get("sample-unit")?
            .metadata
            .extra
            .contains_key("assignee")
    );
    fs::remove_dir(&session)?;
    let locks = LockSettings::default();
    let guard = acquire_lock(
        &pm_root,
        "session-state",
        "other",
        &locks,
        false,
        &now_iso(),
    )?;
    let called = std::cell::Cell::new(false);
    assert!(matches!(
        mutate_session_state(&pm_root, |_| called.set(true)),
        Err(PmRustError::LockConflict { .. })
    ));
    assert!(!called.get());
    drop(guard);
    mutate_session_state(&pm_root, |state| {
        state.insert("marker".to_owned(), serde_json::json!("published"));
    })?;
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&session)?)?["marker"],
        "published"
    );
    fs::create_dir(pm_root.join("locks/sample-unit.lock"))?;
    assert!(matches!(
        ownership_lock_refusal(&pm_root, &mutation, &read_settings(&pm_root)?, TS, 0),
        PmRustError::Io { .. }
    ));
    Ok(())
}

fn root(settings: &str) -> Result<(TempDir, PathBuf), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let pm_root = directory.path().join(".agents/pm");
    fs::create_dir_all(&pm_root)?;
    fs::write(pm_root.join("settings.json"), settings)?;
    Ok((directory, pm_root))
}

fn request() -> CreateItem {
    CreateItem {
        id: "sample-unit".to_owned(),
        title: "Unit create".to_owned(),
        description: String::new(),
        item_type: "Task".to_owned(),
        status: "open".to_owned(),
        priority: 2,
        tags: Vec::new(),
        body: String::new(),
        author: "unit-agent".to_owned(),
        timestamp: Some(TS.to_owned()),
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    }
}

fn settings(prefix: &str, format: &str, ttl: u64) -> String {
    format!(
        r#"{{"id_prefix":"{prefix}","item_format":"{format}","locks":{{"ttl_seconds":{ttl}}}}}"#
    )
}

type CompletedTransaction = (TempDir, PathBuf, String, String, MutationJournal);
/// Builds lock settings without a wait budget for deterministic unit tests.
const fn locks(ttl_seconds: u64) -> LockSettings {
    LockSettings {
        use_created_at: false,
        ttl_seconds,
        wait_ms: 0,
    }
}

#[test]
fn serde_defaults_match_the_supported_create_and_settings_contract()
-> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(type_folder("Plan"), Some("plans"));
    assert!(matches!(
        normalize_encoded_item(Err(ToonError::SerializationError(
            "injected encoder failure".to_owned()
        ))),
        Err(PmRustError::ItemEncoding { reason }) if reason.contains("injected encoder failure")
    ));
    assert!(matches!(
        normalize_item_bytes("title: \"unterminated\n"),
        Err(PmRustError::ItemEncoding { reason }) if reason.contains("unterminated")
    ));
    let mut reserved = request();
    reserved.title = "true".to_owned();
    let (_reserved_directory, reserved_root) = root(&settings("sample-", "toon", 1_800))?;
    let document = create_item(&reserved_root, reserved)?.item;
    assert!(canonical_item_bytes(&document)?.contains("title: \"true\"\n"));
    assert!(canonical_item_bytes(&document)?.contains("tags: []\n"));
    let mut ambiguous_tags = document.clone();
    ambiguous_tags.metadata.tags = ["0", "1.2", "0.", "1.", "false", "null", "true"]
        .map(str::to_owned)
        .to_vec();
    assert_eq!(
        decode_item(
            Path::new("ambiguous-tags.toon"),
            &canonical_item_bytes(&ambiguous_tags)?
        )?,
        ambiguous_tags
    );
    let mut ambiguous_body = document.clone();
    ambiguous_body.body = "0".to_owned();
    assert_eq!(
        decode_item(
            Path::new("ambiguous-body.toon"),
            &canonical_item_bytes(&ambiguous_body)?
        )?,
        ambiguous_body
    );
    let parsed: CreateItem = serde_json::from_str(
        r#"{"id":"sample-default","title":"Defaults","type":"Task","author":"agent"}"#,
    )?;
    assert_eq!(parsed.status, "open");
    assert_eq!(parsed.priority, 2);
    assert_eq!(parsed.description, "");
    assert_eq!(parsed.tags, Vec::<String>::new());
    assert_eq!(parsed.body, "");
    assert!(parsed.timestamp.is_none());
    assert!(parsed.message.is_none());
    assert!(!parsed.force_stale_lock);
    let settings: MutationSettings = serde_json::from_str("{}")?;
    assert_eq!(settings.id_prefix, "");
    assert_eq!(settings.item_format, "toon");
    assert_eq!(settings.locks.ttl_seconds, 1_800);
    let (_directory, pm_root) = root("{}")?;
    let mut created = request();
    created.id = "sample-parented".to_owned();
    let mut item = create_item(&pm_root, created)?.item;
    item.metadata.parent = Some("sample-parent".to_owned());
    let ordered = OrderedDocument::from_document(&item);
    assert_eq!(
        ordered.get("parent"),
        Some(&Value::String("sample-parent".to_owned()))
    );
    Ok(())
}

#[test]
/// Covers `now_iso` and `serialize_portable_path` in the unit-test binary.
///
/// The unit tests always provide an explicit timestamp and never serialize
/// a `CreateResult`, so `now_iso` and `serialize_portable_path` have a
/// zero-count instantiation in this binary. Creating an item without a
/// timestamp exercises `now_iso`, and serializing the result exercises
/// `serialize_portable_path`.
fn now_iso_and_serialize_portable_path_are_covered_in_the_unit_binary()
-> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root(&mutation_settings(0))?;
    let mut no_ts = request();
    no_ts.timestamp = None;
    let result = create_item(&pm_root, no_ts)?;
    // Serializing the result exercises `serialize_portable_path`.
    let serialized = serde_json::to_string(&result)?;
    assert!(serialized.contains("sample-unit"));
    // The item must have a non-empty, UTC-suffixed created_at from `now_iso`.
    assert!(result.item.metadata.created_at.ends_with('Z'));
    Ok(())
}

#[test]
fn validation_rejects_every_unsupported_request_shape() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root(&settings("sample-", "toon", 1_800))?;
    let mut cases = Vec::new();
    for id in [
        "",
        "-leading",
        "trailing-",
        "sample_UPPER",
        "../escape",
        "other-id",
    ] {
        let mut candidate = request();
        candidate.id = id.to_owned();
        cases.push(candidate);
    }
    for field in ["title", "type", "status", "author"] {
        let mut candidate = request();
        match field {
            "title" => candidate.title = "  ".to_owned(),
            "type" => candidate.item_type = String::new(),
            "status" => candidate.status = String::new(),
            "author" => candidate.author = String::new(),
            _ => unreachable!(),
        }
        cases.push(candidate);
    }
    let mut priority = request();
    priority.priority = 5;
    cases.push(priority);
    let mut custom_type = request();
    custom_type.item_type = "Custom".to_owned();
    cases.push(custom_type);
    let mut timestamp = request();
    timestamp.timestamp = Some("not-a-timestamp".to_owned());
    cases.push(timestamp);
    let mut empty_timestamp = request();
    empty_timestamp.timestamp = Some(String::new());
    cases.push(empty_timestamp);
    let mut malformed_utc_timestamp = request();
    malformed_utc_timestamp.timestamp = Some("2026-08-07T10:06:30Z-not-validZ".to_owned());
    cases.push(malformed_utc_timestamp);
    for candidate in cases {
        assert!(matches!(
            create_item(&pm_root, candidate),
            Err(PmRustError::InvalidCreateRequest { .. })
        ));
    }
    let (_directory, markdown_root) = root(&settings("sample-", "markdown", 1_800))?;
    assert!(matches!(
        create_item(&markdown_root, request()),
        Err(PmRustError::InvalidCreateRequest { .. })
    ));
    let (_directory, no_prefix_root) = root("{}")?;
    let mut no_prefix = request();
    no_prefix.id = "independent1".to_owned();
    no_prefix.author = "unknown".to_owned();
    assert_eq!(
        create_item(&no_prefix_root, no_prefix)?.item.metadata.id,
        "independent1"
    );
    Ok(())
}

/// A publication gate excludes contenders before initialization and permits abandoned recovery.
#[test]
fn initialization_gate_protects_a_lock_before_its_payload_is_published()
-> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root(&settings("sample-", "toon", 1_800))?;
    fs::create_dir_all(pm_root.join("locks"))?;
    let gate = pm_root.join("locks/sample-initializing.lock.stale-cleanup");
    fs::create_dir(&gate)?;
    let lock_settings = LockSettings {
        use_created_at: true,
        ..locks(1_800)
    };
    assert!(matches!(
        acquire_lock(
            &pm_root,
            "sample-initializing",
            "contender",
            &lock_settings,
            true,
            TS
        ),
        Err(PmRustError::LockConflict { .. })
    ));
    assert!(!pm_root.join("locks/sample-initializing.lock").exists());
    fs::remove_dir(&gate)?;
    let guard = acquire_lock(
        &pm_root,
        "sample-initializing",
        "initializer",
        &lock_settings,
        true,
        TS,
    )?;
    assert!(matches!(
        acquire_lock(
            &pm_root,
            "sample-initializing",
            "contender",
            &lock_settings,
            true,
            TS
        ),
        Err(PmRustError::LockConflict { .. })
    ));
    drop(guard);
    fs::create_dir(&gate)?;
    thread::sleep(Duration::from_millis(2));
    drop(acquire_lock(
        &pm_root,
        "sample-initializing",
        "recovered",
        &LockSettings {
            use_created_at: true,
            ..locks(0)
        },
        true,
        TS,
    )?);
    assert!(!gate.exists());
    assert!(matches!(
        acquire_lock_attempt(&pm_root, "invalid\0id", "fixture", 1_800, true, TS, true),
        Err(PmRustError::Io { .. })
    ));
    #[cfg(any(unix, windows))]
    {
        // Canonicalization supplies a verbatim Windows path, so only the
        // component limit applies: the lease fits, but the longer gate does not.
        let long_root = pm_root.canonicalize()?;
        let long_id = "x".repeat(237);
        let gate_path = long_root
            .join("locks")
            .join(format!("{long_id}.lock.stale-cleanup"));
        assert!(matches!(
            acquire_lock_attempt(&long_root, &long_id, "fixture", 1_800, true, TS, true),
            Err(PmRustError::Io { path, .. }) if path == gate_path
        ));
        assert!(
            long_root
                .join("locks")
                .join(format!("{long_id}.lock.gate-lease"))
                .is_file()
        );
    }
    Ok(())
}

/// A paused stale observer cannot reclaim the gate installed by another writer.
#[test]
fn stale_gate_reclamation_preserves_a_concurrent_replacement()
-> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root("{}")?;
    fs::create_dir_all(pm_root.join("locks"))?;
    let gate = pm_root.join("locks/sample-race.lock.stale-cleanup");
    fs::create_dir(&gate)?;
    thread::sleep(Duration::from_millis(2));
    let observed = GateObservation::read(&gate)?;
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (finish_tx, finish_rx) = std::sync::mpsc::channel();
    thread::scope(|scope| -> Result<(), Box<dyn std::error::Error>> {
        let writer_gate = &gate;
        let replacement = scope.spawn(move || -> std::io::Result<()> {
            // Keep the old inode allocated so identity cannot be reused.
            fs::rename(writer_gate, writer_gate.with_extension("old"))?;
            fs::create_dir(writer_gate)?;
            #[cfg(windows)]
            {
                let status = std::process::Command::new("powershell.exe")
                    .args([
                        "-NoProfile", "-NonInteractive", "-Command",
                        "[IO.Directory]::SetCreationTimeUtc($env:PM_TEST_GATE_CURRENT, [IO.Directory]::GetCreationTimeUtc($env:PM_TEST_GATE_OLD))",
                    ])
                    .env("PM_TEST_GATE_CURRENT", writer_gate)
                    .env("PM_TEST_GATE_OLD", writer_gate.with_extension("old"))
                    .status()?;
                assert!(status.success(), "could not set the replacement creation time");
            }
            ready_tx.send(()).map_err(std::io::Error::other)?;
            finish_rx.recv().map_err(std::io::Error::other)?;
            assert!(
                writer_gate.is_dir(),
                "stale observer deleted the fresh owner's gate"
            );
            Ok(())
        });
        ready_rx.recv()?;
        #[cfg(windows)]
        assert_eq!(
            observed.metadata.created()?,
            GateObservation::read(&gate)?.metadata.created()?
        );
        let reclaimed = reclaim_stale_gate(&gate, Some(&observed));
        finish_tx.send(())?;
        assert!(
            reclaimed.is_err(),
            "stale observer reclaimed a replacement gate"
        );
        replacement
            .join()
            .map_err(|_| "replacement writer panicked")??;
        Ok(())
    })?;
    // Missing identities and a kernel lease held by another writer fail closed.
    assert!(reclaim_stale_gate(&gate, None).is_err());
    let lease = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(gate.with_extension("gate-lease"))?;
    lease.try_lock()?;
    assert!(matches!(
        acquire_lock_attempt(&pm_root, "sample-race", "contender", 0, true, TS, true),
        Err(PmRustError::LockConflict { .. })
    ));
    assert!(gate.is_dir());
    // A child forked by another thread may inherit this open file description
    // until exec. Explicit unlock must free the lease while that clone lives.
    let inherited = lease.try_clone()?;
    drop(GateLease(lease));
    drop(acquire_lock_attempt(
        &pm_root,
        "sample-race",
        "successor",
        0,
        true,
        TS,
        true,
    )?);
    drop(inherited);
    fs::create_dir(&gate)?;
    // A nonempty gate can contain another runtime's ownership evidence.
    fs::write(gate.join("retained"), "evidence")?;
    assert!(reclaim_stale_gate(&gate, Some(&GateObservation::read(&gate)?)).is_err());
    assert_eq!(fs::read_to_string(gate.join("retained"))?, "evidence");
    let not_directory = pm_root.join("locks/not-a-directory");
    fs::write(&not_directory, "retained")?;
    assert!(
        reclaim_stale_gate(
            &not_directory,
            Some(&GateObservation::read(&not_directory)?)
        )
        .is_err()
    );
    assert_eq!(fs::read_to_string(&not_directory)?, "retained");
    assert!(GateObservation::read(&pm_root.join("missing-gate")).is_err());
    Ok(())
}

#[test]
fn lock_conflicts_and_forced_stale_cleanup_preserve_the_current_owner()
-> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root(&settings("sample-", "toon", 0))?;
    let first = acquire_lock(&pm_root, "sample-unit", "first", &locks(0), false, TS)?;
    assert!(matches!(
        acquire_lock(&pm_root, "sample-unit", "second", &locks(0), false, TS),
        Err(PmRustError::LockConflict { .. })
    ));
    let lock_path = pm_root.join("locks/sample-unit.lock");
    let lock_file = File::options().write(true).open(&lock_path)?;
    lock_file.set_times(
        fs::FileTimes::new().set_modified(SystemTime::now() + Duration::from_secs(60)),
    )?;
    assert!(matches!(
        acquire_lock(&pm_root, "sample-unit", "second", &locks(0), true, TS),
        Err(PmRustError::LockConflict { .. })
    ));
    lock_file.set_times(fs::FileTimes::new().set_modified(SystemTime::now()))?;
    thread::sleep(Duration::from_millis(2));
    let second = acquire_lock(&pm_root, "sample-unit", "second", &locks(0), true, TS)?;
    drop(first);
    assert!(pm_root.join("locks/sample-unit.lock").exists());
    drop(second);
    assert!(!pm_root.join("locks/sample-unit.lock").exists());

    let gated_lock = pm_root.join("locks/sample-gated.lock");
    fs::write(&gated_lock, "foreign")?;
    File::options()
        .write(true)
        .open(&gated_lock)?
        .set_times(fs::FileTimes::new().set_modified(UNIX_EPOCH))?;
    let gate = pm_root.join("locks/sample-gated.lock.stale-cleanup");
    fs::create_dir(&gate)?;
    thread::sleep(Duration::from_millis(2));
    assert!(matches!(
        acquire_lock(&pm_root, "sample-gated", "third", &locks(1_800), true, TS),
        Err(PmRustError::LockConflict { .. })
    ));
    let recovered_gate = acquire_lock(&pm_root, "sample-gated", "third", &locks(0), true, TS)?;
    assert!(!gate.exists());
    drop(recovered_gate);

    let blocked_lock = pm_root.join("locks/sample-blocked-gate.lock");
    fs::write(&blocked_lock, "foreign")?;
    let blocked_gate = pm_root.join("locks/sample-blocked-gate.lock.stale-cleanup");
    fs::create_dir(&blocked_gate)?;
    fs::write(blocked_gate.join("active-owner"), "present")?;
    thread::sleep(Duration::from_millis(2));
    assert!(matches!(
        acquire_lock(
            &pm_root,
            "sample-blocked-gate",
            "third",
            &locks(0),
            true,
            TS
        ),
        Err(PmRustError::LockConflict { .. })
    ));
    let directory_lock = pm_root.join("locks/sample-directory.lock");
    fs::create_dir(&directory_lock)?;
    assert!(matches!(
        acquire_lock(&pm_root, "sample-directory", "third", &locks(0), true, TS),
        Err(PmRustError::Io { .. })
    ));
    #[cfg(unix)]
    {
        let oversized_id = "x".repeat(300);
        assert!(matches!(
            acquire_lock(&pm_root, &oversized_id, "third", &locks(0), true, TS),
            Err(PmRustError::Io { .. })
        ));
    }
    Ok(())
}

fn completed_transaction() -> Result<CompletedTransaction, Box<dyn std::error::Error>> {
    let (directory, pm_root) = root(&settings("sample-", "toon", 1_800))?;
    create_item(&pm_root, request())?;
    let item = fs::read_to_string(pm_root.join("tasks/sample-unit.toon"))?;
    let history = fs::read_to_string(pm_root.join("history/sample-unit.jsonl"))?;
    let journal = MutationJournal {
        version: 1,
        id: "sample-unit".to_owned(),
        item_type: "Task".to_owned(),
        item_bytes: item.clone(),
        history_bytes: history.clone(),
        before_item_hash: EMPTY_DOCUMENT_HASH.to_owned(),
    };
    Ok((directory, pm_root, item, history, journal))
}

fn write_journal(
    pm_root: &Path,
    journal: &MutationJournal,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = pm_root.join("runtime/transactions/create-sample-unit.json");
    fs::create_dir_all(path.parent().ok_or("journal parent")?)?;
    fs::write(path, serde_json::to_string_pretty(journal)?)?;
    Ok(())
}

#[test]
fn recovery_completes_or_rolls_back_every_durable_presence_state()
-> Result<(), Box<dyn std::error::Error>> {
    for (keep_item, keep_history) in [(false, false), (true, false), (false, true), (true, true)] {
        let (_directory, pm_root, item, history, journal) = completed_transaction()?;
        if !keep_item {
            fs::remove_file(pm_root.join("tasks/sample-unit.toon"))?;
        }
        if !keep_history {
            fs::remove_file(pm_root.join("history/sample-unit.jsonl"))?;
        }
        write_journal(&pm_root, &journal)?;
        recover(&pm_root, "sample-unit")?;
        assert!(
            !pm_root
                .join("runtime/transactions/create-sample-unit.json")
                .exists()
        );
        if keep_item || keep_history {
            assert_eq!(
                fs::read_to_string(pm_root.join("tasks/sample-unit.toon"))?,
                item
            );
            assert_eq!(
                fs::read_to_string(pm_root.join("history/sample-unit.jsonl"))?,
                history
            );
        } else {
            assert!(!pm_root.join("tasks/sample-unit.toon").exists());
            assert!(!pm_root.join("history/sample-unit.jsonl").exists());
        }
    }
    Ok(())
}

#[test]
fn recovery_refuses_invalid_mismatched_and_unsupported_journals()
-> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root, _item, _history, mut journal) = completed_transaction()?;
    write_journal(&pm_root, &journal)?;
    fs::write(pm_root.join("tasks/sample-unit.toon"), "foreign")?;
    assert!(matches!(
        recover(&pm_root, "sample-unit"),
        Err(PmRustError::RecoveryConflict { .. })
    ));

    let (_directory, history_root, _item, _history, history_journal) = completed_transaction()?;
    write_journal(&history_root, &history_journal)?;
    fs::write(
        history_root.join("history/sample-unit.jsonl"),
        "foreign history",
    )?;
    assert!(matches!(
        recover(&history_root, "sample-unit"),
        Err(PmRustError::RecoveryConflict { .. })
    ));

    let (_directory, item_read_root, _item, _history, item_read_journal) = completed_transaction()?;
    write_journal(&item_read_root, &item_read_journal)?;
    fs::remove_file(item_read_root.join("tasks/sample-unit.toon"))?;
    fs::create_dir(item_read_root.join("tasks/sample-unit.toon"))?;
    assert!(matches!(
        recover(&item_read_root, "sample-unit"),
        Err(PmRustError::Io { .. })
    ));

    let (_directory, history_read_root, _item, _history, history_read_journal) =
        completed_transaction()?;
    write_journal(&history_read_root, &history_read_journal)?;
    fs::remove_file(history_read_root.join("history/sample-unit.jsonl"))?;
    fs::create_dir(history_read_root.join("history/sample-unit.jsonl"))?;
    assert!(matches!(
        recover(&history_read_root, "sample-unit"),
        Err(PmRustError::Io { .. })
    ));

    fs::write(
        pm_root.join("runtime/transactions/create-sample-unit.json"),
        "not json",
    )?;
    assert!(matches!(
        recover(&pm_root, "sample-unit"),
        Err(PmRustError::RecoveryConflict { .. })
    ));
    journal.version = 2;
    write_journal(&pm_root, &journal)?;
    assert!(matches!(
        recover(&pm_root, "sample-unit"),
        Err(PmRustError::RecoveryConflict { .. })
    ));
    journal.version = 1;
    journal.id = "sample-other".to_owned();
    write_journal(&pm_root, &journal)?;
    assert!(matches!(
        recover(&pm_root, "sample-unit"),
        Err(PmRustError::RecoveryConflict { .. })
    ));
    journal.id = "sample-unit".to_owned();
    journal.item_type = "Custom".to_owned();
    write_journal(&pm_root, &journal)?;
    assert!(matches!(
        recover(&pm_root, "sample-unit"),
        Err(PmRustError::RecoveryConflict { .. })
    ));
    fs::remove_file(pm_root.join("runtime/transactions/create-sample-unit.json"))?;
    fs::create_dir(pm_root.join("runtime/transactions/create-sample-unit.json"))?;
    assert!(matches!(
        recover(&pm_root, "sample-unit"),
        Err(PmRustError::Io { .. })
    ));
    Ok(())
}

#[test]
fn recovery_surfaces_failures_while_restoring_either_half() -> Result<(), Box<dyn std::error::Error>>
{
    let (_directory, history_root, _item, _history, history_journal) = completed_transaction()?;
    write_journal(&history_root, &history_journal)?;
    fs::remove_file(history_root.join("history/sample-unit.jsonl"))?;
    fs::remove_dir(history_root.join("history"))?;
    fs::write(history_root.join("history"), "blocks restoration")?;
    assert!(matches!(
        recover(&history_root, "sample-unit"),
        Err(PmRustError::Io { .. })
    ));

    let (_directory, item_root, _item, _history, item_journal) = completed_transaction()?;
    write_journal(&item_root, &item_journal)?;
    fs::remove_file(item_root.join("tasks/sample-unit.toon"))?;
    fs::remove_dir(item_root.join("tasks"))?;
    fs::write(item_root.join("tasks"), "blocks restoration")?;
    assert!(matches!(
        recover(&item_root, "sample-unit"),
        Err(PmRustError::Io { .. })
    ));
    Ok(())
}

#[test]
fn missing_and_invalid_settings_return_typed_errors() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let pm_root = directory.path().join(".agents/pm");
    fs::create_dir_all(&pm_root)?;
    assert!(matches!(
        create_item(&pm_root, request()),
        Err(PmRustError::Io { .. })
    ));
    fs::write(pm_root.join("settings.json"), "not json")?;
    assert!(matches!(
        create_item(&pm_root, request()),
        Err(PmRustError::InvalidCreateRequest { .. })
    ));
    Ok(())
}

#[test]
#[allow(clippy::too_many_lines)]
fn filesystem_failures_are_typed_and_atomic_temps_are_cleaned()
-> Result<(), Box<dyn std::error::Error>> {
    assert!(matches!(
        atomic_write(Path::new("/"), "value"),
        Err(PmRustError::InvalidCreateRequest { .. })
    ));
    let directory = tempfile::tempdir()?;
    let blocker = directory.path().join("blocker");
    fs::write(&blocker, "file")?;
    assert!(matches!(
        atomic_write(&blocker.join("target"), "value"),
        Err(PmRustError::Io { .. })
    ));
    let target_directory = directory.path().join("target-directory");
    fs::create_dir(&target_directory)?;
    assert!(matches!(
        atomic_write(&target_directory, "value"),
        Err(PmRustError::Io { .. })
    ));
    assert!(matches!(
        remove_file(&target_directory),
        Err(PmRustError::Io { .. })
    ));
    let occupied_temporary = directory.path().join("occupied.tmp");
    fs::write(&occupied_temporary, "occupied")?;
    assert!(matches!(
        atomic_write_with_temporary(
            &directory.path().join("unused-target"),
            &occupied_temporary,
            "value"
        ),
        Err(PmRustError::Io { .. })
    ));
    let read_only_temporary = directory.path().join("read-only.tmp");
    fs::write(&read_only_temporary, "original")?;
    let read_only = File::open(&read_only_temporary)?;
    assert!(matches!(
        commit_temporary(
            read_only,
            &read_only_temporary,
            &directory.path().join("unused-write-target"),
            "value"
        ),
        Err(PmRustError::Io { .. })
    ));
    assert_eq!(
        fs::read_dir(directory.path())?
            .filter_map(Result::ok)
            .filter(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                name.starts_with('.') && name.ends_with(".tmp")
            })
            .count(),
        0
    );
    assert!(matches!(
        read_optional(directory.path()),
        Err(PmRustError::Io { .. })
    ));

    let pm_root = directory.path().join("pm-root");
    fs::create_dir_all(&pm_root)?;
    fs::write(pm_root.join("locks"), "not a directory")?;
    assert!(matches!(
        acquire_lock(
            &pm_root,
            "sample-lock",
            "agent",
            &LockSettings::default(),
            false,
            TS
        ),
        Err(PmRustError::Io { .. })
    ));

    let lock_root = directory.path().join("lock-root");
    fs::create_dir_all(&lock_root)?;
    let lock_path = lock_root.join("existing.lock");
    fs::write(&lock_path, "existing")?;
    assert!(matches!(
        create_lock_file(&lock_path, "replacement", "sample-lock"),
        Err(PmRustError::LockConflict { .. })
    ));
    assert!(matches!(
        remove_stale_lock(&lock_path, "different", "sample-lock"),
        Err(PmRustError::LockConflict { .. })
    ));
    fs::remove_file(&lock_path)?;
    assert!(matches!(
        remove_stale_lock(&lock_path, "existing", "sample-lock"),
        Err(PmRustError::Io { .. })
    ));
    let read_only_lock_path = lock_root.join("read-only.lock");
    fs::write(&read_only_lock_path, "")?;
    let read_only_lock = File::open(&read_only_lock_path)?;
    assert!(matches!(
        write_lock_file(read_only_lock, &read_only_lock_path, "payload"),
        Err(PmRustError::Io { .. })
    ));
    let missing_guard = ItemLock {
        path: lock_root.join("missing.lock"),
        raw: "missing".to_owned(),
    };
    drop(missing_guard);
    Ok(())
}

#[test]
fn atomic_publication_never_replaces_existing_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let existing_target = directory.path().join("existing-target");
    fs::write(&existing_target, "foreign bytes")?;
    assert!(matches!(
        atomic_write(&existing_target, "replacement bytes"),
        Err(PmRustError::Io { .. })
    ));
    assert_eq!(fs::read_to_string(&existing_target)?, "foreign bytes");
    Ok(())
}

#[cfg(unix)]
#[test]
fn parent_directory_sync_reports_real_path_and_sync_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let missing_parent = directory.path().join("missing");
    assert!(matches!(
        sync_parent(&missing_parent.join("target")),
        Err(PmRustError::Io { path, .. }) if path == missing_parent
    ));
    assert!(matches!(
        sync_parent(Path::new("/")),
        Err(PmRustError::InvalidCreateRequest { .. })
    ));
    // Opening the null device succeeds, but syncing it as a directory fails.
    // Unlike procfs, this also exercises the sync error on macOS.
    assert!(matches!(
        sync_parent(Path::new("/dev/null/target")),
        Err(PmRustError::Io { path, .. }) if path == Path::new("/dev/null")
    ));
    #[cfg(target_os = "linux")]
    assert!(matches!(
        sync_parent(Path::new("/proc/target")),
        Err(PmRustError::Io { path, .. }) if path == Path::new("/proc")
    ));
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn canonical_create_toon_round_trips_supported_strings(
        description in "[ -~]{0,40}",
        body in "[ -~]{0,80}",
        tags in prop::collection::vec("[A-Za-z0-9._/-]{1,12}", 0..5),
    ) {
        let mut extra = Map::new();
        extra.insert("author".to_owned(), Value::String("property-agent".to_owned()));
        let document = ItemDocument {
            metadata: ItemMetadata {
                id: "sample-property".to_owned(),
                title: "Property create".to_owned(),
                description,
                item_type: "Task".to_owned(),
                status: "open".to_owned(),
                priority: 2,
                tags,
                created_at: TS.to_owned(),
                updated_at: TS.to_owned(),
                parent: None,
                extra,
            },
            body,
        };
        let decoded = decode_item(
            Path::new("property.toon"),
            &canonical_item_bytes(&document)?
        )?;
        prop_assert_eq!(decoded, document);
    }
}

#[test]
fn an_interrupted_history_write_is_recovered_before_the_retry_conflicts()
-> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root(&settings("sample-", "toon", 1_800))?;
    fs::write(pm_root.join("history"), "blocks the history directory")?;
    assert!(matches!(
        create_item(&pm_root, request()),
        Err(PmRustError::Io { .. })
    ));
    assert!(pm_root.join("tasks/sample-unit.toon").is_file());
    assert!(
        pm_root
            .join("runtime/transactions/create-sample-unit.json")
            .is_file()
    );
    fs::remove_file(pm_root.join("history"))?;
    fs::create_dir(pm_root.join("history"))?;
    assert!(matches!(
        create_item(&pm_root, request()),
        Err(PmRustError::ItemAlreadyExists { .. })
    ));
    assert!(pm_root.join("history/sample-unit.jsonl").is_file());
    assert!(
        !pm_root
            .join("runtime/transactions/create-sample-unit.json")
            .exists()
    );
    Ok(())
}

#[test]
fn create_surfaces_lock_recovery_journal_and_item_stage_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let (_directory, lock_root) = root(&settings("sample-", "toon", 1_800))?;
    let held = acquire_lock(
        &lock_root,
        "sample-unit",
        "holder",
        &LockSettings::default(),
        false,
        TS,
    )?;
    assert!(matches!(
        create_item(&lock_root, request()),
        Err(PmRustError::LockConflict { .. })
    ));
    drop(held);

    let (_directory, recovery_root) = root(&settings("sample-", "toon", 1_800))?;
    let invalid_journal = recovery_root.join("runtime/transactions/create-sample-unit.json");
    fs::create_dir_all(invalid_journal.parent().ok_or("journal parent")?)?;
    fs::write(&invalid_journal, "invalid")?;
    assert!(matches!(
        create_item(&recovery_root, request()),
        Err(PmRustError::RecoveryConflict { .. })
    ));

    let (_directory, journal_root) = root(&settings("sample-", "toon", 1_800))?;
    fs::write(journal_root.join("runtime"), "blocks transaction directory")?;
    assert!(matches!(
        create_item(&journal_root, request()),
        Err(PmRustError::Io { .. })
    ));

    let (_directory, item_root) = root(&settings("sample-", "toon", 1_800))?;
    fs::write(item_root.join("tasks"), "blocks item directory")?;
    assert!(matches!(
        create_item(&item_root, request()),
        Err(PmRustError::Io { .. })
    ));
    assert!(
        item_root
            .join("runtime/transactions/create-sample-unit.json")
            .is_file()
    );

    let (_directory, history_only_root) = root(&settings("sample-", "toon", 1_800))?;
    fs::create_dir(history_only_root.join("history"))?;
    fs::write(
        history_only_root.join("history/sample-unit.jsonl"),
        "existing history",
    )?;
    assert!(matches!(
        create_item(&history_only_root, request()),
        Err(PmRustError::ItemAlreadyExists { .. })
    ));
    Ok(())
}

#[cfg(any(unix, windows))]
#[test]
fn non_utf8_atomic_target_names_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
    use std::ffi::OsString;
    #[cfg(unix)]
    use std::os::unix::ffi::OsStringExt;
    #[cfg(windows)]
    use std::os::windows::ffi::OsStringExt;

    let directory = tempfile::tempdir()?;
    #[cfg(unix)]
    let name = OsString::from_vec(vec![0xff]);
    #[cfg(windows)]
    let name = OsString::from_wide(&[0xD800]);
    let target = directory.path().join(name);
    #[cfg(windows)]
    {
        fs::write(&target, "original")?;
        assert!(target.is_file());
    }
    assert!(matches!(
        atomic_write(&target, "value"),
        Err(PmRustError::InvalidCreateRequest { .. })
    ));
    Ok(())
}

fn mutation_settings(wait_ms: u64) -> String {
    format!(
        r#"{{"id_prefix":"sample-","item_format":"toon","locks":{{"ttl_seconds":1800,"wait_ms":{wait_ms}}}}}"#
    )
}

#[test]
/// Covers the shared staging failure path with an unwritable handle.
fn stage_temporary_reports_write_and_sync_failures() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let read_only_path = directory.path().join("read-only.tmp");
    fs::write(&read_only_path, "original")?;
    let read_only = File::open(&read_only_path)?;
    assert!(matches!(
        stage_temporary(read_only, &directory.path().join("unused.tmp"), "value"),
        Err(PmRustError::Io { .. })
    ));
    Ok(())
}

#[test]
/// Covers every typed filesystem failure of the replacement publisher.
fn atomic_replace_surfaces_stage_and_publish_failures() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let blocker = directory.path().join("blocker");
    fs::write(&blocker, "file")?;
    // A parent that cannot become a directory fails during staging setup.
    assert!(matches!(
        atomic_replace(&blocker.join("target"), "value"),
        Err(PmRustError::Io { .. })
    ));
    // A directory target cannot be replaced by a rename.
    let target_directory = directory.path().join("target-directory");
    fs::create_dir(&target_directory)?;
    let staged = directory.path().join("staged.tmp");
    fs::write(&staged, "staged")?;
    assert!(matches!(
        publish_replacement(&staged, &target_directory),
        Err(PmRustError::Io { .. })
    ));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let locked = directory.path().join("locked");
        fs::create_dir(&locked)?;
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o555))?;
        assert!(matches!(
            atomic_replace(&locked.join("sample-x.toon"), "value"),
            Err(PmRustError::Io { .. })
        ));
    }
    Ok(())
}

/// Failed publication preserves existing bytes, and success replaces without a prior unlink.
#[test]
fn replacement_failure_preserves_existing_target() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let target = directory.path().join("item.toon");
    let staged = directory.path().join("staged.tmp");
    fs::write(&target, "original")?;
    assert!(matches!(
        publish_replacement(&staged, &target),
        Err(PmRustError::Io { path, .. }) if path == target
    ));
    assert_eq!(fs::read_to_string(&target)?, "original");
    fs::write(&staged, "replacement")?;
    publish_replacement(&staged, &target)?;
    assert_eq!(fs::read_to_string(&target)?, "replacement");
    assert!(!staged.exists());
    Ok(())
}

#[test]
/// Covers history-append open and write failures as typed errors.
fn append_history_line_surfaces_open_and_write_failures() -> Result<(), Box<dyn std::error::Error>>
{
    let directory = tempfile::tempdir()?;
    let occupied = directory.path().join("occupied.jsonl");
    fs::create_dir(&occupied)?;
    assert!(matches!(
        append_history_line(&occupied, "{\"op\":\"update\"}\n"),
        Err(PmRustError::Io { .. })
    ));
    // Null devices accept writes but cannot flush durable history. Exercise
    // that error on each OS instead of relying only on Linux's full device.
    #[cfg(unix)]
    assert!(matches!(
        append_history_line(Path::new("/dev/null"), "line\n"),
        Err(PmRustError::Io { .. })
    ));
    #[cfg(windows)]
    assert!(matches!(
        append_history_line(Path::new("NUL"), "line\n"),
        Err(PmRustError::Io { .. })
    ));
    #[cfg(target_os = "linux")]
    {
        // The Linux null-device convention fails every write after opening.
        assert!(matches!(
            append_history_line(Path::new("/dev/full"), "line\n"),
            Err(PmRustError::Io { .. })
        ));
    }
    Ok(())
}

#[test]
/// Proves update, comment, and close fail fast on a live item lock.
fn in_place_mutations_refuse_a_live_lock_within_the_budget()
-> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    let _held = acquire_lock(&pm_root, "sample-unit", "holder", &locks(1_800), false, TS)?;
    let mut update = UpdateItem {
        id: "sample-unit".to_owned(),
        title: Some("Next".to_owned()),
        description: None,
        status: None,
        priority: None,
        tags: None,
        body: None,
        author: "unit-agent".to_owned(),
        timestamp: Some(TS.to_owned()),
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    };
    assert!(matches!(
        update_item(&pm_root, update.clone()),
        Err(PmRustError::LockConflict { .. })
    ));
    assert!(matches!(
        comment_item(
            &pm_root,
            &CommentItem {
                id: "sample-unit".to_owned(),
                text: "text".to_owned(),
                author: "unit-agent".to_owned(),
                timestamp: Some(TS.to_owned()),
                message: None,
                provenance_role: None,
                force_stale_lock: false,
            }
        ),
        Err(PmRustError::LockConflict { .. })
    ));
    assert!(matches!(
        close_item(
            &pm_root,
            &pm_root,
            CloseItem {
                id: "sample-unit".to_owned(),
                reason: "reason".to_owned(),
                author: "unit-agent".to_owned(),
                timestamp: Some(TS.to_owned()),
                provenance_role: None,
                fields: CloseFields::default(),
                force_stale_lock: false,
            }
        ),
        Err(PmRustError::LockConflict { .. })
    ));
    update.title = Some("  ".to_owned());
    let (_other, unlocked_root) = root(&mutation_settings(0))?;
    create_item(
        &unlocked_root,
        CreateItem {
            id: "sample-unit".to_owned(),
            title: "Unit create".to_owned(),
            description: String::new(),
            item_type: "Task".to_owned(),
            status: "open".to_owned(),
            priority: 2,
            tags: Vec::new(),
            body: String::new(),
            author: "unit-agent".to_owned(),
            timestamp: Some(TS.to_owned()),
            message: None,
            provenance_role: None,
            force_stale_lock: false,
        },
    )?;
    // The live lock above still holds `pm_root`, so the blank-title refusal
    // is asserted against the unlocked tracker, which holds the item and no lock.
    assert!(matches!(
        update_item(&unlocked_root, update),
        Err(PmRustError::InvalidMutationRequest { .. })
    ));
    Ok(())
}

#[test]
/// Covers blank-author validation shared by every in-place mutation.
fn mutations_reject_blank_authors_before_any_locking() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    let result = validate_mutation_request("   ", None);
    assert!(matches!(
        result,
        Err(PmRustError::InvalidMutationRequest { .. })
    ));
    Ok(())
}

#[test]
/// Covers the whole-field update surface including tag canonicalization.
fn update_applies_every_supported_field_and_canonicalizes_tags()
-> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    let result = update_item(
        &pm_root,
        UpdateItem {
            id: "sample-unit".to_owned(),
            title: Some("Updated title".to_owned()),
            description: Some("Updated description".to_owned()),
            status: Some("in_progress".to_owned()),
            priority: Some(4),
            tags: Some(vec![
                "zulu".to_owned(),
                "alpha".to_owned(),
                "zulu".to_owned(),
            ]),
            body: Some("Updated body".to_owned()),
            author: "unit-agent".to_owned(),
            timestamp: Some(TS.to_owned()),
            message: Some("whole-field update".to_owned()),
            provenance_role: None,
            force_stale_lock: false,
        },
    )?;
    assert_eq!(result.item.metadata.tags, vec!["alpha", "zulu"]);
    assert_eq!(result.item.body, "Updated body");
    let stored = decode_item(
        &pm_root.join("tasks/sample-unit.toon"),
        &fs::read_to_string(pm_root.join("tasks/sample-unit.toon"))?,
    )?;
    assert_eq!(stored.metadata.description, "Updated description");
    assert_eq!(stored.metadata.priority, 4);
    Ok(())
}

#[test]
/// Covers the remaining per-field refusals of the update surface.
fn update_refuses_blank_titles_statuses_and_out_of_range_priorities()
-> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    for request in [
        UpdateItem {
            id: "sample-unit".to_owned(),
            title: Some("Keep".to_owned()),
            description: None,
            status: Some("   ".to_owned()),
            priority: None,
            tags: None,
            body: None,
            author: "unit-agent".to_owned(),
            timestamp: Some(TS.to_owned()),
            message: None,
            provenance_role: None,
            force_stale_lock: false,
        },
        UpdateItem {
            id: "sample-unit".to_owned(),
            title: Some("Keep".to_owned()),
            description: None,
            status: None,
            priority: Some(5),
            tags: None,
            body: None,
            author: "unit-agent".to_owned(),
            timestamp: Some(TS.to_owned()),
            message: None,
            provenance_role: None,
            force_stale_lock: false,
        },
    ] {
        assert!(matches!(
            update_item(&pm_root, request),
            Err(PmRustError::InvalidMutationRequest { .. })
        ));
    }
    Ok(())
}

#[test]
/// Reports items stored under more than one type folder as duplicates.
fn locate_item_reports_duplicates_across_type_folders() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    fs::create_dir(pm_root.join("issues"))?;
    fs::copy(
        pm_root.join("tasks/sample-unit.toon"),
        pm_root.join("issues/sample-unit.toon"),
    )?;
    assert!(matches!(
        locate_item(&pm_root, "sample-unit"),
        Err(PmRustError::DuplicateItemId { .. })
    ));
    Ok(())
}

/// Writes one durable in-place mutation journal for the sample item.
///
/// The `before_item_hash` is read from the item currently on disk so the
/// journal mirrors what `commit_mutation` records: the hash of the
/// pre-mutation document. Scenarios that leave the pre-mutation item in place
/// before writing the journal therefore produce a journal recovery can roll
/// forward; scenarios that remove or corrupt the item first produce a hash
/// that matches neither image, so recovery refuses.
fn write_mutation_journal(
    pm_root: &Path,
    operation: &str,
    item_bytes: &str,
    history_bytes: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let item_path = pm_root.join("tasks/sample-unit.toon");
    let before_item_hash = fs::read_to_string(&item_path)
        .ok()
        .and_then(|bytes| {
            crate::item::decode_item(&item_path, &bytes)
                .ok()
                .map(|document| history::document_hash(&OrderedDocument::from_document(&document)))
        })
        .unwrap_or_else(|| EMPTY_DOCUMENT_HASH.to_owned());
    let journal = MutationJournal {
        version: 1,
        id: "sample-unit".to_owned(),
        item_type: "Task".to_owned(),
        item_bytes: item_bytes.to_owned(),
        history_bytes: history_bytes.to_owned(),
        before_item_hash,
    };
    let journal_path = pm_root
        .join("runtime/transactions")
        .join(format!("{operation}-sample-unit.json"));
    let _ = fs::remove_file(&journal_path);
    let serialized = serde_json::to_string(&journal).map_err(|error| format!("{error}"))?;
    fs::write(&journal_path, format!("{serialized}\n"))?;
    Ok(())
}

#[test]
#[allow(clippy::too_many_lines)]
/// Covers every journal state an interrupted in-place mutation can leave.
fn mutation_recovery_repairs_completes_and_refuses_every_state()
-> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    let item_path = pm_root.join("tasks/sample-unit.toon");
    let history_path = pm_root.join("history/sample-unit.jsonl");
    let original_item = fs::read_to_string(&item_path)?;
    let original_history = fs::read_to_string(&history_path)?;
    let updated_item = original_item.replace("Unit create", "Recovered title");
    // `commit_mutation` stores ONE history line and `append_history_line` writes
    // it verbatim, so a journal carrying the whole stream makes recovery append
    // the entire history again. The line and the resulting file are separate
    // values here for that reason.
    let history_line = "{\"ts\":\"stub\",\"op\":\"update\"}\n".to_owned();
    let appended_history = format!("{original_history}{history_line}");

    // A journal whose transaction fully committed is simply removed.
    write_mutation_journal(&pm_root, "update", &updated_item, &history_line)?;
    fs::write(&item_path, &updated_item)?;
    fs::write(&history_path, &appended_history)?;
    recover_mutation(&pm_root, "update", "sample-unit")?;
    assert!(
        !pm_root
            .join("runtime/transactions/update-sample-unit.json")
            .exists()
    );

    // A journal whose halves are absent is completed from its bytes.
    write_mutation_journal(&pm_root, "update", &updated_item, &history_line)?;
    fs::remove_file(&item_path)?;
    fs::remove_file(&history_path)?;
    recover_mutation(&pm_root, "update", "sample-unit")?;
    assert_eq!(fs::read_to_string(&item_path)?, updated_item);
    // Exactly the journalled line: the stream was absent, so recovery wrote
    // only what the journal carried.
    assert_eq!(fs::read_to_string(&history_path)?, history_line);
    // Restore the pre-mutation state for the conflict cases below.
    fs::write(&item_path, &original_item)?;
    fs::write(&history_path, &original_history)?;

    // A crash before the item replace leaves the pre-mutation item in place.
    // The journal records the pre-mutation hash, so recovery recognises the
    // before-image and rolls the whole transaction forward instead of
    // blocking the identifier permanently.
    write_mutation_journal(&pm_root, "update", &updated_item, &history_line)?;
    recover_mutation(&pm_root, "update", "sample-unit")?;
    assert_eq!(fs::read_to_string(&item_path)?, updated_item);
    assert!(fs::read_to_string(&history_path)?.ends_with(&history_line));
    assert!(
        !pm_root
            .join("runtime/transactions/update-sample-unit.json")
            .exists()
    );
    fs::write(&item_path, &original_item)?;
    fs::write(&history_path, &original_history)?;

    // A crash after the item replace but before the history append leaves the
    // post-mutation item beside the pre-mutation history. Recovery recognises
    // the after-image and completes the history half instead of refusing.
    write_mutation_journal(&pm_root, "update", &updated_item, &history_line)?;
    fs::write(&item_path, &updated_item)?;
    recover_mutation(&pm_root, "update", "sample-unit")?;
    assert_eq!(fs::read_to_string(&item_path)?, updated_item);
    assert!(fs::read_to_string(&history_path)?.ends_with(&history_line));
    assert!(
        !pm_root
            .join("runtime/transactions/update-sample-unit.json")
            .exists()
    );
    fs::write(&item_path, &original_item)?;
    fs::write(&history_path, &original_history)?;

    // Foreign durable bytes that match neither the pre-mutation nor the
    // post-mutation image refuse recovery instead of being overwritten.
    write_mutation_journal(&pm_root, "update", &updated_item, &history_line)?;
    let foreign_item = original_item.replace("Unit create", "Foreign title");
    fs::write(&item_path, &foreign_item)?;
    assert!(matches!(
        recover_mutation(&pm_root, "update", "sample-unit"),
        Err(PmRustError::RecoveryConflict { .. })
    ));
    fs::write(&item_path, &original_item)?;
    let _ = fs::remove_file(
        pm_root
            .join("runtime/transactions")
            .join("update-sample-unit.json"),
    );

    // A history stream that diverged from the journal still rolls forward when
    // the item matches the before-image: the item half is the authority, and
    // the history half is append-only, so recovery replays the missing line.
    write_mutation_journal(&pm_root, "update", &updated_item, &history_line)?;
    fs::write(
        &history_path,
        format!("{original_history}{{\"foreign\":1}}\n"),
    )?;
    recover_mutation(&pm_root, "update", "sample-unit")?;
    assert_eq!(fs::read_to_string(&item_path)?, updated_item);
    assert!(fs::read_to_string(&history_path)?.ends_with(&history_line));
    fs::write(&item_path, &original_item)?;
    fs::write(&history_path, &original_history)?;

    // Invalid JSON, identity mismatches, and unknown types all refuse.
    let journal_path = pm_root
        .join("runtime/transactions")
        .join("update-sample-unit.json");
    fs::write(&journal_path, "not json")?;
    assert!(matches!(
        recover_mutation(&pm_root, "update", "sample-unit"),
        Err(PmRustError::RecoveryConflict { .. })
    ));
    write_mutation_journal(&pm_root, "update", &updated_item, &history_line)?;
    let mut mismatched: MutationJournal =
        serde_json::from_str(&fs::read_to_string(&journal_path)?)?;
    mismatched.id = "other-id".to_owned();
    fs::write(
        &journal_path,
        &serde_json::to_string(&mismatched).map_err(|error| format!("{error}"))?,
    )?;
    assert!(matches!(
        recover_mutation(&pm_root, "update", "sample-unit"),
        Err(PmRustError::RecoveryConflict { .. })
    ));
    mismatched.id = "sample-unit".to_owned();
    mismatched.item_type = "Scroll".to_owned();
    fs::write(
        &journal_path,
        &serde_json::to_string(&mismatched).map_err(|error| format!("{error}"))?,
    )?;
    assert!(matches!(
        recover_mutation(&pm_root, "update", "sample-unit"),
        Err(PmRustError::RecoveryConflict { .. })
    ));
    Ok(())
}

#[test]
/// Covers recovery replay when the item publication itself fails.
#[cfg(unix)]
fn recovery_surfaces_item_replay_publish_failures() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    let item_path = pm_root.join("tasks/sample-unit.toon");
    let history_path = pm_root.join("history/sample-unit.jsonl");
    let original_history = fs::read_to_string(&history_path)?;
    let updated_item = fs::read_to_string(&item_path)?.replace("Unit create", "Recovered title");
    write_mutation_journal(&pm_root, "update", &updated_item, &original_history)?;

    // The absent item is replayable, but its storage directory refuses writes,
    // so staging the replayed document must surface as a typed IO error.
    fs::remove_file(&item_path)?;
    fs::set_permissions(pm_root.join("tasks"), fs::Permissions::from_mode(0o555))?;
    assert!(matches!(
        recover_mutation(&pm_root, "update", "sample-unit"),
        Err(PmRustError::Io { .. })
    ));
    fs::set_permissions(pm_root.join("tasks"), fs::Permissions::from_mode(0o755))?;
    Ok(())
}

#[cfg(windows)]
#[test]
fn recovery_roll_forward_refuses_a_read_shared_item_then_retries_idempotently()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_SHARE_READ: u32 = 1;
    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    let item_path = pm_root.join("tasks/sample-unit.toon");
    let history_path = pm_root.join("history/sample-unit.jsonl");
    let journal_path = pm_root.join("runtime/transactions/update-sample-unit.json");
    let original_item = fs::read_to_string(&item_path)?;
    let original_history = fs::read_to_string(&history_path)?;
    let updated_item = original_item.replace("Unit create", "Recovered title");
    let history_line = "{\"ts\":\"stub\",\"op\":\"update\"}\n";
    write_mutation_journal(&pm_root, "update", &updated_item, history_line)?;
    // Reads still succeed, so recovery reaches replacement of the before-image.
    let held = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(&item_path)?;
    let Err(PmRustError::Io { path, source }) = recover_mutation(&pm_root, "update", "sample-unit")
    else {
        return Err("read-shared item must refuse recovery replacement".into());
    };
    assert_eq!(path, item_path);
    // Replacement rename reports ERROR_ACCESS_DENIED for the read-only shared
    // handle; the removed pre-delete previously reported a sharing violation.
    assert_eq!(source.kind(), std::io::ErrorKind::PermissionDenied);
    assert_eq!(source.raw_os_error(), Some(5));
    assert_eq!(fs::read_to_string(&item_path)?, original_item);
    assert_eq!(fs::read_to_string(&history_path)?, original_history);
    assert!(journal_path.is_file());
    assert_eq!(fs::read_dir(pm_root.join("tasks"))?.count(), 1);
    drop(held);
    let expected_paths = (item_path.clone(), history_path.clone());
    assert_eq!(
        recover_mutation(&pm_root, "update", "sample-unit")?,
        expected_paths
    );
    assert_eq!(
        recover_mutation(&pm_root, "update", "sample-unit")?,
        expected_paths
    );
    assert_eq!(fs::read_to_string(&item_path)?, updated_item);
    assert_eq!(
        fs::read_to_string(&history_path)?,
        format!("{original_history}{history_line}")
    );
    assert!(!journal_path.exists());
    Ok(())
}

#[test]
/// Covers recovery replay when the history stream cannot be recreated.
fn recovery_surfaces_history_replay_append_failures() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    let item_path = pm_root.join("tasks/sample-unit.toon");
    let original_item = fs::read_to_string(&item_path)?;
    write_mutation_journal(&pm_root, "update", &original_item, "\"stub\": true\n")?;

    // The matching item skips its replay, but re-appending the absent history
    // stream fails because its containing directory no longer exists.
    fs::remove_dir_all(pm_root.join("history"))?;
    assert!(matches!(
        recover_mutation(&pm_root, "update", "sample-unit"),
        Err(PmRustError::Io { .. })
    ));
    Ok(())
}

#[test]
/// Covers the roll-forward arm's history append failure.
///
/// `recovery_surfaces_history_replay_append_failures` covers the
/// `item_is_after` branch's `append_history_line` error. This test covers the
/// same error in the roll-forward arm (item absent, history directory absent),
/// where `atomic_replace` succeeds but the history append cannot create its
/// file because the containing directory is gone.
fn recovery_surfaces_roll_forward_history_append_failures() -> Result<(), Box<dyn std::error::Error>>
{
    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    let item_path = pm_root.join("tasks/sample-unit.toon");
    let original_item = fs::read_to_string(&item_path)?;
    let updated_item = original_item.replace("Unit create", "Recovered title");
    write_mutation_journal(&pm_root, "update", &updated_item, "\"stub\": true\n")?;

    // The item is absent (roll-forward arm), and the history directory is
    // gone so `append_history_line` cannot create its file.
    fs::remove_file(&item_path)?;
    fs::remove_dir_all(pm_root.join("history"))?;
    assert!(matches!(
        recover_mutation(&pm_root, "update", "sample-unit"),
        Err(PmRustError::Io { .. })
    ));
    Ok(())
}

#[test]
/// Covers recovery cleanup when the committed journal cannot be removed.
#[cfg(unix)]
fn recovery_surfaces_journal_cleanup_failures() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    let item_bytes = fs::read_to_string(pm_root.join("tasks/sample-unit.toon"))?;
    let history_bytes = fs::read_to_string(pm_root.join("history/sample-unit.jsonl"))?;
    write_mutation_journal(&pm_root, "update", &item_bytes, &history_bytes)?;

    // Both durable halves already match the journal, so only the cleanup
    // remains — and the read-only transactions directory refuses the removal.
    let transactions = pm_root.join("runtime/transactions");
    fs::set_permissions(&transactions, fs::Permissions::from_mode(0o555))?;
    assert!(matches!(
        recover_mutation(&pm_root, "update", "sample-unit"),
        Err(PmRustError::Io { .. })
    ));
    fs::set_permissions(&transactions, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

#[cfg(windows)]
#[test]
fn recovery_commits_but_retains_a_read_shared_journal_then_retries_idempotently()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_SHARE_READ: u32 = 1;
    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    let item_path = pm_root.join("tasks/sample-unit.toon");
    let history_path = pm_root.join("history/sample-unit.jsonl");
    let journal_path = pm_root.join("runtime/transactions/update-sample-unit.json");
    let original_history = fs::read_to_string(&history_path)?;
    let updated_item = fs::read_to_string(&item_path)?.replace("Unit create", "Recovered title");
    let history_line = "{\"ts\":\"stub\",\"op\":\"update\"}\n";
    write_mutation_journal(&pm_root, "update", &updated_item, history_line)?;
    // Recovery can read the journal and publish both halves, but cannot delete it.
    let held = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(&journal_path)?;
    let Err(PmRustError::Io { path, source }) = recover_mutation(&pm_root, "update", "sample-unit")
    else {
        return Err("read-shared journal must refuse recovery cleanup".into());
    };
    assert_eq!(path, journal_path);
    assert_eq!(source.raw_os_error(), Some(32));
    assert!(journal_path.is_file());
    let committed_history = format!("{original_history}{history_line}");
    assert_eq!(fs::read_to_string(&item_path)?, updated_item);
    assert_eq!(fs::read_to_string(&history_path)?, committed_history);
    drop(held);
    let expected_paths = (item_path.clone(), history_path.clone());
    assert_eq!(
        recover_mutation(&pm_root, "update", "sample-unit")?,
        expected_paths
    );
    assert_eq!(
        recover_mutation(&pm_root, "update", "sample-unit")?,
        expected_paths
    );
    assert_eq!(fs::read_to_string(&item_path)?, updated_item);
    assert_eq!(fs::read_to_string(&history_path)?, committed_history);
    assert!(!journal_path.exists());
    Ok(())
}

#[test]
/// Covers the tracker-relative path fallback for foreign absolute paths.
fn relative_to_tracker_keeps_paths_outside_the_root_absolute() {
    assert_eq!(
        relative_to_tracker(
            Path::new("/tracker"),
            Path::new("/tracker/tasks/sample-unit.toon")
        ),
        PathBuf::from("tasks/sample-unit.toon")
    );
    assert_eq!(
        relative_to_tracker(Path::new("/tracker"), Path::new("/elsewhere/a.jsonl")),
        PathBuf::from("/elsewhere/a.jsonl")
    );
}

#[test]
/// Covers the parentless defensive input-shape failure of the publisher.
fn atomic_replace_rejects_parentless_targets() {
    // The filesystem root has no parent directory component.
    assert!(matches!(
        atomic_replace(Path::new("/"), "value"),
        Err(PmRustError::InvalidCreateRequest { .. })
    ));
}

#[cfg(any(unix, windows))]
#[test]
/// Covers the non-UTF-8 defensive input-shape failure of the publisher.
fn atomic_replace_rejects_non_utf8_targets() -> Result<(), Box<dyn std::error::Error>> {
    use std::ffi::OsString;
    #[cfg(unix)]
    use std::os::unix::ffi::OsStringExt;
    #[cfg(windows)]
    use std::os::windows::ffi::OsStringExt;

    let directory = tempfile::tempdir()?;
    #[cfg(unix)]
    let non_utf8 = OsString::from_vec(b"sample-\xff.toon".to_vec());
    #[cfg(windows)]
    let non_utf8 = OsString::from_wide(&[0xD800]);
    #[cfg(windows)]
    fs::write(directory.path().join(&non_utf8), "original")?;
    assert!(matches!(
        atomic_replace(&directory.path().join(&non_utf8), "value"),
        Err(PmRustError::InvalidCreateRequest { .. })
    ));
    Ok(())
}

#[cfg(windows)]
#[test]
/// A valid NTFS filename leaves no room for the publisher's temporary suffix.
fn atomic_replace_reports_windows_temporary_name_overflow() -> Result<(), Box<dyn std::error::Error>>
{
    let directory = tempfile::tempdir()?;
    let target = directory.path().join("x".repeat(240));
    fs::write(&target, "original")?;
    let Err(PmRustError::Io { path, .. }) = atomic_replace(&target, "replacement") else {
        return Err("an oversized temporary filename must fail to open".into());
    };
    assert_eq!(path.parent(), target.parent());
    assert!(path.file_name().ok_or("missing temporary name")?.len() > 255);
    assert!(path.to_string_lossy().ends_with(".tmp"));
    assert_eq!(fs::read_to_string(&target)?, "original");
    Ok(())
}

#[cfg(windows)]
#[test]
/// An exclusive handle prevents replacing the existing target.
fn atomic_replace_reports_windows_exclusive_target_handle() -> Result<(), Box<dyn std::error::Error>>
{
    use std::os::windows::fs::OpenOptionsExt;

    let directory = tempfile::tempdir()?;
    let target = directory.path().join("sample-unit.toon");
    fs::write(&target, "original")?;
    let held = OpenOptions::new().read(true).share_mode(0).open(&target)?;
    assert!(matches!(
        atomic_replace(&target, "replacement"),
        Err(PmRustError::Io { path, .. }) if path == target
    ));
    drop(held);
    assert_eq!(fs::read_to_string(&target)?, "original");
    assert_eq!(fs::read_dir(directory.path())?.count(), 1);
    Ok(())
}

#[test]
/// Covers rename conflicts when the durable target cannot be replaced.
fn atomic_replace_surfaces_rename_conflicts() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let target_directory = directory.path().join("sample-unit.toon");
    fs::create_dir(&target_directory)?;
    assert!(matches!(
        atomic_replace(&target_directory, "value"),
        Err(PmRustError::Io { .. })
    ));
    Ok(())
}

#[test]
/// Covers typed recovery failures raised before any journal is readable.
fn recover_mutation_surfaces_unreadable_journal_and_document_paths()
-> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    // A transaction path that cannot be a file fails the journal read.
    let transactions = pm_root.join("runtime/transactions");
    let journal_file = transactions.join("update-sample-unit.json");
    fs::create_dir(&journal_file)?;
    assert!(matches!(
        recover_mutation(&pm_root, "update", "sample-unit"),
        Err(PmRustError::Io { .. })
    ));

    // A valid journal beside an unreadable item document fails that read.
    fs::remove_dir(&journal_file)?;
    write_mutation_journal(&pm_root, "update", "intended-item\n", "{\"ts\":\"stub\"}\n")?;
    let item_path = pm_root.join("tasks/sample-unit.toon");
    fs::remove_file(&item_path)?;
    fs::create_dir(&item_path)?;
    assert!(matches!(
        recover_mutation(&pm_root, "update", "sample-unit"),
        Err(PmRustError::Io { .. })
    ));
    fs::remove_dir(&item_path)?;
    Ok(())
}

#[test]
/// Covers locate failures on unreadable and malformed stored documents.
fn locate_item_surfaces_io_and_decode_failures() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    // A malformed stored document fails strict decoding.
    fs::write(pm_root.join("tasks/sample-unit.toon"), "not: [valid\n")?;
    assert!(matches!(
        locate_item(&pm_root, "sample-unit"),
        Err(PmRustError::InvalidItemDocument { .. })
    ));
    // A directory masquerading as an item file is not a readable item. The
    // locator now mirrors the read surface and recurses into directories
    // rather than reading them, so an empty directory named `sample-unit.toon`
    // yields `ItemNotFound` instead of an IO error.
    fs::remove_file(pm_root.join("tasks/sample-unit.toon"))?;
    fs::create_dir(pm_root.join("tasks/sample-unit.toon"))?;
    assert!(matches!(
        locate_item(&pm_root, "sample-unit"),
        Err(PmRustError::ItemNotFound { .. })
    ));
    Ok(())
}

#[test]
/// Proves the recursive locator resolves an item stored in a nested folder.
fn locate_item_finds_a_nested_item() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    // Move the item into a nested folder the old 11-folder top-level probe
    // could never reach. `Workspace::read_items` finds it because it recurses;
    // `locate_item` must agree so `update`, `comment`, and `close` resolve it.
    let nested = pm_root.join("tasks/subdir");
    fs::create_dir_all(&nested)?;
    fs::rename(
        pm_root.join("tasks/sample-unit.toon"),
        nested.join("sample-unit.toon"),
    )?;
    let (found_path, document) = locate_item(&pm_root, "sample-unit")?;
    assert_eq!(found_path, nested.join("sample-unit.toon"));
    assert_eq!(document.metadata.id, "sample-unit");
    Ok(())
}

#[test]
#[cfg(unix)]
/// Proves the recursive locator skips symbolic links rather than following
/// them, matching the read surface's no-symlink invariant.
fn locate_item_skips_symbolic_links() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;
    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    // Plant a symlink that looks like a toon item but points nowhere. The
    // locator must skip it (is_symlink) and still find the real item.
    symlink(
        "/nonexistent-target",
        pm_root.join("tasks/sample-symlink.toon"),
    )?;
    let (found_path, document) = locate_item(&pm_root, "sample-unit")?;
    assert_eq!(found_path, pm_root.join("tasks/sample-unit.toon"));
    assert_eq!(document.metadata.id, "sample-unit");
    // The symlink must not have been followed or read.
    assert!(!found_path.to_string_lossy().contains("symlink"));
    Ok(())
}

#[cfg(windows)]
#[test]
/// Real Windows file and directory links cannot introduce duplicate items.
fn locate_item_skips_windows_symbolic_links() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::windows::fs::{symlink_dir, symlink_file};

    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    let item = pm_root.join("tasks/sample-unit.toon");
    // The file link carries the item's own filename in a sibling directory, so
    // a locator that followed file links would find a second candidate.
    let linked = pm_root.join("tasks/linked");
    fs::create_dir(&linked)?;
    if !windows_fs::symlink_created(symlink_file(&item, linked.join("sample-unit.toon")))? {
        return Ok(());
    }
    if !windows_fs::symlink_created(symlink_dir(
        pm_root.join("tasks"),
        pm_root.join("linked-items"),
    ))? {
        return Ok(());
    }
    let (found, document) = locate_item(&pm_root, "sample-unit")?;
    assert_eq!(found, item);
    assert_eq!(document.metadata.id, "sample-unit");
    Ok(())
}

#[test]
#[cfg(unix)]
/// Proves the recursive locator skips a non-file, non-directory entry.
///
/// A named pipe (FIFO) is neither a directory, a symlink, nor a regular
/// file. The `else if entry_path.is_file()` guard must evaluate to `false`
/// for it and skip it, so the real item beside it is still found. This covers
/// the `is_file() == false` branch of the locator's else-if.
fn locate_item_skips_a_non_file_entry() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::FileTypeExt;
    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    // Move the real item into a nested folder.
    let nested = pm_root.join("tasks/subdir");
    fs::create_dir_all(&nested)?;
    fs::rename(
        pm_root.join("tasks/sample-unit.toon"),
        nested.join("sample-unit.toon"),
    )?;
    // Create a FIFO at the top-level path the item used to occupy. It is not
    // a directory, not a symlink, and not a regular file, so the locator's
    // `else if entry_path.is_file()` guard must evaluate to `false` and skip
    // it, then recurse into `subdir` and find the real item.
    let fifo = pm_root.join("tasks/sample-unit.toon");
    // `status()?` is Ok for a NON-ZERO exit too, so an unavailable or failing
    // mkfifo left no entry at all: the locator never evaluated its `is_file()`
    // guard against a non-file entry, the nested item was still found, and the
    // test passed while reporting coverage of a branch it had not executed.
    let created = std::process::Command::new("mkfifo").arg(&fifo).status()?;
    assert!(
        created.success(),
        "mkfifo failed; the non-file branch would not be exercised"
    );
    assert!(
        fifo.symlink_metadata()?.file_type().is_fifo(),
        "the planted entry must be a FIFO, or the is_file() guard is not the branch under test"
    );
    let (found_path, document) = locate_item(&pm_root, "sample-unit")?;
    assert_eq!(found_path, nested.join("sample-unit.toon"));
    assert_eq!(document.metadata.id, "sample-unit");
    fs::remove_file(&fifo)?;
    Ok(())
}

#[test]
/// An item moved into a nested directory after enumeration is found at its new
/// path: the stale top-level entry is skipped rather than reported.
fn locate_item_follows_an_item_moved_after_enumeration() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    let tasks = pm_root.join("tasks");
    let vanished = tasks.join("sample-unit.toon");
    let nested = tasks.join("nested");
    fs::create_dir(&nested)?;
    let entries = crate::workspace::read_directory(&tasks)?;
    assert_eq!(entries.len(), 2);
    let retained = nested.join("sample-unit.toon");
    fs::rename(&vanished, &retained)?;
    assert!(!vanished.is_symlink() && !vanished.is_dir() && !vanished.is_file());
    let mut found = Vec::new();
    locate_item_entries(entries, false, "sample-unit", &mut found)?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, retained);
    assert_eq!(found[0].1.metadata.id, "sample-unit");
    Ok(())
}

#[test]
#[cfg(unix)]
/// Proves the recursive locator surfaces a typed IO error when a matching
/// toon entry cannot be read.
fn locate_item_surfaces_an_unreadable_item() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    // Make the item file unreadable. `is_file()` still succeeds, but
    // `read_to_string` fails with a permission error that must surface as a
    // typed IO error rather than being silently skipped.
    let item_path = pm_root.join("tasks/sample-unit.toon");
    fs::set_permissions(&item_path, fs::Permissions::from_mode(0o000))?;
    assert!(matches!(
        locate_item(&pm_root, "sample-unit"),
        Err(PmRustError::Io { .. })
    ));
    fs::set_permissions(&item_path, fs::Permissions::from_mode(0o644))?;
    Ok(())
}

#[cfg(windows)]
#[test]
/// Metadata remains readable while an exclusive handle refuses item reads.
fn locate_item_reports_windows_exclusive_item_handle() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::windows::fs::OpenOptionsExt;

    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    let item = pm_root.join("tasks/sample-unit.toon");
    let held = OpenOptions::new().read(true).share_mode(0).open(&item)?;
    assert!(item.is_file());
    assert!(matches!(
        locate_item(&pm_root, "sample-unit"),
        Err(PmRustError::Io { path, .. }) if path == item
    ));
    drop(held);
    assert!(locate_item(&pm_root, "sample-unit").is_ok());
    Ok(())
}

#[test]
/// Proves a close refuses when a stale journal holds foreign item bytes.
fn close_surfaces_a_stale_mutation_journal() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    write_mutation_journal(
        &pm_root,
        "close",
        "intended-item\n",
        "{\"ts\":\"stub\",\"op\":\"close\"}\n",
    )?;
    // Foreign item bytes that match neither the pre-mutation nor the
    // post-mutation image make recovery refuse, so the close surfaces the
    // conflict instead of overwriting the diverged stream.
    fs::write(pm_root.join("tasks/sample-unit.toon"), "foreign bytes\n")?;
    assert!(matches!(
        close_item(
            &pm_root,
            &pm_root,
            CloseItem {
                id: "sample-unit".to_owned(),
                reason: "reason".to_owned(),
                author: "unit-agent".to_owned(),
                timestamp: Some(TS.to_owned()),
                provenance_role: None,
                fields: CloseFields::default(),
                force_stale_lock: false,
            }
        ),
        Err(PmRustError::RecoveryConflict { .. })
    ));
    Ok(())
}

#[test]
#[allow(clippy::too_many_lines)]
/// Covers per-command settings, author, and missing-item refusals.
fn every_mutation_validates_settings_author_and_existence() -> Result<(), Box<dyn std::error::Error>>
{
    // A tracker directory without any settings file fails every surface.
    let bare = tempfile::tempdir()?;
    let empty_root = bare.path().join(".agents/pm");
    fs::create_dir_all(&empty_root)?;
    // Missing settings fail before anything else for each mutation surface.
    assert!(matches!(
        update_item(
            &empty_root,
            UpdateItem {
                id: "sample-unit".to_owned(),
                title: Some("T".to_owned()),
                description: None,
                status: None,
                priority: None,
                tags: None,
                body: None,
                author: "unit-agent".to_owned(),
                timestamp: Some(TS.to_owned()),
                message: None,
                provenance_role: None,
                force_stale_lock: false,
            }
        ),
        Err(PmRustError::Io { .. })
    ));
    assert!(matches!(
        comment_item(
            &empty_root,
            &CommentItem {
                id: "sample-unit".to_owned(),
                text: "text".to_owned(),
                author: "unit-agent".to_owned(),
                timestamp: Some(TS.to_owned()),
                message: None,
                provenance_role: None,
                force_stale_lock: false,
            }
        ),
        Err(PmRustError::Io { .. })
    ));
    assert!(matches!(
        close_item(
            &empty_root,
            &empty_root,
            CloseItem {
                id: "sample-unit".to_owned(),
                reason: "reason".to_owned(),
                author: "unit-agent".to_owned(),
                timestamp: Some(TS.to_owned()),
                provenance_role: None,
                fields: CloseFields::default(),
                force_stale_lock: false,
            }
        ),
        Err(PmRustError::Io { .. })
    ));

    // Blank authors and unknown items are refused in that order.
    let (_dropped, populated) = root(&mutation_settings(0))?;
    create_item(&populated, request())?;
    for author in ["", "   "] {
        assert!(matches!(
            comment_item(
                &populated,
                &CommentItem {
                    id: "sample-unit".to_owned(),
                    text: "text".to_owned(),
                    author: author.to_owned(),
                    timestamp: Some(TS.to_owned()),
                    message: None,
                    provenance_role: None,
                    force_stale_lock: false,
                }
            ),
            Err(PmRustError::InvalidMutationRequest { .. })
        ));
        assert!(matches!(
            close_item(
                &populated,
                &populated,
                CloseItem {
                    id: "sample-unit".to_owned(),
                    reason: "reason".to_owned(),
                    author: author.to_owned(),
                    timestamp: Some(TS.to_owned()),
                    provenance_role: None,
                    fields: CloseFields::default(),
                    force_stale_lock: false,
                }
            ),
            Err(PmRustError::InvalidMutationRequest { .. })
        ));
    }
    assert!(matches!(
        comment_item(
            &populated,
            &CommentItem {
                id: "sample-missing".to_owned(),
                text: "text".to_owned(),
                author: "unit-agent".to_owned(),
                timestamp: Some(TS.to_owned()),
                message: None,
                provenance_role: None,
                force_stale_lock: false,
            }
        ),
        Err(PmRustError::ItemNotFound { .. })
    ));
    assert!(matches!(
        close_item(
            &populated,
            &populated,
            CloseItem {
                id: "sample-missing".to_owned(),
                reason: "reason".to_owned(),
                author: "unit-agent".to_owned(),
                timestamp: Some(TS.to_owned()),
                provenance_role: None,
                fields: CloseFields::default(),
                force_stale_lock: false,
            }
        ),
        Err(PmRustError::ItemNotFound { .. })
    ));
    Ok(())
}

#[test]
/// Covers closing canceled items and configured custom close statuses.
fn close_refuses_canceled_and_honors_configured_close_statuses()
-> Result<(), Box<dyn std::error::Error>> {
    // A canceled item refuses a second terminal transition.
    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    let mut canceled = decode_item(
        &pm_root.join("tasks/sample-unit.toon"),
        &fs::read_to_string(pm_root.join("tasks/sample-unit.toon"))?,
    )?;
    canceled.metadata.status = "canceled".to_owned();
    let canceled_bytes = canonical_item_bytes(&canceled)?;
    atomic_replace(&pm_root.join("tasks/sample-unit.toon"), &canceled_bytes)?;
    assert!(matches!(
        close_item(
            &pm_root,
            &pm_root,
            CloseItem {
                id: "sample-unit".to_owned(),
                reason: "reason".to_owned(),
                author: "unit-agent".to_owned(),
                timestamp: Some(TS.to_owned()),
                provenance_role: None,
                fields: CloseFields::default(),
                force_stale_lock: false,
            }
        ),
        Err(PmRustError::CommandRefusal { .. })
    ));

    // A configured workflow close status replaces the default and gates
    // repeated closes through the same refusal path.
    let settings = r#"{"id_prefix":"sample-","item_format":"toon","locks":{"ttl_seconds":1800,"wait_ms":0},"workflow":{"close_status":"shipped"}}"#;
    let (_other, shipped_root) = root(settings)?;
    create_item(
        &shipped_root,
        CreateItem {
            id: "sample-unit".to_owned(),
            title: "Unit create".to_owned(),
            description: String::new(),
            item_type: "Task".to_owned(),
            status: "open".to_owned(),
            priority: 2,
            tags: Vec::new(),
            body: String::new(),
            author: "unit-agent".to_owned(),
            timestamp: Some(TS.to_owned()),
            message: None,
            provenance_role: None,
            force_stale_lock: false,
        },
    )?;
    let closed = close_item(
        &shipped_root,
        &shipped_root,
        CloseItem {
            id: "sample-unit".to_owned(),
            reason: "shipped to customers".to_owned(),
            author: "unit-agent".to_owned(),
            timestamp: Some(TS.to_owned()),
            provenance_role: None,
            fields: CloseFields::default(),
            force_stale_lock: false,
        },
    )?;
    assert_eq!(closed.item.metadata.status, "shipped");
    assert!(matches!(
        close_item(
            &shipped_root,
            &shipped_root,
            CloseItem {
                id: "sample-unit".to_owned(),
                reason: "again".to_owned(),
                author: "unit-agent".to_owned(),
                timestamp: Some(TS.to_owned()),
                provenance_role: None,
                fields: CloseFields::default(),
                force_stale_lock: false,
            }
        ),
        Err(PmRustError::CommandRefusal { .. })
    ));
    Ok(())
}

#[test]
/// Exercises single-field updates so every change-detection branch runs.
fn single_field_updates_cover_every_change_arm() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    let base = || UpdateItem {
        id: "sample-unit".to_owned(),
        title: None,
        description: None,
        status: None,
        priority: None,
        tags: None,
        body: None,
        author: "unit-agent".to_owned(),
        timestamp: Some(TS.to_owned()),
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    };
    let mut description_only = base();
    description_only.description = Some("Description only".to_owned());
    update_item(&pm_root, description_only)?;
    let mut priority_only = base();
    priority_only.priority = Some(0);
    update_item(&pm_root, priority_only)?;
    let mut tags_only = base();
    tags_only.tags = Some(vec!["solo".to_owned()]);
    update_item(&pm_root, tags_only)?;
    let mut body_only = base();
    body_only.body = Some("Body only".to_owned());
    let result = update_item(&pm_root, body_only)?;
    assert_eq!(result.item.body, "Body only");
    let history = fs::read_to_string(pm_root.join("history/sample-unit.jsonl"))?;
    assert!(history.contains(r#""path":"/metadata/tags""#));
    assert!(history.contains(r#""path":"/body""#));
    Ok(())
}

#[test]
/// Covers comment rows containing commas and quoted-CSV edge cases.
fn comments_with_commas_survive_canonical_row_normalization()
-> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    let text = "note, with, commas";
    comment_item(
        &pm_root,
        &CommentItem {
            id: "sample-unit".to_owned(),
            text: text.to_owned(),
            author: "unit-agent".to_owned(),
            timestamp: Some(TS.to_owned()),
            message: None,
            provenance_role: None,
            force_stale_lock: false,
        },
    )?;
    let stored = decode_item(
        &pm_root.join("tasks/sample-unit.toon"),
        &fs::read_to_string(pm_root.join("tasks/sample-unit.toon"))?,
    )?;
    let stored_text = stored
        .metadata
        .extra
        .get("comments")
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .and_then(|row| row.get("text"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    assert_eq!(stored_text, text);
    let second = comment_item(
        &pm_root,
        &CommentItem {
            id: "sample-unit".to_owned(),
            text: "second \"quoted\" note".to_owned(),
            author: "unit-agent".to_owned(),
            timestamp: Some(TS.to_owned()),
            message: None,
            provenance_role: None,
            force_stale_lock: false,
        },
    )?;
    assert_eq!(
        second
            .item
            .metadata
            .extra
            .get("comments")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(2)
    );
    Ok(())
}

#[test]
/// Covers history-append failures raised while completing a recovery.
fn mutation_recovery_surfaces_history_append_failures() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    // The journal expects to append to a history stream that is absent; the
    // stream path being a directory fails the durable append instead.
    write_mutation_journal(
        &pm_root,
        "update",
        "intended-item\n",
        "{\"ts\":\"stub\",\"op\":\"update\"}\n",
    )?;
    fs::remove_file(pm_root.join("tasks/sample-unit.toon"))?;
    fs::remove_file(pm_root.join("history/sample-unit.jsonl"))?;
    fs::create_dir(pm_root.join("history/sample-unit.jsonl"))?;
    assert!(matches!(
        recover_mutation(&pm_root, "update", "sample-unit"),
        Err(PmRustError::Io { .. })
    ));
    Ok(())
}

#[test]
/// A journal from a future encoding is refused on its version alone.
///
/// The identity guard is `version != 1 || id != id`. The id half is covered by
/// the recovery suite, so the version half was dead to the tests: a journal
/// carrying the correct id but a version this build does not understand was
/// never proven to be refused. Because `||` short circuits, only a
/// correct-id/wrong-version journal reaches that branch.
fn recovery_refuses_a_journal_whose_version_this_build_does_not_understand()
-> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    let original_item = fs::read_to_string(pm_root.join("tasks/sample-unit.toon"))?;
    let original_history = fs::read_to_string(pm_root.join("history/sample-unit.jsonl"))?;
    let appended_history = format!("{original_history}{{\"ts\":\"stub\",\"op\":\"update\"}}\n");
    write_mutation_journal(&pm_root, "update", &original_item, &appended_history)?;
    let journal_path = pm_root
        .join("runtime/transactions")
        .join("update-sample-unit.json");
    let mut journal: MutationJournal = serde_json::from_str(&fs::read_to_string(&journal_path)?)?;
    journal.version = 2;
    fs::write(
        &journal_path,
        &serde_json::to_string(&journal).map_err(|error| format!("{error}"))?,
    )?;
    assert!(matches!(
        recover_mutation(&pm_root, "update", "sample-unit"),
        Err(PmRustError::RecoveryConflict { .. })
    ));
    Ok(())
}

#[test]
/// A tabular header followed by an unindented line must leave the row branch.
///
/// The row normaliser runs only when the encoder is inside a tabular block AND
/// the line is indented. Real encoder output always indents rows after a
/// `{...}:` header, so the indentation half never ran. Ending a block with an
/// unindented key proves the guard tests indentation rather than trusting the
/// block flag alone.
fn normalize_item_bytes_leaves_the_row_path_when_a_block_line_is_unindented()
-> Result<(), Box<dyn std::error::Error>> {
    let encoded = normalize_item_bytes("comments[1]{author,text}:\nid: plain-key")?;
    assert!(
        encoded.contains("id: plain-key"),
        "the unindented key must pass through untouched, got {encoded:?}"
    );
    let row = normalize_item_bytes("comments[1]{author,text}:\n  \"safe\",plain")?;
    assert!(
        !row.contains("\"safe\""),
        "an indented row must reach the normaliser and be unquoted, got {row:?}"
    );
    Ok(())
}

#[test]
/// A quoted tabular field that survives the quote/escape filter but fails the
/// scalar-safety probe must keep its quotes.
///
/// `"true"` and `"05"` pass the strip-and-filter half (simple quotes, no
/// escapes) yet must stay quoted to match canonical encoding: `true` is a
/// boolean and `05` is numeric-like. Without this
/// case the guard-false arm of that match is never exercised.
fn normalize_row_bytes_preserves_quoted_ambiguous_scalars() {
    assert_eq!(
        normalize_row_bytes("  \"true\",plain"),
        "  \"true\",plain",
        "a quoted boolean-looking field must keep its quotes"
    );
    assert_eq!(
        normalize_row_bytes("  \"05\",plain"),
        "  \"05\",plain",
        "a numeric-like string must keep its quotes even when decoding as a string"
    );
    assert_eq!(normalize_row_bytes("  \"0.\",plain"), "  0.,plain");
}

#[test]
/// A quoted row field containing a backslash keeps its bytes verbatim.
///
/// A field is unquoted only when its content holds neither a quote nor a
/// backslash. The backslash half never ran. Unquoting an escaped value would
/// change how the canonical dialect reparses the row. Asserted directly against
/// the row normaliser: routed through a whole document, an earlier branch can
/// satisfy the assertion without the filter ever executing.
fn normalize_row_bytes_preserves_a_quoted_field_containing_a_backslash() {
    assert_eq!(
        normalize_row_bytes("  \"back\\slash\",plain"),
        "  \"back\\slash\",plain",
        "a backslash-bearing value must keep its quotes and leave its neighbour alone"
    );
    assert_eq!(
        normalize_row_bytes("  \"safe\",plain"),
        "  safe,plain",
        "a safe scalar must be unquoted, so the case above is the filter and not the shape"
    );
}

#[test]
/// A quoted field whose content still holds a quote must keep its quotes.
///
/// The unquoting filter is `!contains('"') && !contains('\\')`. Its backslash
/// half is covered above; its quote half never ran, so nothing proved that a
/// doubled inner quote survives normalisation. Unquoting `"a""b"` would emit
/// `a""b`, which the canonical dialect reparses as a different value — a silent
/// corruption of a field the encoder deliberately quoted. Asserted directly
/// against the row normaliser: routed through a whole document, an earlier
/// branch can satisfy the assertion without this filter ever executing.
fn normalize_row_bytes_preserves_a_quoted_field_containing_a_quote() {
    assert_eq!(
        normalize_row_bytes("  \"a\"\"b\",plain"),
        "  \"a\"\"b\",plain",
        "a quote-bearing value must keep its quotes and leave its neighbour alone"
    );
    assert_eq!(
        normalize_row_bytes("  \"safe\",plain"),
        "  safe,plain",
        "a safe scalar must be unquoted, so the case above is the filter and not the shape"
    );
}

#[test]
/// An unterminated quoted scalar must be refused rather than silently truncated.
///
/// `normalize_item_bytes` splits a line on `: "` and strips the closing quote.
/// If the encoder ever emits an opening quote with no closing one the strip
/// yields `None`, and the encode has to fail rather than write a document whose
/// quoting is structurally broken.
fn normalize_item_bytes_refuses_an_unterminated_quoted_scalar()
-> Result<(), Box<dyn std::error::Error>> {
    assert!(matches!(
        normalize_item_bytes("title: \"unterminated"),
        Err(PmRustError::ItemEncoding { .. })
    ));
    assert_eq!(
        normalize_item_bytes("title: \"terminated\"")?,
        "title: terminated\n"
    );
    Ok(())
}

#[test]
#[allow(clippy::too_many_lines)]
/// Closes the last per-instantiation arms the unit binary never exercised on
/// its own: every `type_folder` arm, the mutation timestamp validation arms,
/// the lock retry loop, and the per-mutation refusal arms. The integration
/// binary already covers the `type_folder` arms; these direct calls keep the
/// unit binary self-sufficient so the coverage gate stays at 100 percent.
fn remaining_portable_mutation_arms_are_covered_in_the_unit_binary()
-> Result<(), Box<dyn std::error::Error>> {
    for (item_type, folder) in [
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
    ] {
        assert_eq!(type_folder(item_type), Some(folder));
    }
    assert_eq!(type_folder("Custom"), None);

    // validate_mutation_request: a malformed timestamp reports the mutation
    // variant, and a missing timestamp resolves to the generated value.
    assert!(matches!(
        validate_mutation_request("agent", Some("not-a-timestamp")),
        Err(PmRustError::InvalidMutationRequest { .. })
    ));
    assert!(validate_mutation_request("agent", None).is_ok());

    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;

    // acquire_lock retry loop: a held lock plus a non-zero wait budget
    // exercises the sleep arm before the deadline returns the conflict.
    let held = acquire_lock(&pm_root, "sample-unit", "holder", &locks(1_800), false, TS)?;
    assert!(matches!(
        acquire_lock(
            &pm_root,
            "sample-unit",
            "challenger",
            &LockSettings {
                use_created_at: false,
                ttl_seconds: 1_800,
                wait_ms: 40,
            },
            false,
            TS,
        ),
        Err(PmRustError::LockConflict { .. })
    ));
    drop(held);

    // update_item: a blank author (validate_mutation_request?), an unknown
    // item (recover_and_locate?), and a no-op request (no changed fields).
    assert!(matches!(
        update_item(
            &pm_root,
            UpdateItem {
                id: "sample-unit".to_owned(),
                title: Some("T".to_owned()),
                description: None,
                status: None,
                priority: None,
                tags: None,
                body: None,
                author: "  ".to_owned(),
                timestamp: Some(TS.to_owned()),
                message: None,
                provenance_role: None,
                force_stale_lock: false,
            }
        ),
        Err(PmRustError::InvalidMutationRequest { .. })
    ));
    assert!(matches!(
        update_item(
            &pm_root,
            UpdateItem {
                id: "sample-missing".to_owned(),
                title: Some("T".to_owned()),
                description: None,
                status: None,
                priority: None,
                tags: None,
                body: None,
                author: "unit-agent".to_owned(),
                timestamp: Some(TS.to_owned()),
                message: None,
                provenance_role: None,
                force_stale_lock: false,
            }
        ),
        Err(PmRustError::ItemNotFound { .. })
    ));
    assert!(matches!(
        update_item(
            &pm_root,
            UpdateItem {
                id: "sample-unit".to_owned(),
                title: None,
                description: None,
                status: None,
                priority: None,
                tags: None,
                body: None,
                author: "unit-agent".to_owned(),
                timestamp: Some(TS.to_owned()),
                message: None,
                provenance_role: None,
                force_stale_lock: false,
            }
        ),
        Err(PmRustError::InvalidMutationRequest { .. })
    ));

    // comment_item: empty text refuses before any locking.
    assert!(matches!(
        comment_item(
            &pm_root,
            &CommentItem {
                id: "sample-unit".to_owned(),
                text: "  ".to_owned(),
                author: "unit-agent".to_owned(),
                timestamp: Some(TS.to_owned()),
                message: None,
                provenance_role: None,
                force_stale_lock: false,
            }
        ),
        Err(PmRustError::InvalidMutationRequest { .. })
    ));

    // close_item: an empty closing summary refuses before any locking.
    assert!(matches!(
        close_item(
            &pm_root,
            &pm_root,
            CloseItem {
                id: "sample-unit".to_owned(),
                reason: "  ".to_owned(),
                author: "unit-agent".to_owned(),
                timestamp: Some(TS.to_owned()),
                provenance_role: None,
                fields: CloseFields::default(),
                force_stale_lock: false,
            }
        ),
        Err(PmRustError::CommandRefusal { .. })
    ));

    // comment_item: a scalar `comments` value refuses the append instead of
    // discarding the stored content. Plant the value directly in the item.
    let item_path = pm_root.join("tasks/sample-unit.toon");
    let raw = fs::read_to_string(&item_path)?;
    fs::write(&item_path, format!("{raw}comments: \"not-an-array\"\n"))?;
    assert!(matches!(
        comment_item(
            &pm_root,
            &CommentItem {
                id: "sample-unit".to_owned(),
                text: "note".to_owned(),
                author: "unit-agent".to_owned(),
                timestamp: Some(TS.to_owned()),
                message: None,
                provenance_role: None,
                force_stale_lock: false,
            }
        ),
        Err(PmRustError::InvalidMutationRequest { .. })
    ));
    Ok(())
}

#[test]
#[cfg(unix)]
/// Closes the two arms that need an unreadable directory: the create-recovery
/// `repair?` propagation when the restore write fails, and the recursive
/// locator's `read_directory` error from an unreadable nested folder.
fn remaining_unix_only_mutation_arms_are_covered_in_the_unit_binary()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    // recover: a present item plus a missing history half whose directory is
    // read-only makes the restore `atomic_write` fail at `repair?`, not at the
    // earlier read. The journal must be removed by the caller only on success,
    // so the error leaves the durable transaction in place for a retry.
    let (_directory, recover_root, _item, _history, journal) = completed_transaction()?;
    write_journal(&recover_root, &journal)?;
    fs::remove_file(recover_root.join("history/sample-unit.jsonl"))?;
    let history_dir = recover_root.join("history");
    fs::set_permissions(&history_dir, fs::Permissions::from_mode(0o555))?;
    assert!(matches!(
        recover(&recover_root, "sample-unit"),
        Err(PmRustError::Io { .. })
    ));
    fs::set_permissions(&history_dir, fs::Permissions::from_mode(0o755))?;

    // locate_item_recursive: an unreadable nested directory surfaces the
    // read-directory error from the recursive probe.
    let (_d2, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    let nested = pm_root.join("tasks/subdir");
    fs::create_dir_all(&nested)?;
    fs::set_permissions(&nested, fs::Permissions::from_mode(0o000))?;
    assert!(matches!(
        locate_item(&pm_root, "sample-unit"),
        Err(PmRustError::Io { .. })
    ));
    fs::set_permissions(&nested, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

#[cfg(windows)]
#[test]
/// Native ACLs and sharing restrictions refuse repair and enumeration independently.
fn remaining_windows_mutation_arms_report_real_io_errors() -> Result<(), Box<dyn std::error::Error>>
{
    let (_directory, recover_root, _item, _history, journal) = completed_transaction()?;
    write_journal(&recover_root, &journal)?;
    let history = recover_root.join("history/sample-unit.jsonl");
    fs::remove_file(&history)?;
    let history_dir = recover_root.join("history");
    let mut denied_creation = windows_acl::deny_directory_access(&history_dir, "WD")?;
    assert!(fs::read_dir(&history_dir).is_ok());
    let result = recover(&recover_root, "sample-unit");
    denied_creation.restore()?;
    assert!(matches!(result, Err(PmRustError::Io { .. })));
    assert!(!history.exists());
    assert!(
        recover_root
            .join("runtime/transactions/create-sample-unit.json")
            .is_file()
    );
    recover(&recover_root, "sample-unit")?;
    assert!(history.is_file());

    let (_directory, pm_root) = root(&mutation_settings(0))?;
    create_item(&pm_root, request())?;
    let nested = pm_root.join("tasks/locked");
    fs::create_dir(&nested)?;
    let sentinel = nested.join("sentinel.txt");
    fs::write(&sentinel, "directory enumeration fixture")?;
    let held = windows_fs::exclusive_directory_handle(&nested)?;
    assert!(nested.is_dir());
    let error = fs::read_dir(&nested)
        .err()
        .ok_or("exclusively held directory listing succeeded")?;
    assert_eq!(error.raw_os_error(), Some(32), "{error:?}");
    let result = locate_item(&pm_root, "sample-unit");
    drop(held);
    assert!(matches!(result, Err(PmRustError::Io { path, .. }) if path == nested));
    assert_eq!(
        fs::read_to_string(&sentinel)?,
        "directory enumeration fixture"
    );
    assert!(locate_item(&pm_root, "sample-unit").is_ok());
    Ok(())
}

#[test]
fn windows_pending_delete_reads_as_lock_contention_not_as_a_fault() {
    // Windows keeps a deleted file in a pending-delete state until every handle
    // to it closes, and an open against a file in that state fails with
    // ERROR_ACCESS_DENIED -- surfacing in Rust as PermissionDenied, not as
    // AlreadyExists. A writer releasing its lock therefore made a concurrent
    // writer's create fail fatally instead of retrying, which is how a busy
    // lock became an aborted mutation on the Windows runner while every Unix
    // job passed.
    let Ok(scratch) = tempfile::tempdir() else {
        unreachable!("the fixture builds");
    };
    let absent = scratch.path().join("sample-absent.lock");
    for kind in [
        ErrorKind::AlreadyExists,
        ErrorKind::PermissionDenied,
        ErrorKind::NotFound,
    ] {
        assert!(
            lock_error_is_contention(&absent, &std::io::Error::from(kind)),
            "{kind:?} is a way a held or just-released lock presents itself",
        );
    }
    // Anything else is a real fault and must keep its own diagnostic rather
    // than being retried until the wait budget expires and then reported as a
    // lock conflict.
    for kind in [
        ErrorKind::InvalidInput,
        ErrorKind::OutOfMemory,
        ErrorKind::Unsupported,
    ] {
        assert!(
            !lock_error_is_contention(&absent, &std::io::Error::from(kind)),
            "{kind:?} is not contention and must not be retried as if it were",
        );
    }
    // A directory where the lock file belongs is a structural fault and a
    // permanent one. Unix reports it as AlreadyExists then IsADirectory and
    // Windows reports it as PermissionDenied, so the shape of the path has to
    // decide it -- otherwise the Windows presentation would be retried for the
    // whole wait budget and then reported as contention it is not.
    let occupied = scratch.path().join("sample-directory.lock");
    let Ok(()) = fs::create_dir(&occupied) else {
        unreachable!("the fixture builds");
    };
    for kind in [
        ErrorKind::AlreadyExists,
        ErrorKind::PermissionDenied,
        ErrorKind::NotFound,
    ] {
        assert!(
            !lock_error_is_contention(&occupied, &std::io::Error::from(kind)),
            "a directory at the lock path is a fault, not contention ({kind:?})",
        );
    }
}

#[test]
fn a_lock_path_that_cannot_be_opened_is_reported_as_contention() {
    // Drives `create_lock_file` against a path whose parent is a file rather
    // than a directory, so the open fails with `NotFound` -- the same class the
    // Windows pending-delete window produces, reached deterministically on any
    // platform. Before the fix this returned a filesystem error and killed the
    // mutation; now it is a conflict, which the caller retries inside its wait
    // budget and only then reports as a lock conflict.
    let Ok((directory, pm_root)) = root(r#"{"id_prefix":"sample-","item_format":"toon"}"#) else {
        unreachable!("the fixture builds");
    };
    let locks = pm_root.join("locks");
    let Ok(()) = fs::create_dir_all(&locks) else {
        unreachable!("the fixture builds");
    };
    let path = locks.join("sample-gone.lock");
    let Err(error) = create_lock_file(&path.join("unreachable-child"), "{}", "sample-gone") else {
        unreachable!("creating a lock beneath a non-directory cannot succeed");
    };
    assert!(
        matches!(error, PmRustError::LockConflict { .. }),
        "a vanished or unopenable lock path is contention, got {error:?}",
    );
    drop(directory);
}

#[cfg(unix)]
#[test]
fn an_unreadable_incumbent_lock_is_contention_rather_than_a_filesystem_error() {
    // The second half of the pending-delete window: the create fails because a
    // lock is present, and the read of that incumbent then fails too. Windows
    // produces this with ERROR_ACCESS_DENIED on a file awaiting deletion;
    // an unreadable file reproduces the same `PermissionDenied` deterministically
    // here. Reported as a filesystem error it kills the mutation; reported as
    // contention the caller retries inside its wait budget.
    use std::os::unix::fs::PermissionsExt;
    let Ok((directory, pm_root)) = root(r#"{"id_prefix":"sample-","item_format":"toon"}"#) else {
        unreachable!("the fixture builds");
    };
    let locks = pm_root.join("locks");
    let Ok(()) = fs::create_dir_all(&locks) else {
        unreachable!("the fixture builds");
    };
    let path = locks.join("sample-held.lock");
    let Ok(()) = fs::write(&path, "{}\n") else {
        unreachable!("the fixture builds");
    };
    let Ok(()) = fs::set_permissions(&path, fs::Permissions::from_mode(0o000)) else {
        unreachable!("the fixture builds");
    };
    let outcome = acquire_lock_attempt(
        &pm_root,
        "sample-held",
        "load-agent",
        1800,
        false,
        TS,
        false,
    );
    let Ok(()) = fs::set_permissions(&path, fs::Permissions::from_mode(0o600)) else {
        unreachable!("the fixture is restored so the directory can be removed");
    };
    let Err(error) = outcome else {
        unreachable!("a held lock cannot be acquired");
    };
    assert!(
        matches!(error, PmRustError::LockConflict { .. }),
        "an unreadable incumbent lock is contention, got {error:?}",
    );
    drop(directory);
}

#[cfg(windows)]
#[test]
/// An exclusive second handle refuses incumbent reads until release.
fn a_windows_exclusive_incumbent_lock_is_contention() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::windows::fs::OpenOptionsExt;

    let (_directory, pm_root) = root(&mutation_settings(0))?;
    fs::create_dir_all(pm_root.join("locks"))?;
    let path = pm_root.join("locks/sample-held.lock");
    fs::write(&path, "{}\n")?;
    let held = OpenOptions::new().read(true).share_mode(0).open(&path)?;
    let error = fs::read_to_string(&path)
        .err()
        .ok_or("exclusive incumbent read succeeded")?;
    assert_eq!(error.raw_os_error(), Some(32), "{error:?}");
    assert!(lock_error_is_contention(&path, &error));
    // Check both native contention codes without accepting other uncategorized
    // errors or hiding a structural fault at the lock path.
    let occupied = pm_root.join("locks/directory.lock");
    fs::create_dir(&occupied)?;
    for code in [32, 33] {
        let source = std::io::Error::from_raw_os_error(code);
        assert!(lock_error_is_contention(&path, &source));
        assert!(!lock_error_is_contention(&occupied, &source));
    }
    assert!(!lock_error_is_contention(
        &path,
        &std::io::Error::from_raw_os_error(34)
    ));
    assert_eq!(held.metadata()?.len(), 3);
    let outcome = acquire_lock_attempt(
        &pm_root,
        "sample-held",
        "load-agent",
        1800,
        false,
        TS,
        false,
    );
    assert!(
        matches!(&outcome, Err(PmRustError::LockConflict { id }) if id == "sample-held"),
        "exclusive incumbent lock must remain contention: {:?}",
        outcome.as_ref().err()
    );
    drop(held);
    assert_eq!(fs::read_to_string(&path)?, "{}\n");
    fs::remove_file(&path)?;
    let acquired = acquire_lock_attempt(
        &pm_root,
        "sample-held",
        "load-agent",
        1800,
        false,
        TS,
        false,
    )?;
    drop(acquired);
    Ok(())
}

#[test]
fn a_lock_file_holding_invalid_utf8_is_a_fault_not_contention() {
    // Reaching the read means the create already classified the path as a held
    // lock, so the remaining way that read can fail is the file's contents. A
    // lock whose bytes are not UTF-8 is corrupt, and retrying it for the whole
    // wait budget before reporting a lock conflict would hide that behind a
    // diagnosis that is simply wrong.
    let Ok((directory, pm_root)) = root(r#"{"id_prefix":"sample-","item_format":"toon"}"#) else {
        unreachable!("the fixture builds");
    };
    let locks = pm_root.join("locks");
    let Ok(()) = fs::create_dir_all(&locks) else {
        unreachable!("the fixture builds");
    };
    let Ok(()) = fs::write(locks.join("sample-corrupt.lock"), [0xff, 0xfe, 0x00]) else {
        unreachable!("the fixture builds");
    };
    let outcome = acquire_lock_attempt(
        &pm_root,
        "sample-corrupt",
        "load-agent",
        1800,
        false,
        TS,
        false,
    );
    let Err(error) = outcome else {
        unreachable!("a held lock cannot be acquired");
    };
    assert!(
        matches!(error, PmRustError::Io { .. }),
        "a corrupt lock file keeps its own diagnostic, got {error:?}",
    );
    drop(directory);
}

/// Exercises the public SDK while retaining a real, freshly acquired lock guard.
fn future_ownership_preserves_held_lock(
    claim: bool,
    force: bool,
    governance: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root(&format!(
        r#"{{"governance":{governance},"locks":{{"ttl_seconds":1800,"wait_ms":0}}}}"#
    ))?;
    create_item(&pm_root, request())?;
    let workspace = crate::Workspace::discover(&pm_root)?;
    let mut mutation = OwnershipItem {
        id: "sample-unit".to_owned(),
        author: "unit-agent".to_owned(),
        timestamp: Some(TS.to_owned()),
        message: None,
        force,
        if_available: false,
        provenance_role: None,
    };
    if !claim {
        workspace.claim(&mutation)?;
    }
    let lock_settings = LockSettings {
        use_created_at: true,
        ..locks(1800)
    };
    let held = acquire_lock(
        workspace.pm_root(),
        &mutation.id,
        "live-holder",
        &lock_settings,
        false,
        &now_iso(),
    )?;
    let paths = [
        pm_root.join("tasks/sample-unit.toon"),
        pm_root.join("history/sample-unit.jsonl"),
        held.path.clone(),
    ];
    let before = [
        Some(fs::read(&paths[0])?),
        Some(fs::read(&paths[1])?),
        Some(fs::read(&paths[2])?),
    ];
    mutation.timestamp = Some("9999-12-31T23:59:59.999999999Z".to_owned());
    let result = if claim {
        workspace.claim(&mutation)
    } else {
        workspace.release(&mutation)
    };
    // Inspect every durable artifact while the original guard is still alive.
    // Reading the lock as an Option also exposes an illicit unlink as a mismatch.
    let after = paths.each_ref().map(|path| fs::read(path).ok());
    assert_eq!(after, before, "held-lock bytes changed: {result:?}");
    assert!(matches!(
        result,
        Err(PmRustError::InvalidMutationRequest { reason })
            if reason == "timestamp must not be in the future"
    ));
    mutation.timestamp = Some(TS.to_owned());
    let blocked = if claim {
        workspace.claim(&mutation)
    } else {
        workspace.release(&mutation)
    };
    let Err(PmRustError::OwnershipRefusal { detail, context }) = blocked else {
        return Err("a historical clock must still respect the held lock".into());
    };
    assert_eq!(detail, "Item sample-unit is locked (owner live-holder)");
    assert_eq!(context["code"], "lock_conflict");
    assert_eq!(paths.each_ref().map(|path| fs::read(path).ok()), before);
    drop(held);
    assert!(
        !paths[2].exists(),
        "the original guard must retain its token"
    );
    let historical = if claim {
        workspace.claim(&mutation)?
    } else {
        workspace.release(&mutation)?
    };
    assert_eq!(historical.changed_field_count, 2);
    assert_eq!(historical.item.metadata.updated_at, TS);
    assert!(!paths[2].exists());
    Ok(())
}

#[test]
fn future_ownership_force_claim_preserves_held_lock() -> Result<(), Box<dyn std::error::Error>> {
    future_ownership_preserves_held_lock(true, true, r#"{"preset":"strict"}"#)
}

#[test]
fn future_ownership_force_release_preserves_held_lock() -> Result<(), Box<dyn std::error::Error>> {
    future_ownership_preserves_held_lock(false, true, r#"{"preset":"strict"}"#)
}

#[test]
fn future_ownership_minimal_claim_preserves_held_lock() -> Result<(), Box<dyn std::error::Error>> {
    future_ownership_preserves_held_lock(true, false, r#"{"preset":"minimal"}"#)
}

#[test]
fn future_ownership_minimal_release_preserves_held_lock() -> Result<(), Box<dyn std::error::Error>>
{
    future_ownership_preserves_held_lock(false, false, r#"{"preset":"minimal"}"#)
}

#[test]
fn future_ownership_custom_claim_preserves_held_lock() -> Result<(), Box<dyn std::error::Error>> {
    future_ownership_preserves_held_lock(
        true,
        false,
        r#"{"preset":"custom","force_required_for_stale_lock":false}"#,
    )
}

#[test]
fn future_ownership_custom_release_preserves_held_lock() -> Result<(), Box<dyn std::error::Error>> {
    future_ownership_preserves_held_lock(
        false,
        false,
        r#"{"preset":"custom","force_required_for_stale_lock":false}"#,
    )
}

#[test]
fn ownership_provenance_and_legacy_metadata_are_preserved() -> Result<(), Box<dyn std::error::Error>>
{
    let (_directory, pm_root) = root(&settings("sample-", "toon", 1800))?;
    let mut document = create_item(&pm_root, request())?.item;
    let history_path = pm_root.join("history/sample-unit.jsonl");
    let item_path = pm_root.join("tasks/sample-unit.toon");
    let mutation = OwnershipItem {
        id: "sample-unit".to_owned(),
        author: "unit-agent".to_owned(),
        timestamp: Some(TS.to_owned()),
        message: None,
        force: false,
        if_available: false,
        provenance_role: None,
    };
    assert_eq!(
        ownership_phrase(&history_path, "other")?,
        "assigned to other"
    );
    for (raw, expected) in [
        (
            "invalid\n{}\n{\"patch\":[{\"path\":\"/metadata/title\"}]}\n",
            "assigned to other",
        ),
        (
            "{\"op\":\"update\",\"patch\":[{\"path\":\"/metadata/assignee\"}]}\n",
            "assigned to other",
        ),
        (
            "{\"op\":\"claim\",\"patch\":[{\"path\":\"/metadata/assignee\"}]}\n",
            "claimed by other",
        ),
    ] {
        fs::write(&history_path, raw)?;
        assert_eq!(ownership_phrase(&history_path, "other")?, expected);
    }
    fs::remove_file(&history_path)?;
    assert_eq!(
        ownership_phrase(&history_path, "other")?,
        "assigned to other"
    );
    fs::create_dir(&history_path)?;
    assert!(ownership_phrase(&history_path, "other").is_err());
    fs::remove_dir(&history_path)?;
    // A legacy assignment lacks claim_principal. Reclaim adds only that field.
    document
        .metadata
        .extra
        .insert("assignee".to_owned(), serde_json::json!("unit-agent"));
    document
        .metadata
        .extra
        .insert("claim_principal".to_owned(), Value::Null);
    assert_eq!(ownership_text(&document, "claim_principal"), "");
    fs::write(&item_path, canonical_item_bytes(&document)?)?;
    let result = ownership_item(&pm_root, &mutation, true)?;
    assert_eq!(result.changed_field_count, 1);
    // Principal-only legacy records must also be cleared by release.
    document.metadata.extra.remove("assignee");
    document.metadata.extra.insert(
        "claim_principal".to_owned(),
        serde_json::json!("unit-agent#instance"),
    );
    fs::write(&item_path, canonical_item_bytes(&document)?)?;
    let result = ownership_item(&pm_root, &mutation, false)?;
    assert_eq!(result.changed_field_count, 2);
    assert!(!result.item.metadata.extra.contains_key("claim_principal"));
    Ok(())
}

/// Lifecycle status changes apply ownership policy and clean terminal metadata.
#[test]
fn lifecycle_updates_enforce_owners_and_clear_terminal_fields()
-> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) =
        root(r#"{"governance":{"preset":"custom","ownership_enforcement":"strict"}}"#)?;
    let mut document = create_item(&pm_root, request())?.item;
    let path = pm_root.join("tasks/sample-unit.toon");
    document.metadata.status = "closed".to_owned();
    document.metadata.extra.extend([
        ("assignee".to_owned(), serde_json::json!("owner")),
        ("closed_at".to_owned(), serde_json::json!(TS)),
        ("close_reason".to_owned(), serde_json::json!("Done")),
    ]);
    fs::write(&path, canonical_item_bytes(&document)?)?;
    let mut mutation = OwnershipItem {
        id: "sample-unit".to_owned(),
        author: "other".to_owned(),
        timestamp: Some(TS.to_owned()),
        message: None,
        force: false,
        if_available: false,
        provenance_role: None,
    };
    let options = crate::OwnershipOptions::default();
    let before = fs::read(&path)?;
    assert!(matches!(
        lifecycle_status(&pm_root, &mutation, &options, true),
        Err(PmRustError::OwnershipRefusal { .. })
    ));
    assert_eq!(fs::read(&path)?, before);
    mutation.force = true;
    let receipt = lifecycle_status(&pm_root, &mutation, &options, true)?;
    assert_eq!(receipt["status"], "in_progress");
    assert_eq!(receipt["changed_field_count"], 3);
    let opened = decode_item(&path, &fs::read_to_string(&path)?)?;
    assert!(!opened.metadata.extra.contains_key("close_reason"));
    fs::write(
        pm_root.join("settings.json"),
        r#"{"governance":{"preset":"custom","ownership_enforcement":"warn"}}"#,
    )?;
    mutation.force = false;
    let receipt = lifecycle_status(&pm_root, &mutation, &options, false)?;
    assert_eq!(receipt["status"], "open");
    assert_eq!(
        receipt["warnings"][0],
        "ownership_warning:assignee_conflict:sample-unit:owner"
    );
    let receipt = lifecycle_status(&pm_root, &mutation, &options, false)?;
    assert_eq!(receipt["changed_field_count"], 0);
    fs::write(
        pm_root.join("settings.json"),
        r#"{"governance":{"preset":"minimal"}}"#,
    )?;
    let receipt = lifecycle_status(&pm_root, &mutation, &options, true)?;
    assert_eq!(receipt["status"], "in_progress");
    assert!(receipt.get("warnings").is_none());
    document.metadata.status = "canceled".to_owned();
    fs::write(&path, canonical_item_bytes(&document)?)?;
    mutation.force = true;
    let receipt = lifecycle_status(&pm_root, &mutation, &options, true)?;
    assert_eq!(receipt["changed_field_count"], 3);
    assert!(receipt.get("close_reason").is_none());
    document.metadata.status = "open".to_owned();
    fs::write(&path, canonical_item_bytes(&document)?)?;
    let receipt = lifecycle_status(&pm_root, &mutation, &options, false)?;
    assert_eq!(receipt["changed_field_count"], 0);
    assert_eq!(receipt["close_reason"], "Done");
    Ok(())
}

/// Invalid clocks and held locks never alter an item before a lifecycle update.
#[test]
fn lifecycle_validation_and_lock_faults_preserve_state() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root(r#"{"locks":{"wait_ms":0}}"#)?;
    create_item(&pm_root, request())?;
    let before = fs::read(pm_root.join("tasks/sample-unit.toon"))?;
    let options = crate::OwnershipOptions::default();
    let mut mutation = OwnershipItem {
        id: "sample-unit".to_owned(),
        author: "owner".to_owned(),
        timestamp: Some("2999-01-01T00:00:00Z".to_owned()),
        message: None,
        force: false,
        if_available: false,
        provenance_role: None,
    };
    assert!(matches!(
        lifecycle_status(&pm_root, &mutation, &options, true),
        Err(PmRustError::InvalidMutationRequest { .. })
    ));
    mutation.timestamp = Some("invalid".to_owned());
    assert!(lifecycle_status(&pm_root, &mutation, &options, true).is_err());
    mutation.timestamp = Some(TS.to_owned());
    let lock_path = pm_root.join("locks/sample-unit.lock");
    fs::create_dir_all(pm_root.join("locks"))?;
    fs::write(
        &lock_path,
        serde_json::to_string(&serde_json::json!({"author":"other", "created_at":now_iso()}))?,
    )?;
    assert!(matches!(
        lifecycle_status(&pm_root, &mutation, &options, true),
        Err(PmRustError::OwnershipRefusal { .. })
    ));
    fs::remove_file(&lock_path)?;
    // A corrupt lock is a filesystem fault, rather than ownership contention.
    fs::write(&lock_path, [0xff])?;
    assert!(matches!(
        lifecycle_status(&pm_root, &mutation, &options, true),
        Err(PmRustError::Io { .. })
    ));
    assert_eq!(fs::read(pm_root.join("tasks/sample-unit.toon"))?, before);
    Ok(())
}

/// Custom transitions name the allowed edges when a start is refused.
#[test]
fn lifecycle_custom_workflow_reports_allowed_edges() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root(
        r#"{"governance":{"workflow_enforcement":"strict"},"schema":{"type_workflows":[{"type":"Other"},{"type":"Task","allowed_transitions":[["draft","open"]]}]}}"#,
    )?;
    create_item(&pm_root, request())?;
    let mutation = OwnershipItem {
        id: "sample-unit".to_owned(),
        author: "owner".to_owned(),
        timestamp: Some(TS.to_owned()),
        message: None,
        force: false,
        if_available: false,
        provenance_role: None,
    };
    let error = lifecycle_status(
        &pm_root,
        &mutation,
        &crate::OwnershipOptions::default(),
        true,
    );
    assert!(
        matches!(error, Err(PmRustError::CommandRefusal {detail, ..}) if detail.contains("draft -> open"))
    );
    fs::write(
        pm_root.join("settings.json"),
        r#"{"governance":{"workflow_enforcement":"strict"},"schema":{"type_workflows":[{"type":"Task","allowed_transitions":[["open","closed"],["draft","open"],["open","in_progress"]]}]}}"#,
    )?;
    assert_eq!(
        lifecycle_status(
            &pm_root,
            &mutation,
            &crate::OwnershipOptions::default(),
            true
        )?["status"],
        "in_progress"
    );
    assert_eq!(
        lifecycle_status(
            &pm_root,
            &mutation,
            &crate::OwnershipOptions::default(),
            true
        )?["changed_field_count"],
        0
    );
    fs::write(
        pm_root.join("settings.json"),
        r#"{"governance":{"workflow_enforcement":"strict"},"schema":{"type_workflows":[{"type":"Task","allowed_transitions":[]}]}}"#,
    )?;
    assert!(
        matches!(lifecycle_status(&pm_root, &mutation, &crate::OwnershipOptions::default(), false), Err(PmRustError::CommandRefusal {detail,..}) if detail.contains("(no transitions allowed)"))
    );
    for schema in [
        serde_json::json!({}),
        serde_json::json!({"type_workflows":[]}),
        serde_json::json!({"type_workflows":[{"type":"Task"}]}),
    ] {
        fs::write(
            pm_root.join("settings.json"),
            serde_json::to_vec(
                &serde_json::json!({"governance":{"workflow_enforcement":"strict"},"schema":schema}),
            )?,
        )?;
        assert_eq!(
            lifecycle_status(
                &pm_root,
                &mutation,
                &crate::OwnershipOptions::default(),
                true
            )?["changed_field_count"],
            0
        );
    }
    Ok(())
}

#[test]
fn ownership_preset_precedence_and_lock_payload_clock() -> Result<(), Box<dyn std::error::Error>> {
    for (preset, policy, force) in [
        ("minimal", "none", false),
        ("strict", "strict", true),
        ("default", "warn", true),
        ("custom", "strict", false),
    ] {
        let governance = OwnershipGovernance {
            preset: preset.to_owned(),
            ownership_enforcement: "strict".to_owned(),
            force_required_for_stale_lock: Some(false),
        };
        assert_eq!(governance.policy(), policy);
        assert_eq!(governance.stale_requires_force(), force);
    }
    let governance = OwnershipGovernance {
        preset: "custom".to_owned(),
        ..OwnershipGovernance::default()
    };
    assert_eq!(governance.policy(), "warn");
    assert!(governance.stale_requires_force());
    assert!(ownership_lock_stale("invalid", TS, 1800));
    assert!(ownership_lock_stale("{}", TS, 1800));
    assert!(ownership_lock_stale(
        r#"{"created_at":"2000-01-01T00:00:00Z"}"#,
        TS,
        1800
    ));
    assert!(!ownership_lock_stale(
        &format!(r#"{{"created_at":"{TS}"}}"#),
        TS,
        1800
    ));
    assert!(!ownership_lock_stale(
        &format!(r#"{{"created_at":"{TS}"}}"#),
        "invalid",
        1800
    ));
    let (_directory, pm_root) = root(&settings("sample-", "toon", 1800))?;
    let request = OwnershipItem {
        id: "sample-unit".to_owned(),
        author: "unit-agent".to_owned(),
        timestamp: Some(TS.to_owned()),
        message: None,
        force: false,
        if_available: false,
        provenance_role: None,
    };
    let mut settings: MutationSettings = read_settings(&pm_root)?;
    assert!(matches!(
        ownership_lock_refusal(&pm_root, &request, &settings, TS, 0),
        PmRustError::OwnershipRefusal { .. }
    ));
    fs::create_dir_all(pm_root.join("locks"))?;
    fs::write(
        pm_root.join("locks/sample-unit.lock"),
        format!(r#"{{"created_at":"{TS}"}}"#),
    )?;
    settings.locks.wait_ms = 25;
    let error = ownership_lock_refusal(&pm_root, &request, &settings, TS, 25);
    assert!(error.to_string().contains("after waiting 25ms"));
    fs::write(
        pm_root.join("locks/sample-unit.lock"),
        format!(r#"{{"created_at":"{TS}","owner":"unit-holder"}}"#),
    )?;
    let error = ownership_lock_refusal(&pm_root, &request, &settings, TS, 0);
    assert!(error.to_string().contains("(owner unit-holder)"));

    Ok(())
}

#[test]
/// Structural lock faults and Windows diagnostic read refusals preserve state.
fn ownership_lock_filesystem_faults_remain_faults() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root(&settings("sample-", "toon", 1800))?;
    create_item(&pm_root, request())?;
    fs::create_dir(pm_root.join("locks/sample-unit.lock"))?;
    let request = OwnershipItem {
        id: "sample-unit".to_owned(),
        author: "unit-agent".to_owned(),
        timestamp: Some(TS.to_owned()),
        message: None,
        force: false,
        if_available: false,
        provenance_role: None,
    };
    assert!(matches!(
        ownership_item(&pm_root, &request, true),
        Err(PmRustError::Io { .. })
    ));
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let lock = pm_root.join("locks/sample-unit.lock");
        fs::remove_dir(&lock)?;
        fs::write(&lock, "{}")?;
        let item = pm_root.join("tasks/sample-unit.toon");
        let history = pm_root.join("history/sample-unit.jsonl");
        let before_item = fs::read(&item)?;
        let before_history = fs::read(&history)?;
        let held = OpenOptions::new().read(true).share_mode(0).open(&lock)?;
        let result = ownership_item(&pm_root, &request, true);
        drop(held);
        assert!(matches!(
            result,
            Err(PmRustError::Io { path, source })
                if path == lock && source.raw_os_error() == Some(32)
        ));
        assert_eq!(fs::read(&item)?, before_item);
        assert_eq!(fs::read(&history)?, before_history);
        fs::remove_file(lock)?;
        assert_eq!(
            ownership_item(&pm_root, &request, true)?.changed_field_count,
            2
        );
        assert_eq!(fs::read_to_string(history)?.lines().count(), 2);
    }
    Ok(())
}

#[test]
#[allow(clippy::too_many_lines)]
fn ownership_refusals_and_interrupted_history_append_remain_recoverable()
-> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root(
        r#"{"governance":{"preset":"custom","ownership_enforcement":"strict"},"locks":{"wait_ms":0}}"#,
    )?;
    create_item(&pm_root, request())?;
    let mut mutation = OwnershipItem {
        id: "sample-unit".to_owned(),
        author: "unit-agent".to_owned(),
        timestamp: Some(TS.to_owned()),
        message: None,
        force: false,
        if_available: false,
        provenance_role: None,
    };
    let settings_path = pm_root.join("settings.json");
    let settings_bytes = fs::read(&settings_path)?;
    fs::write(&settings_path, "invalid")?;
    assert!(ownership_item(&pm_root, &mutation, true).is_err());
    fs::write(&settings_path, settings_bytes)?;
    mutation.author = " ".to_owned();
    assert!(ownership_item(&pm_root, &mutation, true).is_err());
    mutation.author = "unit-agent".to_owned();
    mutation.id = "sample-missing".to_owned();
    assert!(ownership_item(&pm_root, &mutation, true).is_err());
    mutation.id = "sample-unit".to_owned();
    let lock_path = pm_root.join("locks/sample-unit.lock");
    for clock in [TS, "2000-01-01T00:00:00Z"] {
        fs::write(&lock_path, format!(r#"{{"created_at":"{clock}"}}"#))?;
        assert!(matches!(
            ownership_item(&pm_root, &mutation, true),
            Err(PmRustError::OwnershipRefusal { .. })
        ));
    }

    let mut timed_settings: Value = serde_json::from_slice(&fs::read(&settings_path)?)?;
    timed_settings["locks"]["wait_ms"] = serde_json::json!(25);
    fs::write(&settings_path, serde_json::to_vec(&timed_settings)?)?;
    fs::write(&lock_path, format!(r#"{{"created_at":"{TS}"}}"#))?;
    let error = ownership_item(&pm_root, &mutation, true);
    assert!(
        matches!(error, Err(PmRustError::OwnershipRefusal { detail, .. }) if detail.contains("after waiting"))
    );
    timed_settings["locks"]["wait_ms"] = serde_json::json!(0);
    fs::write(&settings_path, serde_json::to_vec(&timed_settings)?)?;
    fs::write(&lock_path, r#"{"created_at":"2000-01-01T00:00:00Z"}"#)?;
    mutation.force = true;
    ownership_item(&pm_root, &mutation, true)?;
    mutation.force = false;
    mutation.author = "other-agent".to_owned();
    mutation.if_available = true;
    assert_eq!(
        ownership_item(&pm_root, &mutation, true)?
            .claimed_by
            .as_deref(),
        Some("unit-agent")
    );
    mutation.if_available = false;
    let history_path = pm_root.join("history/sample-unit.jsonl");
    let history_backup = pm_root.join("history-backup.jsonl");
    fs::rename(&history_path, &history_backup)?;
    fs::create_dir(&history_path)?;
    assert!(matches!(
        ownership_item(&pm_root, &mutation, true),
        Err(PmRustError::Io { .. })
    ));
    mutation.author = "unit-agent".to_owned();
    assert!(matches!(
        ownership_item(&pm_root, &mutation, false),
        Err(PmRustError::Io { .. })
    ));
    let journal = pm_root.join("runtime/transactions/release-sample-unit.json");
    assert!(journal.exists());
    fs::remove_dir(&history_path)?;
    fs::rename(&history_backup, &history_path)?;
    ownership_item(&pm_root, &mutation, false)?;
    assert!(!journal.exists());
    assert_eq!(fs::read_to_string(history_path)?.lines().count(), 4);
    ownership_item(&pm_root, &mutation, true)?;
    assert_eq!(
        ownership_item(&pm_root, &mutation, true)?.changed_field_count,
        0
    );
    mutation.author = "other-agent".to_owned();
    assert!(matches!(
        ownership_item(&pm_root, &mutation, true),
        Err(PmRustError::OwnershipRefusal { .. })
    ));
    mutation.force = true;
    assert_eq!(
        ownership_item(&pm_root, &mutation, true)?.warnings,
        ["claim_takeover:unit-agent->other-agent"]
    );
    mutation.author = "unit-agent".to_owned();
    mutation.force = false;
    assert!(matches!(
        ownership_item(&pm_root, &mutation, false),
        Err(PmRustError::OwnershipRefusal { .. })
    ));

    timed_settings["governance"]["ownership_enforcement"] = serde_json::json!("warn");
    fs::write(&settings_path, serde_json::to_vec(&timed_settings)?)?;
    assert_eq!(
        ownership_item(&pm_root, &mutation, false)?.changed_field_count,
        2
    );
    timed_settings["governance"]["ownership_enforcement"] = serde_json::json!("strict");
    fs::write(&settings_path, serde_json::to_vec(&timed_settings)?)?;
    mutation.force = true;
    ownership_item(&pm_root, &mutation, false)?;
    let item_path = pm_root.join("tasks/sample-unit.toon");
    let mut document = decode_item(&item_path, &fs::read_to_string(&item_path)?)?;
    document.metadata.status = "in_progress".to_owned();
    document.metadata.extra.insert(
        "close_reason".to_owned(),
        serde_json::json!("fixture evidence"),
    );
    fs::write(&item_path, canonical_item_bytes(&document)?)?;
    mutation.force = false;
    assert_eq!(
        ownership_item(&pm_root, &mutation, true)?
            .close_reason
            .as_deref(),
        Some("fixture evidence")
    );
    assert_eq!(
        ownership_item(&pm_root, &mutation, false)?.warnings,
        ["released_unclaimed_in_progress"]
    );
    document.metadata.status = "closed".to_owned();
    fs::write(&item_path, canonical_item_bytes(&document)?)?;
    assert!(matches!(
        ownership_item(&pm_root, &mutation, true),
        Err(PmRustError::OwnershipRefusal { .. })
    ));
    mutation.force = true;
    ownership_item(&pm_root, &mutation, true)?;

    // Strict handoffs can clear a foreign claim through the narrow bypass.
    mutation.author = "handoff-agent".to_owned();
    mutation.force = false;
    let bypass = crate::OwnershipOptions {
        release_bypass: true,
        ..crate::OwnershipOptions::default()
    };
    assert_eq!(
        ownership_item_with_options(&pm_root, &mutation, false, &bypass)?.changed_field_count,
        2
    );
    mutation.author = "unit-agent".to_owned();
    mutation.force = true;
    ownership_item(&pm_root, &mutation, true)?;
    mutation.author = "handoff-agent".to_owned();
    assert_eq!(
        ownership_item(&pm_root, &mutation, false)?.changed_field_count,
        2
    );

    Ok(())
}

/// Completion clocks reject malformed calendars and clamp month-end arithmetic.
#[test]
fn closure_completion_and_quote_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    for (governance, expected) in [
        (
            serde_json::json!({"preset":"minimal","close_validation_default":"strict"}),
            "off",
        ),
        (serde_json::json!({"preset":"default"}), "warn"),
        (serde_json::json!({"preset":"strict"}), "strict"),
        (serde_json::json!({"preset":"custom"}), "warn"),
        (
            serde_json::json!({"preset":"custom","close_validation_default":"off"}),
            "off",
        ),
    ] {
        assert_eq!(
            close_mode(None, &serde_json::json!({"governance":governance}))?,
            expected
        );
    }
    assert_eq!(close_completion("now", TS)?, TS);
    for (value, expected) in [
        ("+2h", "2026-08-07T12:06:30.183Z"),
        ("+1d", "2026-08-08T10:06:30.183Z"),
        ("-1w", "2026-07-31T10:06:30.183Z"),
    ] {
        assert_eq!(close_completion(value, TS)?, expected);
    }
    assert_eq!(
        close_completion("+6m", "2026-08-31T23:30:00Z")?,
        "2027-02-28T23:30:00.000Z"
    );
    assert_eq!(
        close_completion("-1m", "2026-03-31T00:00:00Z")?,
        "2026-02-28T00:00:00.000Z"
    );
    assert!(close_completion("now", "invalid clock").is_err());
    for value in [
        "+1q",
        "+",
        "+9223372036854775807h",
        "+9223372036854775807d",
        "+9223372036854775807w",
        "+1200000000m",
        "+1m",
    ] {
        assert!(
            close_completion(value, "9999-12-31T00:00:00Z").is_err(),
            "{value}"
        );
    }
    for value in ["2026-00-01", "2026-13-01", "2026-02-30", "2026-02-00"] {
        assert!(close_calendar(value).is_err());
    }
    for (value, normalized) in [
        ("20260820", "2026-08-20"),
        ("20260820T12:00:00Z", "2026-08-20T12:00:00Z"),
        ("2024-02-29", "2024-02-29"),
        ("text-not-a-date", "text-not-a-date"),
        ("abcd-xx-yy", "abcd-xx-yy"),
        ("日付", "日付"),
    ] {
        assert_eq!(close_calendar(value)?, normalized);
    }
    for value in [
        "plain",
        "_./:@=-",
        "",
        "space token",
        "quote\"",
        "slash\\",
        "space\\ end",
        "tail \\",
        "quoted \\\" value",
        "$value",
        "`value`",
    ] {
        let rendered = quote_command_arg(value);
        if matches!(value, "plain" | "_./:@=-") || (cfg!(windows) && value == "slash\\") {
            assert_eq!(rendered, value);
        } else {
            assert!(rendered.starts_with('"') && rendered.ends_with('"'));
        }
    }
    #[cfg(not(windows))]
    assert_eq!(quote_command_arg("$`\\\""), "\"\\$\\`\\\\\\\"\"");
    Ok(())
}

/// Session refusal diagnostics distinguish invalid payloads, expiry and live ownership.
#[test]
fn closure_session_lock_shape_diagnostics() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root("{}")?;
    fs::create_dir_all(pm_root.join("locks"))?;
    let path = pm_root.join("locks/session-state.lock");
    let valid = serde_json::json!({"id":"session-state","pid":1,"owner":"alpha","created_at":TS,"ttl_seconds":1800,"token":"owner"});
    let mut variants = vec![serde_json::json!(null), serde_json::json!([])];
    for key in ["id", "pid", "owner", "created_at", "ttl_seconds", "token"] {
        let mut value = valid.clone();
        value[key] = serde_json::json!(false);
        variants.push(value);
    }
    let mut blank = valid.clone();
    blank["token"] = serde_json::json!(" ");
    variants.push(blank);
    for value in variants {
        let raw = value.to_string();
        fs::write(&path, &raw)?;
        let error = session_lock_refusal(
            &pm_root,
            PmRustError::LockConflict {
                id: "session-state".to_owned(),
            },
            TS,
            1800,
        );
        assert!(
            matches!(error,PmRustError::CommandRefusal{detail,exit_code:4,..} if detail.contains("lock_info_invalid_shape"))
        );
        assert_eq!(fs::read_to_string(&path)?, raw);
    }
    for raw in ["invalid", ""] {
        fs::write(&path, raw)?;
        assert!(
            matches!(session_lock_refusal(&pm_root,PmRustError::LockConflict{id:"session-state".to_owned()},TS,1800),PmRustError::CommandRefusal{detail,..} if detail.contains("lock_info_invalid_json"))
        );
    }
    fs::write(&path, valid.to_string())?;
    assert!(matches!(
        session_lock_refusal(
            &pm_root,
            PmRustError::LockConflict {
                id: "session-state".to_owned()
            },
            TS,
            1800
        ),
        PmRustError::LockConflict { .. }
    ));
    let mut expired = valid;
    expired["created_at"] = serde_json::json!("2000-01-01T00:00:00Z");
    expired
        .as_object_mut()
        .ok_or("lock object")?
        .shift_remove("token");
    fs::write(&path, expired.to_string())?;
    assert!(
        matches!(session_lock_refusal(&pm_root,PmRustError::LockConflict{id:"session-state".to_owned()},TS,1800),PmRustError::CommandRefusal{detail,..} if !detail.contains("lock_info_invalid"))
    );
    let io = fs::read_to_string(pm_root.join("missing-file"))
        .err()
        .ok_or("missing I/O failure")?;
    assert!(matches!(
        session_lock_refusal(&pm_root, PmRustError::Io { path, source: io }, TS, 1800),
        PmRustError::Io { .. }
    ));
    Ok(())
}

/// Builds a canonical closure request for real SDK storage-policy tests.
fn closure_request() -> CloseItem {
    CloseItem {
        id: "sample-unit".to_owned(),
        reason: "Done".to_owned(),
        author: "unit-agent".to_owned(),
        timestamp: Some(TS.to_owned()),
        provenance_role: None,
        force_stale_lock: false,
        fields: CloseFields::default(),
    }
}

/// Closure ownership and predecessor cleanup preserve strict and advisory governance.
#[test]
fn closure_ownership_and_predecessor_storage() -> Result<(), Box<dyn std::error::Error>> {
    for policy in ["none", "warn", "strict"] {
        for force in [false, true] {
            let settings = serde_json::json!({"governance":{"preset":"custom","ownership_enforcement":policy,"require_close_reason":true},"item_format":"toon"});
            let (_directory, pm_root) = root(&settings.to_string())?;
            create_item(&pm_root, request())?;
            let owner = OwnershipItem {
                id: "sample-unit".to_owned(),
                author: "other-owner".to_owned(),
                timestamp: Some(TS.to_owned()),
                message: None,
                force: false,
                if_available: false,
                provenance_role: None,
            };
            ownership_item_with_options(
                &pm_root,
                &owner,
                true,
                &crate::OwnershipOptions::default(),
            )?;
            let mut closing = closure_request();
            closing.fields.force = force;
            let result = close_item(&pm_root, &pm_root, closing);
            if policy == "strict" && !force {
                assert!(matches!(result, Err(PmRustError::OwnershipRefusal { .. })));
            } else {
                let receipt = result?.close_receipt.ok_or("receipt")?;
                assert_eq!(receipt["status"], "closed");
                if policy == "warn" && !force {
                    assert!(
                        receipt["warnings"][0]
                            .as_str()
                            .is_some_and(|warning| warning.starts_with("ownership_warning"))
                    );
                }
            }
        }
    }
    let (_directory, pm_root) = root("{}")?;
    let created = create_item(&pm_root, request())?;
    let path = pm_root.join(&created.item_path);
    let mut document = created.item;
    document
        .metadata
        .extra
        .insert("blocked_by".to_owned(), serde_json::json!("sample-prior"));
    document
        .metadata
        .extra
        .insert("blocked_reason".to_owned(), serde_json::json!("Waiting"));
    document.metadata.extra.insert("dependencies".to_owned(),serde_json::json!([{"kind":"blocked_by","id":"sample-prior"},{"kind":"blocked_by","id":"sample-other"},{"kind":"related","id":"sample-linked"},{"kind":"blocked_by"}]));
    fs::write(path, canonical_item_bytes(&document)?)?;
    let result = close_item(&pm_root, &pm_root, closure_request())?;
    assert!(!result.item.metadata.extra.contains_key("blocked_by"));
    assert!(!result.item.metadata.extra.contains_key("blocked_reason"));
    assert_eq!(
        result.item.metadata.extra["dependencies"],
        document.metadata.extra["dependencies"]
    );
    assert_eq!(
        result.close_receipt.ok_or("receipt")?["warnings"],
        serde_json::json!(["closed_preserved_predecessors:sample-unit:sample-prior,sample-other"])
    );
    Ok(())
}

/// Duplicate traversal refuses chains and malformed target documents without writes.
#[test]
fn closure_duplicate_chain_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root("{}")?;
    create_item(&pm_root, request())?;
    assert!(matches!(
        close_duplicate(&pm_root, "sample-unit", Some("sample-unit")),
        Err(PmRustError::CommandRefusal { context, .. }) if context["code"] == "duplicate_target_self"
    ));
    let mut target = request();
    target.id = "sample-target".to_owned();
    let created = create_item(&pm_root, target)?;
    let path = pm_root.join(created.item_path);
    for duplicate in ["sample-unit", "sample-absent", "sample-target"] {
        let mut document = created.item.clone();
        document
            .metadata
            .extra
            .insert("duplicate_of".to_owned(), serde_json::json!(duplicate));
        fs::write(&path, canonical_item_bytes(&document)?)?;
        assert!(close_duplicate(&pm_root, "sample-unit", Some("sample-target")).is_err());
    }
    fs::write(&path, "invalid document")?;
    assert!(close_duplicate(&pm_root, "sample-unit", Some("sample-target")).is_err());
    fs::write(&path, canonical_item_bytes(&created.item)?)?;
    assert_eq!(close_duplicate(&pm_root, "sample-unit", Some(" "))?, None);
    assert_eq!(
        close_duplicate(&pm_root, "sample-unit", Some(" sample-target "))?,
        Some("sample-target".to_owned())
    );
    Ok(())
}

/// Legacy serialized close requests default new controls and permit resolution reasons.
#[test]
fn closure_request_deserialization_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let closing: CloseItem = serde_json::from_value(
        serde_json::json!({"id":"sample-unit","author":"unit-agent","resolution":"Fixed"}),
    )?;
    assert!(closing.reason.is_empty());
    assert!(!closing.fields.force);
    assert_eq!(closing.fields.resolution.as_deref(), Some("Fixed"));
    Ok(())
}

/// Full inline closure options are exercised through the SDK on canonical storage.
#[test]
fn closure_sdk_fields_and_evidence_modes() -> Result<(), Box<dyn std::error::Error>> {
    for mode in ["off", "warn", "strict"] {
        let (_directory, pm_root) = root("{}")?;
        create_item(&pm_root, request())?;
        let mut closing = closure_request();
        closing.fields = CloseFields {
            resolution: Some(" Fixed ".to_owned()),
            expected_result: Some("Expected".to_owned()),
            actual_result: Some("Actual".to_owned()),
            completed_at: Some("2026-08-03".to_owned()),
            message: Some("Evidence".to_owned()),
            validate_close: Some(mode.to_owned()),
            ..CloseFields::default()
        };
        let result = close_item(&pm_root, &pm_root, closing)?;
        assert_eq!(result.item.metadata.extra["resolution"], "Fixed");
        assert_eq!(
            result.item.metadata.extra["completed_at"],
            "2026-08-03T00:00:00.000Z"
        );
        assert!(serde_json::to_value(&result)?["item_path"].is_string());
    }
    for mode in ["warn", "strict"] {
        let (_directory, pm_root) = root("{}")?;
        create_item(&pm_root, request())?;
        let mut closing = closure_request();
        closing.fields.validate_close = Some(mode.to_owned());
        let result = close_item(&pm_root, &pm_root, closing);
        if mode == "strict" {
            assert!(matches!(
                result,
                Err(PmRustError::CommandRefusal { exit_code: 2, .. })
            ));
        } else {
            // Successful warn closes return the completion follow-up instead of
            // failure recovery (GH-1464).
            assert!(result?.close_receipt.ok_or("receipt")?["follow_up"].is_object());
        }
    }
    let (_directory, pm_root) = root(r#"{"governance":{"require_close_reason":false}}"#)?;
    create_item(&pm_root, request())?;
    let mut closing = closure_request();
    closing.reason.clear();
    assert!(
        !close_item(&pm_root, &pm_root, closing)?
            .item
            .metadata
            .extra
            .contains_key("close_reason")
    );
    for mode in [
        Some(""),
        Some("none"),
        Some("disabled"),
        Some("invalid"),
        None,
    ] {
        let result = close_mode(mode, &serde_json::json!({}));
        assert_eq!(result.is_err(), mode == Some("invalid"));
    }
    for value in [
        "+9223372036854775807m",
        "-9223372036854775807m",
        "not-a-date",
        "+3d+1h",
    ] {
        assert!(close_completion(value, TS).is_err());
    }
    for value in [
        "xxxx-02-01",
        "2026-xx-01",
        "2026-01-xx",
        "x",
        "abcdefgh",
        "20260820",
    ] {
        let _ = close_calendar(value)?;
    }
    Ok(())
}

/// Duplicate fallback, child validation and preexisting completion evidence remain durable.
#[test]
fn closure_sdk_duplicate_children_and_completion() -> Result<(), Box<dyn std::error::Error>> {
    for mode in ["off", "warn", "strict"] {
        let (_directory, pm_root) = root("{}")?;
        create_item(&pm_root, request())?;
        let mut child = request();
        child.id = "sample-child".to_owned();
        let child = create_item(&pm_root, child)?;
        let mut document = child.item;
        document.metadata.parent = Some("sample-unit".to_owned());
        fs::write(
            pm_root.join(child.item_path),
            canonical_item_bytes(&document)?,
        )?;
        let mut closing = closure_request();
        closing.fields = CloseFields {
            resolution: Some("Fixed".to_owned()),
            expected_result: Some("Expected".to_owned()),
            actual_result: Some("Actual".to_owned()),
            validate_close: Some(mode.to_owned()),
            ..CloseFields::default()
        };
        let result = close_item(&pm_root, &pm_root, closing);
        if mode == "strict" {
            assert!(matches!(
                result,
                Err(PmRustError::CommandRefusal { exit_code: 2, .. })
            ));
        } else {
            assert!(
                result?.close_receipt.ok_or("receipt")?["warnings"][0]
                    .as_str()
                    .is_some_and(|warning| warning.contains("children"))
            );
        }
    }
    let (_directory, pm_root) = root("{}")?;
    create_item(&pm_root, request())?;
    let mut target = request();
    target.id = "sample-target".to_owned();
    let target = create_item(&pm_root, target)?;
    let mut closing = closure_request();
    closing.fields.duplicate_of = Some("sample-target".to_owned());
    closing.fields.validate_close = Some("strict".to_owned());
    closing.fields.resolution = Some("Supplied resolution".to_owned());
    let result = close_item(&pm_root, &pm_root, closing)?;
    assert_eq!(result.item.metadata.extra["duplicate_of"], "sample-target");
    assert_eq!(
        result.item.metadata.extra["resolution"],
        "Supplied resolution"
    );
    let mut repeated = closure_request();
    repeated.fields.force = true;
    assert_eq!(
        close_item(&pm_root, &pm_root, repeated)?
            .item
            .metadata
            .extra["completed_at"],
        result.item.metadata.extra["completed_at"]
    );
    let mut target_doc = target.item;
    target_doc.metadata.extra.insert(
        "duplicate_of".to_owned(),
        serde_json::json!("sample-corrupt"),
    );
    fs::write(
        pm_root.join(target.item_path),
        canonical_item_bytes(&target_doc)?,
    )?;
    fs::write(pm_root.join("tasks/sample-corrupt.toon"), "invalid")?;
    assert!(matches!(
        close_duplicate(&pm_root, "sample-unit", Some("sample-target")),
        Err(PmRustError::InvalidItemDocument { .. })
    ));
    Ok(())
}

/// SDK refusal propagation leaves item and history bytes unchanged.
#[test]
fn closure_sdk_refusals_preserve_before_images() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root("{}")?;
    let created = create_item(&pm_root, request())?;
    let before = fs::read(pm_root.join(&created.item_path))?;
    let history = fs::read(pm_root.join(&created.history_path))?;
    for fields in [
        CloseFields {
            duplicate_of: Some("sample-absent".to_owned()),
            ..CloseFields::default()
        },
        CloseFields {
            validate_close: Some("invalid".to_owned()),
            ..CloseFields::default()
        },
        CloseFields {
            completed_at: Some("2026-02-30".to_owned()),
            ..CloseFields::default()
        },
    ] {
        let mut request = closure_request();
        request.fields = fields;
        assert!(matches!(
            close_item(&pm_root, &pm_root, request),
            Err(PmRustError::CommandRefusal { exit_code: 2, .. })
        ));
        assert_eq!(fs::read(pm_root.join(&created.item_path))?, before);
        assert_eq!(fs::read(pm_root.join(&created.history_path))?, history);
    }
    ownership_item(
        &pm_root,
        &OwnershipItem {
            id: "sample-unit".to_owned(),
            author: "unit-agent".to_owned(),
            timestamp: Some(TS.to_owned()),
            message: None,
            force: false,
            if_available: false,
            provenance_role: None,
        },
        true,
    )?;
    let closed = close_item(&pm_root, &pm_root, closure_request())?;
    assert_eq!(closed.item.metadata.status, "closed");
    assert!(!closed.item.metadata.extra.contains_key("assignee"));
    assert!(
        closed
            .close_receipt
            .ok_or("receipt")?
            .get("warnings")
            .is_none()
    );
    Ok(())
}

/// An unreadable closure recovery journal leaves the before-image and history unchanged.
#[test]
fn closure_recovery_failure_preserves_before_image() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root("{}")?;
    let created = create_item(&pm_root, request())?;
    let before = fs::read(pm_root.join(&created.item_path))?;
    let history = fs::read(pm_root.join(&created.history_path))?;
    fs::create_dir(pm_root.join("runtime/transactions/close-sample-unit.json"))?;
    assert!(close_item(&pm_root, &pm_root, closure_request()).is_err());
    assert_eq!(fs::read(pm_root.join(created.item_path))?, before);
    assert_eq!(fs::read(pm_root.join(created.history_path))?, history);
    Ok(())
}

/// Failed history publication retains the closure journal for an idempotent retry.
#[test]
fn closure_history_failure_retains_recoverable_commit() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root("{}")?;
    let created = create_item(&pm_root, request())?;
    let history_path = pm_root.join(&created.history_path);
    let history_backup = pm_root.join("history-backup.jsonl");
    let before_history = fs::read_to_string(&history_path)?;
    fs::rename(&history_path, &history_backup)?;
    fs::create_dir(&history_path)?;
    assert!(matches!(
        close_item(&pm_root, &pm_root, closure_request()),
        Err(PmRustError::Io { .. })
    ));
    let committed = fs::read_to_string(pm_root.join(&created.item_path))?;
    assert_eq!(
        locate_item(&pm_root, "sample-unit")?.1.metadata.status,
        "closed"
    );
    let journal = pm_root.join("runtime/transactions/close-sample-unit.json");
    assert!(journal.is_file());
    fs::remove_dir(&history_path)?;
    fs::rename(&history_backup, &history_path)?;
    recover_mutation(&pm_root, "close", "sample-unit")?;
    assert!(!journal.exists());
    assert_eq!(
        fs::read_to_string(pm_root.join(created.item_path))?,
        committed
    );
    let recovered = fs::read_to_string(&history_path)?;
    assert!(recovered.starts_with(&before_history));
    assert_eq!(recovered.lines().count(), 2);
    assert_eq!(
        serde_json::from_str::<Value>(recovered.lines().last().ok_or("close history")?)?["op"],
        "close"
    );
    recover_mutation(&pm_root, "close", "sample-unit")?;
    assert_eq!(fs::read_to_string(history_path)?, recovered);
    Ok(())
}

/// Identifier grammar is enforced before even creating a locks directory.
#[test]
fn closure_prelock_identifier_grammar() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root("{}")?;
    for id in ["", "-leading", "trailing-", "bad/id", "bad id"] {
        assert!(matches!(
            acquire_lock(
                &pm_root,
                id,
                "unit-agent",
                &LockSettings::default(),
                false,
                TS
            ),
            Err(PmRustError::InvalidMutationRequest { .. })
        ));
        assert!(!pm_root.join("locks").exists());
    }
    Ok(())
}

/// Complete terminal evidence needs no recovery, while stored children sort deterministically.
#[test]
fn closure_complete_terminal_and_multiple_children() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root("{}")?;
    create_item(&pm_root, request())?;
    let mut closing = closure_request();
    closing.force_stale_lock = true;
    closing.fields = CloseFields {
        resolution: Some("Fixed".to_owned()),
        expected_result: Some("Expected".to_owned()),
        actual_result: Some("Actual".to_owned()),
        ..CloseFields::default()
    };
    close_item(&pm_root, &pm_root, closing)?;
    assert!(
        matches!(close_item(&pm_root,&pm_root,closure_request()),Err(PmRustError::CommandRefusal{context,..}) if context.get("recovery").is_none())
    );
    let (_other, root) = root("{}")?;
    create_item(&root, request())?;
    for id in ["sample-z", "sample-a"] {
        let mut child = request();
        child.id = id.to_owned();
        let child = create_item(&root, child)?;
        let mut document = child.item;
        document.metadata.parent = Some("sample-unit".to_owned());
        fs::write(root.join(child.item_path), canonical_item_bytes(&document)?)?;
    }
    assert_eq!(
        close_item(&root, &root, closure_request())?
            .close_receipt
            .ok_or("receipt")?["warnings"],
        serde_json::json!(["closed_with_active_children:sample-unit:sample-a,sample-z"])
    );
    Ok(())
}

/// Runtime-schema and corrupt-corpus failures refuse before canonical close publication.
#[test]
fn closure_read_failures_preserve_item_bytes() -> Result<(), Box<dyn std::error::Error>> {
    for corrupt_schema in [true, false] {
        let (_directory, pm_root) = root("{}")?;
        let created = create_item(&pm_root, request())?;
        let path = pm_root.join(&created.item_path);
        let before = fs::read(&path)?;
        if corrupt_schema {
            fs::write(
                pm_root.join("settings.json"),
                r#"{"schema":{"files":{"statuses":"missing-statuses.json"}}}"#,
            )?;
        } else {
            fs::write(pm_root.join("tasks/sample-corrupt.toon"), "invalid")?;
        }
        assert!(close_item(&pm_root, &pm_root, closure_request()).is_err());
        assert_eq!(fs::read(path)?, before);
    }
    Ok(())
}

/// A noncanonical tracker marker cannot be used to publish a closure.
#[test]
fn closure_noncanonical_root_refuses_discovery() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("tracker");
    fs::create_dir(&root)?;
    fs::write(root.join("settings.json"), "{}")?;
    let created = create_item(&root, request())?;
    let before = fs::read(root.join(&created.item_path))?;
    assert!(matches!(
        close_item(&root, &root, closure_request()),
        Err(PmRustError::TrackerNotFound { .. })
    ));
    assert_eq!(fs::read(root.join(created.item_path))?, before);
    Ok(())
}

/// Recovery hints must spell the caller-visible tracker path (Node's `path.resolve` form on Windows), never the canonical one.
#[test]
fn close_recovery_spells_the_display_root_not_the_canonical_root()
-> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root("{}")?;
    create_item(&pm_root, request())?;
    let mut closing = closure_request();
    closing.fields.validate_close = Some("strict".to_owned());
    let display = std::path::Path::new("display-root/.agents/pm");
    match close_item(&pm_root, display, closing) {
        Err(PmRustError::CommandRefusal { context, .. }) => {
            let retry = context
                .get("recovery")
                .and_then(|recovery| recovery.get("suggested_retry"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned();
            assert!(retry.contains("display-root/.agents/pm"), "{retry}");
            assert!(!retry.contains(&*pm_root.to_string_lossy()), "{retry}");
            Ok(())
        }
        other => Err(format!("expected a strict close refusal, got {other:?}").into()),
    }
}

/// Future collection fixture clocks cannot remove another writer's live lock.
#[test]
fn future_collection_preserves_held_lock() -> Result<(), Box<dyn std::error::Error>> {
    for (key, add) in [
        ("files", "src/a.rs"),
        ("docs", "README.md"),
        ("tests", "command=cargo test"),
    ] {
        let (directory, pm_root) = root(
            r#"{"governance":{"preset":"minimal"},"locks":{"ttl_seconds":1800,"wait_ms":0}}"#,
        )?;
        create_item(&pm_root, request())?;
        let workspace = crate::Workspace::discover(&pm_root)?;
        let held = acquire_lock(
            &pm_root,
            "sample-unit",
            "live-holder",
            &LockSettings {
                use_created_at: true,
                ..locks(1800)
            },
            false,
            &now_iso(),
        )?;
        let paths = [
            pm_root.join("tasks/sample-unit.toon"),
            pm_root.join("history/sample-unit.jsonl"),
            held.path.clone(),
        ];
        let before = paths.each_ref().map(|path| fs::read(path).ok());
        for force in [false, true] {
            let result = workspace.collection(
                "sample-unit",
                key,
                &crate::CollectionOptions {
                    author: "unit-agent".to_owned(),
                    timestamp: Some("9999-12-31T23:59:59.999Z".to_owned()),
                    add: vec![add.to_owned()],
                    force,
                    ..crate::CollectionOptions::default()
                },
                directory.path(),
            );
            assert_eq!(
                paths.each_ref().map(|path| fs::read(path).ok()),
                before,
                "held-lock bytes changed: {result:?}"
            );
            assert!(
                matches!(result, Err(PmRustError::OwnershipRefusal { context, .. }) if context["code"] == "lock_conflict")
            );
        }
        drop(held);
        assert!(!paths[2].exists());
    }
    Ok(())
}

/// Lease errors retain their exact path and underlying OS or filesystem diagnostic.
#[test]
fn gate_lease_errors_preserve_io_diagnostics() {
    let path = PathBuf::from("locks/sample-unit.lock.gate-lease");
    for source in [
        std::io::Error::from(ErrorKind::Unsupported),
        std::io::Error::from(ErrorKind::PermissionDenied),
        std::io::Error::from_raw_os_error(37),
    ] {
        let kind = source.kind();
        let os_code = source.raw_os_error();
        let message = source.to_string();
        let error = gate_lease_error(fs::TryLockError::Error(source), path.clone(), "sample-unit");
        let PmRustError::Io {
            path: actual,
            source,
        } = error
        else {
            assert!(
                matches!(error, PmRustError::Io { .. }),
                "a lease I/O failure was reported as contention: {error}"
            );
            return;
        };
        assert_eq!(actual, path);
        assert_eq!(source.kind(), kind);
        assert_eq!(source.raw_os_error(), os_code);
        assert_eq!(source.to_string(), message);
    }
    assert!(
        matches!(gate_lease_error(fs::TryLockError::WouldBlock, path, "sample-unit"),
        PmRustError::LockConflict { id } if id == "sample-unit")
    );
}

/// Unpublished lock payloads stay busy, while nonempty malformed payloads remain stale.
#[test]
fn unpublished_lock_payloads_are_not_stale() -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, pm_root) = root("{}")?;
    let request = OwnershipItem {
        id: "sample-unit".to_owned(),
        author: "unit-agent".to_owned(),
        timestamp: Some(TS.to_owned()),
        message: None,
        force: false,
        if_available: false,
        provenance_role: None,
    };
    let settings = read_settings(&pm_root)?;
    fs::create_dir_all(pm_root.join("locks"))?;
    for raw in [
        None,
        Some(""),
        Some(" \n\t"),
        Some("invalid"),
        Some("{}"),
        Some(r#"{"created_at":"2000-01-01T00:00:00Z"}"#),
    ] {
        if let Some(raw) = raw {
            fs::write(pm_root.join("locks/sample-unit.lock"), raw)?;
        }
        let error = ownership_lock_refusal(&pm_root, &request, &settings, TS, 0);
        let PmRustError::OwnershipRefusal {
            detail, context, ..
        } = error
        else {
            return Err(format!("unexpected lock refusal: {error}").into());
        };
        if raw.is_none_or(|value| value.trim().is_empty()) {
            assert_eq!(detail, "Item sample-unit is locked");
            assert_eq!(
                context,
                serde_json::json!({"code":"lock_conflict","recovery":{"retry_after_ms":250}})
            );
        } else {
            assert!(detail.contains("lock is stale"));
        }
    }
    Ok(())
}
