//! TOON rendering contracts for sparse rows, compaction and complete-list summaries.

use serde_json::{Value, json};

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
        json!([{}]),
        json!([{}, 1]),
        json!([{"a":1},{"b":2},{"c":3}]),
    ] {
        assert!(table(&value).is_none());
    }
    let rows = json!([{"id":"one","tags":["a"]},{"id":"two","parent":"root","tags":null}]);
    let (encoded, encoding) = table(&rows).ok_or("missing sparse table")?;
    assert_eq!(
        encoded,
        json!([{"id":"one","tags":"[\"a\"]","parent":null},{"id":"two","tags":"null","parent":"root"}])
    );
    assert_eq!(
        encoding,
        json!({"json_columns":["tags"],"absent":[[0,"parent"]]})
    );
    let brief = json!({"items":[],"count":0,"total":0,"has_more":false,"truncated":false,"projection":{"mode":"brief"},"completeness":{"status":"complete","unreadable_item_count":0,"unreadable_directory_count":0},"sorting":{"sort":"default"},"omission_receipt":{"omitted_field_groups":[{"name":"full_item_fields","restore_with":"--full"}]}});
    assert_eq!(
        complete_brief(&brief),
        json!({"items":[],"count":0,"details":"--full"})
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
    let (rows, _) = table(
        &json!([{"nested":[9_007_199_254_740_993_u64]},{"nested":[9_007_199_254_740_992_u64]}]),
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
