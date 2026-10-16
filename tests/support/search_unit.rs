//! Search corpus, configuration, cursor and failure-boundary contracts.
use super::*;
type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Failed settings, schema and item reads never turn into successful empty searches.
#[test]
fn search_storage_failures_remain_errors() -> TestResult {
    let (_directory, workspace) = fixture(&json!({}))?;
    let settings = workspace.pm_root().join("settings.json");
    std::fs::write(&settings, "invalid")?;
    assert!(
        workspace
            .search("needle", &SearchOptions::default())
            .is_err()
    );
    std::fs::write(&settings, "{}")?;
    let schema = workspace.pm_root().join("schema/statuses.json");
    std::fs::create_dir_all(&schema)?;
    assert!(
        workspace
            .search("needle", &SearchOptions::default())
            .is_err()
    );
    std::fs::remove_dir_all(schema)?;
    assert!(
        workspace
            .search(
                "needle",
                &SearchOptions {
                    output: ListOptions {
                        limit: Some("invalid".to_owned()),
                        ..ListOptions::default()
                    },
                    ..SearchOptions::default()
                }
            )
            .is_err()
    );
    std::fs::write(workspace.pm_root().join("tasks/broken.toon"), "invalid")?;
    assert!(
        workspace
            .search("needle", &SearchOptions::default())
            .is_err()
    );
    Ok(())
}

/// Optional corpus rows and empty controls retain the oracle's normalization boundaries.
#[test]
#[allow(clippy::too_many_lines)]
fn optional_and_malformed_search_rows() -> TestResult {
    let (_directory, workspace) = fixture(
        &json!({"item_types":{"definitions":[{}, {"name":"Task","aliases":[null,"work"]}]}}),
    )?;
    assert_eq!(
        workspace.search("needle\u{0085}unknown", &SearchOptions::default())?["count"],
        0
    );
    let mut document = workspace.get("sample-a")?;
    document.metadata.extra["plan_steps"] = json!([
        {"id":"minimal","order":2,"title":"UniqueSecond","status":"pending","created_at":"fixture","updated_at":"fixture"},
        {"id":"first","order":1,"title":"UniqueFirst","status":"pending","created_at":"fixture","updated_at":"fixture"},
        {"title":"malformed-plan"},
        {"id":"bad-order","title":"malformed-plan","status":"pending","created_at":"fixture","updated_at":"fixture"},
        {"id":"bad-status","order":3,"title":"malformed-plan","status":"wrong","created_at":"fixture","updated_at":"fixture"}
    ]);
    std::fs::write(
        workspace.pm_root().join("tasks/sample-a.toon"),
        toon_format::encode_default(&document)?,
    )?;
    let text = plan(&document.metadata);
    assert!(text.find("UniqueFirst") < text.find("UniqueSecond"));
    assert_eq!(
        workspace.search("malformed-plan", &SearchOptions::default())?["count"],
        0
    );
    assert_eq!(
        workspace.search("UniqueSecond", &SearchOptions::default())?["count"],
        1
    );
    assert_eq!(
        workspace.search("plan-only scope-only", &SearchOptions::default())?["count"],
        3
    );
    assert_eq!(events(&json!([{"title":"Work","all_day":false}])), "Work");
    for query in ["needle unknown:value", "needle tag:"] {
        assert_eq!(
            workspace.search(query, &SearchOptions::default())?["count"],
            3
        );
    }
    assert_eq!(
        workspace.search(
            "needle",
            &SearchOptions {
                status: Some(String::new()),
                ..SearchOptions::default()
            }
        )?["count"],
        3
    );
    assert_eq!(
        workspace.search(
            "needle",
            &SearchOptions {
                output: ListOptions {
                    limit: Some("0".to_owned()),
                    ..ListOptions::default()
                },
                ..SearchOptions::default()
            }
        )?["count"],
        0
    );
    for options in [
        SearchOptions {
            output: ListOptions {
                output_budget: Some("0".to_owned()),
                ..ListOptions::default()
            },
            ..SearchOptions::default()
        },
        SearchOptions {
            output: ListOptions {
                output_limit: Some("invalid".to_owned()),
                ..ListOptions::default()
            },
            ..SearchOptions::default()
        },
        SearchOptions {
            item_type: Some(", ".to_owned()),
            ..SearchOptions::default()
        },
        SearchOptions {
            tag: Some(String::new()),
            ..SearchOptions::default()
        },
    ] {
        assert!(workspace.search("needle", &options).is_err());
    }
    assert!(
        workspace
            .search("tag:CAFÉ", &SearchOptions::default())
            .is_err()
    );
    assert_eq!(
        workspace.search(
            "needle",
            &SearchOptions {
                item_type: Some("work".to_owned()),
                ..SearchOptions::default()
            }
        )?["count"],
        3
    );
    for (maximum, count, applied) in [(0.0, 3, 50.0), (1.9, 1, 1.0), (0.5, 0, 0.0)] {
        let (_directory, workspace) = fixture(&json!({"search":{"max_results":maximum}}))?;
        let result = workspace.search("needle", &SearchOptions::default())?;
        assert_eq!(result["count"], count);
        assert_eq!(result["applied_limit"], applied);
    }
    Ok(())
}

