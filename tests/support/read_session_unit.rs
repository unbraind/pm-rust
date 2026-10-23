//! Session admission, immutable-source, terminal cleanup and refusal contracts.
use super::*;
use std::collections::BTreeSet;

/// Shared fallible test return.
type TestResult = Result<(), Box<dyn std::error::Error>>;
/// Fixed read clock.
const CLOCK: &str = "2026-10-21T12:00:00.000Z";

/// Creates a small real non-git canonical tracker, not a reader mock.
fn fixture(count: usize) -> Result<(tempfile::TempDir, Workspace), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(root.join("tasks/nested"))?;
    fs::create_dir_all(root.join("history"))?;
    fs::write(
        root.join("settings.json"),
        include_str!("../fixtures/list-settings.json"),
    )?;
    fs::write(root.join("history/ignored.toon"), "invalid ignored history")?;
    fs::write(root.join("tasks/ignored.txt"), "ignored")?;
    fs::write(root.join("tasks/extensionless"), "ignored")?;
    for index in 0..count {
        fs::write(
            root.join(format!("tasks/nested/demo-{index}.toon")),
            format!(
                "id: demo-{index}\ntitle: Example {index}\ntype: Task\nstatus: open\npriority: 1\ncreated_at: \"{CLOCK}\"\nupdated_at: \"{CLOCK}\"\nbody: retained\n"
            ),
        )?;
    }
    let workspace = Workspace::discover(directory.path())?;
    Ok((directory, workspace))
}

#[test]
fn session_pages_reuse_canonical_output_and_freeze_source() -> TestResult {
    let (_directory, workspace) = fixture(7)?;
    let mut limits = ReadSessionLimits {
        page_items: 2,
        ..ReadSessionLimits::default()
    };
    for toon in [false, true] {
        limits.toon = toon;
        let mut session = workspace.read_session(&limits, SessionCancellation::default(), CLOCK)?;
        let snapshot = session.workspace.pm_root().to_path_buf();
        assert_eq!(session.receipt().source_items, 7);
        assert!(session.receipt().admitted_memory_bytes < limits.memory_bytes);
        let mut options = session.options.clone();
        let mut ids = BTreeSet::new();
        while let Some(page) = session.next_page()? {
            let expected = workspace.list_page(&ItemFilter::default(), &options, CLOCK)?;
            assert_eq!(
                page.output.stringify_read_json(),
                expected.stringify_read_json()
            );
            assert_eq!(
                crate::stringify_toon(&page.output)?,
                crate::stringify_toon(&expected)?
            );
            options.after = page.output["next_cursor"].as_str().map(str::to_owned);
            for row in page.output["items"].as_array().ok_or("missing items")? {
                assert!(ids.insert(row["id"].as_str().ok_or("missing id")?.to_owned()));
            }
            assert_eq!(page.receipt.complete, ids.len() == 7);
        }
        assert!(session.receipt().complete);
        assert_eq!(session.receipt().pages, 4);
        assert_eq!(ids.len(), 7);
        assert!(!snapshot.exists());
        assert!(session.next_page()?.is_none());
    }
    let mut session = workspace.read_session(&limits, SessionCancellation::default(), CLOCK)?;
    let snapshot = session.workspace.pm_root().to_path_buf();
    fs::remove_dir_all(workspace.pm_root())?;
    assert!(session.next_page()?.is_some());
    session.cancel()?;
    assert!(!snapshot.exists());
    assert!(!session.receipt().complete);
    assert!(matches!(
        session.next_page(),
        Err(SessionRefusal::Cancelled)
    ));
    session.cancel()?;
    Ok(())
}

