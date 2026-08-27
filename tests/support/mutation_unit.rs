use std::fs;
use std::thread;
use std::time::Duration;

use proptest::prelude::*;
use tempfile::TempDir;

use crate::item::decode_item;

use super::*;

const TS: &str = "2026-08-07T10:06:30.183Z";

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
    assert!(parsed.description.is_empty());
    assert!(parsed.tags.is_empty());
    assert!(parsed.body.is_empty());
    assert!(parsed.timestamp.is_none());
    assert!(parsed.message.is_none());
    assert!(!parsed.force_stale_lock);
    let settings: MutationSettings = serde_json::from_str("{}")?;
    assert!(settings.id_prefix.is_empty());
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
    #[cfg(unix)]
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
        let mut extra = BTreeMap::new();
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

#[cfg(unix)]
#[test]
fn non_utf8_atomic_target_names_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let directory = tempfile::tempdir()?;
    let target = directory.path().join(OsString::from_vec(vec![0xff]));
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
            CloseItem {
                id: "sample-unit".to_owned(),
                reason: "reason".to_owned(),
                author: "unit-agent".to_owned(),
                timestamp: Some(TS.to_owned()),
                provenance_role: None,
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
    let appended_history = format!("{original_history}{{\"ts\":\"stub\",\"op\":\"update\"}}\n");

    // A journal whose transaction fully committed is simply removed.
    write_mutation_journal(&pm_root, "update", &updated_item, &appended_history)?;
    fs::write(&item_path, &updated_item)?;
    fs::write(&history_path, &appended_history)?;
    recover_mutation(&pm_root, "update", "sample-unit")?;
    assert!(
        !pm_root
            .join("runtime/transactions/update-sample-unit.json")
            .exists()
    );

    // A journal whose halves are absent is completed from its bytes.
    write_mutation_journal(&pm_root, "update", &updated_item, &appended_history)?;
    fs::remove_file(&item_path)?;
    fs::remove_file(&history_path)?;
    recover_mutation(&pm_root, "update", "sample-unit")?;
    assert_eq!(fs::read_to_string(&item_path)?, updated_item);
    assert!(fs::read_to_string(&history_path)?.ends_with(&appended_history));
    // Restore the pre-mutation state for the conflict cases below.
    fs::write(&item_path, &original_item)?;
    fs::write(&history_path, &original_history)?;

    // A crash before the item replace leaves the pre-mutation item in place.
    // The journal records the pre-mutation hash, so recovery recognises the
    // before-image and rolls the whole transaction forward instead of
    // blocking the identifier permanently.
    write_mutation_journal(&pm_root, "update", &updated_item, &appended_history)?;
    recover_mutation(&pm_root, "update", "sample-unit")?;
    assert_eq!(fs::read_to_string(&item_path)?, updated_item);
    assert!(fs::read_to_string(&history_path)?.ends_with(&appended_history));
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
    write_mutation_journal(&pm_root, "update", &updated_item, &appended_history)?;
    fs::write(&item_path, &updated_item)?;
    recover_mutation(&pm_root, "update", "sample-unit")?;
    assert_eq!(fs::read_to_string(&item_path)?, updated_item);
    assert!(fs::read_to_string(&history_path)?.ends_with(&appended_history));
    assert!(
        !pm_root
            .join("runtime/transactions/update-sample-unit.json")
            .exists()
    );
    fs::write(&item_path, &original_item)?;
    fs::write(&history_path, &original_history)?;

    // Foreign durable bytes that match neither the pre-mutation nor the
    // post-mutation image refuse recovery instead of being overwritten.
    write_mutation_journal(&pm_root, "update", &updated_item, &appended_history)?;
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
    write_mutation_journal(&pm_root, "update", &updated_item, &appended_history)?;
    fs::write(
        &history_path,
        format!("{original_history}{{\"foreign\":1}}\n"),
    )?;
    recover_mutation(&pm_root, "update", "sample-unit")?;
    assert_eq!(fs::read_to_string(&item_path)?, updated_item);
    assert!(fs::read_to_string(&history_path)?.ends_with(&appended_history));
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
    write_mutation_journal(&pm_root, "update", &updated_item, &appended_history)?;
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

#[cfg(unix)]
#[test]
/// Covers the non-UTF-8 defensive input-shape failure of the publisher.
fn atomic_replace_rejects_non_utf8_targets() -> Result<(), Box<dyn std::error::Error>> {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let directory = tempfile::tempdir()?;
    let non_utf8 = OsStr::from_bytes(b"sample-\xff.toon");
    assert!(matches!(
        atomic_replace(&directory.path().join(non_utf8), "value"),
        Err(PmRustError::InvalidCreateRequest { .. })
    ));
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

#[test]
#[cfg(unix)]
/// Proves the recursive locator skips a non-file, non-directory entry.
///
/// A named pipe (FIFO) is neither a directory, a symlink, nor a regular
/// file. The `else if entry_path.is_file()` guard must evaluate to `false`
/// for it and skip it, so the real item beside it is still found. This covers
/// the `is_file() == false` branch of the locator's else-if.
fn locate_item_skips_a_non_file_entry() -> Result<(), Box<dyn std::error::Error>> {
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
    std::process::Command::new("mkfifo").arg(&fifo).status()?;
    let (found_path, document) = locate_item(&pm_root, "sample-unit")?;
    assert_eq!(found_path, nested.join("sample-unit.toon"));
    assert_eq!(document.metadata.id, "sample-unit");
    fs::remove_file(&fifo)?;
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
            CloseItem {
                id: "sample-unit".to_owned(),
                reason: "reason".to_owned(),
                author: "unit-agent".to_owned(),
                timestamp: Some(TS.to_owned()),
                provenance_role: None,
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
            CloseItem {
                id: "sample-unit".to_owned(),
                reason: "reason".to_owned(),
                author: "unit-agent".to_owned(),
                timestamp: Some(TS.to_owned()),
                provenance_role: None,
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
                CloseItem {
                    id: "sample-unit".to_owned(),
                    reason: "reason".to_owned(),
                    author: author.to_owned(),
                    timestamp: Some(TS.to_owned()),
                    provenance_role: None,
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
            CloseItem {
                id: "sample-missing".to_owned(),
                reason: "reason".to_owned(),
                author: "unit-agent".to_owned(),
                timestamp: Some(TS.to_owned()),
                provenance_role: None,
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
            CloseItem {
                id: "sample-unit".to_owned(),
                reason: "reason".to_owned(),
                author: "unit-agent".to_owned(),
                timestamp: Some(TS.to_owned()),
                provenance_role: None,
                force_stale_lock: false,
            }
        ),
        Err(PmRustError::InvalidMutationRequest { .. })
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
        CloseItem {
            id: "sample-unit".to_owned(),
            reason: "shipped to customers".to_owned(),
            author: "unit-agent".to_owned(),
            timestamp: Some(TS.to_owned()),
            provenance_role: None,
            force_stale_lock: false,
        },
    )?;
    assert_eq!(closed.item.metadata.status, "shipped");
    assert!(matches!(
        close_item(
            &shipped_root,
            CloseItem {
                id: "sample-unit".to_owned(),
                reason: "again".to_owned(),
                author: "unit-agent".to_owned(),
                timestamp: Some(TS.to_owned()),
                provenance_role: None,
                force_stale_lock: false,
            }
        ),
        Err(PmRustError::InvalidMutationRequest { .. })
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
/// `"true"` and `"0."` pass the strip-and-filter half (simple quotes, no
/// escapes) yet must stay quoted: unquoting them would change what the decoder
/// reads back (`true` becomes boolean, `0.` becomes integer). Without this
/// case the guard-false arm of that match is never exercised.
fn normalize_row_bytes_preserves_quoted_ambiguous_scalars() {
    assert_eq!(
        normalize_row_bytes("  \"true\",plain"),
        "  \"true\",plain",
        "a quoted boolean-looking field must keep its quotes"
    );
    assert_eq!(
        normalize_row_bytes("  \"0.\",plain"),
        "  \"0.\",plain",
        "a quoted lenient-number field must keep its quotes"
    );
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
            CloseItem {
                id: "sample-unit".to_owned(),
                reason: "  ".to_owned(),
                author: "unit-agent".to_owned(),
                timestamp: Some(TS.to_owned()),
                provenance_role: None,
                force_stale_lock: false,
            }
        ),
        Err(PmRustError::InvalidMutationRequest { .. })
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
