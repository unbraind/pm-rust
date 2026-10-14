//! Native identity, attribution and composed-operation contract checks.

use super::*;

/// Native SDK composition supports explicit identity, default identity and idempotent start.
#[test]
fn sdk_lifecycle_succeeds_with_explicit_and_default_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(&root)?;
    fs::write(
        root.join("settings.json"),
        r#"{"governance":{"preset":"strict"}}"#,
    )?;
    let workspace = Workspace::discover(&root)?;
    workspace.create(crate::CreateItem {
        id: "sample-one".to_owned(),
        title: "One".to_owned(),
        description: String::new(),
        item_type: "Task".to_owned(),
        status: "open".to_owned(),
        priority: 2,
        tags: Vec::new(),
        body: String::new(),
        author: "alpha".to_owned(),
        timestamp: Some("2026-08-22T10:00:00Z".to_owned()),
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    })?;
    let mut request = OwnershipItem {
        id: "sample-one".to_owned(),
        author: "alpha".to_owned(),
        timestamp: Some("2026-08-22T10:00:00Z".to_owned()),
        message: None,
        force: false,
        if_available: false,
        provenance_role: None,
    };
    let default = OwnershipOptions::default();
    let explicit = OwnershipOptions {
        identity: Some(AuthorIdentity::resolve(
            Some("alpha"),
            None,
            &BTreeMap::new(),
        )),
        release_bypass: false,
    };
    assert_eq!(
        workspace.start_task(&request, &default)?["changed_field_count"],
        3
    );
    assert_eq!(
        workspace.start_task(&request, &explicit)?["changed_field_count"],
        0
    );
    request.author = "beta".to_owned();
    assert!(matches!(
        workspace.pause_task(&request, &default),
        Err(PmRustError::OwnershipRefusal { .. })
    ));
    request.author = "alpha".to_owned();
    assert_eq!(
        workspace.pause_task(&request, &explicit)?["changed_field_count"],
        3
    );
    assert_eq!(
        workspace.start_task(&request, &explicit)?["status"],
        "in_progress"
    );
    assert_eq!(workspace.pause_task(&request, &default)?["status"], "open");
    assert!(
        !workspace
            .get("sample-one")?
            .metadata
            .extra
            .contains_key("assignee")
    );
    Ok(())
}

