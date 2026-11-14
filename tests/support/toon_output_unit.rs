//! TOON rendering contracts for sparse rows, compaction and complete-list summaries.

use serde_json::{Value, json};

/// Object rows retain explicit null, empty and nested facts while envelopes remain compact.
#[test]
fn toon_rows_preserve_explicit_facts() -> Result<(), Box<dyn std::error::Error>> {
    let source = json!({
        "empty_envelope": [],
        "rows": [
            {"id":"one","missing":null,"tags":[],"metadata":{},"nested":[null,{}]},
            {},
            null
        ]
    });
    let expected = json!({"rows":[
        {"id":"one","missing":null,"tags":[],"metadata":{},"nested":[null,{}]},
        {}
    ]});
    assert_eq!(super::compact(&source), Some(expected.clone()));
    let rendered = super::stringify_toon(&source.into())?;
    assert_eq!(rendered, format!("{}\n", super::render(&expected, 0)));
    // Fixed wire captured independently from the pinned published formatter.
    assert_eq!(
        rendered,
        "rows[2]:\n  - id: one\n    missing: null\n    tags: []\n    metadata:\n    nested[2]:\n      - null\n      -\n  -\n"
    );
    Ok(())
}

/// Independent wire examples cover empty values, tables and complete brief summaries.
#[test]
fn toon_wire_shapes_preserve_values_and_disclose_sparse_cells()
-> Result<(), Box<dyn std::error::Error>> {
    use super::{compact, complete_brief, render, table};
    assert_eq!(render(&json!({}), 0), "{}");
    assert_eq!(render(&json!([]), 0), "[]");
    assert_eq!(render(&json!("quoted"), 1), "  \"quoted\"");
    assert_eq!(
        compact(
            &json!({"absent":null,"array":[],"object":{},"groups":{"omitted_field_groups":[]}})
        ),
        Some(json!({"groups":{"omitted_field_groups":[]}}))
    );
    for value in [
        Value::Null,
        json!([]),
        json!([{}, 1]),
        json!([{"a":1},{"b":2},{"c":3}]),
    ] {
        assert!(table(&value, false).is_none());
    }
    let rows = json!([{"id":"one","tags":["a"]},{"id":"two","parent":"root","tags":null}]);
    let (encoded, encoding) = table(&rows, false).ok_or("missing sparse table")?;
    assert_eq!(
        encoded,
        json!([{"id":"one","tags":"[\"a\"]","parent":null},{"id":"two","tags":"null","parent":"root"}])
    );
    assert_eq!(
        encoding,
        json!({"json_columns":["tags"],"absent":[[0,"parent"]]})
    );
    let brief = json!({"items":[],"count":0,"total":0,"has_more":false,"truncated":false,"projection":{"mode":"brief","fields":["id","status","type","title"]},"completeness":{"status":"complete","unreadable_item_count":0,"unreadable_directory_count":0},"sorting":{"sort":"default"},"omission_receipt":{"has_omissions":true,"omitted_field_group_count":1,"omitted_field_groups":[{"name":"full_item_fields","restore_with":"--full"}]}});
    assert_eq!(
        complete_brief(&brief),
        json!({"items":[],"count":0,"sorting":{"sort":"default"},"details":"--full"})
    );
    for (pointer, value) in [
        ("/items", json!(null)),
        ("/details", json!("existing")),
        ("/projection/mode", json!("full")),
        ("/completeness/status", json!("partial")),
        ("/has_more", json!(true)),
        ("/truncated", json!(true)),
        ("/count", json!(1)),
        ("/total", json!(1)),
        ("/omission_receipt/omitted_field_groups", json!([])),
    ] {
        let mut altered = brief.clone();
        if pointer == "/details" {
            altered["details"] = value;
        } else if let Some(field) = altered.pointer_mut(pointer) {
            *field = value;
        }
        assert_eq!(complete_brief(&altered), altered);
    }
    let mut missing_sorting = brief.clone();
    missing_sorting
        .as_object_mut()
        .ok_or("object")?
        .shift_remove("sorting");
    assert!(complete_brief(&missing_sorting).get("sorting").is_none());
    let mut sorted = brief;
    sorted["sorting"] = json!({"sort":"title","order":"asc"});
    assert_eq!(complete_brief(&sorted)["sorting"], sorted["sorting"]);
    for value in [Value::Null, json!({"empty":{}})] {
        assert_eq!(super::stringify_toon(&value.into())?, "{}\n");
    }
    assert_eq!(
        render(&json!({"empty":{},"list":[]}), 0),
        "empty: {}\nlist: []"
    );
    // Explicit encoding metadata suppresses automatic re-encoding of table rows.
    let encoded = render(
        &json!({"rows":[{"nested":[1]},{"nested":[2]}],"rows_encoding":{"json_columns":["nested"]}}),
        0,
    );
    assert!(encoded.contains("rows_encoding:"));
    // A union table can be larger than an ordinary pair of tiny nested rows.
    assert!(render(&json!({"rows":[{"a":[1]},{"a":[2]}]}), 0).contains("rows[2]:"));
    assert_eq!(
        super::javascript_numbers(
            "rows[2]{id,n}:\n  \"1e+21\",1000000000000000000000\n  \"escaped\\\"quote\",-0.0000001\n"
        ),
        "rows[2]{id,n}:\n  \"1e+21\",1e+21\n  \"escaped\\\"quote\",-1e-7\n"
    );
    let mut cut: crate::ListOutput =
        json!({"rows":[{"title":"replacement"}],"a/b~c":"safe"}).into();
    cut.cuts
        .insert("/removed".to_owned(), "irrelevant".to_owned());
    assert!(super::stringify_toon(&cut).is_ok());
    cut.cuts
        .insert("/rows/0/title".to_owned(), "\"prefix\\ud83d…\"".to_owned());
    assert!(matches!(
        super::stringify_toon(&cut),
        Err(crate::PmRustError::ToonEncoding { .. })
    ));
    assert_eq!(super::javascript_numbers("n: 123abc"), "n: 123abc");
    assert_eq!(
        super::javascript_numbers(
            "  title 1000000000000000000000 tokens, 1000000000000000000000  \n  - 1000000000000000000000\n"
        ),
        "  title 1000000000000000000000 tokens, 1e+21  \n  - 1e+21\n"
    );
    assert_read_number_domain()?;
    Ok(())
}

