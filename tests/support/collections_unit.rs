//! Collection parsing, preservation and storage-failure regressions.
use super::*;
type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Exercises independent pair spellings, quoting and path anchoring.
#[test]
#[allow(clippy::too_many_lines)]
fn pair_forms_preserve_literal_values() -> TestResult {
    let root = tempfile::tempdir()?;
    let cwd = root.path();
    for (raw, key, expected) in [
        (
            "src/a.rs",
            "files",
            json!({"path":"src/a.rs","scope":"project"}),
        ),
        (
            "src/a=b.rs",
            "files",
            json!({"path":"src/a=b.rs","scope":"project"}),
        ),
        (
            "source:one",
            "files",
            json!({"path":"source:one","scope":"project"}),
        ),
        (
            "9:artifact.rs",
            "files",
            json!({"path":"9:artifact.rs","scope":"project"}),
        ),
        (
            "path=\"a,b.rs\",note=comma,continued",
            "files",
            json!({"path":"a,b.rs","scope":"project","note":"comma,continued"}),
        ),
        (
            "path=a\\,b.rs,scope=global",
            "files",
            json!({"path":"a,b.rs","scope":"global"}),
        ),
        (
            "- path: src/a.rs\n- note: Example",
            "files",
            json!({"path":"src/a.rs","scope":"project","note":"Example"}),
        ),
        (
            "* path: a,b.rs",
            "files",
            json!({"path":"a,b.rs","scope":"project"}),
        ),
        (
            "+ path: src/a.rs",
            "files",
            json!({"path":"src/a.rs","scope":"project"}),
        ),
        (
            "- path: src/a.rs",
            "files",
            json!({"path":"src/a.rs","scope":"project"}),
        ),
        (
            "path=trailing.rs,",
            "files",
            json!({"path":"trailing.rs","scope":"project"}),
        ),
        (
            "- command: cargo test\n--flag value",
            "tests",
            json!({"command":"cargo test\n--flag value","scope":"project"}),
        ),
        (
            "command=echo one, two",
            "tests",
            json!({"command":"echo one, two","scope":"project"}),
        ),
        (
            "command=echo tail\\",
            "tests",
            json!({"command":"echo tail\\","scope":"project"}),
        ),
        (
            "```text\npath=src/a.rs\n```",
            "files",
            json!({"path":"src/a.rs","scope":"project"}),
        ),
        (
            "[Guide](docs/guide.md)",
            "docs",
            json!({"path":"docs/guide.md","scope":"project","note":"Guide"}),
        ),
        (
            "[literal",
            "docs",
            json!({"path":"[literal","scope":"project"}),
        ),
        (
            "[literal)",
            "docs",
            json!({"path":"[literal)","scope":"project"}),
        ),
        (
            "Guide,https://example.invalid",
            "docs",
            json!({"path":"https://example.invalid","scope":"project","note":"Guide"}),
        ),
        (
            ",https://example.invalid",
            "docs",
            json!({"path":",https://example.invalid","scope":"project"}),
        ),
        (
            "https://example.invalid/a,b",
            "files",
            json!({"path":"https://example.invalid/a,b","scope":"project"}),
        ),
        (
            "path=https://example.invalid/guide?q=a=b",
            "docs",
            json!({"path":"https://example.invalid/guide?q=a=b","scope":"project"}),
        ),
        (
            "cargo test FILTER=x",
            "tests",
            json!({"command":"cargo test FILTER=x","scope":"project"}),
        ),
        (
            r#"command="node -e \"console.log('a,b')\"""#,
            "tests",
            json!({"command":"node -e \"console.log('a,b')\"","scope":"project"}),
        ),
        (
            r#"command="bad\q",note="say \"hello\"""#,
            "tests",
            json!({"command":"bad\\q","scope":"project","note":"say \"hello\""}),
        ),
        (
            "cmd=cargo test,scope=global,note=Test",
            "tests",
            json!({"command":"cargo test","scope":"global","note":"Test"}),
        ),
        (
            "command=cargo test,timeout=3.9,pm_context_mode=TRACKER,workspace_context_mode=isolated,shared_host_safe=yes,assert_stdout_contains=one;two",
            "tests",
            json!({"command":"cargo test","scope":"project","timeout_seconds":3,"pm_context_mode":"tracker","workspace_context_mode":"isolated","shared_host_safe":true,"assert_stdout_contains":["one","two"]}),
        ),
        (
            "command=cargo test,timeout=2,timeout_seconds=2,shared_host_safe=no",
            "tests",
            json!({"command":"cargo test","scope":"project","timeout_seconds":2,"shared_host_safe":false}),
        ),
        (
            "command=cargo test,timeout_seconds=2,path=,note=",
            "tests",
            json!({"command":"cargo test","scope":"project","timeout_seconds":2}),
        ),
        (
            r#"command=cargo test,assert_stdout_contains="one\ntwo""#,
            "tests",
            json!({"command":"cargo test","scope":"project","assert_stdout_contains":["one","two"]}),
        ),
        (
            "command=cargo test,assert_stdout_min_lines=1,env_set=PORT=0;MODE=test,env_clear=\"ONE,TWO\",assert_json_field_equals=ok=true,assert_json_field_gte=count=2",
            "tests",
            json!({"command":"cargo test","scope":"project","assert_stdout_min_lines":1,"env_set":{"PORT":"0","MODE":"test"},"env_clear":["ONE","TWO"],"assert_json_field_equals":{"ok":"true"},"assert_json_field_gte":{"count":2.0}}),
        ),
    ] {
        assert_eq!(addition(raw, key, cwd, cwd)?, expected, "{raw}");
    }
    assert_eq!(anchor("../sibling", cwd, &cwd.join("nested")), "sibling");
    assert_eq!(anchor("../../sibling", cwd, cwd), "../../sibling");
    assert_eq!(anchor("a/./b", cwd, &cwd.join("..")), "a/b");
    assert_eq!(
        anchor("https://example.invalid", cwd, cwd),
        "https://example.invalid"
    );
    assert_eq!(anchor(r"C:\fixture\a.rs", cwd, cwd), "C:/fixture/a.rs");
    let absolute = cwd.join("absolute.rs").to_string_lossy().replace('\\', "/");
    assert_eq!(anchor(&absolute, cwd, cwd), absolute);
    Ok(())
}

