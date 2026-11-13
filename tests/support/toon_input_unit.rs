//! Published compact tables restore values and original field order.
use super::*;
use serde_json::json;

/// Real list pages retain list-valued and all-null columns through the public reader.
#[test]
fn round12_list_page_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let rows: Vec<_> = (0..32)
        .map(|i| json!({"id":format!("sample-{i}"),"tags":["alpha","beta"],"parent":null}))
        .collect();
    let source = json!({"items":rows,"count":32});
    let wire = crate::stringify_toon(&source.clone().into())?;
    assert!(wire.contains("list_columns:"));
    assert!(wire.contains("null_column_index"));
    assert_ne!(toon_format::decode_default::<Value>(&wire)?, source);
    assert_eq!(parse_toon(&wire)?, source);
    let ordinary = json!({"items":[{"id":"plain","tags":["contains;semicolon"]}],"count":1});
    assert_eq!(
        parse_toon(&crate::stringify_toon(&ordinary.clone().into())?)?,
        ordinary
    );
    Ok(())
}

/// Nested/sparse encodings retain empty lists and restore indexed null positions.
#[test]
fn round12_nested_sparse_table_restore() -> Result<(), Box<dyn std::error::Error>> {
    let encoded = json!({"page":{"items":[{"a":"","b":"{\"nested\":1}","missing":null},{"a":"x;y","b":"null","missing":"kept"}],"items_encoding":{"json_columns":["b"],"absent":[[0,"missing"]],"list_columns":{"a":";"},"null_columns":["n","last"],"null_column_index":[0,99]}}});
    let wire = toon_format::encode_default(&encoded)?;
    let restored = parse_toon(&wire)?;
    assert_eq!(
        restored["page"]["items"],
        json!([{"n":null,"a":[],"b":{"nested":1},"last":null},{"n":null,"a":["x","y"],"b":null,"missing":"kept","last":null}])
    );
    assert_eq!(
        restored["page"]["items"][0]
            .as_object()
            .ok_or("row")?
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["n", "a", "b", "last"]
    );
    let mut explicit = encoded;
    explicit["page"]["items_encoding"]["column_order"] =
        json!(["last", "b", "a", "n", "missing", "absent"]);
    explicit["page"]["items_encoding"]["null_column_index"] = json!([]);
    assert_eq!(
        parse_toon(&toon_format::encode_default(&explicit)?)?["page"]["items"][0]
            .as_object()
            .ok_or("row")?
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["last", "b", "a", "n"]
    );
    let public =
        json!({"not_a_table":{"public":"kept"},"not_a_table_encoding":{"public":"also kept"}});
    assert_eq!(parse_toon(&toon_format::encode_default(&public)?)?, public);
    assert_eq!(parse_toon("scalar: 1\n")?, json!({"scalar":1}));
    assert_eq!(parse_toon("[]\n")?, json!([]));
    Ok(())
}

/// Malformed table metadata and cells refuse rather than silently losing values.
#[test]
fn round12_invalid_table_read_refuses() -> Result<(), Box<dyn std::error::Error>> {
    assert!(parse_toon("items[2]{id}:\n  one\n").is_err());
    for value in [
        json!({"items":[],"items_encoding":{}}),
        json!({"items":[1],"items_encoding":{"json_columns":[],"absent":[]}}),
        json!({"items":[{"a":1}],"items_encoding":{"json_columns":[],"absent":[],"list_columns":{"a":";"}}}),
        json!({"items":[{"a":"x"}],"items_encoding":{"json_columns":[],"absent":[],"list_columns":{"a":""}}}),
        json!({"items":[{"a":1}],"items_encoding":{"json_columns":["a"],"absent":[]}}),
        json!({"items":[{"a":"bad json"}],"items_encoding":{"json_columns":["a"],"absent":[]}}),
    ] {
        assert!(
            parse_toon(&toon_format::encode_default(&value)?).is_err(),
            "{value}"
        );
        assert!(parse_toon(&toon_format::encode_default(&json!({"nested":value}))?).is_err());
        // A root array is encoded opaquely by the renderer, so its values are data.
        let opaque = json!([value]);
        assert_eq!(parse_toon(&toon_format::encode_default(&opaque)?)?, opaque);
    }
    Ok(())
}

#[path = "toon_data_corpus.rs"]
mod data_corpus;

/// Every JSON kind remains data inside item fields, in expanded and compact tables.
#[test]
fn round13_user_field_names_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    for item in data_corpus::items() {
        for count in [1, 32] {
            let source = json!({"items":vec![item.clone();count],"count":count});
            let wire = crate::stringify_toon(&source.clone().into())?;
            let decoded = parse_toon(&wire);
            assert!(decoded.is_ok(), "{decoded:?}");
            assert_eq!(decoded?, source, "{wire}");
        }
    }
    Ok(())
}

/// Valid-looking instructions inside data must neither remove fields nor rewrite cells.
#[test]
fn round13_instruction_shaped_item_fields_remain_opaque() -> Result<(), Box<dyn std::error::Error>>
{
    let item = data_corpus::instruction_shaped_item();
    let source = json!({"page":{"items":vec![item;32],"count":32}});
    let wire = crate::stringify_toon(&source.clone().into())?;
    assert_eq!(parse_toon(&wire)?, source);
    Ok(())
}

/// Partial, duplicate and empty column hints retain every delivered non-absent cell.
#[test]
fn round14_column_order_is_only_an_order_hint() -> Result<(), Box<dyn std::error::Error>> {
    for order in [json!([]), json!(["b", "b", "ghost"])] {
        let source = json!({"items":[{"a":"[]","b":3,"keep":"value","missing":null}],"items_encoding":{"json_columns":["a"],"absent":[[0,"missing"]],"null_columns":["n"],"null_column_index":[0],"column_order":order}});
        let result = parse_toon(&toon_format::encode_default(&source)?)?;
        assert_eq!(
            result["items"][0],
            json!({"a":[],"b":3,"keep":"value","n":null})
        );
        let keys: Vec<_> = result["items"][0]
            .as_object()
            .ok_or("row")?
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            if order.as_array().ok_or("order")?.is_empty() {
                vec!["n", "a", "b", "keep"]
            } else {
                vec!["b", "n", "a", "keep"]
            }
        );
    }
    Ok(())
}
