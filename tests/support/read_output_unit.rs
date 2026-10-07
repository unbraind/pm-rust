//! Bounded delivery, nested collection and receipt-only recovery contracts.

use super::{Continuation, apply, capture, compact_strings, finalize, receipt, retain};
use crate::ListOptions;
use serde_json::{Value, json};

/// Synthetic envelope with configurable row count and payload size.
fn envelope(count: usize, width: usize) -> Value {
    json!({"items":(0..count).map(|i|json!({"id":format!("demo-{i:04}"),"title":"x".repeat(width)})).collect::<Vec<_>>(),"count":count,"total":count,"has_more":false,"truncated":false,"next_cursor":null})
}

/// Amount-only caps advance an advertised boundary while preserving terminal caps.
#[test]
fn amount_limit_rebases_only_advertised_producer_boundaries()
-> Result<(), Box<dyn std::error::Error>> {
    let incoming = crate::pagination::encode(
        &json!({"version":1,"fingerprint":"fp","after_id":"previous","after_index":9}),
    );
    for budget in ["unbounded", "100000"] {
        let options = ListOptions {
            after: Some(incoming.clone()),
            output_limit: Some("2".to_owned()),
            output_budget: Some(budget.to_owned()),
            ..ListOptions::default()
        };
        let mut source = envelope(5, 0);
        source["applied_limit"] = json!(5);
        source["next_cursor"] = json!(crate::pagination::encode(
            &json!({"version":1,"fingerprint":"fp","after_id":"demo-0004","after_index":14,"snapshot":"snap"})
        ));
        let result = apply(source, &options)?;
        let cursor = crate::pagination::query(
            result["next_cursor"]
                .as_str()
                .ok_or("missing producer cursor")?,
            "fp",
        )?;
        assert_eq!(cursor["after_index"], 11);
        assert_eq!(cursor["after_id"], "demo-0001");
        assert_eq!(cursor["snapshot"], "snap");
        assert_eq!(result["count"], 2);
        assert_eq!(result["applied_limit"], 5);
        assert_eq!(result["read_output"]["within_budget"], true);
        assert_eq!(result["read_output"]["rows_compacted"], false);
        assert!(result.get("output_budget_truncation").is_none());
        let terminal = apply(envelope(5, 0), &options)?;
        assert!(terminal["next_cursor"].is_null());
        assert_eq!(terminal["has_more"], true);
        assert_eq!(terminal["truncated"], true);
    }
    Ok(())
}

#[test]
fn amount_and_alias_receipts_preserve_requested_dimensions()
-> Result<(), Box<dyn std::error::Error>> {
    for options in [
        ListOptions {
            brief: true,
            output_limit: Some("1".to_owned()),
            output_budget: Some("unbounded".to_owned()),
            ..ListOptions::default()
        },
        ListOptions {
            no_truncate: true,
            output_limit: Some("4".to_owned()),
            output_cursor: Some(crate::pagination::encode(
                &json!({"v":1,"c":"list","p":"items","o":1,"n":2,"f":crate::pagination::collection(&envelope(2,0)["items"])}),
            )),
            ..ListOptions::default()
        },
    ] {
        let result = apply(envelope(2, 0), &options)?;
        assert_eq!(result["count"], 1);
        assert!(result["read_output"].is_object());
    }
    let zero = ListOptions {
        limit: Some("0".to_owned()),
        output_budget: Some("1".to_owned()),
        ..ListOptions::default()
    };
    assert!(apply(envelope(2, 0), &zero)?["output_budget_exceeded"].is_object());
    assert!(
        apply(
            envelope(2, 0),
            &ListOptions {
                output_budget: Some("bad".to_owned()),
                ..ListOptions::default()
            }
        )
        .is_err()
    );
    assert!(
        apply(
            envelope(2, 0),
            &ListOptions {
                output_limit: Some("bad".to_owned()),
                ..ListOptions::default()
            }
        )
        .is_err()
    );
    let _ = receipt(
        &ListOptions {
            no_truncate: true,
            ..ListOptions::default()
        },
        false,
    );
    let _ = receipt(
        &ListOptions {
            brief: true,
            ..ListOptions::default()
        },
        false,
    );
    let _ = receipt(
        &ListOptions {
            limit: Some("2".to_owned()),
            ..ListOptions::default()
        },
        false,
    );
    assert!(
        apply(
            envelope(0, 0),
            &ListOptions {
                full: true,
                output_budget: Some("unbounded".to_owned()),
                ..ListOptions::default()
            }
        )?["read_output"]
            .is_object()
    );
    Ok(())
}

