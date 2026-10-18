//! Focus ranking, packing, lifecycle and agenda boundary acceptance.
use super::*;
use std::fs;
const NOW: &str = "2026-08-22T10:00:00.000Z";
/// Creates a minimal real tracker and validated lifecycle registry.
fn workspace() -> Result<(tempfile::TempDir, Workspace, Lifecycle), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let pm = root.path().join(".agents/pm");
    fs::create_dir_all(pm.join("tasks"))?;
    fs::create_dir(pm.join("history"))?;
    fs::write(
        pm.join("settings.json"),
        r#"{"id_prefix":"sample-","item_format":"toon","author_default":"fixture"}"#,
    )?;
    let workspace = Workspace::discover(root.path())?;
    let lifecycle = Lifecycle::load(workspace.pm_root())?;
    Ok((root, workspace, lifecycle))
}
/// Builds metadata with controlled signal and lifecycle inputs.
fn item(id: &str, status: &str) -> Value {
    json!({"id":id,"title":id,"description":"","type":"Task","status":status,"priority":2,"tags":[],"created_at":NOW,"updated_at":NOW})
}
/// Structural ordering handles every lifecycle bucket and optional tiebreak.
#[test]
fn structural_and_signal_ordering() -> Result<(), Box<dyn std::error::Error>> {
    let (_root, workspace, lifecycle) = workspace()?;
    for (status, rank) in [
        ("in_progress", 0),
        ("open", 1),
        ("blocked", 2),
        ("draft", 3),
        ("closed", 7),
        ("unknown", 6),
    ] {
        assert_eq!(status_rank(&item("sample", status), &lifecycle), rank);
    }
    assert_eq!(
        optional(&Value::Null, &Value::Null, false),
        std::cmp::Ordering::Equal
    );
    assert!(optional(&Value::Null, &json!(1), false).is_gt());
    assert!(optional(&json!(1), &Value::Null, false).is_lt());
    assert!(optional(&json!(1), &json!(2), false).is_lt());
    assert!(optional(&json!(NOW), &json!("2026-08-23T10:00:00.000Z"), true).is_lt());
    let mut items = Vec::new();
    for (i, risk) in ["critical", "high", "medium", "low", "unknown"]
        .iter()
        .enumerate()
    {
        let mut row = item(
            &format!("sample-{i}"),
            if i == 0 { "in_progress" } else { "open" },
        );
        row["risk"] = json!(risk);
        row["assignee"] = json!(if i == 1 { "fixture" } else { "other" });
        row["deadline"] = json!(if i % 2 == 0 {
            "2026-08-21T10:00:00.000Z"
        } else {
            "2026-08-23T10:00:00.000Z"
        });
        row["parent"] = json!("sample-0");
        row["comments"] = json!([{"text":"evidence"}]);
        row["dependencies"] =
            json!([{"id":"sample-external","kind":"related"},{"id":"","kind":"related"}]);
        items.push(row);
    }
    assert_eq!(
        ranked(
            &workspace,
            &items,
            NOW,
            &lifecycle,
            "fixture",
            &Affinity::default()
        )?
        .len(),
        5
    );
    assert_eq!(
        ranked(&workspace, &[], NOW, &lifecycle, "", &Affinity::default())?.len(),
        0
    );
    assert_eq!(
        ranked(
            &workspace,
            &[item("sample-one", "open")],
            NOW,
            &lifecycle,
            "",
            &Affinity::default()
        )?
        .len(),
        1
    );
    assert_eq!(pack(&[], 100, &BTreeSet::new()).len(), 0);
    assert_eq!(
        pack(
            &items
                .iter()
                .cloned()
                .map(|item| (item, 0.5))
                .collect::<Vec<_>>(),
            1,
            &BTreeSet::new()
        )
        .len(),
        0
    );
    assert_eq!(
        pack(
            &items
                .into_iter()
                .map(|item| (item, 0.5))
                .collect::<Vec<_>>(),
            1000,
            &BTreeSet::new()
        )
        .len(),
        5
    );
    assert_eq!(utc_day("2026-08-22T23:30:00.000-02:00"), "2026-08-23");
    assert_eq!(utc_day("invalid"), "");
    Ok(())
}
/// History recency distinguishes substantive events and falls back to release cohorts.
#[test]
fn recency_evidence() -> Result<(), Box<dyn std::error::Error>> {
    let (_root, workspace, _) = workspace()?;
    let mut row = item("sample-one", "open");
    assert_eq!(recency(&workspace, &row)?, NOW);
    row["release"] = json!("2026.7.1-beta");
    assert_eq!(recency(&workspace, &row)?, "2026-07-01T00:00:00.000Z");
    row["release"] = json!("invalid");
    assert_eq!(recency(&workspace, &row)?, NOW);
    fs::write(
        workspace.pm_root().join("history/sample-one.jsonl"),
        format!(
            "{}\n{}\n{}\n",
            json!({"ts":"2026-08-23T00:00:00.000Z","op":"create","event_class":"substantive"}),
            json!({"ts":"2026-08-24T00:00:00.000Z","op":"update","event_class":"maintenance"}),
            json!({"ts":"2026-08-25T00:00:00.000Z","op":"update","patch":[{"op":"replace","path":"/metadata/updated_at","value":NOW}]})
        ),
    )?;
    assert_eq!(recency(&workspace, &row)?, "2026-08-23T00:00:00.000Z");
    Ok(())
}
/// Forward and inverse blockers preserve canonical identity and terminal resolution.
#[test]
fn blocked_focus_and_descendants() -> Result<(), Box<dyn std::error::Error>> {
    let (_root, _workspace, lifecycle) = workspace()?;
    let mut a = item("sample-a", "open");
    let mut b = item("sample-b", "closed");
    let mut c = item("sample-c", "open");
    a["blocked_by"] = json!(" sample-b ");
    a["dependencies"] =
        json!([{"id":"missing","kind":"blocked_by"},{"id":"sample-b","kind":"related"}]);
    c["dependencies"] = json!([{"id":"sample-a","kind":"blocks"}]);
    assert_eq!(
        blockers(&a, &[a.clone(), b.clone(), c.clone()], &lifecycle),
        vec!["missing", "sample-c"]
    );
    b["parent"] = json!("sample-a");
    c["parent"] = json!("sample-b");
    assert!(container(
        "sample-a",
        &[a.clone(), b.clone(), c.clone()],
        &lifecycle
    ));
    c["status"] = json!("closed");
    a["parent"] = json!("sample-c");
    a["status"] = json!("closed");
    assert!(!container(
        "sample-a",
        &[a.clone(), b.clone(), c.clone()],
        &lifecycle
    ));
    assert!(!container("missing", &[a.clone()], &lifecycle));
    a["blocked_by"] = json!("no-active-blocker");
    let marked = BTreeSet::from(["sample-a".to_owned()]);
    let focused = focus(&a, &[a.clone()], &marked, &lifecycle);
    assert_eq!(focused["blocked"], true);
    assert_eq!(
        focus(&a, &[a.clone()], &BTreeSet::new(), &lifecycle).get("blocked"),
        None
    );
    Ok(())
}
/// Default agenda caps before terminal filtering and emits compact focus references.
#[test]
fn future_agenda_rows() -> Result<(), Box<dyn std::error::Error>> {
    let (_root, _workspace, lifecycle) = workspace()?;
    let mut a = item("sample-a", "open");
    a["deadline"] = json!("2026-08-23T10:00:00.000Z");
    a["reminders"] = json!([{"at":"2026-08-21T10:00:00.000Z","text":"past"},{"at":"2026-08-23T10:00:00.000Z","text":"future"}]);
    a["events"] = json!([{"start_at":"2026-08-23T10:00:00.000Z","title":"Scheduled","all_day":false,"timezone":"UTC"}]);
    let mut closed = a.clone();
    closed["id"] = json!("sample-closed");
    closed["status"] = json!("closed");
    for ids in [BTreeSet::new(), BTreeSet::from(["sample-a".to_owned()])] {
        let (summary, rows) = agenda(&[a.clone(), closed.clone()], &ids, NOW, 10, &lifecycle);
        assert_eq!(rows.len(), 3);
        assert_eq!(summary["scheduled"], 1);
        assert_eq!(summary["deadlines"], 1);
        assert_eq!(summary["reminders"], 1);
    }
    assert_eq!(
        agenda(&[a], &BTreeSet::new(), NOW, 0, &lifecycle).1.len(),
        0
    );
    Ok(())
}

