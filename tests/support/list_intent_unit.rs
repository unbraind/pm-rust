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
        ListOptions {
            intent: Some("triage".to_owned()),
            output_budget: Some("bad".to_owned()),
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
    super::stabilize(&mut result, false);
    assert_eq!(
        result["context_intent"]["estimated_tokens"],
        crate::read_output::measure(&result, false)
    );
    assert_eq!(
        result["read_output"]["estimated_tokens"],
        crate::read_output::measure(&result, false)
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

/// Final dimensional clipping cannot turn an infeasible full declaration into a feasible one.
#[test]
fn round5_clipped_declaration_preserves_infeasibility() -> Result<(), Box<dyn std::error::Error>> {
    let options = ListOptions {
        intent: Some("triage".to_owned()),
        output_budget: Some("120000".to_owned()),
        output_limit: Some("1".to_owned()),
        ..Default::default()
    };
    let source = json!({"items":[{"id":"first","title":"small"},{"id":"second","title":"x".repeat(20000)}],"count":2,"total":2,"has_more":false,"next_cursor":null});
    let result = super::deliver(source.into(), &options, &options, Some(120_000), None)?;
    assert_eq!(result["items"].as_array().ok_or("items")?.len(), 1);
    assert!(
        result["context_intent"]["estimated_tokens"]
            .as_u64()
            .ok_or("estimate")?
            < 3200
    );
    assert_eq!(result["context_intent"]["declaration_feasible"], false);
    Ok(())
}

/// A bounded final response either fits its intent or discloses omission after receipt overhead.
#[test]
fn round5_finalized_intent_budget_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    for toon in [false, true] {
        for continuation in [None, Some("fingerprint")] {
            for budget in (256..=2000).step_by(7) {
                let options = ListOptions {
                    intent: Some("triage".to_owned()),
                    limit: Some("8".to_owned()),
                    token_budget: Some(budget.to_string()),
                    output_limit: Some("100".to_owned()),
                    full: true,
                    toon,
                    ..Default::default()
                };
                let source = json!({"items":(0..8).map(|i|json!({"id":format!("demo-{i}"),"title":"x".repeat(240)})).collect::<Vec<_>>(),"count":8,"total":20,"has_more":true,"next_cursor":crate::pagination::encode(&json!({"version":1,"fingerprint":"fp","after_id":"demo-7","after_index":7}))});
                let result = super::deliver(
                    source.into(),
                    &options,
                    &options,
                    Some(budget),
                    continuation,
                )?;
                if result.get("budget_exceeded").is_some()
                    || result.get("output_budget_exceeded").is_some()
                {
                    assert_eq!(result["context_intent"]["result_omitted"], true);
                } else {
                    let rendered = if toon {
                        crate::stringify_toon(&result)?
                    } else {
                        result.stringify_read_json()
                    };
                    assert!(
                        rendered.len().div_ceil(4) <= budget,
                        "final wire exceeded {budget}"
                    );
                }
            }
        }
    }
    let result = crate::ListOutput::from(json!({"items":[]}));
    assert!(super::settled(&result, &serde_json::Value::Null, &ListOptions::default()).is_none());
    assert!(
        super::settled(
            &result,
            &json!({"result_omitted":true}),
            &ListOptions::default()
        )
        .is_none()
    );
    assert_eq!(super::measure_intent(&json!(42).into(), false), 1);
    Ok(())
}

/// Final receipt overhead can invalidate a declaration, while small delivered pages stay feasible.
#[test]
fn round5_final_receipt_feasibility_downgrades_only() -> Result<(), Box<dyn std::error::Error>> {
    for toon in [false, true] {
        for width in [1, 12_400] {
            let mut before = crate::ListOutput::from(
                json!({"items":[{"id":"demo-a","title":"x".repeat(width)}],"context_intent":{"command":"list","intent":"triage","declared_token_budget":3200,"estimated_tokens":0,"declaration_feasible":true}}),
            );
            super::update_intent(&mut before, toon);
            assert!(super::measure_intent(&before, toon) <= 3200);
            let options = ListOptions {
                toon,
                full: true,
                output_budget: Some("4000".to_owned()),
                ..Default::default()
            };
            let result = crate::read_output::apply(before, &options)?;
            let measured = super::measure_intent(&result, toon);
            assert_eq!(result["context_intent"]["declaration_feasible"], width == 1);
            assert_eq!(result["context_intent"]["estimated_tokens"], measured);
            assert_eq!(measured > 3200, width != 1);
        }
    }
    Ok(())
}

/// Explicitly unbounded pages retain oversized rows and report the declared cost honestly.
#[test]
fn round5_unbounded_intent_keeps_rows_and_receipt_guards() -> Result<(), Box<dyn std::error::Error>>
{
    let options = ListOptions {
        intent: Some("triage".to_owned()),
        output_budget: Some("unbounded".to_owned()),
        ..Default::default()
    };
    let source = json!({"items":[{"id":"demo-one","title":"x".repeat(20_000)}],"count":1});
    let result = super::attach(source.clone(), &options, 256);
    assert_eq!(result["items"], source["items"]);
    assert_eq!(result["context_intent"]["within_budget"], false);
    assert!(result.get("budget_exceeded").is_none());
    for receipt in [
        json!({"estimated_tokens":"bad","token_budget":256}),
        json!({"estimated_tokens":400,"token_budget":null}),
    ] {
        let result = crate::ListOutput::from(json!({"context_intent":receipt}));
        assert!(
            super::settled(&result, &serde_json::Value::Null, &ListOptions::default()).is_none()
        );
    }
    let result = crate::ListOutput::from(
        json!({"context_intent":{},"output_budget_exceeded":{"omitted_result":true}}),
    );
    assert!(super::settled(&result, &serde_json::Value::Null, &ListOptions::default()).is_none());
    for marker in ["budget_exceeded", "output_budget_exceeded"] {
        let mut source = json!({"context_intent":{"result_omitted":true}});
        source[marker] = json!({"omitted_result":true});
        let result = super::finalize(source.into(), &ListOptions::default(), Some("fingerprint"))?;
        assert!(result.get("continuation_contract").is_none());
        assert!(result.get(marker).is_some());
    }
    let options = ListOptions {
        output_cursor: Some("invalid".to_owned()),
        ..Default::default()
    };
    assert!(
        super::deliver(
            json!({"items":[]}).into(),
            &ListOptions::default(),
            &options,
            Some(3200),
            None
        )
        .is_err()
    );
    Ok(())
}

/// A final page whose strings barely compact exercises the last receipt-overhead boundary.
fn final_receipt_boundary_source() -> serde_json::Value {
    json!({"items":(0..43).map(|i|json!({"id":format!("demo-{i}"),"title":"x".repeat(254),"status":"open","type":"Task","priority":2,"parent":null,"assignee":null,"risk":null,"blocked_by":null})).collect::<Vec<_>>(),"filters":{"tag":"a".repeat(134)},"count":43,"total":43,"has_more":false,"next_cursor":null})
}

/// A one-token finalized-envelope floor refuses; the adjacent budget keeps every row.
#[test]
fn round5_finalized_receipt_floor_and_adjacent_allow() -> Result<(), Box<dyn std::error::Error>> {
    let source = final_receipt_boundary_source();
    for (budget, omitted) in [(3236, true), (3237, false)] {
        let options = ListOptions {
            intent: Some("triage".to_owned()),
            token_budget: Some(budget.to_string()),
            limit: Some("43".to_owned()),
            output_limit: Some("100".to_owned()),
            full: true,
            toon: true,
            ..Default::default()
        };
        let initial = super::attach(source.clone(), &options, budget);
        assert_eq!(initial["items"].as_array().ok_or("initial rows")?.len(), 43);
        assert!(initial.get("budget_exceeded").is_none());
        let result = super::deliver(
            source.clone().into(),
            &options,
            &options,
            Some(budget),
            None,
        )?;
        assert_eq!(result["context_intent"]["result_omitted"], omitted);
        if omitted {
            assert_eq!(
                result["budget_exceeded"]["reason"],
                "effective_budget_infeasible"
            );
            assert!(result.get("items").is_none());
        } else {
            assert_eq!(result["items"].as_array().ok_or("final rows")?.len(), 43);
            assert!(crate::read_output::measure(&result, true) <= budget);
        }
    }
    Ok(())
}

/// A valid snapshot cannot be replayed after overhead projection changes its row bytes.
#[test]
fn round5_second_projection_rechecks_a_valid_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let source = final_receipt_boundary_source();
    let mut options = ListOptions {
        intent: Some("triage".to_owned()),
        token_budget: Some("3236".to_owned()),
        limit: Some("43".to_owned()),
        output_limit: Some("100".to_owned()),
        full: true,
        toon: true,
        ..Default::default()
    };
    let first = super::attach(source.clone(), &options, 3236);
    let fingerprint = first.row_fingerprint();
    let cursor = crate::pagination::encode(
        &json!({"v":1,"c":"list","p":"items","o":0,"n":43,"f":fingerprint}),
    );
    crate::pagination::output_command(&cursor, &first["items"], &fingerprint, "list")?;
    options.output_cursor = Some(cursor);
    assert!(
        matches!(super::deliver(source.into(),&options,&options,Some(3236),None),Err(crate::PmRustError::ReadCursor{code,..}) if code=="read_output_cursor_stale")
    );
    Ok(())
}