#[test]
fn nested_compaction_discloses_noncontinuable_rows_and_recovery_budget()
-> Result<(), Box<dyn std::error::Error>> {
    let mut source = envelope(1, 0);
    source["items"][0]["tags"] = json!(vec!["x".repeat(240); 50]);
    let result = apply(
        source,
        &ListOptions {
            output_budget: Some("800".to_owned()),
            ..ListOptions::default()
        },
    )?;
    assert_eq!(
        result["output_budget_truncation"]["continuation_kind"],
        Value::Null
    );
    assert_eq!(result["continuation_kind"], "none");
    assert_eq!(
        result["output_budget_truncation"]["continuation_available"],
        false
    );
    assert_eq!(result["read_output"]["rows_compacted"], true);
    let paths = result["read_output"]["compacted_row_paths"]
        .as_array()
        .ok_or("missing paths")?;
    assert!(paths.contains(&json!("items.0.tags")));
    assert!(
        result["items"][0]["tags"]
            .as_array()
            .is_some_and(|rows| rows.len() < 50)
    );
    Ok(())
}

#[test]
fn producer_rebasing_and_disclosure_keep_indices_and_snapshot() {
    let producer = json!({"version":1,"fingerprint":"fp","after_id":"demo-0000","after_index":0});
    let state = Continuation {
        count: 3,
        total: 3,
        offset: 0,
        fingerprint: "snap".to_owned(),
        producer: Some(producer),
        existing: false,
    };
    let mut result = envelope(2, 0);
    result["read_output"] = json!({"rows_compacted":true,"compacted_row_paths":["items"]});
    finalize(&mut result, &state, 1000, "canonical", &[], 2000, false);
    assert_eq!(result["continuation_kind"], "producer_cursor");
    let cursor = result["next_cursor"].as_str().unwrap_or_default();
    assert_eq!(
        crate::pagination::query(cursor, "fp")
            .ok()
            .unwrap_or_default()["after_index"],
        2
    );
    result["items"] = json!([]);
    finalize(&mut result, &state, 1000, "canonical", &[], 2000, false);
    assert_eq!(result["continuation_kind"], "output_cursor");
    let mut source = envelope(1, 0);
    source["next_cursor"] = json!(crate::pagination::encode(
        &json!({"version":1,"fingerprint":"fp","after_id":"demo-a","after_index":1})
    ));
    assert!(
        capture(&source, &ListOptions::default(), None)
            .producer
            .is_some()
    );
    source["next_cursor"] = json!("bad");
    assert!(
        capture(&source, &ListOptions::default(), None)
            .producer
            .is_none()
    );
    source["next_cursor"] = json!(crate::pagination::encode(
        &json!({"version":1,"fingerprint":"fp","after_id":"demo-a"})
    ));
    assert!(
        capture(&source, &ListOptions::default(), None)
            .producer
            .is_none()
    );
    source["next_cursor"] = json!(crate::pagination::encode(&json!({"after_index":1})));
    assert!(
        capture(&source, &ListOptions::default(), None)
            .producer
            .is_none()
    );
    source["next_cursor"] = json!(base64::Engine::encode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        "invalid-json"
    ));
    assert!(
        capture(&source, &ListOptions::default(), None)
            .producer
            .is_none()
    );
    let mut absent = json!({"read_output":{"rows_compacted":false}});
    finalize(&mut absent, &state, 1000, "default", &[], 2000, false);
    assert!(absent.get("count").is_none());
    retain(&mut absent, "/missing", &[], 0);
}

