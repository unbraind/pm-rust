//! Real-filesystem admission and recovery policy boundaries.
use super::*;

/// Builds a canonical historical document without deriving its expected policy from production.
fn document(id: &str, status: &str) -> ItemDocument {
    ItemDocument {
        metadata: crate::ItemMetadata {
            id: id.to_owned(),
            title: id.to_owned(),
            description: String::new(),
            item_type: "Task".to_owned(),
            status: status.to_owned(),
            priority: 2,
            tags: vec![],
            created_at: "2026-08-22T10:00:00.000Z".to_owned(),
            updated_at: "2026-08-22T10:00:00.000Z".to_owned(),
            parent: None,
            extra: serde_json::Map::new(),
        },
        body: String::new(),
    }
}
/// Persists actual canonical files in a plain tracker.
fn disk() -> Result<(tempfile::TempDir, PathBuf, ItemDocument), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let root = temp.path().join(".agents/pm");
    fs::create_dir_all(root.join("tasks"))?;
    fs::write(root.join("settings.json"), r#"{"id_prefix":"sample-"}"#)?;
    let before = document("sample-own", "open");
    for item in [&before, &document("sample-gate", "open")] {
        fs::write(
            root.join(format!("tasks/{}.toon", item.metadata.id)),
            toon_format::encode_default(item)?,
        )?;
    }
    Ok((temp, root, before))
}
/// Calls the actual shared admission boundary with a real configured tracker.
fn admission(
    root: &Path,
    before: &ItemDocument,
    force: bool,
    phase: Phase,
) -> Result<Admission, PmRustError> {
    admit_lifecycle_transition(
        root,
        &OwnershipItem {
            id: before.metadata.id.clone(),
            author: "alpha".to_owned(),
            timestamp: None,
            message: None,
            force,
            if_available: false,
            provenance_role: None,
        },
        before,
        phase,
    )
}
/// Reads independent refusal context rather than accepting any error as success.
fn context(result: Result<Admission, PmRustError>) -> Result<Value, Box<dyn std::error::Error>> {
    match result {
        Err(PmRustError::OwnershipRefusal { context, .. }) => Ok(context),
        _ => Err("expected blocker refusal".into()),
    }
}

/// Optional, external, aliased and repeated declarations retain deterministic recovery policy.
#[test]
fn round5_declared_relationship_recovery_corpus() -> Result<(), Box<dyn std::error::Error>> {
    let (_temp, root, mut before) = disk()?;
    for scalar in ["", "no-active-blocker", "No-Active-Blocker"] {
        before
            .metadata
            .extra
            .insert("blocked_by".to_owned(), json!(scalar));
        assert_eq!(
            admission(&root, &before, false, Phase::AtomicStart)?.status,
            "in_progress"
        );
    }
    before
        .metadata
        .extra
        .insert("blocked_by".to_owned(), json!("sample-gate"));
    before.metadata.extra.insert(
        "dependencies".to_owned(),
        json!([
            {"id":"SAMPLE-GATE","kind":"depends-on"},{"id":"#gate","kind":"blocked_by"},
            {"id":"remote","kind":"depends_on","source_kind":"global"},
            {"id":"gitlab:fixture/one","kind":"blocked_by"},
            {"id":"https://tracker.invalid/one","kind":"blocked_by"},
            {"id":"ignored","kind":"related"},{"kind":"blocked_by"}
        ]),
    );
    let refused = context(admission(&root, &before, false, Phase::AtomicStart))?;
    assert_eq!(refused["code"], "start_blocked_by_open_dependency");
    let rows = refused["details"]["open_blockers"]
        .as_array()
        .ok_or("blockers")?;
    assert_eq!(rows.len(), 4);
    assert_eq!(
        rows.iter().filter(|row| row["external"] == false).count(),
        1
    );
    assert_eq!(refused["next_steps"][0], "pm get sample-gate");
    assert_eq!(
        refused["next_steps"][1],
        "pm get sample-own --fields \"blocked_by,dependencies\""
    );
    // Ambiguous scalar/edge spellings must never emit an ineffective removal command.
    for deps in [
        json!([{"id":"#gate","kind":"blocked_by"}]),
        json!([{"id":"sample-gate","kind":"blocked_by"},{"id":"#gate","kind":"blocked_by"}]),
    ] {
        before
            .metadata
            .extra
            .insert("dependencies".to_owned(), deps);
        assert_eq!(
            unblock(&before, "sample-gate", &json!({"id_prefix":"sample-"})),
            "pm get sample-own --fields \"blocked_by,dependencies\""
        );
    }
    before.metadata.extra.remove("blocked_by");
    before.metadata.extra.insert(
        "dependencies".to_owned(),
        json!([{"id":"sample-gate","kind":"blocked_by"}]),
    );
    assert_eq!(
        unblock(&before, "sample-gate", &json!({"id_prefix":"sample-"})),
        "pm update sample-own --dep-remove \"id=sample-gate,kind=blocked_by\""
    );
    before.metadata.extra.insert(
        "dependencies".to_owned(),
        json!([{"id":"gate","kind":"blocked_by"}]),
    );
    assert_eq!(
        unblock(&before, "sample-gate", &json!({"id_prefix":"sample-"})),
        "pm get sample-own --fields \"blocked_by,dependencies\""
    );
    for id in [".", "..", "../gate", "unknown:gate", "missing"] {
        before.metadata.extra.remove("dependencies");
        before
            .metadata
            .extra
            .insert("blocked_by".to_owned(), json!(id));
        let refused = context(admission(&root, &before, false, Phase::AtomicStart))?;
        assert_eq!(
            refused["details"]["open_blockers"][0]["status"],
            Value::Null
        );
    }
    Ok(())
}

