//! Collection parsing, preservation and storage-failure regressions.
use super::*;
type TestResult = Result<(), Box<dyn std::error::Error>>;

use crate::test_process_environment as process_environment;

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

/// Refusing a conflicting note preserves storage and explicit append projects only the edited link.
#[test]
fn artifact_note_conflicts_preserve_storage_and_report_explicit_append() -> TestResult {
    let (directory, workspace) = tracker()?;
    let root = directory.path();
    let mut options = CollectionOptions {
        author: "fixture".to_owned(),
        timestamp: Some("2026-08-22T10:00:00.000Z".to_owned()),
        add: vec!["path=a.rs,note=Stored".to_owned(), "b.rs".to_owned()],
        ..CollectionOptions::default()
    };
    workspace.collection("sample-one", "files", &options, root)?;
    let item = workspace.pm_root().join("tasks/sample-one.toon");
    let history = workspace.pm_root().join("history/sample-one.jsonl");
    let item_bytes = std::fs::read(&item)?;
    let history_bytes = std::fs::read(&history)?;

    options.add = vec!["path=a.rs,note=Incoming".to_owned()];
    let Err(PmRustError::CommandRefusal { context, .. }) =
        workspace.collection("sample-one", "files", &options, root)
    else {
        return Err("expected a linked artifact note conflict".into());
    };
    assert_eq!(context["code"], "linked_artifact_note_conflict");
    for command in context["next_steps"].as_array().ok_or("repairs")? {
        let args =
            crate::parse_command_args(command.as_str().ok_or("command")?).ok_or("arguments")?;
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--timestamp", "2026-08-22T10:00:00.000Z"])
        );
        assert!(args.windows(2).any(|pair| pair == ["--author", "fixture"]));
    }
    let mut detected = options.clone();
    detected.timestamp = None;
    detected.identity = Some(crate::AuthorIdentity::resolve(
        None,
        None,
        &std::collections::BTreeMap::from([("CODEX_THREAD_ID".into(), "sample-session".into())]),
    ));
    let Err(PmRustError::CommandRefusal { context, .. }) =
        workspace.collection("sample-one", "files", &detected, root)
    else {
        return Err("expected detected session conflict".into());
    };
    for repair in context["next_steps"].as_array().ok_or("repairs")? {
        let args =
            crate::parse_command_args(repair.as_str().ok_or("command")?).ok_or("arguments")?;
        assert!(
            !args
                .iter()
                .any(|token| token == "--timestamp" || token == "--author")
        );
    }
    assert_eq!(std::fs::read(item)?, item_bytes);
    assert_eq!(std::fs::read(history)?, history_bytes);

    options.note_conflict = Some("append".to_owned());
    let result = workspace.collection("sample-one", "files", &options, root)?;
    assert_eq!(
        result["files"],
        json!([{"path":"a.rs","scope":"project","note":"Stored; Incoming"}])
    );
    assert_eq!(result["total_count"], 2);
    assert_eq!(
        result["mutation_receipt"]["link_outcomes"][0]["outcome"],
        "note_appended"
    );
    options.add = vec!["path=a.rs,note=Next".to_owned()];
    workspace.collection("sample-one", "files", &options, root)?;
    options.add = vec!["path=a.rs,note=Stored; Incoming".to_owned()];
    let replay = workspace.collection("sample-one", "files", &options, root)?;
    assert_eq!(replay["changed"], false);
    assert_eq!(
        replay["mutation_receipt"]["link_outcomes"][0]["outcome"],
        "unchanged"
    );
    options.note_conflict = Some("REPLACE".to_owned());
    options.full_history = true;
    options.add = vec!["path=a.rs,note=Replacement".to_owned(); 2];
    let replaced = workspace.collection("sample-one", "files", &options, root)?;
    assert_eq!(replaced["count"], 2);
    assert_eq!(
        replaced["mutation_receipt"]["link_outcomes"],
        json!([{"path":"a.rs","scope":"project","outcome":"note_replaced"}])
    );
    options.add = vec!["path=b.rs,note=New rationale".to_owned()];
    let added = workspace.collection("sample-one", "files", &options, root)?;
    assert_eq!(added["files"][1]["note"], "New rationale");
    assert_eq!(
        added["mutation_receipt"]["link_outcomes"][0]["outcome"],
        "note_added"
    );
    options.add.clear();
    options.note_conflict = None;
    options.remove = vec!["a.rs".to_owned(), "b.rs".to_owned()];
    let removed = workspace.collection("sample-one", "files", &options, root)?;
    assert_eq!(removed["changed"], true);
    assert_eq!(removed["count"], 0);
    Ok(())
}