/// Creates a real tracker with a deliberately rich searchable document.
fn fixture(settings: &Value) -> Result<(tempfile::TempDir, Workspace), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    std::fs::create_dir_all(root.join("tasks"))?;
    std::fs::write(root.join("settings.json"), settings.to_string())?;
    for (id, title, status, priority) in [
        ("sample-a", "Title needle", "open", 1),
        ("sample-b", "Title needle", "open", 1),
        ("sample-c", "Title needle", "closed", 0),
    ] {
        let item = json!({"id":id,"title":title,"description":"description-only","type":"Task","status":status,"priority":priority,"tags":["CAFÉ"],"created_at":"2026-08-22T10:00:00.000Z","updated_at":"2026-08-22T10:00:00.000Z","body":"body-only","comments":[{"text":"comments-only"}],"notes":[{"text":"notes-only"}],"learnings":[{"text":"learnings-only"}],"reminders":[{"at":"2026-08-22T10:00:00.000Z","text":"reminders-only"}],"events":[{"title":"events-only","description":"event-description","all_day":true,"timezone":"UTC","recurrence":{"freq":"weekly","interval":2,"by_weekday":["mon"],"by_month_day":[1]}}],"dependencies":[{"id":"dependency-only","kind":"related"}],"plan_mode":"plan-only","plan_scope":"scope-only","plan_harness":"harness-only","plan_resume_context":"resume-only","plan_steps":[{"id":"step-1","order":1,"created_at":"fixture","updated_at":"fixture","title":"step-only","status":"pending","body":"step-body","owner":"owner-only","evidence":"evidence-only","blocked_reason":"blocked-only","superseded_by":"superseded-only","linked_items":[{"id":"linked-item","kind":"related","note":"linked-note"}],"files":[{"path":"plan-file","note":"file-note"}],"tests":[{"command":"plan-test","path":"test-path","note":"test-note"}],"docs":[{"path":"plan-doc","note":"doc-note"}]}],"plan_decisions":[{"decision":"decision-only","rationale":"rationale-only","evidence":"decision-evidence"}],"plan_discoveries":[{"text":"discovery-only"}],"plan_validation":[{"text":"validation-only","command":"validation-command","expected":"validation-expected"}]});
        std::fs::write(
            root.join("tasks").join(format!("{id}.toon")),
            format!("{}\n", toon_format::encode_default(&item)?),
        )?;
    }
    Ok((directory, Workspace::discover(&root)?))
}

/// Every advertised corpus field participates while literal phrase normalization stays stable.
#[test]
fn corpus_fields_and_weight_configuration() -> TestResult {
    let (_directory, workspace) = fixture(&json!({"id_prefix":"sample-"}))?;
    for query in [
        "description-only",
        "body-only",
        "comments-only",
        "notes-only",
        "learnings-only",
        "reminders-only",
        "events-only",
        "all day",
        "weekly",
        "dependency-only",
        "plan-only",
        "step-only",
        "decision-only",
        "discovery-only",
        "validation-only",
    ] {
        assert_eq!(
            workspace.search(query, &SearchOptions::default())?["count"],
            3,
            "{query}"
        );
    }
    assert_eq!(
        phrase("\u{feff}CAfé\u{feff}One\u{a0}Two\u{0085}"),
        "café one two\u{0085}"
    );
    assert_eq!(
        workspace.search("sample-a", &SearchOptions::default())?["items"][0]["score"],
        1000.0
    );
    assert_eq!(
        workspace.search("a", &SearchOptions::default())?["items"][0]["score"],
        900.0
    );
    assert_eq!(
        workspace.search("title needle", &SearchOptions::default())?["items"][0]["id"],
        "sample-a"
    );
    let (_directory, workspace) = fixture(
        &json!({"id_prefix":"sample-","search":{"max_results":1,"score_threshold":99999,"tuning":{"title_exact_bonus":-1,"title_weight":"invalid"}}}),
    )?;
    assert_eq!(
        workspace.search("needle", &SearchOptions::default())?["count"],
        0
    );
    assert_eq!(
        workspace.search("sample-a", &SearchOptions::default())?["count"],
        1
    );
    let document: ItemDocument = serde_json::from_value(
        json!({"id":"sample-z","title":"one one two","type":"Task","status":"open","priority":1,"created_at":"x","updated_at":"x"}),
    )?;
    let hit = score(
        document.clone(),
        &["one", "one", "two"],
        "one one two",
        &json!({}),
    )
    .ok_or("expected hit")?;
    assert_eq!(hit.fields, vec!["title"]);
    assert!(score(document, &["missing"], "missing", &json!({})).is_none());
    Ok(())
}