/// Corrupt or ambiguous blockers are bypassed only by warn/force, with the correct classification.
#[test]
fn round5_unverifiable_blocker_corpus() -> Result<(), Box<dyn std::error::Error>> {
    for (contents, code) in [
        (b"invalid".as_slice(), "item_document_invalid"),
        (b"\xff".as_slice(), "item_document_encoding_invalid"),
    ] {
        let (_temp, root, mut before) = disk()?;
        before
            .metadata
            .extra
            .insert("blocked_by".to_owned(), json!("sample-gate"));
        fs::write(root.join("tasks/sample-gate.toon"), contents)?;
        assert!(admission(&root, &before, false, Phase::AtomicStart).is_err());
        for (mode, force) in [("warn", false), ("strict", true)] {
            fs::write(
                root.join("settings.json"),
                json!({"id_prefix":"sample-","governance":{"start_blocker_enforcement":mode}})
                    .to_string(),
            )?;
            assert_eq!(
                admission(&root, &before, force, Phase::AtomicStart)?.warnings,
                [format!("start_blocker_unverifiable:sample-own:{code}")]
            );
            assert_eq!(fs::read(root.join("tasks/sample-gate.toon"))?, contents);
        }
    }
    let (_temp, root, mut before) = disk()?;
    before
        .metadata
        .extra
        .insert("blocked_by".to_owned(), json!("sample-gate"));
    fs::create_dir(root.join("tasks/nested"))?;
    fs::copy(
        root.join("tasks/sample-gate.toon"),
        root.join("tasks/nested/sample-gate.toon"),
    )?;
    assert!(matches!(
        admission(&root, &before, false, Phase::AtomicStart),
        Err(PmRustError::DuplicateItemId { .. })
    ));
    assert_eq!(
        admission(&root, &before, true, Phase::AtomicStart)?.warnings,
        ["start_blocker_unverifiable:sample-own:item_identity_ambiguous"]
    );
    fs::remove_file(root.join("tasks/nested/sample-gate.toon"))?;
    fs::write(
        root.join("tasks/sample-gate.toon"),
        toon_format::encode_default(&document("other-gate", "open"))?,
    )?;
    assert_eq!(
        admission(&root, &before, true, Phase::AtomicStart)?.warnings,
        ["start_blocker_unverifiable:sample-own:item_identity_conflict"]
    );
    assert!(
        unverifiable_code(&PmRustError::CommandRefusal {
            detail: String::new(),
            exit_code: 2,
            context: json!({"code":"other"})
        })
        .is_none()
    );
    assert_eq!(
        unverifiable_code(&PmRustError::CommandRefusal {
            detail: String::new(),
            exit_code: 4,
            context: json!({"code":"item_format_version_unsupported"})
        }),
        Some("item_format_version_unsupported")
    );
    assert!(
        unverifiable_code(&PmRustError::ItemNotFound {
            id: "absent".to_owned()
        })
        .is_none()
    );
    assert!(matches!(
        blocker_paths(&root.join("absent"), true, "gate", &mut vec![]),
        Err(PmRustError::Io { .. })
    ));
    Ok(())
}