/// Invalid intent never reaches a durable write.
#[test]
#[allow(clippy::too_many_lines)]
fn malformed_collection_intent_is_refused() -> TestResult {
    let root = tempfile::tempdir()?;
    let cwd = root.path();
    for (key, raw) in [
        ("files", "path=foo,broken"),
        ("files", "path=src/a.rs,broken"),
        ("files", "=value"),
        ("files", "bad key=value"),
        ("files", ""),
        ("files", "scope=global"),
        ("docs", "scope=global"),
        ("files", "path=x,wrong=1,other=2"),
        ("files", "a,b"),
        ("docs", "a,b"),
        ("files", "path=x,scope=wrong"),
        ("tests", "command="),
        ("tests", "command=x,scope=wrong"),
        ("tests", "cmd=x,command=y"),
        ("tests", "command=x,timeout=1,timeout_seconds=2"),
        ("tests", "command=x,timeout=0"),
        ("tests", "command=x,timeout=NaN"),
        ("tests", "command=x,timeout=bad"),
        ("tests", "command=x,pm_context_mode=wrong"),
        ("tests", "command=x,workspace_context_mode=wrong"),
        ("tests", "command=x,shared_host_safe=wrong"),
        ("tests", "command=pm test-all"),
        ("tests", "command=x,assert_stdout_min_lines=bad"),
        ("tests", "command=x,env_set=bad"),
        ("tests", "command=x,env_set=;"),
        ("tests", "command=x,env_set=PM_PATH=bad"),
        ("tests", "command=x,env_set=1BAD=value"),
        ("tests", "command=x,env_set=B-AD=value"),
        ("tests", "command=x,env_clear=FORCE_COLOR"),
        ("tests", "command=x,assert_json_field_equals=bad"),
        ("tests", "command=x,assert_json_field_equals=ok="),
        ("tests", "command=x,assert_json_field_equals=;"),
        ("tests", "command=x,assert_json_field_gte=count=bad"),
    ] {
        assert!(addition(raw, key, cwd, cwd).is_err(), "{raw}");
    }
    for (key, selector) in [
        ("files", ""),
        ("tests", ""),
        ("tests", "command="),
        ("tests", "path="),
        ("files", "scope=x"),
        ("files", "path=x,note=why"),
        ("files", "path="),
        ("files", "path=foo,broken"),
        ("files", "wrong=x"),
        ("files", "wrong=x,other=y"),
        ("tests", "wrong=x"),
        ("tests", "wrong_key=x"),
        ("tests", "wrong-key=x"),
        ("tests", "path:"),
    ] {
        assert!(
            prepare(
                key,
                &CollectionOptions {
                    remove: vec![selector.to_owned()],
                    ..CollectionOptions::default()
                },
                cwd,
                cwd
            )
            .is_err(),
            "{selector}"
        );
    }
    for index in ["0", "-1", "no", "01"] {
        assert!(
            prepare(
                "tests",
                &CollectionOptions {
                    remove_index: vec![index.to_owned()],
                    ..CollectionOptions::default()
                },
                cwd,
                cwd
            )
            .is_err()
        );
    }
    assert!(
        prepare(
            "files",
            &CollectionOptions {
                add: vec!["./x".to_owned()],
                remove: vec!["x".to_owned()],
                ..CollectionOptions::default()
            },
            cwd,
            cwd
        )
        .is_err()
    );
    assert!(
        prepare(
            "files",
            &CollectionOptions {
                add: vec!["x".to_owned(), "y".to_owned()],
                remove: vec!["y".to_owned(), "x".to_owned()],
                ..CollectionOptions::default()
            },
            cwd,
            cwd
        )
        .is_err()
    );
    Ok(())
}