/// Malformed or short raw fragments refuse TOON without panicking.
#[test]
fn raw_overrides_refuse_toon_without_panicking() {
    for raw in ["invalid", "\"\\\\u\""] {
        let output = crate::ListOutput::with_json_overrides(
            json!("safe"),
            std::collections::BTreeMap::from([(String::new(), raw.to_owned())]),
        );
        assert!(matches!(
            super::stringify_toon(&output),
            Err(crate::PmRustError::ToonEncoding { .. })
        ));
    }
}

/// Verifies scalar and nested read coercion without changing numeric strings.
fn assert_read_number_domain() -> Result<(), Box<dyn std::error::Error>> {
    use super::{render, table};
    assert_eq!(
        table(&json!([{}]), false),
        Some((json!([{}]), json!({"json_columns":[],"absent":[]})))
    );
    let (rows, _) = table(
        &json!([{"nested":[9_007_199_254_740_993_u64]},{"nested":[9_007_199_254_740_992_u64]}]),
        false,
    )
    .ok_or("missing numeric table")?;
    assert_eq!(rows[0]["nested"], "[9007199254740992]");
    assert_eq!(
        render(&json!({"n":9_007_199_254_740_993_u64}), 0),
        "n: 9007199254740992"
    );
    let mut numeric = json!([-9_007_199_254_740_993_i64, 9_007_199_254_740_993_u64, -42, 42, 1.5, {"safe":null,"text":"9007199254740993"}]);
    crate::json_output::normalize_read_numbers(&mut numeric);
    assert_eq!(
        crate::stringify_json(&numeric, false),
        "[-9007199254740992,9007199254740992,-42,42,1.5,{\"safe\":null,\"text\":\"9007199254740993\"}]"
    );
    Ok(())
}
/// Brief projection keeps continuation and source evidence, rejecting ambiguous metadata.
#[test]
fn brief_envelopes_preserve_partial_and_unchecked_sources() {
    let base = json!({"items":[{"id":"a"}],"count":1,"total":1,"has_more":false,"truncated":false,"next_cursor":null,"projection":{"mode":"brief","fields":["id"]},"sorting":{"sort":"default","order":"asc"},"completeness":{"status":"complete","unreadable_item_count":0,"unreadable_directory_count":0},"omission_receipt":{"has_omissions":true,"omitted_field_group_count":1,"omitted_field_groups":[{"name":"full_item_fields","restore_with":"--full"}]}});
    assert_eq!(
        super::complete_brief(&base),
        json!({"items":[{"id":"a"}],"count":1,"details":"--full"})
    );
    let mut page = base.clone();
    page["has_more"] = json!(true);
    page["truncated"] = json!(true);
    page["total"] = json!(2);
    page["next_cursor"] = json!("next");
    assert_eq!(
        super::complete_brief(&page),
        json!({"items":[{"id":"a"}],"count":1,"total":2,"has_more":true,"completeness":{"status":"complete"},"next_cursor":"next","details":"--full"})
    );
    for (pointer, value) in [
        ("/completeness/status", json!("unchecked")),
        ("/completeness/extra", json!("retained")),
    ] {
        let mut altered = base.clone();
        if pointer.ends_with("extra") {
            altered["completeness"]["extra"] = value;
        } else {
            altered["completeness"]["status"] = value;
        }
        let projected = super::complete_brief(&altered);
        assert_eq!(projected["total"], 1);
        assert_eq!(projected["has_more"], false);
        assert_eq!(projected.pointer(pointer), altered.pointer(pointer));
    }
    for (pointer, value) in [
        ("/has_more", Value::Null),
        ("/total", json!("bad")),
        ("/total", json!(1.5)),
        ("/total", json!(9_007_199_254_740_992_u64)),
        ("/total", json!(-1)),
        ("/total", json!(0)),
        ("/count", json!(2)),
        ("/truncated", json!(true)),
        ("/next_cursor", json!("unexpected")),
        ("/projection/fields", json!([])),
        ("/projection/fields", json!(["id", "status"])),
        ("/completeness/unreadable_item_count", json!(1)),
        ("/completeness/unreadable_directory_count", json!(1)),
    ] {
        let mut altered = base.clone();
        if let Some(target) = altered.pointer_mut(pointer) {
            *target = value;
        }
        assert_eq!(super::complete_brief(&altered), altered, "{pointer}");
    }
    for cursor in [Value::Null, json!(""), json!(0)] {
        let mut altered = page.clone();
        altered["next_cursor"] = cursor;
        assert_eq!(super::complete_brief(&altered), altered);
    }
    page["total"] = json!(1);
    assert_eq!(super::complete_brief(&page), page);
    let mut extra = base.clone();
    extra["projection"]["extra"] = json!(true);
    assert_eq!(super::complete_brief(&extra), extra);
    assert!(!super::standard_brief(&Value::Null));
    assert!(super::standard_pagination(
        &json!({"count":1.0,"total":1.0,"has_more":false,"truncated":false}),
        1
    ));
}

