//! Ordering and refusal paths for the unbounded published-list slice.

use std::cmp::Ordering;
use std::fs;

/// Compares the timestamp component of the cached production sort key.
fn compare_time(left: &str, right: &str) -> Ordering {
    super::timestamp_key(left).cmp(&super::timestamp_key(right))
}
use crate::{ItemFilter, PmRustError, Workspace};

#[test]
fn timestamps_compare_instants_then_spelling_and_invalid_values() {
    assert_eq!(
        compare_time("2026-10-02T00:00:00Z", "2026-10-01T23:00:00Z"),
        Ordering::Greater
    );
    assert_eq!(
        compare_time("2026-10-02T00:00:00Z", "2026-10-02T00:00:00Z"),
        Ordering::Equal
    );
    assert_eq!(
        compare_time("2026-10-02T00:00:00Z", "2026-10-02T01:00:00+01:00"),
        Ordering::Less
    );
    assert_eq!(
        compare_time("invalid", "2026-10-02T00:00:00Z"),
        Ordering::Less
    );
    assert_eq!(
        compare_time("2026-10-02T00:00:00Z", "invalid"),
        Ordering::Greater
    );
    assert_eq!(compare_time("a", "b"), Ordering::Less);
}

#[test]
fn unsupported_csv_and_invalid_items_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(root.join("tasks"))?;
    fs::write(root.join("settings.json"), "{}")?;
    let workspace = Workspace::discover(directory.path())?;
    let filters = ItemFilter {
        status: Some("open,closed".to_owned()),
        ..ItemFilter::default()
    };
    assert!(matches!(
        workspace.list_unbounded(&filters, false, "clock"),
        Err(PmRustError::InvalidReadRequest { .. })
    ));
    for all in [false, true] {
        let result = workspace.list_unbounded(&ItemFilter::default(), all, "clock")?;
        assert_eq!(result["count"], 0);
        assert_eq!(result["total"], 0);
    }
    fs::write(root.join("tasks/bad.toon"), "invalid")?;
    assert!(matches!(
        workspace.list_unbounded(&ItemFilter::default(), false, "clock"),
        Err(PmRustError::InvalidItemDocument { .. })
    ));
    Ok(())
}

/// Mixed invalid and offset timestamps must define a transitive total order.
#[test]
fn timestamp_order_is_transitive_for_mixed_validity() {
    let values = [
        "2026-10-02T00:00:00+01:00",
        "2026-10-01T23:30:00Z",
        "2026-10-01T23:45:00Z?",
    ];
    for left in values {
        for middle in values {
            for right in values {
                if compare_time(left, middle).is_lt() && compare_time(middle, right).is_lt() {
                    assert!(
                        compare_time(left, right).is_lt(),
                        "cycle: {left}, {middle}, {right}"
                    );
                }
            }
        }
    }
}

/// Sorting parses each selected timestamp once, rather than per comparison.
#[test]
fn timestamp_parse_work_is_linear() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(root.join("tasks"))?;
    fs::write(root.join("settings.json"), "{}")?;
    for index in 0..16 {
        let id = format!("demo-{index:02}");
        fs::write(
            root.join("tasks").join(format!("{id}.toon")),
            format!(
                "id: {id}\ntitle: {id}\ndescription: \"\"\ntype: Task\nstatus: open\npriority: 1\ntags: []\ncreated_at: \"2026-09-01T00:00:00.000Z\"\nupdated_at: \"2026-09-01T00:00:00.000Z\"\nbody: \"\"\n"
            ),
        )?;
    }
    let workspace = Workspace::discover(directory.path())?;
    super::TIMESTAMP_PARSES.with(|counter| counter.set(Some(0)));
    let result = workspace.list_unbounded(&ItemFilter::default(), false, "clock");
    let parses = super::TIMESTAMP_PARSES.with(|counter| counter.replace(None));
    assert_eq!(result?["count"], 16);
    assert_eq!(parses, Some(16));
    Ok(())
}