/// Exact identity deduplication, note replacement and selector indices retain other rows.
#[test]
#[allow(clippy::too_many_lines)]
fn merging_preserves_unselected_rows() -> TestResult {
    let root = tempfile::tempdir()?;
    let cwd = root.path();
    let options = CollectionOptions {
        add: vec![
            "path=b,note=new".to_owned(),
            "path=a,scope=global".to_owned(),
        ],
        remove: vec!["missing".to_owned()],
        ..CollectionOptions::default()
    };
    let edit = prepare("files", &options, cwd, cwd)?;
    let (rows, removed) = merge(
        "files",
        &[json!({"path":"b","scope":"project","note":"old"})],
        &edit,
        false,
    );
    assert_eq!(removed, 0);
    assert_eq!(
        rows,
        vec![
            json!({"path":"a","scope":"global"}),
            json!({"path":"b","scope":"project","note":"new"})
        ]
    );
    let options = CollectionOptions {
        remove: vec!["path: a".to_owned()],
        remove_index: vec!["2".to_owned()],
        ..CollectionOptions::default()
    };
    let edit = prepare("tests", &options, cwd, cwd)?;
    let (rows, removed) = merge(
        "tests",
        &[
            json!({"command":"one","path":"a","scope":"project"}),
            json!({"command":"two","scope":"project"}),
            json!({"command":"three","scope":"project"}),
        ],
        &edit,
        false,
    );
    assert_eq!(removed, 2);
    assert_eq!(rows[0]["command"], "three");
    let edit = prepare(
        "tests",
        &CollectionOptions {
            remove: vec!["command: three".to_owned()],
            ..CollectionOptions::default()
        },
        cwd,
        cwd,
    )?;
    let (rows, removed) = merge("tests", &rows, &edit, false);
    assert_eq!(removed, 1);
    assert!(rows.is_empty());
    let edit = prepare("files", &CollectionOptions::default(), cwd, cwd)?;
    let (rows, _) = merge(
        "files",
        &[
            json!({"path":"x","scope":"project","note":"old"}),
            json!({"path":"x","scope":"project","note":"new"}),
        ],
        &edit,
        false,
    );
    assert_eq!(
        rows,
        vec![json!({"path":"x","scope":"project","note":"new"})]
    );
    let edit = prepare(
        "docs",
        &CollectionOptions {
            add: vec!["path=x,scope=global".to_owned()],
            ..CollectionOptions::default()
        },
        cwd,
        cwd,
    )?;
    let (rows, removed) = merge("docs", &rows, &edit, false);
    assert_eq!(removed, 0);
    assert_eq!(
        rows.iter()
            .map(|row| row["scope"].as_str().unwrap_or_default())
            .collect::<Vec<_>>(),
        ["global", "project"]
    );
    assert_eq!(distance("", "abc"), 3);
    assert_eq!(distance("a", ""), 1);
    assert_eq!(distance("😀", "a"), 2);
    let edit = prepare(
        "tests",
        &CollectionOptions {
            add: vec!["command=keep".to_owned()],
            remove: vec![
                "remove-command".to_owned(),
                "remove-path".to_owned(),
                "absent".to_owned(),
            ],
            ..CollectionOptions::default()
        },
        cwd,
        cwd,
    )?;
    let (rows, removed) = merge(
        "tests",
        &[
            json!({"command":"keep","scope":"project"}),
            json!({"command":"remove-command","scope":"project"}),
            json!({"command":"other","path":"remove-path","scope":"project"}),
        ],
        &edit,
        false,
    );
    assert_eq!(removed, 2);
    assert_eq!(rows, vec![json!({"command":"keep","scope":"project"})]);
    for raw in ["=value", "name.with.dot=value", "name=", "name=two words"] {
        let edit = prepare(
            "tests",
            &CollectionOptions {
                remove: vec![raw.to_owned()],
                ..CollectionOptions::default()
            },
            cwd,
            cwd,
        )?;
        let (rows, removed) = merge(
            "tests",
            &[json!({"command":raw,"scope":"project"})],
            &edit,
            false,
        );
        assert_eq!(removed, 1);
        assert!(rows.is_empty());
    }
    Ok(())
}

