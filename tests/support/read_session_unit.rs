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