/// Explicit producer policy boundaries and snapshot-independent item identity.
#[test]
fn page_limits_offsets_and_deleted_item_fallback() -> Result<(), Box<dyn std::error::Error>> {
    use crate::{ListOptions, pagination};
    use serde_json::json;
    for invalid in [
        "",
        "negative",
        "-1",
        "18446744073709551616",
        "9007199254740992",
    ] {
        assert!(super::integer(invalid, 0).is_err());
    }
    assert!(super::integer("0", 1).is_err());
    assert_eq!(super::integer(" 002 ", 0)?, 2);
    let rows = (0..10_000)
        .map(|i| json!({"id":format!("demo-{i:05}")}))
        .collect::<Vec<_>>();
    let source = json!({"items":rows,"total":rows.len(),"filters":{},"completeness":{},"projection":{},"sorting":{},"now":"fixed","omission_receipt":{}});
    let (defaults, legacy) = super::page_envelope(source.clone(), &ListOptions::default(), "fp")?;
    assert!(!legacy);
    assert_eq!(defaults["count"], 20);
    assert_eq!(defaults["applied_limit"], 20);
    for options in [
        ListOptions {
            limit: Some("0".to_owned()),
            ..ListOptions::default()
        },
        ListOptions {
            offset: Some("10000".to_owned()),
            ..ListOptions::default()
        },
    ] {
        assert_eq!(
            super::page_envelope(source.clone(), &options, "fp")?.0["count"],
            0
        );
    }
    let cursor = json!({"version":1,"fingerprint":"fp","after_id":"removed","after_index":2});
    let options = ListOptions {
        limit: Some("3".to_owned()),
        after: Some(pagination::encode(&cursor)),
        ..ListOptions::default()
    };
    assert_eq!(
        super::page_envelope(source.clone(), &options, "fp")?.0["items"][0]["id"],
        "demo-00002"
    );
    let missing = json!({"version":1,"fingerprint":"fp","after_id":"removed"});
    assert!(
        super::page_envelope(
            source.clone(),
            &ListOptions {
                after: Some(pagination::encode(&missing)),
                ..ListOptions::default()
            },
            "fp"
        )
        .is_err()
    );
    for options in [
        ListOptions {
            offset: Some("bad".to_owned()),
            ..ListOptions::default()
        },
        ListOptions {
            limit: Some("bad".to_owned()),
            ..ListOptions::default()
        },
    ] {
        assert!(super::page_envelope(source.clone(), &options, "fp").is_err());
    }
    Ok(())
}

/// SDK conflicts fail before any workspace read; default and terminal page modes work.
#[test]
fn sdk_page_contract_refusals_and_status_fingerprints() -> Result<(), Box<dyn std::error::Error>> {
    use crate::ListOptions;
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(root.join("tasks"))?;
    fs::write(root.join("settings.json"), "{}")?;
    let workspace = Workspace::discover(directory.path())?;
    for options in [
        ListOptions {
            full: true,
            brief: true,
            ..ListOptions::default()
        },
        ListOptions {
            after: Some("x".to_owned()),
            offset: Some("0".to_owned()),
            ..ListOptions::default()
        },
        ListOptions {
            token_budget: Some("1000".to_owned()),
            ..ListOptions::default()
        },
        ListOptions {
            intent: Some("unknown".to_owned()),
            ..ListOptions::default()
        },
    ] {
        assert!(
            workspace
                .list_page(&ItemFilter::default(), &options, "fixed")
                .is_err()
        );
    }
    for status in [
        "all",
        "open",
        "in_progress",
        "blocked",
        "draft",
        "closed",
        "canceled",
    ] {
        let filters = ItemFilter {
            status: Some(status.to_owned()),
            item_type: Some("Task".to_owned()),
            id: Some("absent".to_owned()),
        };
        let page = workspace.list_page(
            &filters,
            &ListOptions {
                brief: true,
                output_budget: Some("unbounded".to_owned()),
                ..ListOptions::default()
            },
            "fixed",
        )?;
        assert_eq!(page["count"], 0);
    }
    let page = workspace.list_page(
        &ItemFilter::default(),
        &ListOptions {
            all: true,
            brief: true,
            no_truncate: true,
            limit: Some("1".to_owned()),
            output_budget: Some("unbounded".to_owned()),
            ..ListOptions::default()
        },
        "fixed",
    )?;
    assert_eq!(page["filters"]["no_truncate"], true);
    Ok(())
}