/// Nested explicit-start scopes deduplicate only their own warning and restore on exit.
#[test]
fn round5_admission_phases_and_warning_scope() -> Result<(), Box<dyn std::error::Error>> {
    let (_temp, root, mut before) = disk()?;
    before
        .metadata
        .extra
        .insert("blocked_by".to_owned(), json!("sample-gate"));
    fs::write(
        root.join("settings.json"),
        r#"{"id_prefix":"sample-","governance":{"start_blocker_enforcement":"warn"}}"#,
    )?;
    assert!(!explicit_claim());
    with_explicit_start(|| -> Result<(), Box<dyn std::error::Error>> {
        assert!(explicit_claim());
        assert_eq!(
            admission(&root, &before, false, Phase::Claim)?
                .warnings
                .len(),
            1
        );
        with_explicit_start(|| -> Result<(), Box<dyn std::error::Error>> {
            assert_eq!(
                admission(&root, &before, false, Phase::Start)?
                    .warnings
                    .len(),
                1
            );
            Ok(())
        })?;
        assert!(
            admission(&root, &before, false, Phase::Start)?
                .warnings
                .is_empty()
        );
        Ok(())
    })?;
    assert!(!explicit_claim());
    before.metadata.status = "closed".to_owned();
    assert_eq!(
        context(admission(&root, &before, true, Phase::AtomicStart))?["code"],
        "terminal_state_conflict"
    );
    fs::write(
        root.join("settings.json"),
        r#"{"governance":{"workflow_enforcement":"warn","start_blocker_enforcement":"off"},"schema":{"type_workflows":[{"type":"Task","allowed_transitions":[]}]}}"#,
    )?;
    before.metadata.status = "open".to_owned();
    let allowed = admission(&root, &before, false, Phase::Start)?;
    assert_eq!(allowed.status, "in_progress");
    assert!(allowed.warnings[0].starts_with("workflow_transition_not_allowed:"));
    before.metadata.status = "in_progress".to_owned();
    assert!(
        admission(&root, &before, false, Phase::Start)?
            .warnings
            .is_empty()
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            root.join("tasks/sample-gate.toon"),
            root.join("tasks/alias.toon"),
        )?;
        let mut paths = vec![];
        blocker_paths(&root, true, "alias", &mut paths)?;
        assert!(paths.is_empty());
    }
    Ok(())
}

/// A disappearing blocker is an actual read failure, never an authorized warning bypass.
#[test]
fn round5_blocker_read_failure_is_not_unverifiable() -> Result<(), Box<dyn std::error::Error>> {
    let (_temp, root, _before) = disk()?;
    let error = read_blocker(&root.join("tasks/absent.toon"), "sample-absent")
        .err()
        .ok_or("missing read admitted")?;
    assert!(matches!(error, PmRustError::Io { .. }));
    assert!(unverifiable_code(&error).is_none());
    assert!(
        unverifiable_code(&PmRustError::CommandRefusal {
            detail: String::new(),
            exit_code: 2,
            context: json!({})
        })
        .is_none()
    );
    Ok(())
}

/// Non-document I/O failures remain fatal even when warning or force admission is selected.
#[test]
fn round5_unverifiable_policy_never_hides_io() -> Result<(), Box<dyn std::error::Error>> {
    let (_temp, root, mut before) = disk()?;
    before
        .metadata
        .extra
        .insert("blocked_by".to_owned(), json!("sample-gate"));
    let settings = json!({"id_prefix":"sample-","governance":{"start_blocker_enforcement":"warn"}});
    let registry = Lifecycle::load(&root)?;
    assert!(matches!(
        admit_blockers(&root.join("missing"), &settings, &registry, &before, true),
        Err(PmRustError::Io { .. })
    ));
    before.metadata.extra.remove("blocked_by");
    before.metadata.extra.insert(
        "dependencies".to_owned(),
        json!([{"id":"other","kind":"related"},{"id":"sample-gate","kind":"blocked_by"}]),
    );
    assert_eq!(
        unblock(&before, "sample-gate", &settings),
        "pm update sample-own --dep-remove \"id=sample-gate,kind=blocked_by\""
    );
    Ok(())
}