/// A note repair under strict ownership requires the session that holds the item.
#[test]
fn artifact_note_repair_uses_the_original_detected_session() -> TestResult {
    let (directory, workspace) = tracker()?;
    let root = directory.path();
    std::fs::write(
        workspace.pm_root().join("settings.json"),
        r#"{"governance":{"preset":"strict"}}"#,
    )?;
    let identity = |session: &str| {
        crate::AuthorIdentity::resolve(
            None,
            None,
            &std::collections::BTreeMap::from([("CODEX_THREAD_ID".into(), session.into())]),
        )
    };
    let owner = identity("session-a");
    let other = identity("session-b");
    assert_eq!(owner.author, other.author);
    workspace.claim_with_options(
        &crate::OwnershipItem {
            id: "sample-one".into(),
            author: owner.author.clone(),
            timestamp: None,
            message: None,
            force: false,
            if_available: false,
            provenance_role: None,
        },
        &crate::OwnershipOptions {
            identity: Some(owner.clone()),
            ..crate::OwnershipOptions::default()
        },
    )?;
    let mut options = CollectionOptions {
        author: owner.author.clone(),
        identity: Some(owner.clone()),
        add: vec!["path=a.rs,note=Stored".into()],
        ..CollectionOptions::default()
    };
    workspace.collection("sample-one", "files", &options, root)?;
    options.add = vec!["path=a.rs,note=Incoming".into()];
    let Err(PmRustError::CommandRefusal { context, .. }) =
        workspace.collection("sample-one", "files", &options, root)
    else {
        return Err("expected note conflict in the original session".into());
    };
    assert_eq!(context["code"], "linked_artifact_note_conflict");
    let guidance = context["required"].as_str().ok_or("repair requirements")?;
    assert!(guidance.contains("original detected session"));
    assert!(guidance.contains("original AuthorIdentity"));
    options.note_conflict = Some("append".into());
    options.identity = Some(other);
    assert!(matches!(
        workspace.collection("sample-one", "files", &options, root),
        Err(PmRustError::OwnershipRefusal { context, .. }) if context["code"] == "ownership_conflict"
    ));
    options.identity = Some(owner);
    let result = workspace.collection("sample-one", "files", &options, root)?;
    assert_eq!(result["files"][0]["note"], "Stored; Incoming");
    Ok(())
}