/// SDK projection and intent options preserve selected row identity.
#[test]
fn sdk_projection_and_intent_preserve_rows() -> Result<(), Box<dyn std::error::Error>> {
    use crate::ListOptions;
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(root.join("tasks"))?;
    fs::write(root.join("settings.json"), "{}")?;
    let workspace = Workspace::discover(directory.path())?;
    fs::write(
        root.join("tasks/demo-one.toon"),
        "id: demo-one\ntitle: One\ndescription: \"\"\ntype: Task\nstatus: open\npriority: 1\ntags: []\ncreated_at: \"2026-09-01T00:00:00.000Z\"\nupdated_at: \"2026-09-01T00:00:00.000Z\"\nbody: \"\"\n",
    )?;
    for options in [
        ListOptions::default(),
        ListOptions {
            full: true,
            ..ListOptions::default()
        },
        ListOptions {
            intent: Some("triage".to_owned()),
            ..ListOptions::default()
        },
        ListOptions {
            intent: Some("triage".to_owned()),
            full: true,
            token_budget: Some("1000".to_owned()),
            output_budget: Some("unbounded".to_owned()),
            ..ListOptions::default()
        },
        ListOptions {
            intent: Some("triage".to_owned()),
            brief: true,
            ..ListOptions::default()
        },
    ] {
        let page = workspace.list_page(&ItemFilter::default(), &options, "fixed")?;
        assert_eq!(page["items"][0]["id"], "demo-one");
    }
    assert!(
        workspace
            .list_page(
                &ItemFilter {
                    status: Some("open".to_owned()),
                    ..ItemFilter::default()
                },
                &ListOptions {
                    all: true,
                    ..ListOptions::default()
                },
                "fixed"
            )
            .is_err()
    );
    Ok(())
}

/// Published ordering aliases normalize without accepting unsupported fields or directions.
#[test]
fn ordering_controls_normalize_and_refuse_invalid_requests()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::ListOptions;
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(&root)?;
    fs::write(root.join("settings.json"), "{}")?;
    let workspace = Workspace::discover(directory.path())?;

    for (raw, expected) in [
        (" UPDATED ", "updated_at"),
        ("modified", "updated_at"),
        ("update", "updated_at"),
        ("create", "created_at"),
        (" CREATED ", "created_at"),
        ("title", "title"),
    ] {
        let mut options = ListOptions {
            sort: Some(raw.to_owned()),
            order: Some(" DESC ".to_owned()),
            ..ListOptions::default()
        };
        super::normalize_ordering(&mut options)?;
        assert_eq!(options.sort.as_deref(), Some(expected));
        assert_eq!(options.order.as_deref(), Some("desc"));
    }
    for (sort, order) in [
        (Some("unknown"), None),
        (None, Some("desc")),
        (Some("title"), Some("sideways")),
    ] {
        let mut options = ListOptions {
            sort: sort.map(str::to_owned),
            order: order.map(str::to_owned),
            ..ListOptions::default()
        };
        assert!(matches!(
            super::normalize_ordering(&mut options),
            Err(PmRustError::InvalidReadRequest { .. })
        ));
        assert!(matches!(
            workspace.list_page(&ItemFilter::default(), &options, "clock"),
            Err(PmRustError::InvalidReadRequest { .. })
        ));
    }
    Ok(())
}

/// Hierarchy selection honors inverse aliases and ignores unresolved/non-hierarchy edges.
#[test]
fn parent_index_supports_inverse_edges_and_dangling_targets()
-> Result<(), Box<dyn std::error::Error>> {
    use serde_json::json;
    let document = |id: &str,
                    extra: serde_json::Value|
     -> Result<crate::ItemDocument, serde_json::Error> {
        let mut value = json!({"id":id,"title":id,"type":"Task","status":"open","priority":1,"created_at":"clock","updated_at":"clock"});
        if let (Some(fields), Some(extra)) = (value.as_object_mut(), extra.as_object()) {
            fields.extend(extra.clone());
        }
        serde_json::from_value(value)
    };
    let corpus = vec![
        document(
            "demo-root",
            json!({"dependencies":[{"id":"demo-leaf","kind":"child"},{"id":"ghost","kind":"child"},{"kind":"child"},{"id":"demo-other","kind":"related"}]}),
        )?,
        document("demo-leaf", json!({}))?,
        document(
            "demo-other",
            json!({"dependencies":[{"id":"demo-root","kind":"child_of"}]}),
        )?,
    ];
    let parents = super::parent_relations(
        &corpus,
        &crate::list_registry::Hierarchy::new(&serde_json::Value::Null),
    );
    assert_eq!(parents["demo-leaf"], ["demo-root".to_owned()].into());
    assert_eq!(parents["demo-other"], ["demo-root".to_owned()].into());
    assert!(!parents.contains_key("ghost"));
    assert_sdk_list_selection(corpus)?;
    Ok(())
}