/// Creates a real native tracker with one canonical item and history stream.
fn tracker() -> Result<(tempfile::TempDir, Workspace), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let pm = root.path().join(".agents/pm");
    std::fs::create_dir_all(&pm)?;
    std::fs::write(pm.join("settings.json"), "{}")?;
    let workspace = Workspace::discover(root.path())?;
    workspace.create(serde_json::from_value(json!({"id":"sample-one","title":"One","type":"Task","author":"fixture","timestamp":"2026-08-22T10:00:00.000Z"}))?)?;
    Ok((root, workspace))
}

/// Read-only lists, no-op edits, missing entries and I/O failures preserve authoritative state.
#[test]
#[allow(clippy::too_many_lines)]
fn collection_storage_and_validation_boundaries() -> TestResult {
    let (directory, workspace) = tracker()?;
    let root = directory.path();
    let mut options = CollectionOptions {
        author: "fixture".to_owned(),
        timestamp: Some("2026-08-22T10:00:00.000Z".to_owned()),
        ..CollectionOptions::default()
    };
    assert_eq!(
        workspace.collection("sample-one", "files", &options, root)?["count"],
        0
    );
    assert!(
        workspace
            .collection("sample-one", "comments", &options, root)
            .is_err()
    );
    assert!(
        workspace
            .collection("missing", "files", &options, root)
            .is_err()
    );
    let suggestion = missing(workspace.pm_root(), "sample-on");
    let PmRustError::CommandRefusal { context, .. } = suggestion else {
        return Err("expected a typed missing item refusal".into());
    };
    assert!(
        context["next_steps"][0]
            .as_str()
            .is_some_and(|step| step.contains("sample-one"))
    );
    assert!(
        workspace
            .collection(
                "sample-one",
                "files",
                &CollectionOptions {
                    remove_index: vec!["1".to_owned()],
                    ..options.clone()
                },
                root
            )
            .is_err()
    );
    options.note = Some("why".to_owned());
    assert!(
        workspace
            .collection("sample-one", "files", &options, root)
            .is_err()
    );
    options.note = None;
    std::fs::write(root.join("exists.rs"), "")?;
    std::fs::create_dir(root.join("folder"))?;
    options.add = vec![
        "exists.rs".to_owned(),
        "folder".to_owned(),
        "missing.rs".to_owned(),
        "https://example.invalid".to_owned(),
        "invalid\0path".to_owned(),
    ];
    let result = workspace.collection("sample-one", "files", &options, root)?;
    assert_eq!(result["validation"]["checked"], 5);
    assert_eq!(result["validation"]["existing_files"], json!(["exists.rs"]));
    assert_eq!(
        result["validation"]["non_file_paths"],
        json!(["folder", "invalid\0path"])
    );
    assert_eq!(
        workspace.collection("sample-one", "files", &options, root)?["changed"],
        false
    );
    assert_eq!(
        workspace.collection("sample-one", "docs", &options, root)?["count"],
        5
    );
    let history = std::fs::read_to_string(workspace.pm_root().join("history/sample-one.jsonl"))?;
    assert_eq!(
        serde_json::from_str::<Value>(history.lines().last().ok_or("missing history")?)?["op"],
        "docs_add"
    );
    options.add.clear();
    options.remove = vec!["missing".to_owned()];
    assert!(
        workspace
            .collection("sample-one", "tests", &options, root)
            .is_err()
    );
    options.add = vec!["new.rs".to_owned()];
    options.remove.clear();
    let path = workspace.pm_root().join("tasks/sample-one.toon");
    let before = std::fs::read(&path)?;
    std::fs::write(&path, "invalid")?;
    assert!(
        workspace
            .collection("sample-one", "files", &CollectionOptions::default(), root)
            .is_err()
    );
    std::fs::write(path, before)?;
    assert!(
        workspace
            .collection(
                "sample-one",
                "files",
                &CollectionOptions {
                    add: vec!["scope=project".to_owned()],
                    ..options.clone()
                },
                root
            )
            .is_err()
    );
    std::fs::write(workspace.pm_root().join("settings.json"), "invalid")?;
    assert!(
        workspace
            .collection("sample-one", "files", &options, root)
            .is_err()
    );
    std::fs::write(workspace.pm_root().join("settings.json"), "{}")?;
    std::fs::remove_file(workspace.pm_root().join("history/sample-one.jsonl"))?;
    std::fs::create_dir(workspace.pm_root().join("history/sample-one.jsonl"))?;
    assert!(
        workspace
            .collection("sample-one", "files", &options, root)
            .is_err()
    );
    assert!(
        workspace
            .pm_root()
            .join("runtime/transactions/files_add-sample-one.json")
            .is_file()
    );
    Ok(())
}

