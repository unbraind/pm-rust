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
        force_stale_lock: false,
    }
}

fn settings(prefix: &str, format: &str, ttl: u64) -> String {
    format!(
        r#"{{"id_prefix":"{prefix}","item_format":"{format}","locks":{{"ttl_seconds":{ttl}}}}}"#
    )
}

type CompletedTransaction = (TempDir, PathBuf, String, String, CreateJournal);

#[test]
fn serde_defaults_match_the_supported_create_and_settings_contract()
-> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(type_folder("Plan"), Some("plans"));
    let mut reserved = request();
    reserved.title = "true".to_owned();
    let (_reserved_directory, reserved_root) = root(&settings("sample-", "toon", 1_800))?;
    let document = create_item(&reserved_root, reserved)?.item;
    assert!(canonical_item_bytes(&document).contains("title: \"true\"\n"));
    let mut list_like_scalar = document.clone();
    list_like_scalar.metadata.description = "-A".to_owned();
    assert_eq!(
        decode_item(
            Path::new("list-like-scalar.toon"),
            &canonical_item_bytes(&list_like_scalar)
        )?,
        list_like_scalar
    );
    let mut ambiguous_tags = document.clone();
    ambiguous_tags.metadata.tags = ["0", "1.2", "false", "null", "true"]
        .map(str::to_owned)
        .to_vec();
    assert_eq!(
        decode_item(
            Path::new("ambiguous-tags.toon"),
            &canonical_item_bytes(&ambiguous_tags)
        )?,
        ambiguous_tags
    );
    let mut ambiguous_body = document.clone();
    ambiguous_body.body = "0".to_owned();
    assert_eq!(
        decode_item(
            Path::new("ambiguous-body.toon"),
            &canonical_item_bytes(&ambiguous_body)
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
    assert_eq!(metadata_value(&item)["parent"], "sample-parent");
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
    let first = acquire_lock(&pm_root, "sample-unit", "first", 0, false, TS)?;
    assert!(matches!(
        acquire_lock(&pm_root, "sample-unit", "second", 0, false, TS),
        Err(PmRustError::LockConflict { .. })
    ));
    let lock_path = pm_root.join("locks/sample-unit.lock");
    let lock_file = File::options().write(true).open(&lock_path)?;
    lock_file.set_times(
        fs::FileTimes::new().set_modified(SystemTime::now() + Duration::from_secs(60)),
    )?;
    assert!(matches!(
        acquire_lock(&pm_root, "sample-unit", "second", 0, true, TS),
        Err(PmRustError::LockConflict { .. })
    ));
    lock_file.set_times(fs::FileTimes::new().set_modified(SystemTime::now()))?;
    thread::sleep(Duration::from_millis(2));
    let second = acquire_lock(&pm_root, "sample-unit", "second", 0, true, TS)?;
    drop(first);
    assert!(pm_root.join("locks/sample-unit.lock").exists());
    drop(second);
    assert!(!pm_root.join("locks/sample-unit.lock").exists());

    fs::write(pm_root.join("locks/sample-gated.lock"), "foreign")?;
    fs::create_dir(pm_root.join("locks/sample-gated.lock.stale-cleanup"))?;
    thread::sleep(Duration::from_millis(2));
    assert!(matches!(
        acquire_lock(&pm_root, "sample-gated", "third", 0, true, TS),
        Err(PmRustError::LockConflict { .. })
    ));
    let directory_lock = pm_root.join("locks/sample-directory.lock");
    fs::create_dir(&directory_lock)?;
    assert!(matches!(
        acquire_lock(&pm_root, "sample-directory", "third", 0, true, TS),
        Err(PmRustError::Io { .. })
    ));
    let oversized_id = "x".repeat(300);
    assert!(matches!(
        acquire_lock(&pm_root, &oversized_id, "third", 0, true, TS),
        Err(PmRustError::Io { .. })
    ));
    Ok(())
}

fn completed_transaction() -> Result<CompletedTransaction, Box<dyn std::error::Error>> {
    let (directory, pm_root) = root(&settings("sample-", "toon", 1_800))?;
    create_item(&pm_root, request())?;
    let item = fs::read_to_string(pm_root.join("tasks/sample-unit.toon"))?;
    let history = fs::read_to_string(pm_root.join("history/sample-unit.jsonl"))?;
    let journal = CreateJournal {
        version: 1,
        id: "sample-unit".to_owned(),
        item_type: "Task".to_owned(),
        item_bytes: item.clone(),
        history_bytes: history.clone(),
    };
    Ok((directory, pm_root, item, history, journal))
}

fn write_journal(
    pm_root: &Path,
    journal: &CreateJournal,
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
        acquire_lock(&pm_root, "sample-lock", "agent", 1_800, false, TS),
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
        let decoded = decode_item(Path::new("property.toon"), &canonical_item_bytes(&document))?;
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
    let held = acquire_lock(&lock_root, "sample-unit", "holder", 1_800, false, TS)?;
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