/// Brief contexts preserve read-only storage and explicitly reject deferred data boundaries.
#[test]
fn context_api_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let (_root, workspace, _lifecycle) = workspace()?;
    for options in [
        ContextOptions::default(),
        ContextOptions {
            no_extension_health: true,
            limit: Some("0".to_owned()),
            ..Default::default()
        },
        ContextOptions {
            no_tags: true,
            limit: Some("1".to_owned()),
            token_budget: Some("1600".to_owned()),
            ..Default::default()
        },
    ] {
        assert_eq!(workspace.context(&options, NOW)?["depth"], "brief");
    }
    for (depth, limit, budget) in [
        ("bad", None, None),
        ("standard", None, None),
        ("deep", None, None),
        ("full", None, None),
        ("brief", Some("-1"), None),
        ("brief", None, Some("0")),
    ] {
        assert!(
            workspace
                .context(
                    &ContextOptions {
                        depth: Some(depth.to_owned()),
                        limit: limit.map(ToString::to_string),
                        token_budget: budget.map(ToString::to_string),
                        no_extension_health: true,
                        ..Default::default()
                    },
                    NOW
                )
                .is_err()
        );
    }
    let mut blocked = item("sample-blocked", "blocked");
    blocked["blocked_by"] = json!("missing");
    fs::write(
        workspace.pm_root().join("tasks/sample-blocked.toon"),
        toon_format::encode_default(&blocked)?,
    )?;
    let result = workspace.context(
        &ContextOptions {
            no_extension_health: true,
            ..Default::default()
        },
        NOW,
    )?;
    assert_eq!(result["blocked_fallback"].as_array().map(Vec::len), Some(1));
    assert_eq!(result["summary"]["returned_focus"]["blocked"], 1);
    blocked["events"] = json!([{"start_at":NOW,"recurrence":{"freq":"daily"}}]);
    fs::write(
        workspace.pm_root().join("tasks/sample-blocked.toon"),
        toon_format::encode_default(&blocked)?,
    )?;
    assert!(workspace.context(&ContextOptions::default(), NOW).is_err());
    fs::remove_file(workspace.pm_root().join("tasks/sample-blocked.toon"))?;
    let runtime = workspace.pm_root().join("runtime");
    fs::create_dir_all(&runtime)?;
    for (name, value) in [
        ("context-signals-context.json", json!({"format_version":99})),
        (
            "context-signals-context.json",
            json!({"format_version":3,"signal_set_version":2,"items":[{"id":"missing"}]}),
        ),
    ] {
        fs::write(runtime.join(name), serde_json::to_vec(&value)?)?;
        assert!(context_state_boundary(&workspace, &[]).is_err());
        fs::remove_file(runtime.join(name))?;
    }
    fs::write(runtime.join("context-signals-context.json"), "not JSON")?;
    assert!(context_state_boundary(&workspace, &[]).is_err());
    fs::remove_file(runtime.join("context-signals-context.json"))?;
    let extensions = workspace.pm_root().join("extensions");
    fs::create_dir_all(extensions.join("good"))?;
    fs::write(
        extensions.join("good/manifest.json"),
        r#"{"name":"fixture-extension","version":"1.0.0","entry":"entry.js"}"#,
    )?;
    fs::write(extensions.join("good/entry.js"), "export {};")?;
    assert!(extension_health(&workspace).is_some());
    fs::create_dir(extensions.join("bad"))?;
    fs::write(extensions.join("file"), "not a directory")?;
    assert!(extension_health(&workspace).is_some());
    Ok(())
}