/// Sparse and empty projections retain recovery-relevant shapes instead of hiding their contract.
#[test]
fn round5_toon_projection_and_sorting_corpus() -> Result<(), Box<dyn std::error::Error>> {
    for fields in [
        json!(null),
        json!([]),
        json!([""]),
        json!([null]),
        json!(["id"]),
    ] {
        let value = json!({"mode":"fields","fields":fields});
        assert_eq!(super::standard_brief(&value), fields == json!(["id"]));
    }
    assert!(!super::rows_match(&json!({"items":[]})));
    let mut empty = serde_json::Map::new();
    super::compact_echoes(
        &mut empty,
        &json!({"projection":{"mode":"fields","fields":["id"]},"items":[],"filters":{"custom":"kept"},"sorting":{"sort":"default","order":"asc","effective_keys":"invalid"}}),
    );
    assert_eq!(empty["projection"]["fields"], json!(["id"]));
    assert_eq!(empty["sorting"]["effective_keys"], "invalid");
    for pairs in [
        json!(["bad"]),
        json!([["id"]]),
        json!([[null, "asc"]]),
        json!([["id", "other"]]),
    ] {
        let mut map = serde_json::Map::new();
        super::compact_echoes(
            &mut map,
            &json!({"sorting":{"sort":"default","order":"asc","effective_keys":pairs}}),
        );
        assert_eq!(map["sorting"]["effective_keys"], pairs);
    }
    for projection in [
        json!({"mode":"fields","fields":["id"]}),
        json!({"mode":"brief","fields":["id"]}),
    ] {
        for items in [
            json!(null),
            json!([null]),
            json!([{}]),
            json!([{"extra":1}]),
            json!([{"id":"kept"}]),
        ] {
            let rows = json!({"items":items,"projection":projection});
            assert_eq!(super::rows_match(&rows), items == json!([{"id":"kept"}]));
        }
    }
    assert_eq!(super::stringify_toon(&Value::Null.into())?, "{}\n");
    Ok(())
}