#[test]
fn admission_refuses_before_querying_and_cancellation_releases_resources() -> TestResult {
    let (_directory, workspace) = fixture(1)?;
    let token = SessionCancellation::default();
    token.cancel();
    assert!(matches!(
        workspace.read_session(&ReadSessionLimits::default(), token, CLOCK),
        Err(SessionRefusal::Cancelled)
    ));
    for (memory_bytes, page_items, output_tokens) in [
        (0, 1, 1),
        (1, 0, 1),
        (1, 1, 0),
        (1, 1_000_001, 1),
        (1, 1, 1_000_001),
    ] {
        assert!(matches!(
            workspace.read_session(
                &ReadSessionLimits {
                    memory_bytes,
                    page_items,
                    output_tokens,
                    toon: false
                },
                SessionCancellation::default(),
                CLOCK
            ),
            Err(SessionRefusal::InvalidLimits)
        ));
    }
    for ceiling in [1, 9 * 1024 * 1024] {
        let limits = ReadSessionLimits {
            memory_bytes: ceiling,
            ..ReadSessionLimits::default()
        };
        assert!(matches!(
            workspace.read_session(&limits, SessionCancellation::default(), CLOCK),
            Err(SessionRefusal::MemoryLimit)
        ));
    }
    let token = SessionCancellation::default();
    let mut session =
        workspace.read_session(&ReadSessionLimits::default(), token.clone(), CLOCK)?;
    let snapshot = session.workspace.pm_root().to_path_buf();
    token.cancel();
    assert!(matches!(
        session.next_page(),
        Err(SessionRefusal::Cancelled)
    ));
    assert!(!snapshot.exists());
    let session = workspace.read_session(
        &ReadSessionLimits::default(),
        SessionCancellation::default(),
        CLOCK,
    )?;
    let snapshot = session.workspace.pm_root().to_path_buf();
    drop(session);
    assert!(!snapshot.exists());
    let errors = [
        SessionRefusal::Cancelled,
        SessionRefusal::MemoryLimit,
        SessionRefusal::InvalidLimits,
        SessionRefusal::UnsupportedSource,
        SessionRefusal::Source(PmRustError::OutputBudgetExceeded),
        SessionRefusal::Storage(std::io::Error::other("synthetic storage refusal")),
    ];
    for error in errors {
        assert!(error.code().starts_with("session_"));
        assert!(error.to_string().contains(';'));
    }
    Ok(())
}

#[test]
fn snapshot_validates_all_items_and_schema_before_delivery() -> TestResult {
    let (_directory, workspace) = fixture(1)?;
    let open = || {
        workspace.read_session(
            &ReadSessionLimits::default(),
            SessionCancellation::default(),
            CLOCK,
        )
    };
    let item = workspace.pm_root().join("tasks/nested/demo-0.toon");
    fs::copy(&item, workspace.pm_root().join("tasks/duplicate.toon"))?;
    let Err(SessionRefusal::Source(PmRustError::DuplicateItemId { first, second, .. })) = open()
    else {
        return Err("duplicate source must refuse".into());
    };
    assert_ne!(
        first, second,
        "refusal must identify both conflicting files"
    );
    fs::write(workspace.pm_root().join("tasks/duplicate.toon"), "invalid")?;
    assert!(matches!(
        open(),
        Err(SessionRefusal::Source(
            PmRustError::InvalidItemDocument { .. }
        ))
    ));
    fs::write(workspace.pm_root().join("tasks/duplicate.toon"), [255])?;
    assert!(matches!(open(), Err(SessionRefusal::UnsupportedSource)));
    fs::remove_file(workspace.pm_root().join("tasks/duplicate.toon"))?;
    for settings in [
        "invalid",
        r#"{"schema":{"files":{"statuses":"../external.json"}}}"#,
    ] {
        fs::write(workspace.pm_root().join("settings.json"), settings)?;
        assert!(matches!(open(), Err(SessionRefusal::UnsupportedSource)));
    }
    fs::write(
        workspace.pm_root().join("settings.json"),
        r#"{"schema":{"files":{"statuses":"schema/custom.json"}}}"#,
    )?;
    fs::create_dir_all(workspace.pm_root().join("schema"))?;
    fs::write(workspace.pm_root().join("schema/custom.json"), "{}")?;
    fs::write(workspace.pm_root().join("schema/workflows.json"), "{}")?;
    let mut session = open()?;
    assert!(session.next_page()?.is_some());
    let (_empty, workspace) = fixture(0)?;
    let mut session = workspace.read_session(
        &ReadSessionLimits::default(),
        SessionCancellation::default(),
        CLOCK,
    )?;
    assert!(
        session
            .next_page()?
            .ok_or("empty final page missing")?
            .receipt
            .complete
    );
    Ok(())
}

