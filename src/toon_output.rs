//! Published TOON list projection, empty-field compaction and table rendering.

use serde_json::{Map, Value, json};

use crate::ListOutput;

/// Renders a list envelope using the published compact TOON wire contract.
///
/// # Errors
/// Returns the pinned codec refusal when a JSON string cut contains an unpaired surrogate.
pub fn stringify_toon(output: &ListOutput) -> Result<String, crate::PmRustError> {
    validate(output)?;
    Ok(render_output(output))
}

/// Renders an envelope whose encoding has already been validated by the read pipeline.
pub(crate) fn render_output(output: &ListOutput) -> String {
    let value = complete_brief(&output.value);
    let compacted = compact(&value).unwrap_or_else(|| json!({}));
    format!("{}\n", render(&compacted, 0))
}

/// Removes redundant metadata only when a complete identity list proves it redundant.
fn complete_brief(value: &Value) -> Value {
    if value["items"].is_array()
        && value.get("details").is_none()
        && value["projection"]["mode"] == "brief"
        && value["completeness"]
            == json!({"status":"complete","unreadable_item_count":0,"unreadable_directory_count":0})
        && value["has_more"] == false
        && value["truncated"] == false
        && value["count"] == value["items"].as_array().map_or(0, Vec::len)
        && value["total"] == value["count"]
        && value["omission_receipt"]["omitted_field_groups"]
            == json!([{"name":"full_item_fields","restore_with":"--full"}])
    {
        let mut projected = value.as_object().cloned().unwrap_or_default();
        for key in [
            "total",
            "has_more",
            "truncated",
            "next_cursor",
            "completeness",
            "projection",
            "now",
            "omission_receipt",
            "sorting",
        ] {
            projected.shift_remove(key);
        }
        if value["sorting"].is_object() {
            projected.insert("sorting".to_owned(), value["sorting"].clone());
        }
        projected.insert("details".to_owned(), json!("--full"));
        Value::Object(projected)
    } else {
        value.clone()
    }
}

/// Compacts envelope metadata while retaining explicit facts inside object rows.
fn compact(value: &Value) -> Option<Value> {
    compact_value(value, false)
}

/// Preserves null and empty descendants once traversal enters an object row.
fn compact_value(value: &Value, preserve_empty: bool) -> Option<Value> {
    match value {
        Value::Null => preserve_empty.then_some(Value::Null),
        Value::Array(rows) => {
            let rows: Vec<_> = rows
                .iter()
                .filter_map(|row| compact_value(row, preserve_empty || row.is_object()))
                .collect();
            (!rows.is_empty() || preserve_empty).then_some(Value::Array(rows))
        }
        Value::Object(fields) => {
            let fields: Map<_, _> = fields
                .iter()
                .filter_map(|(key, value)| {
                    let value = if key == "omitted_field_groups" && value == &json!([]) {
                        Some(value.clone())
                    } else {
                        compact_value(value, preserve_empty)
                    };
                    value.map(|value| (key.clone(), value))
                })
                .collect();
            (!fields.is_empty() || preserve_empty).then_some(Value::Object(fields))
        }
        _ => Some(value.clone()),
    }
}