/// Session updates cap the active workset, preserve focus, and tolerate lineage cycles.
#[test]
fn semantic_worksets_preserve_focus_and_bound_lineage() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(root.join("tasks"))?;
    fs::write(root.join("settings.json"), "{}")?;
    for (id, parent) in [("sample-one", "sample-two"), ("sample-two", "sample-one")] {
        fs::write(
            root.join(format!("tasks/{id}.toon")),
            format!(
                "id: {id}\ntitle: Work\ndescription: \"\"\ntype: Task\nstatus: open\npriority: 2\ntags: []\ncreated_at: \"2026-08-22T10:00:00Z\"\nupdated_at: \"2026-08-22T10:00:00Z\"\nparent: {parent}\nbody: \"\"\n"
            ),
        )?;
    }
    let workspace = Workspace::discover(&root)?;
    let request = OwnershipItem {
        id: "sample-one".to_owned(),
        author: "alpha".to_owned(),
        timestamp: Some("2026-08-22T10:00:00Z".to_owned()),
        message: None,
        force: false,
        if_available: false,
        provenance_role: None,
    };
    let options = OwnershipOptions::default();
    record_semantics(&workspace, &request, &options, true)?;
    let session = root.join("runtime/session.json");
    let mut state: Value = serde_json::from_slice(&fs::read(&session)?)?;
    let key = format!("author-{}", hex_prefix(b"alpha", 24));
    assert_eq!(
        state["semantic_attribution"][&key]["evidence"],
        json!([
            "claim:sample-one",
            "lineage:sample-one",
            "lineage:sample-two"
        ])
    );
    state["semantic_attribution"][&key]["focused_item_id"] = json!("sample-focus");
    state["semantic_attribution"][&key]["active_item_ids"] = json!(
        (0..65)
            .map(|i| format!("sample-{i:03}"))
            .collect::<Vec<_>>()
    );
    fs::write(&session, crate::stringify_json(&state, false))?;
    record_semantics(&workspace, &request, &options, true)?;
    let state: Value = serde_json::from_slice(&fs::read(&session)?)?;
    let semantic = &state["semantic_attribution"][&key];
    assert_eq!(
        semantic["active_item_ids"].as_array().map(Vec::len),
        Some(64)
    );
    assert_eq!(semantic["evidence"].as_array().map(Vec::len), Some(32));
    assert_eq!(semantic["topic"], "sample-focus");
    assert_eq!(semantic["confidence"], "high");
    record_semantics(&workspace, &request, &options, false)?;
    let mut state: Value = serde_json::from_slice(&fs::read(&session)?)?;
    assert_eq!(
        state["semantic_attribution"][&key]["role"],
        "release-operator"
    );
    state["semantic_attribution"][&key]["active_item_ids"] = json!([]);
    fs::write(&session, crate::stringify_json(&state, false))?;
    record_semantics(&workspace, &request, &options, false)?;
    let mut state: Value = serde_json::from_slice(&fs::read(&session)?)?;
    assert_eq!(state["semantic_attribution"][&key]["topic"], "sample-focus");
    state["semantic_attribution"][&key]
        .as_object_mut()
        .ok_or("missing semantic object")?
        .remove("focused_item_id");
    fs::write(&session, crate::stringify_json(&state, false))?;
    record_semantics(&workspace, &request, &options, false)?;
    let state: Value = serde_json::from_slice(&fs::read(&session)?)?;
    assert!(state.get("semantic_attribution").is_none());
    record_semantics(&workspace, &request, &options, false)?;
    let mut missing = request.clone();
    missing.id = "sample-absent".to_owned();
    assert!(record_semantics(&workspace, &missing, &options, true).is_err());
    Ok(())
}

/// A later history failure preserves the completed pause step and recovery journal.
#[test]
fn pause_failure_preserves_item_and_reports_completed_update()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(&root)?;
    fs::write(root.join("settings.json"), "{}")?;
    let workspace = Workspace::discover(&root)?;
    workspace.create(crate::CreateItem {
        id: "sample-one".to_owned(),
        title: "Ready".to_owned(),
        description: String::new(),
        item_type: "Task".to_owned(),
        status: "open".to_owned(),
        priority: 2,
        tags: vec![],
        body: String::new(),
        author: "alpha".to_owned(),
        timestamp: Some("2026-08-22T10:00:00Z".to_owned()),
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    })?;
    let request = OwnershipItem {
        id: "sample-one".to_owned(),
        author: "alpha".to_owned(),
        timestamp: Some("2026-08-22T10:00:00Z".to_owned()),
        message: None,
        force: false,
        if_available: false,
        provenance_role: None,
    };
    workspace.claim(&request)?;
    let history = root.join("history/sample-one.jsonl");
    fs::remove_file(&history)?;
    fs::create_dir(&history)?;
    let outcome = workspace.pause_task(&request, &OwnershipOptions::default());
    assert!(
        matches!(outcome, Err(PmRustError::CommandRefusal {exit_code:1, context,..}) if context["why"].as_str().is_some_and(|why| why.starts_with("update completed for sample-one")))
    );
    assert_eq!(workspace.get("sample-one")?.metadata.status, "open");
    assert!(
        root.join("runtime/transactions/release-sample-one.json")
            .is_file()
    );
    Ok(())
}