#[test]
fn small_output_budget_refuses_and_terminal_query_failure_cleans_up() -> TestResult {
    let (_directory, workspace) = fixture(2)?;
    let limits = ReadSessionLimits {
        output_tokens: 1,
        ..ReadSessionLimits::default()
    };
    let mut session = workspace.read_session(&limits, SessionCancellation::default(), CLOCK)?;
    let snapshot = session.workspace.pm_root().to_path_buf();
    assert!(matches!(
        session.next_page(),
        Err(SessionRefusal::Source(_))
    ));
    assert!(!snapshot.exists());
    assert!(session.next_page()?.is_none());
    Ok(())
}

thread_local! {
    /// Real storage faults immediately after private directory allocation.
    pub(super) static SNAPSHOT_CREATED_HOOK: std::cell::Cell<fn(&Path)> = const { std::cell::Cell::new(path_noop) };
    /// Real copied-file faults before subsequent snapshot reads.
    pub(super) static AFTER_COPY_HOOK: std::cell::Cell<fn(&Path, &Path)> = const { std::cell::Cell::new(paths_noop) };
    /// Snapshot selected for a post-query cleanup fault.
    static QUERY_FAULT_PATH: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
    /// Owns every displaced snapshot and restores its replacement after the test.
    static QUERY_FAULT_GUARD: std::cell::RefCell<Option<CleanupFault>> = const { std::cell::RefCell::new(None) };
    /// Deterministic filesystem drift/cancellation after actual metadata admission.
    pub(super) static BEFORE_COPY_HOOK: std::cell::Cell<fn(&Path, &SessionCancellation)> = const { std::cell::Cell::new(copy_noop) };
    /// Deterministic cancellation after the actual canonical query.
    pub(super) static AFTER_QUERY_HOOK: std::cell::Cell<fn(&SessionCancellation)> = const { std::cell::Cell::new(query_noop) };
}

/// Leaves a real source unchanged by default.
fn copy_noop(_: &Path, _: &SessionCancellation) {}
/// Leaves ordinary query delivery unchanged.
fn query_noop(_: &SessionCancellation) {}
/// Changes the actual source length after admission and before bounded reading.
fn grow_source(path: &Path, _: &SessionCancellation) {
    let _ = fs::write(path, "changed-size source");
}
/// Cancels after admission of one real source file.
fn cancel_copy(_: &Path, token: &SessionCancellation) {
    token.cancel();
}
/// Constructs a private ledger for deterministic boundary tests.
fn admission(token: &SessionCancellation) -> Admission<'_> {
    Admission {
        ceiling: u64::MAX,
        used: 0,
        bytes: 0,
        items: 0,
        hash: Sha256::new(),
        ids: BTreeMap::new(),
        cancellation: token,
    }
}

#[test]
fn source_drift_and_mid_operation_cancellation_are_fail_closed() -> TestResult {
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("source");
    let target = directory.path().join("target");
    fs::write(&source, "original")?;
    let token = SessionCancellation::default();
    BEFORE_COPY_HOOK.with(|hook| hook.set(grow_source));
    let result = admission(&token).copy(&source, &target, Path::new("source"), false);
    BEFORE_COPY_HOOK.with(|hook| hook.set(copy_noop));
    assert!(matches!(result, Err(SessionRefusal::UnsupportedSource)));
    assert!(!target.exists());
    BEFORE_COPY_HOOK.with(|hook| hook.set(cancel_copy));
    let result = admission(&token).copy(&source, &target, Path::new("source"), false);
    BEFORE_COPY_HOOK.with(|hook| hook.set(copy_noop));
    assert!(matches!(result, Err(SessionRefusal::Cancelled)));
    let (_directory, workspace) = fixture(1)?;
    let mut session = workspace.read_session(
        &ReadSessionLimits::default(),
        SessionCancellation::default(),
        CLOCK,
    )?;
    let snapshot = session.workspace.pm_root().to_path_buf();
    AFTER_QUERY_HOOK.with(|hook| hook.set(SessionCancellation::cancel));
    let result = session.next_page();
    AFTER_QUERY_HOOK.with(|hook| hook.set(query_noop));
    assert!(matches!(result, Err(SessionRefusal::Cancelled)));
    assert!(!snapshot.exists());
    assert!(!session.receipt().complete);
    Ok(())
}

