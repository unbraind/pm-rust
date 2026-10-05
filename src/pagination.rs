//! Published producer and snapshot continuation encodings.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::PmRustError;

/// Constructs a published cursor refusal without exposing tracker paths.
pub(crate) fn refusal(code: &'static str, detail: impl Into<String>) -> PmRustError {
    PmRustError::ReadCursor {
        code,
        detail: detail.into(),
    }
}

/// Serializes JSON with recursively sorted object keys for published hashing.
pub(crate) fn stable(value: &Value) -> String {
    fn sorted(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut map = map
                    .iter()
                    .map(|(key, value)| (key.clone(), sorted(value)))
                    .collect::<serde_json::Map<_, _>>();
                map.sort_keys();
                Value::Object(map)
            }
            Value::Array(rows) => Value::Array(rows.iter().map(sorted).collect()),
            scalar => scalar.clone(),
        }
    }
    sorted(value).to_string()
}

/// Encodes a JSON cursor with the published unpadded base64url alphabet.
pub(crate) fn encode(value: &Value) -> String {
    URL_SAFE_NO_PAD.encode(value.to_string())
}

/// Decodes JSON from a base64url cursor, preserving the caller's error contract.
fn decode(raw: &str, code: &'static str, detail: &str) -> Result<Value, PmRustError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|_| refusal(code, detail))?;
    serde_json::from_slice(&bytes).map_err(|_| refusal(code, detail))
}

/// Decodes and validates the public query-cursor shape and semantic fingerprint.
pub(crate) fn query(raw: &str, fingerprint: &str) -> Result<Value, PmRustError> {
    let raw = raw.trim();
    let code = "invalid_query_cursor";
    if raw.is_empty()
        || raw.len() > 4096
        || !raw
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(refusal(code, "Query cursor is malformed."));
    }
    let cursor = decode(raw, code, "Query cursor is malformed.")?;
    if cursor["version"] != 1
        || !cursor["fingerprint"].is_string()
        || cursor["after_id"].as_str().is_none_or(str::is_empty)
        || cursor.get("after_index").is_some_and(|v| !safe_integer(v))
        || cursor
            .get("snapshot")
            .is_some_and(|v| v.as_str().is_none_or(str::is_empty))
    {
        return Err(refusal(
            code,
            "Query cursor version or payload is unsupported.",
        ));
    }
    if cursor["fingerprint"] != fingerprint {
        return Err(refusal(
            code,
            format!(
                "Query cursor does not match the current filters, sort, or query ({} != {fingerprint}).",
                cursor["fingerprint"].as_str().unwrap_or_default()
            ),
        ));
    }
    Ok(cursor)
}

/// Recognizes nonnegative integers in JavaScript's exactly representable range.
fn safe_integer(value: &Value) -> bool {
    value.as_u64().is_some_and(|n| n <= 9_007_199_254_740_991)
}

/// Computes the query fingerprint from the published semantic contract.
pub(crate) fn fingerprint(contract: &Value) -> String {
    crate::history::sha256_digest(format!("list\0{}", stable(contract)).as_bytes())[..24].to_owned()
}

/// Creates a producer cursor after one delivered row.
pub(crate) fn after(fingerprint: &str, row: &Value, index: usize) -> String {
    encode(&json!({"version":1,"fingerprint":fingerprint,"after_id":row["id"],"after_index":index}))
}

/// Computes the published sixteen-character row-collection fingerprint.
pub(crate) fn collection(rows: &Value) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(
        stable(&json!({"path":"items","value":rows})).as_bytes(),
    ))[..16]
        .to_owned()
}

/// Validates an output continuation against the complete requested row collection.
pub(crate) fn output(raw: &str, rows: &Value) -> Result<Value, PmRustError> {
    let invalid = "The read-output continuation cursor is malformed or unsupported.";
    let code = "read_output_cursor_invalid";
    if raw.len() > 4096 {
        return Err(refusal(code, invalid));
    }
    let cursor = decode(raw, code, invalid)?;
    if cursor["v"] != 1
        || !matches!(
            cursor["c"].as_str(),
            Some(
                "list"
                    | "context"
                    | "search"
                    | "get"
                    | "next"
                    | "health"
                    | "deps"
                    | "graph"
                    | "history"
                    | "activity"
                    | "validate"
                    | "events"
                    | "contracts"
                    | "comments"
                    | "notes"
                    | "files"
                    | "docs"
                    | "stats"
                    | "aggregate"
                    | "duplicates"
                    | "package-catalog"
                    | "package-manage"
                    | "comments-audit"
                    | "assurance"
            )
        )
        || cursor["p"].as_str().is_none_or(str::is_empty)
        || !safe_integer(&cursor["o"])
        || !safe_integer(&cursor["n"])
        || cursor["n"] == 0
        || cursor["f"].as_str().is_none_or(str::is_empty)
    {
        return Err(refusal(code, invalid));
    }
    if cursor["c"] != "list" {
        return Err(refusal(
            "read_output_cursor_command_mismatch",
            format!(
                "The read-output cursor belongs to {}, not list.",
                cursor["c"].as_str().unwrap_or_default()
            ),
        ));
    }
    let length = rows.as_array().map_or(0, Vec::len);
    if cursor["p"] != "items"
        || cursor["n"] != length
        || cursor["o"].as_u64().unwrap_or_default() > length as u64
        || cursor["f"] != collection(rows)
    {
        return Err(refusal(
            "read_output_cursor_stale",
            "The read-output continuation no longer matches the declared row collection; restart the bounded read.",
        ));
    }
    Ok(cursor)
}

#[cfg(test)]
#[path = "../tests/support/pagination_unit.rs"]
mod tests;
