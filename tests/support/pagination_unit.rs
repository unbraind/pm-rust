//! Refusal-shape boundaries and canonical hash contracts for list continuations.

use super::{collection, encode, fingerprint, output, query, query_root, stable};
use serde_json::{Value, json};

/// Valid producer cursor with optional snapshot and fallback position.
fn producer() -> Value {
    json!({"version":1,"fingerprint":"expected","after_id":"demo-a","after_index":0,"snapshot":"snap"})
}
/// Valid snapshot continuation over two synthetic rows.
fn continuation(rows: &Value) -> Value {
    json!({"v":1,"c":"list","p":"items","o":1,"n":2,"f":collection(rows)})
}

#[test]
fn stable_hashes_ignore_object_key_order_but_keep_row_order() {
    assert_eq!(
        stable(&json!({"z":[{"b":2,"a":1}],"a":true})),
        "{\"a\":true,\"z\":[{\"a\":1,\"b\":2}]}"
    );
    assert_ne!(collection(&json!([1, 2])), collection(&json!([2, 1])));
}

#[test]
fn query_roots_match_node_without_changing_native_path_separators() {
    // Synthetic drive/share names exercise Windows namespace spelling on every OS.
    for (native, node) in [
        (r"\\?\Q:\fixture\.agents\pm", r"Q:\fixture\.agents\pm"),
        (
            r"\\?\UNC\fixture\share\.agents\pm",
            r"\\fixture\share\.agents\pm",
        ),
        (r"Q:\fixture\.agents\pm", r"Q:\fixture\.agents\pm"),
        (r"\\fixture\share\.agents\pm", r"\\fixture\share\.agents\pm"),
        ("fixture/.agents/pm", "fixture/.agents/pm"),
        (r"fixture\name/.agents/pm", r"fixture\name/.agents/pm"),
    ] {
        assert_eq!(query_root(native), node);
    }
    // Measured with the published 2026.10.5 SDK's createQueryFingerprint.
    let contract = json!({"pmRoot":query_root(r"\\?\Q:\fixture\.agents\pm")});
    assert_eq!(fingerprint(&contract), "86cd0674cda9746c0b6a58d2");
    assert_ne!(
        fingerprint(&json!({"pmRoot":r"\\?\Q:\fixture\.agents\pm"})),
        fingerprint(&contract)
    );
}

#[test]
fn producer_refusals_distinguish_malformed_unsupported_and_mismatched()
-> Result<(), Box<dyn std::error::Error>> {
    for raw in [
        "",
        " ",
        "a",
        "a=",
        "bad",
        "AA",
        "a-",
        "a_",
        &"a".repeat(4097),
    ] {
        assert!(query(raw, "expected").is_err(), "accepted {raw:?}");
    }
    let valid = producer();
    assert_eq!(query(&format!(" {} ", encode(&valid)), "expected")?, valid);
    assert!(query(&encode(&valid), "different").is_err());
    for (key, value) in [
        ("version", json!(2)),
        ("fingerprint", json!(null)),
        ("after_id", json!(null)),
        ("after_id", json!("")),
        ("after_index", json!(-1)),
        ("after_index", json!(1.5)),
        ("after_index", json!(9_007_199_254_740_992u64)),
        ("snapshot", json!(0)),
        ("snapshot", json!("")),
    ] {
        let mut cursor = valid.clone();
        cursor[key] = value;
        assert!(
            query(&encode(&cursor), "expected").is_err(),
            "accepted {cursor}"
        );
    }
    let legacy = json!({"version":1,"fingerprint":"expected","after_id":"demo-a"});
    assert_eq!(query(&encode(&legacy), "expected")?, legacy);
    Ok(())
}

#[test]
fn snapshot_refusals_validate_shape_command_collection_and_offset()
-> Result<(), Box<dyn std::error::Error>> {
    let rows = json!([{"id":"demo-a"},{"id":"demo-b"}]);
    let valid = continuation(&rows);
    assert_eq!(output(&encode(&valid), &rows)?, valid);
    for raw in ["a", "bad", &"a".repeat(4097)] {
        assert!(output(raw, &rows).is_err());
    }
    for (key, value) in [
        ("v", json!(2)),
        ("c", json!(null)),
        ("c", json!("unknown")),
        ("c", json!("get")),
        ("p", json!(null)),
        ("p", json!("")),
        ("p", json!("absent")),
        ("o", json!(-1)),
        ("o", json!(3)),
        ("n", json!(-1)),
        ("n", json!(0)),
        ("n", json!(3)),
        ("f", json!(null)),
        ("f", json!("")),
        ("f", json!("mismatch")),
    ] {
        let mut cursor = valid.clone();
        cursor[key] = value;
        assert!(
            output(&encode(&cursor), &rows).is_err(),
            "accepted {cursor}"
        );
    }
    let mut end = valid.clone();
    end["o"] = json!(2);
    assert_eq!(output(&encode(&end), &rows)?, end);
    assert!(output(&encode(&valid), &Value::Null).is_err());
    Ok(())
}