#[test]
fn identity_precedence_and_session_principals_are_stable() {
    let env = BTreeMap::from([
        ("PI_AGENT".to_owned(), "1".to_owned()),
        ("PI_SESSION_ID".to_owned(), "fixture-session".to_owned()),
        ("PI_MODEL".to_owned(), "fixture-model".to_owned()),
    ]);
    let detected = AuthorIdentity::resolve(None, None, &env);
    assert_eq!(detected.author, "harness:pi");
    assert_eq!(detected.principal, "harness:pi#7ab00c683dce32ba62598011");
    let asserted = AuthorIdentity::resolve(Some(" agent "), Some("default-agent"), &env);
    assert_eq!(asserted.author, "agent");
    assert_eq!(asserted.principal, "agent");
    assert_eq!(asserted.source, "asserted");
    let configured = AuthorIdentity::resolve(None, Some("default-agent"), &env);
    assert_eq!(configured.principal, "default-agent");
    assert_eq!(configured.source, "configured");
    assert_eq!(
        AuthorIdentity::resolve(Some(" "), None, &env).source,
        "unknown"
    );
    assert_eq!(
        AuthorIdentity::resolve(None, None, &BTreeMap::new()).author,
        "unknown"
    );
}

#[test]
fn attribution_scopes_restore_after_nested_calls_and_errors() {
    let outer = AuthorIdentity::resolve(Some("outer"), None, &BTreeMap::new());
    let inner = AuthorIdentity::resolve(Some("inner"), None, &BTreeMap::new());
    outer.with(|| {
        assert_eq!(
            ATTRIBUTION.with(|slot| slot
                .borrow()
                .as_ref()
                .map(|identity| identity.author.clone())),
            Some("outer".to_owned())
        );
        let result: Result<(), &str> = inner.with(|| Err("synthetic failure"));
        assert_eq!(result, Err("synthetic failure"));
        assert_eq!(
            ATTRIBUTION.with(|slot| slot
                .borrow()
                .as_ref()
                .map(|identity| identity.author.clone())),
            Some("outer".to_owned())
        );
    });
    assert!(ATTRIBUTION.with(|slot| slot.borrow().is_none()));
}

/// Harness precedence, overrides and configured identities retain their sources.
#[test]
fn all_harnesses_and_provenance_overrides_resolve_independently() {
    for (signal, harness, model, session) in [
        (
            "CLAUDECODE",
            "claude-code",
            "ANTHROPIC_MODEL",
            "CLAUDE_CODE_SESSION_ID",
        ),
        ("CODEX_CI", "codex", "CODEX_MODEL", "CODEX_THREAD_ID"),
        ("PI_AGENT", "pi", "PI_MODEL", "PI_SESSION_ID"),
        (
            "OPENCODE",
            "opencode",
            "OPENCODE_MODEL",
            "OPENCODE_SESSION_ID",
        ),
        ("CURSOR_AGENT", "cursor", "CURSOR_MODEL", "CURSOR_TRACE_ID"),
        ("AIDER", "aider", "AIDER_MODEL", "AIDER_SESSION_ID"),
        (
            "GEMINI_CLI",
            "gemini-cli",
            "GEMINI_MODEL",
            "GEMINI_CLI_SESSION_ID",
        ),
    ] {
        let env = BTreeMap::from([
            (signal.to_owned(), "1".to_owned()),
            (model.to_owned(), "fixture-model".to_owned()),
            (session.to_owned(), "fixture-session".to_owned()),
            ("CODEX_REASONING_EFFORT".to_owned(), "high".to_owned()),
            ("CLAUDE_EFFORT".to_owned(), "high".to_owned()),
            ("CODEX_SESSION_ROLE".to_owned(), "reviewer".to_owned()),
            ("PM_AGENT_TOPIC".to_owned(), "fixture-topic".to_owned()),
            ("PM_AGENT_VERSION".to_owned(), "fixture-version".to_owned()),
        ]);
        let identity = AuthorIdentity::resolve(None, None, &env);
        assert_eq!(identity.harness.as_deref(), Some(harness));
        assert_eq!(identity.provenance["model"]["source"], "environment");
        assert_eq!(
            identity.provenance["topic"],
            json!({"value":"fixture-topic", "source":"override"})
        );
        assert_eq!(identity.source, "detected");
        assert!(
            identity
                .principal
                .starts_with(&format!("harness:{harness}#"))
        );
    }
    let env = BTreeMap::from([
        ("CI".to_owned(), "1".to_owned()),
        ("PM_AUTHOR".to_owned(), "asserted-agent".to_owned()),
        ("PM_AGENT_MODEL".to_owned(), "override-model".to_owned()),
    ]);
    let identity = AuthorIdentity::resolve(None, Some("configured-agent"), &env);
    assert_eq!(identity.author, "asserted-agent");
    assert_eq!(identity.principal, "asserted-agent");
    assert_eq!(identity.harness.as_deref(), Some("ci"));
    assert_eq!(identity.provenance["model"]["source"], "override");
    let detected = AuthorIdentity::resolve(
        None,
        None,
        &BTreeMap::from([("CI".to_owned(), "1".to_owned())]),
    );
    assert_eq!(detected.principal, "harness:ci");
    assert!(detected.instance.is_none());
}