/// Collection transactions enforce claims and preserve state across lock and recovery faults.
#[test]
#[allow(clippy::too_many_lines)]
fn collection_ownership_and_lock_boundaries() -> TestResult {
    let (directory, workspace) = tracker()?;
    let cwd = directory.path();
    let root = workspace.pm_root();
    let mut options = CollectionOptions {
        author: "fixture".to_owned(),
        timestamp: Some("2026-08-22T10:00:00.000Z".to_owned()),
        add: vec!["src/a.rs".to_owned()],
        ..CollectionOptions::default()
    };
    let lock = root.join("locks/sample-one.lock");
    for wait in [0, 1] {
        std::fs::write(
            root.join("settings.json"),
            json!({"locks":{"wait_ms":wait}}).to_string(),
        )?;
        std::fs::write(&lock,json!({"id":"sample-one","pid":1,"owner":"holder","token":"fixture","created_at":"2026-08-22T10:00:00.000Z","ttl_seconds":1800}).to_string())?;
        assert!(matches!(
            workspace.collection("sample-one", "files", &options, cwd),
            Err(PmRustError::OwnershipRefusal { .. })
        ));
        std::fs::remove_file(&lock)?;
    }
    std::fs::remove_dir_all(root.join("locks"))?;
    std::fs::write(root.join("locks"), "blocked")?;
    assert!(matches!(
        workspace.collection("sample-one", "files", &options, cwd),
        Err(PmRustError::Io { .. })
    ));
    std::fs::remove_file(root.join("locks"))?;
    std::fs::create_dir(root.join("locks"))?;
    std::fs::create_dir_all(root.join("runtime/transactions"))?;
    let journal = root.join("runtime/transactions/files_add-sample-one.json");
    std::fs::write(&journal, "invalid")?;
    assert!(
        workspace
            .collection("sample-one", "files", &options, cwd)
            .is_err()
    );
    std::fs::remove_file(journal)?;
    assert!(
        workspace
            .collection("absent", "files", &options, cwd)
            .is_err()
    );
    workspace.claim(&crate::OwnershipItem {
        id: "sample-one".to_owned(),
        author: "other".to_owned(),
        timestamp: options.timestamp.clone(),
        message: None,
        force: false,
        if_available: false,
        provenance_role: None,
    })?;
    std::fs::write(
        root.join("settings.json"),
        json!({"governance":{"preset":"strict"}}).to_string(),
    )?;
    let item_path = root.join("tasks/sample-one.toon");
    let before = std::fs::read(&item_path)?;
    assert!(matches!(
        workspace.collection("sample-one", "files", &options, cwd),
        Err(PmRustError::OwnershipRefusal { .. })
    ));
    assert_eq!(std::fs::read(&item_path)?, before);
    let mut legacy = workspace.get("sample-one")?;
    legacy.metadata.extra.remove("claim_principal");
    std::fs::write(&item_path, toon_format::encode_default(&legacy)?)?;
    assert!(matches!(
        workspace.collection("sample-one", "files", &options, cwd),
        Err(PmRustError::OwnershipRefusal { .. })
    ));
    std::fs::write(&item_path, &before)?;
    options.force = true;
    assert_eq!(
        workspace.collection("sample-one", "files", &options, cwd)?["changed"],
        true
    );
    options.force = false;
    options.author = "other".to_owned();
    options.add = vec!["owned.rs".to_owned()];
    assert_eq!(
        workspace.collection("sample-one", "files", &options, cwd)?["changed"],
        true
    );
    options.author = "fixture".to_owned();
    options.add = vec!["permitted.rs".to_owned()];
    std::fs::write(
        root.join("settings.json"),
        json!({"governance":{"preset":"minimal"}}).to_string(),
    )?;
    assert_eq!(
        workspace.collection("sample-one", "files", &options, cwd)?["changed"],
        true
    );
    options.note = Some("standalone".to_owned());
    options.add = vec!["path=inline.rs,note=Inline".to_owned()];
    let result = workspace.collection("sample-one", "files", &options, cwd)?;
    assert!(
        result["files"]
            .as_array()
            .ok_or("missing files")?
            .iter()
            .any(|row| row["note"] == "Inline")
    );
    options.note = Some(" ".to_owned());
    options.add = vec!["blank.rs".to_owned()];
    assert_eq!(
        workspace.collection("sample-one", "files", &options, cwd)?["changed"],
        true
    );
    Ok(())
}

