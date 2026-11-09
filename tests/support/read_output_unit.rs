//! Bounded delivery, nested collection and receipt-only recovery contracts.

use super::{Continuation, capture, compact_strings, finalize, receipt, retain};
use crate::ListOptions;
use serde_json::{Value, json};

/// Every collection walker preserves declarations while continuing to compact actual rows.
#[test]
fn intent_declarations_are_immutable_during_wire_compaction()
-> Result<(), Box<dyn std::error::Error>> {
    for toon in [false, true] {
        for count in [2, 20, 80] {
            let groups = (0..count)
                .map(|i| format!("field-group-{i}"))
                .collect::<Vec<_>>();
            let declaration = json!({"command":"context","included_field_groups":groups,"future":{"groups":groups},"description":"m".repeat(500)});
            let mut source = json!({"context_intent":declaration,"activity":[{"title":"a"},{"title":"b"}],"read_output":{"compacted_row_paths":[]}});
            super::compact_strings(&mut source, true);
            assert_eq!(source["context_intent"], declaration);
            let mut candidates = Vec::new();
            super::collections(&source, "", &mut candidates);
            assert_eq!(candidates, vec![("/activity".to_owned(), 2)]);
            let mut output = crate::ListOutput::from(source);
            super::compact_collections(&mut output, toon, 1, |_| {});
            assert_eq!(output["context_intent"], declaration);
            assert_eq!(output["activity"].as_array().map(Vec::len), Some(1));
            assert_eq!(
                output["read_output"]["compacted_row_paths"],
                json!(["activity"])
            );
            for budget in [400, 600, 800, 1000, 1200, 1600] {
                let mut public_declaration = declaration.clone();
                public_declaration["token_budget"] = json!(budget);
                public_declaration["declared_token_budget"] = json!(3000);
                let source = json!({"context_intent":public_declaration,"activity":[{"title":"a"},{"title":"b"}]});
                let mut list_declaration = public_declaration.clone();
                list_declaration["command"] = json!("list");
                list_declaration["source"] = json!("core");
                list_declaration["intent"] = json!("triage");
                let list_source = json!({"context_intent":list_declaration,"items":[{"id":"sample-a","title":"a"},{"id":"sample-b","title":"b"}],"count":2,"total":2});
                let outputs = [
                    super::apply_orientation(
                        source.into(),
                        &ListOptions {
                            toon,
                            token_budget: Some(budget.to_string()),
                            ..Default::default()
                        },
                        "context",
                        &["activity"],
                    )?,
                    super::apply_command(
                        list_source.into(),
                        &ListOptions {
                            toon,
                            output_budget: Some(budget.to_string()),
                            ..Default::default()
                        },
                        "list",
                    )?,
                ];
                for output in outputs {
                    if output.get("context_intent").is_some() {
                        assert_eq!(
                            output["context_intent"]["included_field_groups"],
                            declaration["included_field_groups"]
                        );
                        assert_eq!(output["context_intent"]["future"], declaration["future"]);
                        assert_eq!(
                            output["context_intent"]["description"],
                            declaration["description"]
                        );
                    } else {
                        assert_eq!(output["read_output"]["result_omitted"], true);
                    }
                }
            }
        }
    }
    Ok(())
}

/// Adapts simple unit-test envelopes to the lossless output type.
fn apply(value: Value, options: &ListOptions) -> Result<crate::ListOutput, crate::PmRustError> {
    super::apply(value.into(), options)
}

/// Synthetic envelope with configurable row count and payload size.
fn envelope(count: usize, width: usize) -> Value {
    json!({"items":(0..count).map(|i|json!({"id":format!("demo-{i:04}"),"title":"x".repeat(width)})).collect::<Vec<_>>(),"count":count,"total":count,"has_more":false,"truncated":false,"next_cursor":null})
}

