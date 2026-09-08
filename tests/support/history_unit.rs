//! Unit acceptance for canonical ordering, patching, hashing, and encoding.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::history::{
    HistoryPatch, OrderedDocument, canonical_metadata_pairs, classify_history_event, document_hash,
    history_entry, history_line, history_patch, javascript_number,
};

/// The published classifier retains unknown work while distinguishing bookkeeping updates.
#[test]
fn event_classification_preserves_work_and_recognizes_maintenance() {
    for operation in [
        "docs_add",
        "files_add",
        "history:author-acknowledge",
        "history_author_acknowledge",
        "history_compact",
        "history_compact_baseline",
        "history_redact",
        "history_repair",
        "normalize",
        "release",
        "test_run_track",
        "tests_add",
        "tests_remove",
        "update_audit",
        "update_ownership_bypass",
    ] {
        assert_eq!(classify_history_event(operation, &[]), "maintenance");
    }
    for operation in [
        "create",
        "comment_add",
        "close",
        "future_operation",
        "update",
    ] {
        assert_eq!(classify_history_event(operation, &[]), "substantive");
    }
    for field in [
        "actual_result",
        "affected_version",
        "assignee",
        "blocked_by",
        "blocked_reason",
        "close_reason",
        "confidence",
        "customer_impact",
        "definition_of_ready",
        "dependencies",
        "docs",
        "environment",
        "expected_result",
        "files",
        "fixed_version",
        "goal",
        "impact",
        "learnings",
        "notes",
        "objective",
        "order",
        "outcome",
        "parent",
        "release",
        "reminders",
        "reporter",
        "repro_steps",
        "resolution",
        "reviewer",
        "severity",
        "sprint",
        "tests",
        "test_runs",
        "unblock_note",
        "updated_at",
        "value",
        "why_now",
    ] {
        for prefix in ["metadata", "front_matter"] {
            let patch = [HistoryPatch {
                op: "replace",
                path: format!("/{prefix}/{field}"),
                value: Some(json!("value")),
            }];
            assert_eq!(classify_history_event("update", &patch), "maintenance");
        }
    }
    for path in [
        "/metadata/title",
        "/body",
        "/metadata",
        "/",
        "/unexpected/updated_at",
    ] {
        let patch = [HistoryPatch {
            op: "replace",
            path: path.to_owned(),
            value: Some(json!("value")),
        }];
        assert_eq!(classify_history_event("update", &patch), "substantive");
    }
}

/// Record integrity must cover attribution even when the item snapshots are identical.
#[test]
fn history_record_hash_covers_author_and_provenance() -> Result<(), Box<dyn std::error::Error>> {
    let mut hashes = std::collections::BTreeSet::new();
    for (author, role) in [("one", None), ("two", None), ("one", Some("implementer"))] {
        let entry = history_entry(
            "2026-09-04T00:00:00.000Z",
            author,
            "create",
            role,
            Vec::new(),
            "before".to_owned(),
            "after".to_owned(),
            None,
        );
        let value: Value = serde_json::from_str(&history_line(&entry))?;
        assert_eq!(value["event_class"], "substantive");
        assert_eq!(value["record_hash_version"], 1);
        let hash = value["record_hash"].as_str().ok_or("missing record hash")?;
        assert_eq!(hash.len(), 64);
        hashes.insert(hash.to_owned());
    }
    assert_eq!(hashes.len(), 3);
    Ok(())
}

/// Builds one ordered document from a JSON metadata object and a body.
fn ordered(metadata: &Value, body: &str) -> OrderedDocument {
    let Value::Object(entries) = metadata else {
        // The test table only ever supplies JSON objects.
        return OrderedDocument {
            metadata: Vec::new(),
            body: body.to_owned(),
        };
    };
    let pairs = entries
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    OrderedDocument {
        metadata: pairs,
        body: body.to_owned(),
    }
}

#[test]
fn lookups_report_present_and_absent_metadata_keys() {
    let document = ordered(&json!({"id": "sample-x", "title": "T"}), "body");
    assert!(document.contains("id"));
    assert!(!document.contains("absent"));
    assert_eq!(document.get("title"), Some(&Value::String("T".to_owned())));
    assert_eq!(document.get("missing"), None);
}