/// Exercises the public SDK over a real tracker with filters and nullable sort fields.
fn assert_sdk_list_selection(
    corpus: Vec<crate::ItemDocument>,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::ListOptions;
    use serde_json::json;
    let directory = sdk_selection_tracker(corpus)?;
    let workspace = Workspace::discover(directory.path())?;
    for sort in [
        "priority",
        "deadline",
        "updated_at",
        "created_at",
        "title",
        "parent",
    ] {
        for order in ["asc", "desc"] {
            let result = workspace.list_page(
                &ItemFilter::default(),
                &ListOptions {
                    sort: Some(sort.to_owned()),
                    order: Some(order.to_owned()),
                    full: true,
                    output_budget: Some("unbounded".to_owned()),
                    ..ListOptions::default()
                },
                "clock",
            )?;
            assert_eq!(result["count"], 4);
            assert_eq!(result["sorting"], json!({"sort":sort,"order":order}));
        }
    }
    let filtered = workspace.list_page(
        &ItemFilter::default(),
        &ListOptions {
            tag: Some("alpha".to_owned()),
            parent: Some("demo-root".to_owned()),
            sort: Some("title".to_owned()),
            order: Some("desc".to_owned()),
            ..ListOptions::default()
        },
        "clock",
    )?;
    assert_eq!(
        filtered["items"]
            .as_array()
            .map(|rows| rows.iter().map(|row| row["id"].clone()).collect::<Vec<_>>()),
        Some(vec![json!("demo-other"), json!("demo-leaf")])
    );
    let missing = workspace.list_page(
        &ItemFilter::default(),
        &ListOptions {
            tag: Some("missing,alpha,missing".to_owned()),
            ..ListOptions::default()
        },
        "clock",
    )?;
    assert_eq!(missing["warnings"], json!(["unknown_tags:missing"]));
    for status in ["blocked", "draft", "canceled"] {
        let result = workspace.list_page(
            &ItemFilter {
                status: Some(status.to_owned()),
                ..ItemFilter::default()
            },
            &ListOptions::default(),
            "clock",
        )?;
        if status == "blocked" {
            assert_eq!(result["count"], 2);
        } else {
            assert_eq!(result["projection"]["mode"], "full");
        }
    }
    assert!(
        workspace
            .list_page(
                &ItemFilter {
                    status: Some("open,closed".to_owned()),
                    ..ItemFilter::default()
                },
                &ListOptions::default(),
                "clock"
            )
            .is_err()
    );
    Ok(())
}

/// Materializes canonical selection fixtures while retaining separate malformed-edge checks.
fn sdk_selection_tracker(
    mut corpus: Vec<crate::ItemDocument>,
) -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    use serde_json::json;
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(root.join("tasks"))?;
    fs::write(root.join("settings.json"), "{}")?;
    corpus[0].metadata.status = "blocked".to_owned();
    corpus[0]
        .metadata
        .extra
        .insert("deadline".to_owned(), json!("2026-10-01T00:00:00.000Z"));
    corpus[1]
        .metadata
        .extra
        .insert("blocked_by".to_owned(), json!("demo-root"));
    corpus[2].metadata.parent = Some("demo-root".to_owned());
    corpus[2]
        .metadata
        .extra
        .insert("deadline".to_owned(), json!("2026-10-02T00:00:00.000Z"));
    let mut fourth = corpus[2].clone();
    fourth.metadata.id = "demo-fourth".to_owned();
    fourth.metadata.title = "demo-fourth".to_owned();
    corpus.push(fourth);
    for (index, item) in corpus.iter_mut().enumerate() {
        item.metadata.tags = vec![
            if index == 0 || index == 3 {
                "beta"
            } else {
                "alpha"
            }
            .to_owned(),
        ];
        // Malformed and dangling edges are exercised by the pure index boundary above.
        if let Some(deps) = item
            .metadata
            .extra
            .get_mut("dependencies")
            .and_then(serde_json::Value::as_array_mut)
        {
            deps.retain(|dep| dep["id"].as_str().is_some_and(|id| id != "ghost"));
        }
        fs::write(
            root.join(format!("tasks/{}.toon", item.metadata.id)),
            toon_format::encode_default(item)?,
        )?;
    }
    Ok(directory)
}