/// Custom lifecycle classification survives row truncation and disappears when no longer needed.
#[test]
fn custom_focus_classification_survives_compaction() {
    let mut result = json!({
        "summary":{"focus_row_states":{"high_level":"iI","low_level":"oO","blocked_fallback":""}},
        "high_level":[{"status":"working"},{"status":"working","blocked":true}],
        "low_level":[{"status":"open"},{"status":"blocked","blocked":true}],
        "blocked_fallback":[]
    });
    super::finalize_orientation(&mut result, &[], 1000, "legacy", &[], 2000, "context");
    assert_eq!(
        result["summary"]["returned_focus"],
        json!({"active_items":4,"in_progress":2,"open":2,"blocked":2})
    );
    assert_eq!(result["summary"]["focus_row_states"]["high_level"], "iI");
    result["high_level"] = json!([{"status":"working"}]);
    super::finalize_orientation(&mut result, &[], 1000, "legacy", &[], 2000, "context");
    assert_eq!(result["summary"]["focus_row_states"]["high_level"], "i");
    assert_eq!(result["summary"]["returned_focus"]["in_progress"], 1);
    result["high_level"] = json!([]);
    super::finalize_orientation(&mut result, &[], 1000, "legacy", &[], 2000, "context");
    assert!(result["summary"].get("focus_row_states").is_none());
    assert_eq!(result["summary"]["returned_focus"]["in_progress"], 0);
    result["high_level"] = json!([{"status":"in_progress","blocked":true}]);
    super::finalize_orientation(&mut result, &[], 1000, "legacy", &[], 2000, "context");
    assert_eq!(result["summary"]["returned_focus"]["in_progress"], 1);
    let states = "I".repeat(300);
    let mut payload =
        json!({"summary":{"focus_row_states":{"low_level":states}},"title":"x".repeat(300)});
    assert!(compact_strings(&mut payload, true));
    assert_eq!(payload["summary"]["focus_row_states"]["low_level"], states);
}

/// Complete orientation responses compact strings, declared rows and nested payloads with receipts.
#[test]
#[allow(clippy::too_many_lines)]
fn orientation_budget_recovery_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    for toon in [true, false] {
        for budget in ["1", "800", "1600", "6000", "12000"] {
            let options = ListOptions {
                toon,
                token_budget: Some(budget.to_owned()),
                ..Default::default()
            };
            let source = json!({"depth":"brief","summary":{"returned_focus":{"active_items":40,"in_progress":40,"open":0,"blocked":0}},"high_level":[],"low_level":(0..40).map(|i|json!({"id":format!("sample-{i}"),"title":"x".repeat(500),"status":"in_progress","tags":vec!["a";40]})).collect::<Vec<_>>(),"blocked_fallback":[],"agenda":{"events":[]}});
            let output = super::apply_orientation(
                source.into(),
                &options,
                "context",
                &[
                    "high_level",
                    "low_level",
                    "blocked_fallback",
                    "agenda.events",
                ],
            )?;
            assert!(output["read_output"].is_object());
            if output.get("output_budget_exceeded").is_none() {
                assert_eq!(output["read_output"]["within_budget"], true);
                assert_eq!(
                    output["summary"]["returned_focus"]["in_progress"],
                    output["low_level"].as_array().map_or(0, Vec::len)
                );
            }
        }
        let source = json!({"projection":{"row_key":"compact_history"},"count":400,"compact_history":(0..400).map(|i|json!({"index":i,"ts":"2026-08-22T10:00:00.000Z","op":"update","author":"fixture","patch_ops":1,"changed_fields":["title"]})).collect::<Vec<_>>()});
        let output = super::apply_orientation(
            source.into(),
            &ListOptions {
                toon,
                brief: true,
                ..Default::default()
            },
            "history",
            &["compact_history"],
        )?;
        assert_eq!(output["read_output"]["within_budget"], true);
        assert!(output["count"].as_u64().is_some_and(|n| n < 400));
        assert_eq!(output["has_more"], true);
        let output=super::apply_orientation(json!({"high_level":[],"low_level":[{"id":"sample-a","tags":vec!["abc";1000]}],"summary":{}}).into(),&ListOptions{toon,token_budget:Some("1000".to_owned()),..Default::default()},"context",&["low_level"])?;
        assert!(output["read_output"].is_object());
    }
    for raw in ["", "bad"] {
        assert!(
            super::apply_orientation(
                json!({}).into(),
                &ListOptions {
                    token_budget: Some(raw.to_owned()),
                    ..Default::default()
                },
                "context",
                &[]
            )?
            .get("read_output")
            .is_none()
        );
    }
    assert!(
        super::apply_orientation(
            json!({}).into(),
            &ListOptions {
                full: true,
                ..Default::default()
            },
            "history",
            &[]
        )?
        .get("read_output")
        .is_none()
    );
    Ok(())
}