#[test]
fn patch_diffs_include_removals_of_dropped_metadata_keys() {
    let before = ordered(
        &json!({"id": "sample-x", "status": "open", "assignee": "alice"}),
        "same",
    );
    let after = ordered(&json!({"id": "sample-x", "status": "closed"}), "same");
    let patch = history_patch(&before, &after);
    assert_eq!(
        patch,
        vec![
            crate::history::HistoryPatch {
                op: "remove",
                path: "/metadata/assignee".to_owned(),
                value: None,
            },
            crate::history::HistoryPatch {
                op: "replace",
                path: "/metadata/status".to_owned(),
                value: Some(Value::String("closed".to_owned())),
            },
        ]
    );
}

#[test]
fn unknown_metadata_keys_sort_after_every_canonical_key() {
    let mut metadata = crate::item::ItemMetadata {
        id: "sample-x".to_owned(),
        title: "T".to_owned(),
        description: String::new(),
        item_type: "Task".to_owned(),
        status: "open".to_owned(),
        priority: 2,
        tags: Vec::new(),
        created_at: "2026-08-22T10:00:00.000Z".to_owned(),
        updated_at: "2026-08-22T10:00:00.000Z".to_owned(),
        parent: Some("sample-p".to_owned()),
        extra: BTreeMap::default(),
    };
    metadata.extra.insert("zz_custom".to_owned(), json!(1));
    metadata.extra.insert("aa_custom".to_owned(), json!(2));
    let pairs = canonical_metadata_pairs(&metadata);
    let keys: Vec<&str> = pairs.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(
        keys,
        vec![
            "id",
            "title",
            "description",
            "type",
            "status",
            "priority",
            "tags",
            "created_at",
            "updated_at",
            "parent",
            // Unknown keys follow every known key in lexicographic order.
            "aa_custom",
            "zz_custom"
        ]
    );
}

#[test]
fn hashes_are_stable_and_entry_lines_carry_the_published_epoch() {
    let document = ordered(&json!({"id": "sample-x"}), "b");
    let again = ordered(&json!({"id": "sample-x"}), "b");
    assert_eq!(document_hash(&document), document_hash(&again));
    let entry = history_entry(
        "2026-08-22T10:00:00.000Z",
        "fixture-agent",
        "update",
        Some("implementer"),
        Vec::new(),
        "before".to_owned(),
        "after".to_owned(),
        None,
    );
    let line = history_line(&entry);
    assert!(line.contains(r#""author_source":"asserted""#));
    assert!(
        line.contains(r#""agent_provenance":{"role":{"value":"implementer","source":"argv"}}"#)
    );
    assert!(line.contains(r#""item_hash_version":3"#));
    assert!(!line.contains("message"));
    assert!(line.ends_with('\n'));
}

#[test]
fn floats_hash_the_way_json_stringify_renders_them() {
    // The published CLI hashes `JSON.stringify` output. `serde_json` disagrees
    // with it about exactly two things, and both change the digest for any item
    // carrying a float in its metadata: a whole-valued float keeps a `.0`, and an
    // exponent loses its `+`. Each expectation below is the literal output of
    // `JSON.stringify(<value>)` in Node.
    for (value, expected) in [
        (json!(30.0), "30"),
        (json!(-0.0), "-0"),
        // Exponent form is asserted here precisely because the renderer does not
        // touch it: these prove serde_json's own output already matches
        // JSON.stringify, which is why no exponent branch exists.
        (json!(1.0e21), "1e+21"),
        (json!(1.0e-7), "1e-7"),
        (json!(1.0e100), "1e+100"),
        (json!(0.1), "0.1"),
        (json!(-2.5), "-2.5"),
    ] {
        let Value::Number(number) = &value else {
            unreachable!("the table holds only numbers");
        };
        assert_eq!(javascript_number(number), expected, "rendering {value}");
    }
    // Integers are already agreed on and must not be routed through the float
    // path, where a large u64 would lose precision through f64.
    for (value, expected) in [
        (json!(30), "30"),
        (json!(-4), "-4"),
        (json!(9_007_199_254_740_993_u64), "9007199254740993"),
    ] {
        let Value::Number(number) = &value else {
            unreachable!("the table holds only numbers");
        };
        assert_eq!(javascript_number(number), expected, "rendering {value}");
    }
    // The digest must actually change with the rendering, so this is not a test
    // of a function nothing calls.
    let whole = ordered(&json!({"id": "sample-x", "budget": 30.0}), "b");
    let integer = ordered(&json!({"id": "sample-x", "budget": 30}), "b");
    assert_eq!(
        document_hash(&whole),
        document_hash(&integer),
        "a whole-valued float and the same integer must hash identically, as they do in JavaScript",
    );
}