/// Test scheduler callback, confined to one admission thread.
type PathReadHook = Box<dyn FnMut(&Path)>;
thread_local! {
    /// Places real filesystem changes at a read boundary to model concurrent disappearance.
    static PATH_READ: RefCell<Option<PathReadHook>> = const { RefCell::new(None) };
}
/// Runs the test-scoped scheduler before the actual filesystem read.
pub(super) fn before_path_read(path: &Path) {
    PATH_READ.with(|hook| {
        if let Some(hook) = hook.borrow_mut().as_mut() {
            hook(path);
        }
    });
}
/// Clears a filesystem scheduler on every test exit, including a panic.
struct ClearPathRead;
impl Drop for ClearPathRead {
    fn drop(&mut self) {
        PATH_READ.with(|hook| {
            hook.borrow_mut().take();
        });
    }
}

/// A directory disappearing during either traversal remains an I/O refusal under force.
#[test]
fn round5_blocker_traversal_disappearance_refuses() -> Result<(), Box<dyn std::error::Error>> {
    for fallback in [false, true] {
        let (_temp, root, mut before) = disk()?;
        before.metadata.extra.insert(
            "blocked_by".to_owned(),
            json!(if fallback { "missing" } else { "sample-gate" }),
        );
        let watched = if fallback {
            root.clone()
        } else {
            root.join("tasks")
        };
        let moved = watched.with_extension("moved");
        let mut visits = 0;
        PATH_READ.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move |path| {
                if path == watched {
                    visits += 1;
                    if visits == if fallback { 2 } else { 1 } {
                        assert!(
                            fs::rename(&watched, &moved).is_ok(),
                            "scratch directory move"
                        );
                    }
                }
            }));
        });
        let _clear = ClearPathRead;
        assert!(matches!(
            admission(&root, &before, true, Phase::AtomicStart),
            Err(PmRustError::Io { .. })
        ));
    }
    Ok(())
}

/// External dependency removal is explicit; malformed historical edges receive inspection advice.
#[test]
fn round5_external_and_incomplete_unblock_advice() {
    let mut before = document("sample-own", "open");
    let settings = json!({"id_prefix":"sample-"});
    before.metadata.extra.insert(
        "dependencies".to_owned(),
        json!([{"id":"github:fixture/one","kind":"blocked_by"}]),
    );
    assert_eq!(
        unblock(&before, "github:fixture/one", &settings),
        "pm update sample-own --dep-remove \"id=github:fixture/one,kind=blocked_by\""
    );
    before
        .metadata
        .extra
        .insert("blocked_by".to_owned(), json!("different"));
    before
        .metadata
        .extra
        .insert("dependencies".to_owned(), json!([{"kind":"blocked_by"}]));
    assert_eq!(
        unblock(&before, "", &settings),
        "pm get sample-own --fields \"blocked_by,dependencies\""
    );
}

/// Closed blockers admit progress, force admits open blockers, and complete missing IDs stay unresolved.
#[test]
fn round5_closed_and_force_open_blockers_use_shared_policy()
-> Result<(), Box<dyn std::error::Error>> {
    let (_temp, root, mut before) = disk()?;
    before
        .metadata
        .extra
        .insert("blocked_by".to_owned(), json!("sample-gate"));
    assert_eq!(
        admission(&root, &before, true, Phase::AtomicStart)?.warnings,
        ["start_blocker_override:sample-own:sample-gate"]
    );
    fs::write(
        root.join("tasks/sample-gate.toon"),
        toon_format::encode_default(&document("sample-gate", "closed"))?,
    )?;
    assert_eq!(
        admission(&root, &before, false, Phase::AtomicStart)?.status,
        "in_progress"
    );
    assert!(blocker(&root, "sample-missing", &json!({"id_prefix":"sample-"}))?.is_none());
    Ok(())
}