/// A compacted nested collection cannot advertise a declared-row continuation it did not truncate.
#[test]
fn orientation_nested_only_recovery() {
    let state = super::OrientationRows {
        path: "low_level".to_owned(),
        pointer: "/low_level".to_owned(),
        rows: vec![json!({"id":"a"})],
        fingerprint: "snapshot".to_owned(),
    };
    let mut result = json!({"low_level":[{"id":"a"}],"read_output":{"rows_compacted":true,"compacted_row_paths":["low_level.0.tags"]}});
    super::finalize_orientation(&mut result, &[state], 1000, "legacy", &[], 2000, "context");
    assert_eq!(result["continuation_kind"], "none");
    assert_eq!(
        result["output_budget_truncation"]["continuation_available"],
        false
    );
}

/// Orientation preserves the codec's refusal for a string cut between surrogate code units.
#[test]
fn orientation_toon_cut_refusal() {
    // The pair-preserving cut never manufactures a lone surrogate, so a real
    // astral title renders fine (GH-1478); only a retained malformed override
    // keeps the pinned codec refusal.
    let title = format!("{}😀{}", "x".repeat(239), "y".repeat(20_000));
    let source =
        json!({"message":title,"high_level":[],"low_level":[],"blocked_fallback":[],"summary":{}});
    assert!(
        super::apply_orientation(
            source.clone().into(),
            &ListOptions {
                toon: true,
                token_budget: Some("1000".to_owned()),
                ..Default::default()
            },
            "context",
            &["high_level", "low_level"]
        )
        .is_ok()
    );
    let mut malformed = crate::ListOutput::from(source);
    malformed.cuts.insert(
        "/message".to_owned(),
        format!("\"{}\\ud83d…\"", "x".repeat(239)),
    );
    assert!(matches!(
        super::apply_orientation(
            malformed,
            &ListOptions {
                toon: true,
                token_budget: Some("1000".to_owned()),
                ..Default::default()
            },
            "context",
            &["high_level", "low_level"]
        ),
        Err(crate::PmRustError::ToonEncoding { .. })
    ));
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
    let selected_context = super::receipt_for(
        &ListOptions {
            brief: true,
            ..ListOptions::default()
        },
        false,
        "context",
    );
    assert_eq!(
        selected_context["legacy_aliases_used"],
        json!(["--section"])
    );
    assert_eq!(selected_context["requested_dimensions"], json!(["include"]));
    assert_eq!(
        selected_context["migration_hints"],
        json!(["--section is a compatibility alias; prefer --output-include <csv>."])
    );
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
    assert_toon_budget_contract()?;
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
        command: "list",
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
    assert!(compact_strings(&mut value, true));
    assert_eq!(value["rows"][0], format!("{}…", "😀".repeat(120)));
    assert_eq!(value["rows"][1], "x".repeat(240));
    assert!(!compact_strings(&mut json!([true, null, "short"]), true));
    let mut measured = crate::ListOutput::from(json!({"read_output":{"estimated_tokens":0}}));
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
        command: "list",
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

/// Checks default TOON ceilings and the retained malformed-cut codec refusal.
fn assert_toon_budget_contract() -> Result<(), Box<dyn std::error::Error>> {
    let toon = apply(
        envelope(200, 400),
        &ListOptions {
            toon: true,
            ..ListOptions::default()
        },
    )?;
    assert_eq!(toon["read_output"]["budget_tokens"], 4000);
    assert_eq!(toon["read_output"]["within_budget"], true);
    assert_eq!(toon["has_more"], true);
    let options = ListOptions {
        toon: true,
        ..ListOptions::default()
    };
    // The pair-preserving cut keeps the astral character intact, so bounded
    // TOON output renders instead of refusing (GH-1478).
    let title = format!("{}😀{}", "x".repeat(239), "y".repeat(300));
    let mut cut: crate::ListOutput = envelope(1, 0).into();
    cut.value["items"][0]["title"] = json!(title);
    compact_strings(&mut cut.value, true);
    assert!(super::apply(cut, &options).is_ok());
    let mut original = envelope(200, 400);
    original["items"][0]["title"] = json!(format!("{}😀{}", "x".repeat(239), "y".repeat(300)));
    assert!(apply(original, &options).is_ok());
    // A retained malformed override still reaches the pinned codec refusal.
    let mut malformed: crate::ListOutput = envelope(1, 0).into();
    malformed.cuts.insert(
        "/items/0/title".to_owned(),
        format!("\"{}\\ud83d…\"", "x".repeat(239)),
    );
    assert!(matches!(
        super::apply(malformed, &options),
        Err(crate::PmRustError::ToonEncoding { .. })
    ));
    Ok(())
}

/// Final self-referential receipts never advertise a ceiling smaller than the retained packet.
#[test]
fn stabilized_orientation_receipts_respect_final_ceilings() -> Result<(), Box<dyn std::error::Error>>
{
    for toon in [false, true] {
        let mut both_receipts = 0;
        for padding in 0..8 {
            for budget in [319, 320, 321, 998, 999, 1000, 1001] {
                let mut source: crate::ListOutput = json!({
                    "output_default":"toon", "depth":"brief",
                    "low_level":[{"id":"sample-a","tags":vec!["evidence"; 180 + padding]}],
                    "context_intent":{"command":"context","estimated_tokens":0,"within_budget":true,
                        "token_budget":budget,"declared_token_budget":320,"declaration_feasible":true,"result_omitted":false}
                }).into();
                super::update(&mut source, "context_intent", false);
                let output = super::apply_orientation(
                    source,
                    &ListOptions {
                        toon,
                        token_budget: Some(budget.to_string()),
                        ..Default::default()
                    },
                    "context",
                    &["low_level"],
                )?;
                if output["context_intent"].is_object() && output["read_output"].is_object() {
                    both_receipts += 1;
                    let compact = super::estimate(&output, false);
                    assert_eq!(output["context_intent"]["estimated_tokens"], compact);
                    assert_eq!(output["context_intent"]["within_budget"], compact <= budget);
                    assert_eq!(
                        output["context_intent"]["declaration_feasible"],
                        compact <= 320
                    );
                }
                if output["read_output"]["within_budget"] == true {
                    assert!(super::measure(&output, toon) <= budget);
                }
            }
        }
        assert!(
            both_receipts > 0,
            "must check returned packets with both receipts"
        );
        for budget in [99, 100, 999, 1000] {
            let mut stale: crate::ListOutput = json!({"padding":"x".repeat(budget * 4),
                "context_intent":{"estimated_tokens":0,"within_budget":true,"declaration_feasible":true,
                    "token_budget":budget,"declared_token_budget":budget,"result_omitted":false},
                "read_output":{"estimated_tokens":0}}).into();
            super::update_orientation_output(&mut stale, toon);
            assert_eq!(stale["context_intent"]["within_budget"], false);
            assert_eq!(stale["context_intent"]["declaration_feasible"], false);
            assert_eq!(
                stale["context_intent"]["estimated_tokens"],
                super::estimate(&stale, false)
            );
            assert_eq!(
                stale["read_output"]["estimated_tokens"],
                super::measure(&stale, toon)
            );
        }
    }
    Ok(())
}

/// Every shared read command enforces its wire ceiling after all receipt updates.
#[test]
fn shared_read_commands_bound_final_self_referential_receipts()
-> Result<(), Box<dyn std::error::Error>> {
    for command in ["list", "search", "files", "docs", "test"] {
        for toon in [false, true] {
            for padding in 0..4 {
                for budget in [319, 320, 321, 998, 999, 1000, 1001] {
                    let source = envelope(8, 80 + padding);
                    let options = ListOptions {
                        toon,
                        output_budget: Some(budget.to_string()),
                        intent: (command == "list").then(|| "triage".to_owned()),
                        ..Default::default()
                    };
                    let source = if command == "list" {
                        crate::list_intent::attach(source, &options, 5000)
                    } else {
                        source.into()
                    };
                    let output = super::apply_command(source, &options, command)?;
                    if output["read_output"]["within_budget"] == true {
                        assert!(
                            super::measure(&output, toon) <= budget,
                            "{command}, toon={toon}, padding={padding}, budget={budget}"
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
/// Item metadata named `context_intent` compacts while the top-level declaration stays immutable.
#[test]
fn item_metadata_named_context_intent_compacts_under_budgets()
-> Result<(), Box<dyn std::error::Error>> {
    let long = "m".repeat(3000);
    for toon in [false, true] {
        let source = json!({
            "items": [{"id": "sample-a", "title": "a", "context_intent": long}],
            "count": 1,
            "total": 1,
            "has_more": false,
            "truncated": false,
            "next_cursor": null
        });
        for (budget, delivered) in [
            (150usize, false),
            (300, true),
            (500, true),
            (700, true),
            (5000, true),
        ] {
            let output = super::apply_command(
                source.clone().into(),
                &ListOptions {
                    output_budget: Some(budget.to_string()),
                    toon,
                    ..Default::default()
                },
                "list",
            )?;
            let omitted = output.get("output_budget_exceeded").is_some();
            assert_eq!(!omitted, delivered, "budget {budget}, toon {toon}");
            if delivered && budget <= 700 {
                // The fitting item is delivered with the metadata compacted,
                // instead of the whole result being omitted.
                let cell = output["items"][0]["context_intent"]
                    .as_str()
                    .ok_or("missing metadata cell")?;
                assert_eq!(
                    cell.encode_utf16().count(),
                    241,
                    "budget {budget}, toon {toon}"
                );
                assert!(cell.ends_with('…'), "budget {budget}, toon {toon}");
            }
            if delivered && budget == 400 {
                // A generous budget never compacts and never omits.
                assert_eq!(
                    output["items"][0]["context_intent"].as_str(),
                    Some(long.as_str())
                );
            }
        }
        // The top-level declaration protection is unaffected: a large
        // declaration string under the reserved key stays byte-identical.
        let mut declaration = json!({"command":"context","description":"m".repeat(600)});
        let mut envelope = json!({"context_intent": declaration.clone(), "items": [], "count": 0});
        super::compact_strings(&mut envelope, true);
        assert_eq!(envelope["context_intent"], declaration);
        declaration["future"] = json!(null);
        let mut unprotected = json!({"rows": [{"context_intent": declaration.clone()}]});
        super::compact_strings(&mut unprotected, false);
        assert!(
            unprotected["rows"][0]["context_intent"]["description"]
                .as_str()
                .is_some_and(|text| text.ends_with('…')),
            "nested identical metadata compacts without top-level protection"
        );
    }
    Ok(())
}

/// Every orientation command compacts nested user strings before dropping a fitting result.
#[test]
fn orientation_wire_string_compaction_retains_fitting_rows()
-> Result<(), Box<dyn std::error::Error>> {
    for command in ["context", "history", "deps"] {
        for toon in [false, true] {
            for budget in [1000usize, 2000] {
                let source = json!({"rows":[{"id":"sample-a","title":"m".repeat(10000),"context_intent":"n".repeat(10000)}]});
                let output = super::apply_orientation(
                    source.into(),
                    &ListOptions {
                        toon,
                        token_budget: Some(budget.to_string()),
                        ..Default::default()
                    },
                    command,
                    &["rows"],
                )?;
                assert_ne!(
                    output["read_output"]["result_omitted"], true,
                    "{command} {toon} {budget}"
                );
                assert_eq!(output["read_output"]["strings_compacted"], true);
                for key in ["title", "context_intent"] {
                    assert_eq!(
                        output["rows"][0][key]
                            .as_str()
                            .map(|text| text.encode_utf16().count()),
                        Some(241)
                    );
                }
                assert!(super::measure(&output, toon) <= budget);
            }
        }
    }
    Ok(())
}