/// Search metadata is compacted only when its complete or paged declaration is self-consistent.
#[test]
fn round5_search_projection_metadata_corpus() -> Result<(), Box<dyn std::error::Error>> {
    let fields = json!({"query":"fixture","items":[],"count":0,"projection":{"mode":"fields","fields":["id"]},"now":"2026-08-22T10:00:00Z","omission_receipt":{"has_omissions":true,"omitted_field_group_count":1,"omitted_field_groups":[{"name":"full_item_fields","restore_with":"--full"}]}});
    for mode in [None, Some("settings"), Some("cli")] {
        let mut source = fields.clone();
        if let Some(mode) = mode {
            source["mode_source"] = json!(mode);
        }
        let result = super::complete_search(&source);
        assert_eq!(result["details"], "--full");
        assert_eq!(result["projection"], fields["projection"]);
        assert_eq!(
            result.get("mode_source").is_some(),
            mode == Some("settings")
        );
    }
    for (pointer, value) in [
        ("/query", Value::Null),
        ("/items", Value::Null),
        ("/now", Value::Null),
        ("/projection/mode", json!("full")),
        ("/omission_receipt", json!({})),
    ] {
        let mut source = fields.clone();
        *source.pointer_mut(pointer).ok_or("search pointer")? = value;
        assert_eq!(super::complete_search(&source), source);
    }
    let mut details = fields.clone();
    details["details"] = json!("already bounded");
    assert_eq!(super::complete_search(&details), details);
    for changed in [
        json!({"has_more":false}),
        json!({"truncated":false}),
        json!({"next_cursor":"opaque"}),
        json!({"total":1}),
        json!({"count":1}),
    ] {
        let mut source = fields.clone();
        source
            .as_object_mut()
            .ok_or("search envelope")?
            .extend(changed.as_object().ok_or("search patch")?.clone());
        assert_eq!(super::complete_search(&source), source);
    }
    let standard = json!({"query":"fixture","items":[],"count":0,"mode_source":"cli","omission_receipt":{"has_omissions":true,"omitted_field_group_count":1,"omitted_field_groups":[{"name":"projection_metadata","restore_with":"--full"}]}});
    assert_eq!(super::complete_search(&standard)["mode_source"], "cli");
    let mut without_mode = standard.clone();
    without_mode
        .as_object_mut()
        .ok_or("standard search")?
        .shift_remove("mode_source");
    let projected = super::complete_search(&without_mode);
    assert_eq!(projected["details"], "--full");
    assert!(projected.get("mode_source").is_none());
    let mut page = fields;
    page["items"] = json!([{"id":"first"}]);
    page["count"] = json!(1);
    page["total"] = json!(2);
    page["has_more"] = json!(true);
    page["truncated"] = json!(true);
    page["next_cursor"] = json!("opaque");
    let projected = super::complete_search(&page);
    assert_eq!(projected["details"], "--full");
    assert_eq!(projected["items"], page["items"]);
    assert_eq!(projected["total"], 2);
    assert_eq!(projected["next_cursor"], "opaque");
    Ok(())
}

/// Semicolons, empty strings and typed values stay in JSON cells rather than ambiguous list cells.
#[test]
fn round5_toon_list_cell_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    for (list, expected) in [
        (json!([""]), "[\"\"]"),
        (json!(["a;b"]), "[\"a;b\"]"),
        (json!([true]), "[true]"),
    ] {
        let (rows, receipt) = super::table(
            &json!([{"id":"one","tags":list},{"id":"two","tags":[]}]),
            true,
        )
        .ok_or("table")?;
        assert_eq!(rows[0]["tags"], expected);
        assert_eq!(receipt["json_columns"], json!(["tags"]));
    }
    Ok(())
}

/// Numeric legacy limits and string limits use the same canonical echo comparison.
#[test]
fn round5_numeric_filter_limit_echo() {
    for limit in [json!(2), json!("2")] {
        let mut map = serde_json::Map::new();
        super::compact_echoes(
            &mut map,
            &json!({"filters":{"limit":limit,"tag":"kept"},"applied_limit":2}),
        );
        assert_eq!(map["filters"], json!({"tag":"kept"}));
    }
}

/// Equivalent field echoes disappear only when their row declaration already preserves the information.
#[test]
fn round5_equivalent_field_filter_echoes() {
    let mut map = serde_json::Map::new();
    super::compact_echoes(
        &mut map,
        &json!({"filters":{"fields":" id, , title "},"projection":{"mode":"fields","fields":["id","title"]},"items":[{"id":"one","title":"kept"}]}),
    );
    assert!(map.is_empty());
    super::compact_echoes(
        &mut map,
        &json!({"filters":{"fields":"id,title"},"projection":{"mode":"fields","fields":["id"]},"items":[{"id":"one"}]}),
    );
    assert_eq!(map["filters"]["fields"], "id,title");
}
