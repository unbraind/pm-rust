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
