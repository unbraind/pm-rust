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
    let defaults = super::page_envelope(source.clone(), &ListOptions::default(), "fp")?;
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
            super::page_envelope(source.clone(), &options, "fp")?["count"],
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
        super::page_envelope(source.clone(), &options, "fp")?["items"][0]["id"],
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