#[test]
fn unsupported_files_and_depth_refuse_with_recoverable_storage_errors() -> TestResult {
    let directory = tempfile::tempdir()?;
    let token = SessionCancellation::default();
    let mut ledger = admission(&token);
    assert!(matches!(
        ledger.copy(
            directory.path(),
            &directory.path().join("target"),
            Path::new("dir"),
            false
        ),
        Err(SessionRefusal::UnsupportedSource)
    ));
    assert!(matches!(
        ledger.walk(directory.path(), directory.path(), Path::new(""), 33),
        Err(SessionRefusal::UnsupportedSource)
    ));
    assert!(matches!(
        ledger.copy(
            &directory.path().join("absent"),
            &directory.path().join("target"),
            Path::new("absent"),
            false
        ),
        Err(SessionRefusal::Storage(_))
    ));
    #[cfg(unix)]
    {
        let (_directory, workspace) = fixture(1)?;
        std::os::unix::fs::symlink("missing", workspace.pm_root().join("tasks/link"))?;
        assert!(matches!(
            workspace.read_session(&ReadSessionLimits::default(), token, CLOCK),
            Err(SessionRefusal::UnsupportedSource)
        ));
    }
    #[cfg(windows)]
    {
        let (_directory, workspace) = fixture(1)?;
        // Directory junctions need no symlink privilege on Windows runners.
        let target = directory.path().join("junction");
        fs::create_dir(&target)?;
        let output = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(workspace.pm_root().join("tasks/link"))
            .arg(target)
            .output()?;
        assert!(output.status.success());
        assert!(matches!(
            workspace.read_session(&ReadSessionLimits::default(), token, CLOCK),
            Err(SessionRefusal::UnsupportedSource)
        ));
    }
    Ok(())
}

#[test]
fn schema_directory_links_cannot_escape_snapshot_admission() -> TestResult {
    let (directory, workspace) = fixture(1)?;
    let outside = directory.path().join("outside-schema");
    fs::create_dir(&outside)?;
    fs::write(outside.join("statuses.json"), "{}")?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, workspace.pm_root().join("schema"))?;
    #[cfg(windows)]
    {
        let output = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(workspace.pm_root().join("schema"))
            .arg(&outside)
            .output()?;
        assert!(output.status.success());
    }
    assert!(matches!(
        workspace.read_session(
            &ReadSessionLimits::default(),
            SessionCancellation::default(),
            CLOCK
        ),
        Err(SessionRefusal::UnsupportedSource)
    ));
    Ok(())
}

/// Caller clock storage participates in admission before its owned copy is retained.
#[test]
fn clock_storage_is_admitted_before_session_construction() -> TestResult {
    let (_directory, workspace) = fixture(1)?;
    let limits = ReadSessionLimits {
        memory_bytes: 16 * 1024 * 1024,
        ..ReadSessionLimits::default()
    };
    assert!(matches!(
        workspace.read_session(
            &limits,
            SessionCancellation::default(),
            &"x".repeat(128 * 1024)
        ),
        Err(SessionRefusal::MemoryLimit)
    ));
    Ok(())
}

/// A real storage loss during a canonical query terminates and cleans the session.
#[test]
fn query_storage_failure_releases_snapshot_and_never_certifies_completion() -> TestResult {
    let (_directory, workspace) = fixture(1)?;
    let mut session = workspace.read_session(
        &ReadSessionLimits::default(),
        SessionCancellation::default(),
        CLOCK,
    )?;
    let snapshot = session.workspace.pm_root().to_path_buf();
    fs::remove_file(snapshot.join("settings.json"))?;
    assert!(matches!(
        session.next_page(),
        Err(SessionRefusal::Source(_))
    ));
    assert!(!session.receipt().complete);
    assert!(!snapshot.exists());
    assert!(session.next_page()?.is_none());
    Ok(())
}

/// Leaves an allocated snapshot unchanged during ordinary tests.
fn path_noop(_: &Path) {}
/// Leaves copied bytes unchanged during ordinary tests.
fn paths_noop(_: &Path, _: &Path) {}
/// Blocks creation of the private tracker with an actual regular file.
fn block_snapshot_parent(root: &Path) {
    let _ = fs::write(root.join(".agents"), "blocked");
}
/// Removes a completed settings copy before the constructor reads it.
fn remove_settings_copy(source: &Path, target: &Path) {
    if source
        .file_name()
        .is_some_and(|name| name == "settings.json")
    {
        let _ = fs::remove_file(target);
    }
}
/// Removes an admitted source before opening it.
fn remove_source(source: &Path, _: &SessionCancellation) {
    let _ = fs::remove_file(source);
}