/// Persisted semantics fill only missing dimensions for the matching session.
#[test]
fn session_semantics_preserve_explicit_dimensions() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let mut identity = AuthorIdentity::resolve(
        None,
        None,
        &BTreeMap::from([
            ("PI_AGENT".to_owned(), "1".to_owned()),
            ("PI_SESSION_ID".to_owned(), "fixture-session".to_owned()),
        ]),
    );
    let instance = identity.instance.clone().ok_or("missing instance")?;
    fs::create_dir_all(directory.path().join("runtime"))?;
    fs::write(
        directory.path().join("runtime/session.json"),
        crate::stringify_json(
            &json!({"semantic_attribution": {instance.clone(): {"role":"reviewer", "topic":"fixture-topic", "confidence":"high", "rule_version":"v2", "evidence":["focus:sample-own"]}}}),
            false,
        ),
    )?;
    let inferred = identity.with_semantics(directory.path());
    assert_eq!(inferred.provenance["role"]["source"], "inferred");
    assert_eq!(inferred.provenance["topic"]["value"], "fixture-topic");
    identity.provenance.insert(
        "role".to_owned(),
        json!({"value":"planner", "source":"override"}),
    );
    assert_eq!(
        identity.with_semantics(directory.path()).provenance["role"]["value"],
        "planner"
    );
    let unknown = AuthorIdentity::resolve(None, None, &BTreeMap::new());
    assert_eq!(unknown.with_semantics(directory.path()), unknown);
    fs::write(directory.path().join("runtime/session.json"), "{}")?;
    assert_eq!(identity.with_semantics(directory.path()), identity);
    fs::write(
        directory.path().join("runtime/session.json"),
        crate::stringify_json(&json!({"semantic_attribution": {instance: null}}), false),
    )?;
    assert_eq!(identity.with_semantics(directory.path()), identity);
    Ok(())
}

/// Session suffixes distinguish detected session keys from ordinary author keys.
#[test]
fn principal_suffixes_choose_session_or_author_semantic_keys()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(&root)?;
    fs::write(root.join("settings.json"), "{}")?;
    let workspace = Workspace::discover(&root)?;
    workspace.create(crate::CreateItem {
        id: "sample-one".to_owned(),
        title: "One".to_owned(),
        description: String::new(),
        item_type: "Task".to_owned(),
        status: "open".to_owned(),
        priority: 2,
        tags: Vec::new(),
        body: String::new(),
        author: "alpha".to_owned(),
        timestamp: Some("2026-08-22T10:00:00Z".to_owned()),
        message: None,
        provenance_role: None,
        force_stale_lock: false,
    })?;
    for (author, session_key) in [
        ("alpha#short", None),
        ("alpha#zzzzzzzzzzzzzzzzzzzzzzzz", None),
        (
            "alpha#0123456789abcdef01234567",
            Some("0123456789abcdef01234567"),
        ),
    ] {
        let request = OwnershipItem {
            id: "sample-one".to_owned(),
            author: author.to_owned(),
            timestamp: Some("2026-08-22T10:00:00Z".to_owned()),
            message: None,
            force: false,
            if_available: false,
            provenance_role: None,
        };
        workspace.claim(&request)?;
        let state: Value = serde_json::from_slice(&fs::read(root.join("runtime/session.json"))?)?;
        let key = session_key.map_or_else(
            || format!("author-{}", hex_prefix(author.as_bytes(), 24)),
            str::to_owned,
        );
        assert!(state["semantic_attribution"].get(&key).is_some());
        workspace.release(&request)?;
    }
    Ok(())
}

