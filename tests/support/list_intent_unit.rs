//! Intent selection, budget feasibility and estimate stabilization.

use crate::ListOptions;
use serde_json::json;

#[test]
fn intent_controls_validate_overrides_and_constrain_explicit_pages()
-> Result<(), Box<dyn std::error::Error>> {
    for options in [
        ListOptions {
            token_budget: Some("1000".to_owned()),
            ..ListOptions::default()
        },
        ListOptions {
            intent: Some("unknown".to_owned()),
            ..ListOptions::default()
        },
        ListOptions {
            intent: Some("triage".to_owned()),
            token_budget: Some("255".to_owned()),
            ..ListOptions::default()
        },
        ListOptions {
            intent: Some("triage".to_owned()),
            limit: Some("bad".to_owned()),
            ..ListOptions::default()
        },
    ] {
        assert!(super::prepare(&mut options.clone()).is_err());
    }
    let mut options = ListOptions {
        intent: Some("triage".to_owned()),
        limit: Some("1".to_owned()),
        ..ListOptions::default()
    };
    assert_eq!(super::prepare(&mut options)?, Some(3200));
    assert_eq!(options.limit.as_deref(), Some("1"));
    let result = super::attach(json!({"items":[{"id":"demo-a"}],"count":1}), &options, 3200);
    assert_eq!(
        result["context_intent"]["binding_constraint"],
        "explicit_limit"
    );
    assert_eq!(
        result["context_intent"]["degradation"],
        "bounded_fields_and_rows"
    );
    Ok(())
}

#[test]
fn noncontinuable_intent_rows_disclose_infeasible_declaration() {
    let options = ListOptions {
        intent: Some("triage".to_owned()),
        ..ListOptions::default()
    };
    let result = super::attach(
        json!({"items":[{"id":"demo-a","notes":vec!["x".repeat(240);100]}],"count":1}),
        &options,
        3200,
    );
    assert_eq!(
        result["budget_exceeded"]["reason"],
        "declared_budget_infeasible"
    );
    assert_eq!(result["context_intent"]["declaration_feasible"], false);
    let result = super::attach(json!({"items":[],"next_cursor":"bad"}), &options, 10000);
    assert_eq!(result["context_intent"]["result_omitted"], false);
    // Base64 that is not JSON must fail closed inside cursor decode.
    let result = super::attach(
        json!({"items":[],"next_cursor":"bm90LWpzb24"}),
        &options,
        10000,
    );
    assert_eq!(result["context_intent"]["result_omitted"], false);
    assert!(result.get("budget_exceeded").is_none());
}

#[test]
fn both_receipts_stabilize_after_projection_changes() {
    let mut result = crate::ListOutput::from(
        json!({"items":[],"context_intent":{"estimated_tokens":0},"read_output":{"estimated_tokens":0}}),
    );
    super::stabilize(&mut result);
    assert_eq!(
        result["context_intent"]["estimated_tokens"],
        crate::read_output::estimate(&result, false)
    );
    assert_eq!(
        result["read_output"]["estimated_tokens"],
        crate::read_output::estimate(&result, true)
    );
}

/// Compaction on a final page rebases the preceding cursor and preserves counts.
#[test]
fn final_page_budget_rebases_from_the_source_index() {
    for existing in [false, true] {
        let cursor = crate::pagination::encode(
            &json!({"version":1,"fingerprint":"fp","after_id":"preceding","after_index":9}),
        );
        let options = ListOptions {
            intent: Some("triage".to_owned()),
            after: Some(cursor),
            ..ListOptions::default()
        };
        let mut source = json!({"items":(0..8).map(|i|json!({"id":format!("demo-{i}"),"title":"x".repeat(240)})).collect::<Vec<_>>(),"count":8,"applied_limit":8,"next_cursor":if existing {json!(crate::pagination::encode(&json!({"version":1,"fingerprint":"fp","after_id":"demo-7","after_index":17})))} else {serde_json::Value::Null}});
        if existing && let Some(map) = source.as_object_mut() {
            map.shift_remove("applied_limit");
        }
        let result = super::attach(source, &options, 700);
        assert_eq!(
            result["context_intent"]["degradation"],
            "budget_row_compaction"
        );
        let count = result["count"].as_u64().unwrap_or_default();
        assert!(count > 0 && count < 8);
        let next = result["next_cursor"].as_str().unwrap_or_default();
        assert_eq!(
            crate::pagination::query(next, "fp")
                .ok()
                .unwrap_or_default()["after_index"],
            9 + count
        );
    }
}

/// A legacy final-page cursor without `after_index` must not rebase from zero.
#[test]
fn legacy_final_page_without_after_index_does_not_rebase_from_zero() {
    let cursor =
        crate::pagination::encode(&json!({"version":1,"fingerprint":"fp","after_id":"preceding"}));
    let options = ListOptions {
        intent: Some("triage".to_owned()),
        after: Some(cursor),
        ..ListOptions::default()
    };
    let source = json!({"items":(0..8).map(|i|json!({"id":format!("demo-{i}"),"title":"x".repeat(240)})).collect::<Vec<_>>(),"count":8,"applied_limit":8,"next_cursor":serde_json::Value::Null});
    let result = super::attach(source, &options, 700);
    assert!(result.get("items").is_none());
    assert!(result.get("next_cursor").is_none());
    assert_eq!(result["budget_exceeded"]["omitted_result"], true);
    assert_eq!(result["context_intent"]["result_omitted"], true);
    assert_ne!(
        result["context_intent"]["degradation"],
        "budget_row_compaction"
    );
}

/// Intent receipts can omit an infeasible single row or retain an unchanged cursor.
#[test]
fn cursor_intents_disclose_effective_infeasibility_without_losing_metadata() {
    let cursor = crate::pagination::encode(
        &json!({"version":1,"fingerprint":"fp","after_id":"demo-a","after_index":0}),
    );
    let options = ListOptions {
        intent: Some("triage".to_owned()),
        ..ListOptions::default()
    };
    let result = super::attach(json!({"items":[],"next_cursor":cursor}), &options, 10000);
    assert_eq!(
        result["context_intent"]["degradation"],
        "bounded_fields_and_rows"
    );
    let result = super::attach(
        json!({"items":[{"id":"demo-a","notes":vec!["x".repeat(240);100]}],"count":1,"next_cursor":cursor}),
        &options,
        256,
    );
    assert_eq!(
        result["budget_exceeded"]["reason"],
        "effective_budget_infeasible"
    );
    assert_eq!(result["context_intent"]["result_omitted"], true);
}