/// Trusted feedback and semantic claims affect ranking and required identity admission.
#[test]
fn session_and_usage_affinity() -> Result<(), Box<dyn std::error::Error>> {
    let (_root, workspace, lifecycle) = workspace()?;
    let identity = crate::AuthorIdentity::resolve(None, Some("fixture"), &BTreeMap::new());
    let runtime = workspace.pm_root().join("runtime");
    fs::create_dir_all(&runtime)?;
    assert!(affinity(&workspace, &identity, NOW).claims.is_empty());
    let key = format!(
        "author-{}",
        &crate::history::sha256_digest(b"fixture")[..24]
    );
    let session = json!({"semantic_attribution":{key:{"role":"implementer","confidence":"high","topic":"fixture","rule_version":"v2","active_item_ids":["sample-a"],"focused_item_id":"sample-b","evidence":["lineage:sample-parent","other"]}}});
    fs::write(runtime.join("session.json"), serde_json::to_vec(&session)?)?;
    let events = [
        json!({"kind":"serve","schema_version":2,"serve_id":"serve-one","author":"fixture","at":"2026-08-21T10:00:00.000Z","rows":[{"id":"sample-a"},{"id":"not-delivered"}]}),
        json!({"kind":"delivery","schema_version":2,"serve_id":"serve-one","author":"fixture","at":"2026-08-21T10:00:00.000Z","delivered_item_ids":["sample-a"]}),
        json!({"kind":"touch","author":"fixture","item_id":"sample-a","at":"2026-08-21T11:00:00.000Z"}),
        json!({"kind":"serve","schema_version":2,"serve_id":"unconfirmed","author":"fixture","at":"2026-08-21T10:00:00.000Z","rows":[{"id":"sample-b"}]}),
    ];
    fs::write(
        runtime.join("context-usage.jsonl"),
        events.iter().fold(String::new(), |mut text, event| {
            use std::fmt::Write;
            let _ = writeln!(text, "{event}");
            text
        }),
    )?;
    let affinity = affinity(&workspace, &identity, NOW);
    assert_eq!(affinity.claims.len(), 3);
    assert_eq!(affinity.usage.get("sample-a"), Some(&1.0));
    let rows = vec![item("sample-a", "open"), item("sample-b", "open")];
    assert_eq!(
        ranked(&workspace, &rows, NOW, &lifecycle, "fixture", &affinity)?.len(),
        2
    );
    let candidates = vec![
        (item("required", "open"), 0.01),
        (item("optional", "open"), 1.0),
    ];
    let required = BTreeSet::from(["required".to_owned()]);
    assert_eq!(pack(&candidates, 15, &required)[0]["id"], "required");
    assert!(pack(&candidates, 1, &required).is_empty());
    Ok(())
}