/// Real corpus selection honors configured type aliases and strict status boundaries.
#[test]
fn search_filters_and_invalid_inputs() -> TestResult {
    let (_directory, workspace) =
        fixture(&json!({"item_types":{"definitions":[{"name":"Task","aliases":["work"]}]}}))?;
    assert_eq!(
        workspace.search(
            "needle",
            &SearchOptions {
                item_type: Some("work".to_owned()),
                tag: Some("café".to_owned()),
                ..SearchOptions::default()
            }
        )?["count"],
        3
    );
    for (query, options) in [
        (
            "needle",
            SearchOptions {
                status: Some("Unknown".to_owned()),
                ..SearchOptions::default()
            },
        ),
        (
            "needle",
            SearchOptions {
                status: Some("all,open".to_owned()),
                ..SearchOptions::default()
            },
        ),
        (
            "needle",
            SearchOptions {
                item_type: Some("Unknown".to_owned()),
                ..SearchOptions::default()
            },
        ),
        ("", SearchOptions::default()),
    ] {
        assert!(workspace.search(query, &options).is_err());
    }
    for (query, options, count) in [
        ("needle status:open", SearchOptions::default(), 2),
        ("needle type:Feature", SearchOptions::default(), 0),
        (
            "needle tag:CAFÉ",
            SearchOptions {
                tag: Some("absent".to_owned()),
                ..SearchOptions::default()
            },
            0,
        ),
        ("needle tag:CAFÉ tag:absent", SearchOptions::default(), 3),
        (
            "needle",
            SearchOptions {
                status: Some(" ALL ".to_owned()),
                ..SearchOptions::default()
            },
            3,
        ),
    ] {
        assert_eq!(workspace.search(query, &options)?["count"], count);
    }
    for raw in ["-1", "NaN", "Infinity", "bad"] {
        assert!(limit(raw).is_err());
    }
    assert_eq!(limit("")?, (0, 0.0));
    assert_eq!(limit("2.9")?, (2, 2.0));
    for raw in ["0x10", "0X10", "0b10000", "0B10000", "0o20", "0O20"] {
        assert_eq!(limit(raw)?, (16, 16.0));
    }
    assert!(limit("0xQ").is_err());
    for (raw, expected) in [
        ("0x0", 0.0),
        ("0x100000000000000000000000000000000", 2.0_f64.powi(128)),
        ("0x20000000000001", 9_007_199_254_740_992.0),
        ("0x20000000000003", 9_007_199_254_740_996.0),
        ("0x40000000000003", 18_014_398_509_481_988.0),
        ("0x40000000000001", 18_014_398_509_481_984.0),
    ] {
        assert_eq!(limit(raw)?.1.to_bits(), expected.to_bits());
    }
    assert!(limit("0x").is_err());
    assert!(limit(&format!("0x1{}", "0".repeat(256))).is_err());
    assert_eq!(limit("-0")?, (0, 0.0));
    assert_eq!(limit("1e100")?, (usize::MAX, 1e100));
    Ok(())
}

/// Producer continuations preserve fallback positions and reject stale query or snapshot claims.
#[test]
fn search_continuation_boundaries() -> TestResult {
    let (_directory, workspace) = fixture(&json!({}))?;
    let mut options = SearchOptions {
        output: ListOptions {
            limit: Some("1".to_owned()),
            ..ListOptions::default()
        },
        ..SearchOptions::default()
    };
    let first = workspace.search("needle", &options)?;
    let raw = first["next_cursor"].as_str().ok_or("missing cursor")?;
    options.output.after = Some(raw.to_owned());
    let second = workspace.search("needle", &options)?;
    assert_eq!(second["items"][0]["id"], "sample-b");
    let mut cursor = crate::pagination::query(
        raw,
        second["continuation_contract"]["fingerprint"]
            .as_str()
            .ok_or("missing fingerprint")?,
    )?;
    cursor
        .as_object_mut()
        .map(|map| map.shift_remove("after_index"));
    options.output.after = Some(crate::pagination::encode(&cursor));
    assert_eq!(
        workspace.search("needle", &options)?["continuation_contract"]["fingerprint"],
        "opaque_cursor"
    );
    cursor["after_id"] = json!("absent");
    options.output.after = Some(crate::pagination::encode(&cursor));
    assert!(workspace.search("needle", &options).is_err());
    cursor["after_index"] = json!(1);
    options.output.after = Some(crate::pagination::encode(&cursor));
    assert_eq!(
        workspace.search("needle", &options)?["items"][0]["id"],
        "sample-b"
    );
    cursor["snapshot"] = json!("foreign");
    options.output.after = Some(crate::pagination::encode(&cursor));
    assert!(workspace.search("needle", &options).is_err());
    options.output.after = Some("!".to_owned());
    assert!(workspace.search("needle", &options).is_err());
    Ok(())
}