#[test]
fn estimates_and_string_compaction_handle_nested_unicode() {
    let mut value = json!({"rows":["😀".repeat(130),"x".repeat(240),null,3]});
    assert!(compact_strings(&mut value));
    assert_eq!(value["rows"][0], format!("{}…", "😀".repeat(120)));
    assert!(!compact_strings(&mut json!([true, null, "short"])));
    let mut measured = json!({"read_output":{"estimated_tokens":0}});
    super::update(&mut measured, "read_output", true);
    assert_eq!(
        measured["read_output"]["estimated_tokens"],
        super::estimate(&measured, true)
    );
}

/// Multiple nested collections compact in deterministic size and path order.
#[test]
fn multiple_collections_and_resumed_budgets_preserve_receipts()
-> Result<(), Box<dyn std::error::Error>> {
    let mut source = envelope(2, 0);
    for index in 0..2 {
        source["items"][index]["notes"] = json!(vec!["x".repeat(240); 50]);
    }
    let result = apply(
        source,
        &ListOptions {
            output_budget: Some("800".to_owned()),
            ..ListOptions::default()
        },
    )?;
    assert_eq!(
        result["read_output"]["compacted_row_paths"],
        json!(["items.0.notes", "items.1.notes"])
    );
    let first = apply(envelope(500, 0), &ListOptions::default())?;
    assert_eq!(first["read_output"]["budget_source"], "default");
    let original = envelope(100, 0);
    let cursor = crate::pagination::encode(
        &json!({"v":1,"c":"list","p":"items","o":1,"n":100,"f":crate::pagination::collection(&original["items"])}),
    );
    let resumed = apply(
        original,
        &ListOptions {
            output_cursor: Some(cursor),
            output_budget: Some("900".to_owned()),
            token_budget: Some("1000".to_owned()),
            ..ListOptions::default()
        },
    )?;
    assert_eq!(resumed["continuation_kind"], "output_cursor");
    assert_eq!(
        resumed["read_output"]["legacy_aliases_used"],
        json!(["--token-budget"])
    );
    let _ = receipt(
        &ListOptions {
            token_budget: Some("1000".to_owned()),
            ..ListOptions::default()
        },
        false,
    );
    let state = Continuation {
        count: 3,
        total: 3,
        offset: 0,
        fingerprint: "snap".to_owned(),
        producer: Some(
            json!({"version":1,"fingerprint":"fp","after_id":"demo-0002","after_index":9}),
        ),
        existing: true,
    };
    let mut result = envelope(2, 0);
    result["read_output"] = json!({"rows_compacted":true,"compacted_row_paths":["items"]});
    finalize(&mut result, &state, 1000, "canonical", &[], 2000, false);
    let cursor = result["next_cursor"].as_str().ok_or("missing cursor")?;
    assert_eq!(crate::pagination::query(cursor, "fp")?["after_index"], 8);
    Ok(())
}

/// Audit-only budgets, legacy costs and continuation-only envelopes remain distinct.
#[test]
fn output_policy_audits_cover_requested_controls_and_absent_counts()
-> Result<(), Box<dyn std::error::Error>> {
    for options in [
        ListOptions {
            token_budget: Some("1000".to_owned()),
            output_limit: Some("1".to_owned()),
            ..ListOptions::default()
        },
        ListOptions {
            full: true,
            output_limit: Some("100".to_owned()),
            ..ListOptions::default()
        },
        ListOptions {
            full: true,
            output_budget: Some("10000".to_owned()),
            ..ListOptions::default()
        },
    ] {
        assert!(apply(envelope(2, 0), &options)?["read_output"].is_object());
    }
    assert!(
        apply(
            json!({"items":[]}),
            &ListOptions {
                output_budget: Some("1".to_owned()),
                ..ListOptions::default()
            }
        )?["output_budget_exceeded"]
            .is_object()
    );
    assert!(
        apply(
            envelope(2, 0),
            &ListOptions {
                output_cursor: Some("bad".to_owned()),
                ..ListOptions::default()
            }
        )
        .is_err()
    );
    let result = apply(
        envelope(100, 0),
        &ListOptions {
            output_limit: Some("unbounded".to_owned()),
            output_budget: Some("900".to_owned()),
            ..ListOptions::default()
        },
    )?;
    assert_eq!(
        result["output_budget_truncation"]["overridden_dimensions"],
        json!(["amount"])
    );
    Ok(())
}
