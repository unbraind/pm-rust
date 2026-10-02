//! Ordering and refusal paths for the unbounded published-list slice.

use std::cmp::Ordering;
use std::fs;

use super::compare_time;
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
        Ordering::Greater
    );
    assert_eq!(
        compare_time("2026-10-02T00:00:00Z", "invalid"),
        Ordering::Less
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