/// A real replacement of private snapshot storage, with scoped cleanup ownership.
struct CleanupFault {
    /// Exact generated path replaced by a regular file.
    original: PathBuf,
    /// Owns the displaced snapshot and its contents.
    _backup: tempfile::TempDir,
}

impl Drop for CleanupFault {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.original);
    }
}

/// Replaces a private snapshot with a file so remove-directory cleanup must fail.
fn cleanup_fault(original: &Path) -> Result<CleanupFault, Box<dyn std::error::Error>> {
    let backup = tempfile::tempdir()?;
    fs::rename(original, backup.path().join("snapshot"))?;
    fs::write(original, "replacement")?;
    Ok(CleanupFault {
        original: original.to_path_buf(),
        _backup: backup,
    })
}

/// Injects a real post-query storage replacement without changing query results.
fn fault_after_query(_: &SessionCancellation) {
    let path = QUERY_FAULT_PATH.with(|path| path.borrow().clone());
    if let Some(path) = path
        && let Ok(guard) = cleanup_fault(&path)
    {
        QUERY_FAULT_GUARD.with(|slot| *slot.borrow_mut() = Some(guard));
    }
}

/// Adds peer cancellation after a real post-query storage replacement.
fn cancel_and_fault_after_query(token: &SessionCancellation) {
    fault_after_query(token);
    token.cancel();
}

/// Every failed cleanup leaves the session terminal without certifying delivery.
#[test]
fn cleanup_failure_never_certifies_or_counts_an_undelivered_page() -> TestResult {
    for mode in [
        "cancel-before",
        "query-error",
        "cancel-after",
        "omitted",
        "complete",
    ] {
        let (_directory, workspace) = fixture(1)?;
        let token = SessionCancellation::default();
        let limits = ReadSessionLimits {
            output_tokens: if mode == "omitted" { 1 } else { 4000 },
            ..ReadSessionLimits::default()
        };
        let mut session = workspace.read_session(&limits, token.clone(), CLOCK)?;
        let root = session
            .snapshot
            .as_ref()
            .ok_or("snapshot missing")?
            .path()
            .to_path_buf();
        let mut guard = None;
        if matches!(mode, "cancel-before" | "query-error") {
            guard = Some(cleanup_fault(&root)?);
            if mode == "cancel-before" {
                token.cancel();
            }
        } else {
            QUERY_FAULT_PATH.with(|path| *path.borrow_mut() = Some(root));
            AFTER_QUERY_HOOK.with(|hook| {
                hook.set(if mode == "cancel-after" {
                    cancel_and_fault_after_query
                } else {
                    fault_after_query
                });
            });
        }
        let result = session.next_page();
        AFTER_QUERY_HOOK.with(|hook| hook.set(query_noop));
        QUERY_FAULT_PATH.with(|path| path.borrow_mut().take());
        QUERY_FAULT_GUARD.with(|slot| drop(slot.borrow_mut().take()));
        drop(guard);
        assert!(
            matches!(result, Err(SessionRefusal::Storage(_))),
            "cleanup mode {mode}: {result:?}"
        );
        assert!(
            !session.receipt().complete,
            "undelivered completion in {mode}"
        );
        assert_eq!(session.receipt().pages, 0, "undelivered page in {mode}");
        assert!(session.snapshot.is_none());
    }
    Ok(())
}