/// SDK transactions preserve provenance, no-op history and exact removal receipts.
#[test]
fn linked_test_sdk_transactions_preserve_authoritative_bytes() -> TestResult {
    let (directory, workspace) = tracker()?;
    let root = directory.path();
    let mut options = CollectionOptions {
        author: "fixture".to_owned(),
        timestamp: Some("2026-08-22T10:00:00.000Z".to_owned()),
        add: vec!["command=cargo test,path=tests/a.rs,note=Evidence".to_owned()],
        ..CollectionOptions::default()
    };
    for (author, timestamp) in [("", "2026-08-22T10:00:00.000Z"), ("fixture", "invalid")] {
        assert!(
            workspace
                .collection(
                    "sample-one",
                    "tests",
                    &CollectionOptions {
                        author: author.to_owned(),
                        timestamp: Some(timestamp.to_owned()),
                        ..options.clone()
                    },
                    root
                )
                .is_err()
        );
    }
    options.identity = Some(crate::AuthorIdentity::resolve(
        Some("fixture"),
        None,
        &std::collections::BTreeMap::from([
            ("PI_AGENT".to_owned(), "1".to_owned()),
            ("PM_AGENT_MODEL".to_owned(), "fixture-model".to_owned()),
            ("PM_AGENT_VERSION".to_owned(), "fixture-version".to_owned()),
        ]),
    ));
    assert_eq!(
        workspace.collection("sample-one", "tests", &options, root)?["count"],
        1
    );
    let history = workspace.pm_root().join("history/sample-one.jsonl");
    let before = std::fs::read(&history)?;
    assert!(String::from_utf8_lossy(&before).contains("fixture-version"));

    assert_eq!(
        workspace.collection("sample-one", "tests", &options, root)?["changed"],
        false
    );
    assert_eq!(std::fs::read(&history)?, before);
    assert_eq!(
        workspace.collection("sample-one", "tests", &CollectionOptions::default(), root)?["count"],
        1
    );
    options.add.clear();
    options.remove_index = vec!["1".to_owned()];
    let result = workspace.collection("sample-one", "tests", &options, root)?;
    assert_eq!(result["removed"], 1);
    assert_eq!(result["count"], 0);
    assert!(
        !workspace
            .get("sample-one")?
            .metadata
            .extra
            .contains_key("tests")
    );
    options.remove_index.clear();
    options.add = vec!["command=one".to_owned(), "command=two".to_owned()];
    assert_eq!(
        workspace.collection("sample-one", "tests", &options, root)?["count"],
        2
    );
    options.add.clear();
    options.remove = vec!["command=one".to_owned()];
    assert_eq!(
        workspace.collection("sample-one", "tests", &options, root)?["removed"],
        1
    );
    options.remove.clear();
    options.add = vec!["z.rs".to_owned(), "a.rs".to_owned()];
    options.append_stable = true;
    options.note = Some("Shared".to_owned());
    let result = workspace.collection("sample-one", "files", &options, root)?;
    assert_eq!(result["files"][0]["path"], "z.rs");
    assert_eq!(result["files"][0]["note"], "Shared");
    Ok(())
}