/// Weekly recurrence preserves exclusions, past counts, UTC duration and published expansion caps.
#[test]
fn weekly_calendar_contract() {
    let mut event = json!({"start_at":"2026-08-19T10:00:00.000Z","end_at":"2026-08-19T10:30:00.000Z","recurrence":{"freq":"weekly","by_weekday":["fri","mon","wed"],"count":3}});
    assert_eq!(event_dates(&event, NOW), vec!["2026-08-24T10:00:00.000Z"]);
    event["recurrence"]["exdates"] = json!(["2026-08-24T12:00:00.000+02:00"]);
    assert_eq!(event_dates(&event, NOW), vec!["2026-08-26T10:00:00.000Z"]);
    event["recurrence"]["until"] = json!("2026-08-25T00:00:00.000Z");
    assert!(event_dates(&event, NOW).is_empty());
    event["recurrence"]["by_month_day"] = json!([26]);
    assert!(event_dates(&event, NOW).is_empty());
    assert_eq!(
        event_end(&event, "2026-08-26T10:00:00.000Z"),
        "2026-08-26T10:30:00.000Z"
    );
    assert_eq!(event_end(&json!({}), NOW), Value::Null);
    assert_eq!(event_dates(&json!({"start_at":NOW}), NOW), vec![NOW]);
    assert!(
        event_dates(
            &json!({"start_at":"invalid","recurrence":{"freq":"weekly"}}),
            NOW
        )
        .is_empty()
    );
    assert!(event_dates(&event, "invalid").is_empty());
    assert!(
        event_dates(
            &json!({"start_at":"1900-01-01T00:00:00.000Z","recurrence":{"freq":"weekly"}}),
            NOW
        )
        .is_empty()
    );
    assert!(
        event_dates(
            &json!({"start_at":NOW,"recurrence":{"freq":"weekly","by_weekday":["unknown"]}}),
            NOW
        )
        .is_empty()
    );
    let large = json!({"start_at":NOW,"recurrence":{"freq":"weekly","interval":1_000_000_000_000_000_000u64}});
    assert_eq!(event_dates(&large, NOW), vec![NOW]);
    assert_eq!(recurrence_summary(&Value::Null), Value::Null);
    assert_eq!(
        recurrence_summary(
            &json!({"freq":"weekly","interval":2,"count":3,"until":NOW,"by_weekday":["fri","mon"],"by_month_day":[20,1],"exdates":["b","a"]})
        ),
        format!(
            "freq=weekly,interval=2,count=3,until={NOW},by_weekday=mon|fri,by_month_day=1|20,exdates=a|b"
        )
    );
    assert_eq!(weekday_index("unknown"), None);
}

/// Usage reads retain only a bounded tail and drop a clipped first line.
#[test]
fn usage_ledger_byte_boundary() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("ledger.jsonl");
    assert_eq!(usage_text(&path), "");
    fs::write(&path, "unclipped\n")?;
    assert_eq!(usage_text(&path), "unclipped\n");
    fs::write(&path, "x".repeat(262_145))?;
    assert_eq!(usage_text(&path), "");
    fs::write(&path, format!("{}\ntail\n", "x".repeat(262_145)))?;
    assert_eq!(usage_text(&path), "tail\n");
    Ok(())
}