/// Constructor and publication failures remove partially admitted private copies.
#[test]
fn actual_snapshot_and_copy_storage_faults_are_typed() -> TestResult {
    let (_directory, workspace) = fixture(1)?;
    let open = || {
        workspace.read_session(
            &ReadSessionLimits::default(),
            SessionCancellation::default(),
            CLOCK,
        )
    };
    SNAPSHOT_CREATED_HOOK.with(|hook| hook.set(block_snapshot_parent));
    let result = open();
    SNAPSHOT_CREATED_HOOK.with(|hook| hook.set(path_noop));
    assert!(matches!(result, Err(SessionRefusal::Storage(_))));
    AFTER_COPY_HOOK.with(|hook| hook.set(remove_settings_copy));
    let result = open();
    AFTER_COPY_HOOK.with(|hook| hook.set(paths_noop));
    assert!(matches!(result, Err(SessionRefusal::Storage(_))));
    fs::create_dir_all(workspace.pm_root().join("schema/status-directory"))?;
    fs::write(
        workspace.pm_root().join("settings.json"),
        r#"{"schema":{"files":{"statuses":"schema/status-directory"}}}"#,
    )?;
    assert!(matches!(open(), Err(SessionRefusal::UnsupportedSource)));
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("source");
    fs::write(&source, "source bytes")?;
    let token = SessionCancellation::default();
    BEFORE_COPY_HOOK.with(|hook| hook.set(remove_source));
    let result = admission(&token).copy(
        &source,
        &directory.path().join("target"),
        Path::new("source"),
        false,
    );
    BEFORE_COPY_HOOK.with(|hook| hook.set(copy_noop));
    assert!(matches!(result, Err(SessionRefusal::Storage(_))));
    fs::write(&source, "source bytes")?;
    assert!(matches!(
        admission(&token).copy(&source, Path::new(""), Path::new("source"), false),
        Err(SessionRefusal::UnsupportedSource)
    ));
    let blocker = directory.path().join("blocker");
    fs::write(&blocker, "blocked")?;
    assert!(matches!(
        admission(&token).copy(&source, &blocker.join("child"), Path::new("source"), false),
        Err(SessionRefusal::Storage(_))
    ));
    assert!(matches!(
        admission(&token).copy(&source, directory.path(), Path::new("source"), false),
        Err(SessionRefusal::Storage(_))
    ));
    token.cancel();
    assert!(matches!(
        admission(&token).copy(&source, directory.path(), Path::new("source"), false),
        Err(SessionRefusal::Cancelled)
    ));
    assert!(matches!(
        admission(&token).walk(directory.path(), directory.path(), Path::new(""), 0),
        Err(SessionRefusal::Cancelled)
    ));
    Ok(())
}

/// Real directory records preserve enumeration, cancellation, type and charge faults.
#[test]
fn directory_fault_boundaries_preserve_authoritative_failures() -> TestResult {
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("entry");
    fs::write(&source, "bytes")?;
    let token = SessionCancellation::default();
    assert!(matches!(
        admission(&token).walk(&source, directory.path(), Path::new(""), 0),
        Err(SessionRefusal::Storage(_))
    ));
    let mut ledger = admission(&token);
    ledger.ceiling = 0;
    assert!(matches!(
        ledger.walk(directory.path(), directory.path(), Path::new(""), 0),
        Err(SessionRefusal::MemoryLimit)
    ));
    let mut iterator = fs::read_dir(directory.path())?
        .chain(std::iter::once(Err(io::Error::other("enumeration fault"))));
    assert!(matches!(
        admission(&token).collect_entries(&mut iterator),
        Err(SessionRefusal::Storage(_))
    ));
    let entry = fs::read_dir(directory.path())?
        .next()
        .transpose()?
        .ok_or("entry missing")?;
    assert!(matches!(
        admission(&token).visit_entry(
            &entry,
            Err(io::Error::other("type fault")),
            directory.path(),
            Path::new(""),
            0
        ),
        Err(SessionRefusal::Storage(_))
    ));
    let mut iterator = fs::read_dir(directory.path())?.inspect(|_| token.cancel());
    assert!(matches!(
        admission(&token).collect_entries(&mut iterator),
        Err(SessionRefusal::Cancelled)
    ));
    Ok(())
}

/// A failed bounded I/O stream never masquerades as successfully read partial bytes.
struct ReadFailure;
impl Read for ReadFailure {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("read fault"))
    }
}

#[test]
fn bounded_source_reader_preserves_errors_and_overflow_detection_byte() -> TestResult {
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("source");
    fs::write(&source, "abc")?;
    assert_eq!(read_source_file(&source, 1)?, b"ab");
    assert!(read_source_file(&directory.path().join("missing"), 1).is_err());
    assert!(read_source_bytes(&mut ReadFailure).is_err());
    Ok(())
}