/// Encodes arrays using the canonical TOON codec and applies nesting indentation.
fn encode(value: &Value, depth: usize) -> String {
    // Value serialization is infallible for the built-in JSON data model.
    javascript_numbers(&toon_format::encode_default(value).unwrap_or_default())
        .lines()
        .map(|line| format!("{}{line}", "  ".repeat(depth)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Renders object scalars with quoted strings and arrays with the TOON codec.
fn render(value: &Value, depth: usize) -> String {
    if let Some(fields) = value.as_object() {
        if fields.is_empty() {
            return "{}".to_owned();
        }
        fields
            .iter()
            .map(|(key, value)| {
                let indent = "  ".repeat(depth);
                if value.is_array() {
                    let expanded = encode(&json!({key:value}), depth);
                    if fields.contains_key(&format!("{key}_encoding")) {
                        expanded
                    } else {
                        table(value).map_or_else(
                            || expanded.clone(),
                            |(rows, encoding)| {
                                let tabular = encode(
                                    &json!({key:rows, format!("{key}_encoding"):encoding}),
                                    depth,
                                );
                                if tabular.len() < expanded.len() {
                                    tabular
                                } else {
                                    expanded.clone()
                                }
                            },
                        )
                    }
                } else if value.as_object().is_some_and(|fields| !fields.is_empty()) {
                    format!("{indent}{key}:\n{}", render(value, depth + 1))
                } else {
                    format!("{indent}{key}: {}", read_json(value))
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else if value.is_array() {
        encode(value, depth)
    } else {
        format!("{}{}", "  ".repeat(depth), read_json(value))
    }
}

/// Builds a bounded union-column table for nested or sparse object rows.
fn table(value: &Value) -> Option<(Value, Value)> {
    let rows = value.as_array()?;
    if rows.len() < 2 || !rows.iter().all(Value::is_object) {
        return None;
    }
    let mut columns = Vec::<String>::new();
    let mut present = 0;
    for row in rows {
        for key in row.as_object().into_iter().flat_map(Map::keys) {
            present += 1;
            if !columns.contains(key) {
                columns.push(key.clone());
            }
        }
    }
    if rows.len() * columns.len() > present * 2 {
        return None;
    }
    let nested: Vec<_> = columns
        .iter()
        .filter(|key| {
            rows.iter()
                .any(|row| row[*key].is_array() || row[*key].is_object())
        })
        .cloned()
        .collect();
    let mut absent = Vec::new();
    let mut encoded = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let mut fields = Map::new();
        for key in &columns {
            let value = if let Some(value) = row.get(key) {
                if nested.contains(key) {
                    json!(read_json(value))
                } else {
                    value.clone()
                }
            } else {
                absent.push(json!([index, key]));
                Value::Null
            };
            fields.insert(key.clone(), value);
        }
        encoded.push(Value::Object(fields));
    }
    Some((
        json!(encoded),
        json!({"json_columns":nested,"absent":absent}),
    ))
}

#[cfg(test)]
#[path = "../tests/support/toon_output_unit.rs"]
mod tests;

/// Rewrites bare numeric tokens to JavaScript spelling while preserving quoted cells.
fn javascript_numbers(encoded: &str) -> String {
    let mut output = String::new();
    let mut token = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for ch in encoded.chars() {
        if quoted {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
        } else if ch == '"' || ch == '\n' || ",:[]{}".contains(ch) {
            append_token(&mut output, &mut token);
            output.push(ch);
            quoted = ch == '"';
        } else {
            token.push(ch);
        }
    }
    append_token(&mut output, &mut token);
    output
}

/// Emits one cell from the codec, whose numeric tokens are finite by construction.
fn append_token(output: &mut String, token: &mut String) {
    let trimmed = token.trim();
    let numeric = trimmed.strip_prefix("- ").unwrap_or(trimmed).trim_start();
    if numeric.starts_with(|ch: char| ch.is_ascii_digit() || ch == '-')
        && let Ok(number) = numeric.parse::<f64>()
    {
        let start = token.len() - token.trim_start().len() + trimmed.len() - numeric.len();
        output.push_str(&token[..start]);
        output.push_str(&crate::history::javascript_float(number));
        output.push_str(&token[token.trim_end().len()..]);
    } else {
        output.push_str(token);
    }
    token.clear();
}

/// Rejects the first surviving split code unit in wire traversal order.
pub(crate) fn validate(output: &ListOutput) -> Result<(), crate::PmRustError> {
    if let Some(raw) = first_cut(&output.value, "", &output.cuts) {
        let unit = raw
            .rsplit_once("\\u")
            .and_then(|(_, tail)| tail.get(..4))
            .unwrap_or("");
        return Err(crate::PmRustError::ToonEncoding {
            detail: format!(
                "Cannot encode string value containing an unpaired surrogate U+{} at index 239",
                unit.to_uppercase()
            ),
        });
    }
    Ok(())
}

/// Locates lossless JSON overrides without rejecting cuts belonging to removed rows.
fn first_cut<'a>(
    value: &Value,
    pointer: &str,
    cuts: &'a std::collections::BTreeMap<String, String>,
) -> Option<&'a str> {
    if let Some(raw) = cuts.get(pointer) {
        return Some(raw);
    }
    match value {
        Value::Array(rows) => rows
            .iter()
            .enumerate()
            .find_map(|(index, row)| first_cut(row, &format!("{pointer}/{index}"), cuts)),
        Value::Object(fields) => fields.iter().find_map(|(key, value)| {
            first_cut(
                value,
                &format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1")),
                cuts,
            )
        }),
        _ => None,
    }
}

/// Serializes scalar and nested table cells with the published read numeric domain.
fn read_json(value: &Value) -> String {
    let mut value = value.clone();
    crate::json_output::normalize_read_numbers(&mut value);
    crate::stringify_json(&value, false)
}