/// Windows junctions are traversal boundaries, just like Unix symbolic links.
#[cfg(windows)]
#[test]
fn round7_windows_junction_is_not_a_blocker_source() -> Result<(), Box<dyn std::error::Error>> {
    let (_temp, root, _before) = disk()?;
    let external = tempfile::tempdir()?;
    fs::write(
        external.path().join("sample-gate.toon"),
        "unrelated external document",
    )?;
    let junction = root.join("tasks/ignored");
    let output = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&junction)
        .arg(external.path())
        .output()?;
    assert!(output.status.success(), "junction creation failed");
    assert!(junction.is_symlink());
    let mut paths = vec![];
    blocker_paths(&root, true, "sample-gate", &mut paths)?;
    assert_eq!(paths, vec![root.join("tasks/sample-gate.toon")]);
    fs::remove_dir(junction)?;
    Ok(())
}

/// Default-file aliases and workflow rules share the captured admission registry.
#[test]
fn round8_default_alias_and_workflow_rules() -> Result<(), Box<dyn std::error::Error>> {
    for external_rules in [false, true] {
        for allowed in [false, true] {
            let (_temp, root, before) = disk()?;
            fs::create_dir_all(root.join("schema"))?;
            let rules = json!([{"type":"Task","allowed_transitions":if allowed {json!([["open","in_progress"]])} else {json!([["draft","in_progress"]])}}]);
            let mut settings = json!({"governance":{"workflow_enforcement":"strict"},"schema":{"statuses":[{"id":"open","roles":["active"]}]}});
            if !external_rules {
                settings["schema"]["type_workflows"] = rules.clone();
            }
            fs::write(root.join("settings.json"), settings.to_string())?;
            fs::write(
                root.join("schema/statuses.json"),
                json!({"statuses":[{"id":"doing","roles":["active"],"aliases":["in_progress"]}]})
                    .to_string(),
            )?;
            if external_rules {
                fs::write(
                    root.join("schema/workflows.json"),
                    json!({"type_workflows":rules}).to_string(),
                )?;
            }
            for phase in [Phase::AtomicStart, Phase::Start] {
                let result = admission(&root, &before, false, phase);
                if allowed {
                    assert_eq!(result?.status, "doing");
                } else {
                    assert!(matches!(
                        result,
                        Err(PmRustError::CommandRefusal { exit_code: 2, .. })
                    ));
                }
            }
        }
    }
    Ok(())
}

/// FIFO entries remain unresolved and replacement handles cannot block admission.
#[cfg(unix)]
#[test]
fn round12_fifo_blockers_are_not_items() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write as _;
    let (_temp, root, mut before) = disk()?;
    let path = root.join("tasks/sample-gate.toon");
    fs::remove_file(&path)?;
    rustix::fs::mknodat(
        rustix::fs::CWD,
        &path,
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        0,
    )?;
    let mut found = Vec::new();
    blocker_paths(&root, true, "sample-gate", &mut found)?;
    assert!(found.is_empty());
    before
        .metadata
        .extra
        .insert("blocked_by".to_owned(), json!("sample-gate"));
    assert_eq!(
        context(admission(&root, &before, false, Phase::AtomicStart))?["code"],
        "start_blocked_by_open_dependency"
    );
    assert!(admission(&root, &before, true, Phase::AtomicStart).is_ok());
    // Keep a live producer so reverting to blocking fs::read cannot return an EOF.
    let fd = rustix::fs::open(
        &path,
        rustix::fs::OFlags::RDWR | rustix::fs::OFlags::NONBLOCK,
        rustix::fs::Mode::empty(),
    )?;
    let mut producer = fs::File::from(fd);
    producer
        .write_all(toon_format::encode_default(&document("sample-gate", "closed"))?.as_bytes())?;
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        let _ = tx.send(read_blocker(&path, "sample-gate").is_err());
    });
    let outcome = rx.recv_timeout(std::time::Duration::from_secs(3));
    drop(producer);
    reader.join().map_err(|_| "reader panicked")?;
    assert_eq!(
        outcome.ok(),
        Some(true),
        "opened FIFO must refuse before waiting for EOF"
    );
    Ok(())
}
