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
        assert!(parse_toon(&toon_format::encode_default(&json!([value]))?).is_err());
    }
    Ok(())
}