/// Output compaction must use the existing snapshot cursor until all rows are served.
#[test]
fn resource_session_continues_output_budget_pages_without_missing_rows() -> TestResult {
    let (_directory, workspace) = fixture(300)?;
    for toon in [false, true] {
        let mut session = workspace.read_session(
            &ReadSessionLimits {
                page_items: 300,
                output_tokens: 1000,
                toon,
                ..ReadSessionLimits::default()
            },
            SessionCancellation::default(),
            CLOCK,
        )?;
        let mut ids = BTreeSet::new();
        let mut cursors = 0;
        while let Some(page) = session.next_page()? {
            assert!(session.receipt().pages <= 300);
            if page.output["continuation_kind"] == "output_cursor" {
                cursors += 1;
            }
            for row in page.output["items"].as_array().ok_or("missing rows")? {
                assert!(ids.insert(row["id"].as_str().ok_or("missing id")?.to_owned()));
            }
        }
        assert!(
            cursors > 0,
            "fixture must exercise snapshot output continuations"
        );
        assert_eq!(ids.len(), 300);
        assert!(session.receipt().complete);
    }
    Ok(())
}

/// A fresh native SDK process preserves real temporary-directory creation failures.
#[test]
fn snapshot_temp_creation_failure_is_typed() -> TestResult {
    const SOURCE: &str = "PM_RUST_SESSION_TEMP_FAILURE_SOURCE";
    if let Some(source) = std::env::var_os(SOURCE) {
        let workspace = Workspace::discover(&PathBuf::from(source))?;
        assert!(matches!(
            workspace.read_session(
                &ReadSessionLimits::default(),
                SessionCancellation::default(),
                CLOCK,
            ),
            Err(SessionRefusal::Storage(_))
        ));
        return Ok(());
    }
    let (directory, _workspace) = fixture(1)?;
    let scratch = tempfile::tempdir()?;
    let blocker = scratch.path().join("blocked-temp");
    fs::write(&blocker, "not a directory")?;
    let result = std::process::Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "read_session::tests::snapshot_temp_creation_failure_is_typed",
            "--nocapture",
        ])
        .env(SOURCE, directory.path())
        .env("TMPDIR", &blocker)
        .env("TMP", &blocker)
        .env("TEMP", &blocker)
        .output()?;
    assert!(
        result.status.success(),
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8(result.stdout)?.contains("1 passed"));
    Ok(())
}

/// Registry lookup faults preserve canonical errors instead of certifying defaults.
#[test]
fn schema_parent_file_preserves_canonical_lookup_error() -> TestResult {
    for (name, configured) in [("statuses", false), ("statuses", true), ("workflows", true)] {
        let (_directory, workspace) = fixture(1)?;
        let mut settings: serde_json::Value =
            serde_json::from_slice(&fs::read(workspace.pm_root().join("settings.json"))?)?;
        if !configured {
            settings["schema"]["files"] = serde_json::json!({});
        }
        if name == "workflows" {
            settings["schema"]["files"]["statuses"] = serde_json::json!("statuses.json");
            fs::write(workspace.pm_root().join("statuses.json"), "{}")?;
        }
        fs::write(
            workspace.pm_root().join("settings.json"),
            serde_json::to_vec(&settings)?,
        )?;
        let blocker = workspace.pm_root().join("schema");
        fs::write(&blocker, "schema is a regular file")?;
        let Err(PmRustError::Io {
            path: expected_path,
            source: expected_error,
        }) = workspace.list_page(&ItemFilter::default(), &ListOptions::default(), CLOCK)
        else {
            return Err("canonical list must refuse the blocked schema parent".into());
        };
        assert_eq!(expected_error.kind(), io::ErrorKind::NotADirectory);
        assert_eq!(
            expected_path,
            workspace.read_pm_root().join(format!("schema/{name}.json"))
        );
        let result = workspace.read_session(
            &ReadSessionLimits::default(),
            SessionCancellation::default(),
            CLOCK,
        );
        let Err(SessionRefusal::Source(PmRustError::Io { path, source })) = result else {
            return Err("schema lookup must refuse before session certification".into());
        };
        assert_eq!(path, expected_path);
        assert_eq!(source.kind(), expected_error.kind());
        assert_eq!(source.raw_os_error(), expected_error.raw_os_error());
        assert_eq!(fs::read_to_string(blocker)?, "schema is a regular file");
    }
    Ok(())
}