/// An SDK repair retains an asserted identity that a fallback CLI author cannot reproduce.
#[test]
fn artifact_note_repair_requires_the_asserted_sdk_principal() -> TestResult {
    let (directory, workspace) = tracker()?;
    std::fs::write(
        workspace.pm_root().join("settings.json"),
        r#"{"governance":{"preset":"strict"}}"#,
    )?;
    workspace.claim(&crate::OwnershipItem {
        id: "sample-one".into(),
        author: "owner".into(),
        timestamp: None,
        message: None,
        force: false,
        if_available: false,
        provenance_role: None,
    })?;
    let mut options = CollectionOptions {
        add: vec!["path=a.rs,note=Stored".into()],
        author: "fallback".into(),
        identity: Some(crate::AuthorIdentity::resolve(
            Some("owner"),
            None,
            &std::collections::BTreeMap::default(),
        )),
        ..Default::default()
    };
    let root = directory.path();
    workspace.collection("sample-one", "files", &options, root)?;
    options.add = vec!["path=a.rs,note=Incoming".into()];
    let Err(PmRustError::CommandRefusal { context, .. }) =
        workspace.collection("sample-one", "files", &options, root)
    else {
        return Err("expected a note conflict for the asserted owner".into());
    };
    assert_eq!(context["code"], "linked_artifact_note_conflict");
    assert!(context.get("recovery").is_none());
    assert!(
        context["required"]
            .as_str()
            .ok_or("repair guidance")?
            .contains("original AuthorIdentity")
    );

    options.note_conflict = Some("append".into());
    let mut fallback = options.clone();
    fallback.identity = None;
    assert!(matches!(
        workspace.collection("sample-one", "files", &fallback, root),
        Err(PmRustError::OwnershipRefusal { context, .. }) if context["code"] == "ownership_conflict"
    ));
    let result = workspace.collection("sample-one", "files", &options, root)?;
    assert_eq!(result["files"][0]["note"], "Stored; Incoming");
    Ok(())
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
    assert_eq!(result["mutation_receipt"]["removed_count"], 1);
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
        workspace.collection("sample-one", "tests", &options, root)?["mutation_receipt"]["removed_count"],
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
        let mut command = std::process::Command::new("git");
        process_environment::isolate_environment(&mut command);
        let init = command
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
            process_environment::isolate_environment(&mut command);
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

/// Hook repository coordinates cannot redirect linked-test provenance or fixture setup.
#[test]
fn sdk_collection_git_source_ignores_hook_repository() -> TestResult {
    const CHILD: &str = "PM_RUST_COLLECTION_HOOK_CHILD";
    if std::env::var_os(CHILD).is_some() {
        // This also proves the ordinary fixture's init and assertion children
        // stay isolated when this process inherits a hook's repository context.
        sdk_collection_git_source_context()?;
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
        assert_eq!(
            result["tests"][0]["provenance"]["source_ref"],
            "source-branch"
        );
        return Ok(());
    }
    let source = tempfile::tempdir()?;
    let other = tempfile::tempdir()?;
    for (directory, branch) in [(&source, "source-branch"), (&other, "hook-branch")] {
        let mut command = std::process::Command::new("git");
        process_environment::isolate_environment(&mut command);
        let output = command
            .args(["init", "--quiet", "--initial-branch", branch])
            .arg(directory.path())
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let git_dir = other.path().join(".git");
    let mut command = std::process::Command::new(std::env::current_exe()?);
    process_environment::isolate_environment(&mut command);
    let output = command
        .args([
            "--exact",
            "collections::tests::sdk_collection_git_source_ignores_hook_repository",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .env("PM_SOURCE_WORKSPACE_ROOT", source.path())
        .env("GIT_DIR", &git_dir)
        .env("GIT_WORK_TREE", other.path())
        .env("GIT_INDEX_FILE", git_dir.join("index"))
        .env("GIT_OBJECT_DIRECTORY", git_dir.join("objects"))
        .env("GIT_ALTERNATE_OBJECT_DIRECTORIES", git_dir.join("objects"))
        .env("GIT_COMMON_DIR", &git_dir)
        .env("GIT_NAMESPACE", "hook-namespace")
        .output()?;
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("test result: ok. 1 passed;"));
    Ok(())
}

/// Explicit note policies refuse invalid values, missing additions and linked-test use.
#[test]
fn sdk_note_policy_refusals_are_read_only() -> TestResult {
    let (directory, workspace) = tracker()?;
    let path = workspace.pm_root().join("history/sample-one.jsonl");
    let before = std::fs::read(&path)?;
    for (key, mode, add, expected) in [
        (
            "files",
            "invalid",
            vec!["source.rs".to_owned()],
            "invalid_argument_value",
        ),
        ("files", "append", vec![], "missing_required_argument"),
        (
            "tests",
            "replace",
            vec!["command=cargo fmt".to_owned()],
            "unknown_option",
        ),
    ] {
        let options = CollectionOptions {
            author: "fixture".to_owned(),
            add,
            note_conflict: Some(mode.to_owned()),
            ..CollectionOptions::default()
        };
        let Err(PmRustError::CommandRefusal { context, .. }) =
            workspace.collection("sample-one", key, &options, directory.path())
        else {
            return Err(format!("expected a {key} note policy refusal").into());
        };
        assert_eq!(context["code"], expected);
    }
    assert_eq!(std::fs::read(path)?, before);
    Ok(())
}

/// Each in-batch conflict is described and the retry retains nonconflicting options.
#[test]
fn sdk_note_conflict_retry_preserves_the_request() -> TestResult {
    let (directory, workspace) = tracker()?;
    let item = workspace.pm_root().join("tasks/sample-one.toon");
    let history = workspace.pm_root().join("history/sample-one.jsonl");
    let before = (std::fs::read(&item)?, std::fs::read(&history)?);
    for (key, path, append_stable) in [("docs", "guide.md", false), ("files", "source.rs", true)] {
        let options = CollectionOptions {
            author: "fixture".to_owned(),
            add: ["first", "second", "third"]
                .map(|note| format!("path={path},note={note}"))
                .to_vec(),
            remove: vec!["unrelated.md".to_owned()],
            note: Some("fallback".to_owned()),
            force: true,
            append_stable,
            full_history: true,
            message: Some("Link rationale".to_owned()),
            ..CollectionOptions::default()
        };
        let Err(PmRustError::CommandRefusal { context, .. }) =
            workspace.collection("sample-one", key, &options, directory.path())
        else {
            return Err(format!("expected an in-batch {key} conflict").into());
        };
        assert_eq!(context["code"], "linked_artifact_note_conflict");
        let rows = context["details"]["conflicts"]
            .as_array()
            .ok_or("conflict rows")?;
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| row["source"] == "this_write"));
        let retry = context["recovery"]["suggested_retry_args"]
            .as_array()
            .ok_or("retry args")?;
        for flag in [
            "--remove",
            "--note",
            "--force",
            "--full-history",
            "--message",
        ] {
            assert!(retry.contains(&json!(flag)), "{key}: {flag}");
        }
        assert_eq!(retry.contains(&json!("--append-stable")), append_stable);
        assert_eq!(retry[0], "--pm-path");

        let Err(PmRustError::CommandRefusal { context, .. }) =
            workspace.collection_for_cli("sample-one", key, &options, directory.path())
        else {
            return Err(format!("expected a CLI {key} conflict").into());
        };
        assert_eq!(retry[4], "--artifact-cwd");
        let cli_retry = [&retry[2..4], &retry[6..]].concat();
        assert_eq!(
            context["recovery"]["suggested_retry_args"],
            json!(cli_retry)
        );
    }
    assert_eq!((std::fs::read(item)?, std::fs::read(history)?), before);
    Ok(())
}

/// Leading-hyphen notes use one token for both replay binaries.
#[test]
fn note_repair_arguments_preserve_leading_hyphens() {
    let options = CollectionOptions {
        author: "-fixture".to_owned(),
        note: Some("--json".to_owned()),
        message: Some("-rationale".to_owned()),
        ..CollectionOptions::default()
    };
    for mode in ["append", "replace"] {
        assert_eq!(
            note_retry_args(Vec::new(), &options, mode, true),
            [
                "--note=--json",
                "--note-conflict",
                mode,
                "--message=-rationale",
                "--author=-fixture"
            ]
        );
        assert_eq!(
            note_retry_args(Vec::new(), &options, mode, false),
            [
                "--note=--json",
                "--note-conflict",
                mode,
                "--message",
                "-rationale",
                "--author",
                "-fixture"
            ]
        );
    }
}

/// SDK repair guidance captures the absolute operand base at the time of the conflict.
#[test]
fn sdk_note_retry_resolves_the_caller_directory() -> TestResult {
    let (directory, workspace) = tracker()?;
    let options = CollectionOptions {
        author: "fixture".to_owned(),
        add: ["first", "second"]
            .map(|note| format!("path=source.rs,note={note}"))
            .to_vec(),
        ..CollectionOptions::default()
    };
    let missing = directory.path().join("missing").join("..");
    for (cwd, expected) in [
        (Path::new("."), std::env::current_dir()?),
        (&missing, directory.path().canonicalize()?),
    ] {
        let Err(PmRustError::CommandRefusal { context, .. }) =
            workspace.collection("sample-one", "files", &options, cwd)
        else {
            return Err("expected an SDK note conflict".into());
        };
        let retry = &context["recovery"]["suggested_retry_args"];
        assert_eq!(retry[4], "--artifact-cwd");
        let resolved = Path::new(retry[5].as_str().ok_or("artifact directory")?);
        assert!(resolved.is_absolute());
        assert_eq!(resolved, expected);
    }
    assert!(matches!(
        workspace.collection("sample-one", "files", &options, Path::new("")),
        Err(PmRustError::Io { .. })
    ));
    let file = directory.path().join("base.txt");
    std::fs::write(&file, "file fixture")?;
    assert!(matches!(
        workspace.collection("sample-one", "files", &options, &file),
        Err(PmRustError::CommandRefusal { exit_code: 2, .. })
    ));
    assert!(matches!(
        workspace.collection_for_cli_with_artifact_cwd(
            "sample-one",
            "files",
            &options,
            Path::new(""),
            Some(Path::new("")),
        ),
        Err(PmRustError::Io { .. })
    ));
    Ok(())
}

/// An alias of the project root preserves the logical suffix for replay.
#[cfg(unix)]
#[test]
fn artifact_cwd_resolves_root_alias_without_following_its_suffix() -> TestResult {
    let (directory, workspace) = tracker()?;
    let aliases = tempfile::tempdir()?;
    let alias = aliases.path().join("alias");
    std::os::unix::fs::symlink(directory.path(), &alias)?;
    std::os::unix::fs::symlink(aliases.path(), directory.path().join("vendor"))?;
    let project = workspace.artifact_project_root();
    for suffix in ["vendor", "missing"] {
        assert_eq!(
            resolve_artifact_cwd(&alias.join(suffix), project)?,
            project.join(suffix)
        );
    }
    let external_missing = aliases.path().canonicalize()?.join("missing/sub");
    assert_eq!(
        resolve_artifact_cwd(&external_missing, project)?,
        external_missing
    );
    Ok(())
}

/// Native-only options keep their values and receive a native repair command.
#[test]
fn cli_note_repair_identifies_other_native_only_options() -> TestResult {
    let (_directory, workspace) = tracker()?;
    let cwd = workspace
        .pm_root()
        .ancestors()
        .nth(2)
        .ok_or("tracker project")?;
    for (key, timestamp, append_stable, artifact_cwd, flag) in [
        (
            "files",
            Some("2026-08-22T10:00:00.000Z"),
            false,
            None,
            "--timestamp",
        ),
        ("docs", None, true, None, "--append-stable"),
        ("files", None, false, Some(Path::new(".")), "--artifact-cwd"),
    ] {
        let options = CollectionOptions {
            author: "fixture".to_owned(),
            add: ["first", "second"]
                .map(|note| format!("path=resource.md,note={note}"))
                .to_vec(),
            timestamp: timestamp.map(str::to_owned),
            append_stable,
            ..CollectionOptions::default()
        };
        let Err(PmRustError::CommandRefusal { context, .. }) = workspace
            .collection_for_cli_with_artifact_cwd("sample-one", key, &options, cwd, artifact_cwd)
        else {
            return Err("expected a note conflict".into());
        };
        assert_eq!(context["code"], "linked_artifact_note_conflict");
        let steps = context["next_steps"].as_array().ok_or("repair commands")?;
        for step in steps {
            let command = step.as_str().ok_or("repair command")?;
            let args = crate::parse_command_args(command).ok_or("repair arguments")?;
            assert_eq!(args[..2], ["pm-rust", "--pm-path"]);
            assert!(args.iter().any(|arg| arg == flag));
            assert_eq!(args[5], "--artifact-cwd");
            assert_eq!(Path::new(&args[6]), cwd);
        }
    }
    Ok(())
}

/// A redacted requested note cannot turn into executable SDK guidance.
#[test]
fn sdk_note_conflict_requires_original_note_after_redaction() -> TestResult {
    let (directory, workspace) = tracker()?;
    let mut options = CollectionOptions {
        author: "fixture".into(),
        add: vec!["path=source.rs,note=first".into()],
        ..Default::default()
    };
    workspace.collection("sample-one", "files", &options, directory.path())?;
    options.add = vec!["source.rs".into()];
    options.note = Some("See https://fixture:synthetic-secret@example.invalid".into());
    let Err(PmRustError::CommandRefusal { context, .. }) =
        workspace.collection("sample-one", "files", &options, directory.path())
    else {
        return Err("expected a note conflict".into());
    };
    assert!(context.get("recovery").is_none());
    assert_eq!(
        context["details"]["conflicts"][0]["requested_note"],
        "See https://[redacted]@example.invalid"
    );
    let steps = context["next_steps"].as_array().ok_or("guidance")?;
    assert_eq!(steps.len(), 1);
    assert!(
        steps[0]
            .as_str()
            .ok_or("step")?
            .contains("Supply the original note")
    );
    assert!(!steps[0].to_string().contains("[redacted]"));
    assert_eq!(
        workspace.get("sample-one")?.metadata.extra["files"][0]["note"],
        "first"
    );
    Ok(())
}

/// An SDK retry cannot substitute a different path for a non-UTF-8 tracker.
#[cfg(target_os = "linux")]
#[test]
fn sdk_note_conflict_with_non_utf8_tracker_has_no_replay_command() -> TestResult {
    use std::os::unix::ffi::OsStrExt as _;

    let (directory, _) = tracker()?;
    let parent = tempfile::tempdir()?;
    let target = parent
        .path()
        .join(std::ffi::OsStr::from_bytes(b"selected-\xff"));
    std::fs::rename(directory.path(), &target)?;
    let workspace = Workspace::discover(&target)?;
    let options = CollectionOptions {
        author: "fixture".to_owned(),
        add: vec![
            "path=source.rs,note=first".to_owned(),
            "path=source.rs,note=second".to_owned(),
        ],
        ..Default::default()
    };
    let Err(PmRustError::CommandRefusal { context, .. }) =
        workspace.collection("sample-one", "files", &options, parent.path())
    else {
        return Err("expected a note conflict".into());
    };
    assert_eq!(context["code"], "linked_artifact_note_conflict");
    assert!(context.get("recovery").is_none());
    assert!(
        context["next_steps"][0]
            .as_str()
            .ok_or("repair guidance")?
            .contains("UTF-8")
    );
    assert_eq!(context["details"]["conflicts"][0]["path"], "source.rs");

    let mut options = CollectionOptions {
        author: "fixture".into(),
        add: vec!["keep.rs".into(), "relative.rs".into(), "absolute.rs".into()],
        ..Default::default()
    };
    workspace.collection("sample-one", "files", &options, &target)?;
    options.add.clear();
    let outside = tempfile::tempdir()?;
    for (cwd, removed) in [
        (parent.path(), "relative.rs"),
        (outside.path(), "absolute.rs"),
    ] {
        options.remove = vec![removed.into()];
        let receipt = if removed == "relative.rs" {
            workspace.collection_for_cli("sample-one", "files", &options, cwd)?
        } else {
            workspace.collection("sample-one", "files", &options, cwd)?
        };
        assert_eq!(
            receipt["total_count"],
            if removed == "relative.rs" { 2 } else { 1 }
        );
        let group = &receipt["omission_receipt"]["omitted_field_groups"][0];
        assert_eq!(group["name"], "files_history");
        assert!(group.get("restore_with").is_none());
        assert!(
            group["restore_guidance"]
                .as_str()
                .ok_or("manual restore guidance")?
                .contains("UTF-8")
        );
    }
    Ok(())
}

/// CLI callers retain relative restore commands, including when invoked from the tracker itself.
#[test]
fn cli_omitted_rows_retain_tracker_root_as_invocation_directory() -> TestResult {
    let (_directory, workspace) = tracker()?;
    let mut options = CollectionOptions {
        author: "fixture".into(),
        add: vec!["a.rs".into(), "b.rs".into()],
        ..Default::default()
    };
    workspace.collection("sample-one", "files", &options, workspace.pm_root())?;
    options.add.clear();
    options.remove.push("a.rs".into());
    let result =
        workspace.collection_for_cli("sample-one", "files", &options, workspace.pm_root())?;
    assert_eq!(
        result["omission_receipt"]["omitted_field_groups"][0]["restore_with"],
        "pm --pm-path . files sample-one"
    );
    let project = workspace
        .pm_root()
        .parent()
        .and_then(Path::parent)
        .ok_or("project")?;
    let alias = project.join("scratch").join("..");
    std::fs::create_dir(project.join("scratch"))?;
    options.remove.clear();
    options.add.push("c.rs".into());
    let result = workspace.collection_for_cli("sample-one", "files", &options, &alias)?;
    assert_eq!(
        result["omission_receipt"]["omitted_field_groups"][0]["restore_with"],
        "pm --pm-path .agents/pm files sample-one"
    );
    let other = tempfile::tempdir()?;
    options.add.clear();
    options.remove.push("c.rs".into());
    let result = workspace.collection_for_cli("sample-one", "files", &options, other.path())?;
    let hint = result["omission_receipt"]["omitted_field_groups"][0]["restore_with"]
        .as_str()
        .ok_or("scope")?;
    let args = crate::parse_command_args(hint).ok_or("read command")?;
    assert_eq!(args.len(), 5);
    assert_eq!(args[3..], ["files", "sample-one"]);
    assert_eq!(
        other.path().join(&args[2]).canonicalize()?,
        workspace.pm_root().canonicalize()?
    );
    Ok(())
}

/// Listing and linked-test operations have no artifact operand base to resolve.
#[test]
fn sdk_collection_reads_ignore_the_artifact_base() -> TestResult {
    let (directory, workspace) = tracker()?;
    let file = directory.path().join("base.txt");
    std::fs::write(&file, "file fixture")?;
    let listed = CollectionOptions::default();
    for key in ["files", "docs", "tests"] {
        let expected = workspace.collection("sample-one", key, &listed, directory.path())?;
        for unused in [Path::new(""), file.as_path()] {
            assert_eq!(
                workspace.collection("sample-one", key, &listed, unused)?,
                expected
            );
        }
    }
    let tests = CollectionOptions {
        author: "fixture".into(),
        add: vec!["command=fixture".into()],
        ..Default::default()
    };
    assert_eq!(
        workspace.collection("sample-one", "tests", &tests, Path::new(""))?["count"],
        1
    );
    Ok(())
}

/// Removing through a project alias targets the logical link even if its directory is a symlink.
#[cfg(unix)]
#[test]
fn project_alias_removal_preserves_internal_directory_symlinks() -> TestResult {
    let (directory, workspace) = tracker()?;
    let aliases = tempfile::tempdir()?;
    let alias = aliases.path().join("project");
    let direct = aliases.path().join("subdirectory");
    let project = directory.path();
    let vendor = project.join("vendor");
    let shared = project.join("shared");
    std::fs::create_dir(&shared)?;
    std::os::unix::fs::symlink(project, &alias)?;
    std::os::unix::fs::symlink(&shared, &vendor)?;
    std::os::unix::fs::symlink(&shared, &direct)?;
    let add = CollectionOptions {
        author: "fixture".into(),
        add: vec!["vendor/source.rs".into(), "shared/source.rs".into()],
        ..Default::default()
    };
    let remove = CollectionOptions {
        author: "fixture".into(),
        remove: vec!["source.rs".into()],
        full_history: true,
        ..Default::default()
    };
    for (cwd, expected) in [
        (alias.join("vendor"), "shared/source.rs"),
        (vendor, "shared/source.rs"),
        (direct, "vendor/source.rs"),
    ] {
        workspace.collection("sample-one", "files", &add, project)?;
        let receipt = workspace.collection("sample-one", "files", &remove, &cwd)?;
        assert_eq!(receipt["mutation_receipt"]["removed_count"], 1);
        assert_eq!(receipt["files"][0]["path"], expected);
        assert_eq!(receipt["count"], 1);
    }
    Ok(())
}