/// Partial receipts retain the original error category and recovery instructions.
#[test]
fn partial_failures_keep_policy_context_and_persisted_step() {
    for (error, exit) in [
        (
            PmRustError::OwnershipRefusal {
                detail: "conflict".to_owned(),
                context: json!({"code":"ownership_conflict", "why":"Original reason.", "next_steps":["Original step."]}),
            },
            4,
        ),
        (
            PmRustError::CommandRefusal {
                detail: "invalid".to_owned(),
                exit_code: 2,
                context: json!({}),
            },
            2,
        ),
        (
            PmRustError::InvalidReadRequest {
                reason: "invalid storage".to_owned(),
            },
            1,
        ),
    ] {
        let result = partial_failure(error, "sample-own", "update");
        let PmRustError::CommandRefusal {
            exit_code, context, ..
        } = result
        else {
            unreachable!()
        };
        assert_eq!(exit_code, exit);
        assert_eq!(context["item_id"], "sample-own");
        assert!(
            context["why"]
                .as_str()
                .is_some_and(|why| why.starts_with("update completed for sample-own"))
        );
        assert!(
            context["next_steps"][0]
                .as_str()
                .is_some_and(|step| step.contains("pm history sample-own"))
        );
        if exit == 4 {
            assert_eq!(context["next_steps"][1], "Original step.");
            assert!(
                context["why"]
                    .as_str()
                    .is_some_and(|why| why.ends_with("Original reason."))
            );
        }
    }
}

/// Workset topics preserve short IDs and bound large ASCII worksets with a digest.
#[test]
fn workset_topics_are_bounded_and_stable() {
    assert_eq!(workset_topic(&["sample-one".to_owned()]), "sample-one");
    assert_eq!(
        workset_topic(&["sample-one".to_owned(), "sample-two".to_owned()]),
        "workset:sample-one+sample-two"
    );
    let ids = vec!["a".repeat(128), "b".repeat(128)];
    let first = workset_topic(&ids);
    assert_eq!(first.len(), 251);
    assert_eq!(first, workset_topic(&ids));
}

/// External schema files override embedded defaults and invalid objects fail closed.
#[test]
fn external_runtime_schema_precedence_and_errors() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    fs::write(
        root.join("settings.json"),
        crate::stringify_json(
            &json!({"schema":{"workflow":{"open_status":"open"}, "files":{"statuses":"statuses.json", "workflows":"workflows.json"}}}),
            false,
        ),
    )?;
    fs::write(
        root.join("statuses.json"),
        r#"{"statuses":[{"id":"ready","roles":["active"]}]}"#,
    )?;
    fs::write(
        root.join("workflows.json"),
        r#"{"workflow":{"open_status":"ready"},"type_workflows":[]}"#,
    )?;
    let settings = runtime_settings(root)?;
    assert_eq!(settings["schema"]["workflow"]["open_status"], "ready");
    assert_eq!(settings["schema"]["statuses"][0]["id"], "ready");
    fs::write(root.join("workflows.json"), "{}")?;
    assert_eq!(
        runtime_settings(root)?["schema"]["workflow"]["open_status"],
        "open"
    );
    for invalid in ["[]", "invalid"] {
        fs::write(root.join("workflows.json"), invalid)?;
        assert!(matches!(
            runtime_settings(root),
            Err(PmRustError::InvalidReadRequest { .. })
        ));
    }
    fs::remove_file(root.join("workflows.json"))?;
    assert!(matches!(
        runtime_settings(root),
        Err(PmRustError::Io { .. })
    ));
    fs::remove_file(root.join("settings.json"))?;
    assert!(matches!(
        runtime_settings(root),
        Err(PmRustError::Io { .. })
    ));
    Ok(())
}