/// An unavailable source context omits provenance without refusing a valid SDK mutation.
#[test]
fn sdk_collection_without_source_context() -> TestResult {
    const CHILD: &str = "PM_RUST_COLLECTION_SOURCE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let directory = tempfile::tempdir()?;
        let output = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "collections::tests::sdk_collection_without_source_context",
            ])
            .env(CHILD, "1")
            .env("GITHUB_HEAD_REF", "")
            .env("GITHUB_REF_NAME", "")
            .env(
                "PM_SOURCE_WORKSPACE_ROOT",
                directory.path().join("unavailable"),
            )
            .env_remove("PM_PATH")
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("test result: ok. 1 passed;"),
            "child filter did not execute exactly one test: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        return Ok(());
    }
    let (directory, workspace) = tracker()?;
    let result = workspace.collection(
        "sample-one",
        "tests",
        &CollectionOptions {
            author: "fixture".to_owned(),
            timestamp: Some("2026-08-22T10:00:00.000Z".to_owned()),
            add: vec!["command=cargo test".to_owned()],
            ..CollectionOptions::default()
        },
        directory.path(),
    )?;
    assert!(result["tests"][0]["provenance"].get("source_ref").is_none());
    Ok(())
}

/// Optional provenance reads a real source branch and survives a missing Git executable.
#[test]
fn sdk_collection_git_source_context() -> TestResult {
    const CHILD: &str = "PM_RUST_COLLECTION_GIT_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let source = tempfile::tempdir()?;
        let init = std::process::Command::new("git")
            .args(["init", "--quiet", "--initial-branch=source-branch"])
            .arg(source.path())
            .output()?;
        assert!(
            init.status.success(),
            "{}",
            String::from_utf8_lossy(&init.stderr)
        );
        let empty = source.path().join("empty-bin");
        std::fs::create_dir(&empty)?;
        for mode in ["with-git", "without-git"] {
            let mut command = std::process::Command::new(std::env::current_exe()?);
            command
                .args([
                    "--exact",
                    "collections::tests::sdk_collection_git_source_context",
                ])
                .env(CHILD, mode)
                .env("GITHUB_HEAD_REF", "")
                .env("GITHUB_REF_NAME", "")
                .env("PM_SOURCE_WORKSPACE_ROOT", source.path())
                .env_remove("PM_PATH");
            if mode == "without-git" {
                command.env("PATH", &empty);
            }
            let output = command.output()?;
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("test result: ok. 1 passed;"));
        }
        return Ok(());
    }
    let (directory, workspace) = tracker()?;
    let result = workspace.collection(
        "sample-one",
        "tests",
        &CollectionOptions {
            author: "fixture".to_owned(),
            timestamp: Some("2026-08-22T10:00:00.000Z".to_owned()),
            add: vec!["command=cargo test".to_owned()],
            ..CollectionOptions::default()
        },
        directory.path(),
    )?;
    let source = &result["tests"][0]["provenance"]["source_ref"];
    if std::env::var(CHILD)? == "with-git" {
        assert_eq!(source, "source-branch");
    } else {
        assert_eq!(source, &Value::Null);
    }
    Ok(())
}