/// An exact-ID filter decodes only its target unless corpus-dependent filters are requested.
#[test]
fn exact_id_selection_avoids_unrelated_payloads() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(root.join("tasks"))?;
    fs::write(root.join("settings.json"), "{}")?;
    let item = "id: sample-target\ntitle: Example\ntype: Task\nstatus: open\npriority: 1\ncreated_at: date\nupdated_at: date\n";
    fs::write(root.join("tasks/sample-target.toon"), item)?;
    let damaged = root.join("tasks/sample-damaged.toon");
    fs::write(&damaged, "<<<<<<< conflict\n")?;
    let workspace = Workspace::discover(directory.path())?;
    let filters = ItemFilter {
        id: Some("sample-target".to_owned()),
        ..Default::default()
    };
    let options = crate::ListOptions::default();
    assert_eq!(
        workspace.list_page(&filters, &options, "2026-08-22T10:00:00.000Z")?["count"],
        1
    );
    let absent = ItemFilter {
        id: Some("missing".to_owned()),
        ..Default::default()
    };
    assert_eq!(
        workspace.list_page(&absent, &options, "2026-08-22T10:00:00.000Z")?["count"],
        0
    );
    fs::write(root.join("tasks/sample-target.toon"), "invalid target")?;
    assert!(
        workspace
            .list_page(&filters, &options, "2026-08-22T10:00:00.000Z")
            .is_err()
    );
    fs::write(root.join("tasks/sample-target.toon"), item)?;
    for options in [
        crate::ListOptions {
            tag: Some("unknown".to_owned()),
            ..Default::default()
        },
        crate::ListOptions {
            parent: Some("parent".to_owned()),
            ..Default::default()
        },
    ] {
        assert!(
            workspace
                .list_page(&filters, &options, "2026-08-22T10:00:00.000Z")
                .is_err()
        );
    }
    let blocked = ItemFilter {
        status: Some("blocked".to_owned()),
        ..filters
    };
    assert!(
        workspace
            .list_page(&blocked, &options, "2026-08-22T10:00:00.000Z")
            .is_err()
    );
    Ok(())
}

/// Brief context parses update/history sort timestamps once per item, rather than per comparison.
#[test]
fn context_timestamp_parse_work_is_linear() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(root.join("tasks"))?;
    fs::write(root.join("settings.json"), "{}")?;
    for index in 0..50 {
        let id = format!("sample-{index:03}");
        let item = serde_json::json!({"id":id,"title":id,"type":"Task","status":"open","priority":1,"created_at":"2026-08-22T10:00:00.000Z","updated_at":"2026-08-22T10:00:00.000Z"});
        fs::write(
            root.join(format!("tasks/{id}.toon")),
            toon_format::encode_default(&item)?,
        )?;
    }
    let workspace = Workspace::discover(directory.path())?;
    super::TIMESTAMP_PARSES.set(Some(0));
    let result = workspace.context_with_global(
        &crate::ContextOptions {
            limit: Some("10".to_owned()),
            no_extension_health: true,
            ..Default::default()
        },
        "2026-08-22T10:00:00.000Z",
        None,
    );
    let parses = super::TIMESTAMP_PARSES.replace(None).unwrap_or_default();
    assert_eq!(result?["summary"]["total_items"], 50);
    assert!(
        parses <= 5 * 50 + 32,
        "{parses} timestamp parses for 50 items"
    );
    Ok(())
}
