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
    let value = complete_search(&complete_brief(&output.value));
    let compacted = compact(&value).unwrap_or_else(|| json!({}));
    format!("{}\n", render(&compacted, 0))
}

/// Recognizes published row projections with a nonempty declared field list.
fn standard_brief(value: &Value) -> bool {
    value.as_object().is_some_and(|fields| fields.len() == 2)
        && matches!(value["mode"].as_str(), Some("brief" | "compact" | "fields"))
        && value["fields"].as_array().is_some_and(|fields| {
            !fields.is_empty()
                && fields
                    .iter()
                    .all(|field| field.as_str().is_some_and(|field| !field.is_empty()))
        })
}

/// Verifies that the declared projection describes every delivered row exactly.
fn rows_match(value: &Value) -> bool {
    let Some(fields) = value["projection"]["fields"].as_array() else {
        return false;
    };
    value["items"].as_array().is_some_and(|rows| {
        rows.iter().all(|row| {
            row.as_object().is_some_and(|row| {
                row.len() == fields.len()
                    && fields
                        .iter()
                        .all(|field| field.as_str().is_some_and(|field| row.contains_key(field)))
            })
        })
    })
}

/// Suppresses echoed filter arguments and renders canonical default sorting compactly.
fn compact_echoes(projected: &mut Map<String, Value>, value: &Value) {
    if let Some(sorting) = value.get("sorting") {
        let default = sorting["sort"] == "default"
            && sorting["order"] == "asc"
            && sorting.as_object().is_some_and(|fields| {
                fields
                    .keys()
                    .all(|key| ["sort", "order", "effective_keys"].contains(&key.as_str()))
            });
        if default {
            if let Some(keys) = sorting["effective_keys"].as_array().filter(|keys| {
                keys.iter().all(|pair| {
                    pair.as_array().is_some_and(|pair| {
                        pair.len() == 2
                            && pair[0].is_string()
                            && matches!(pair[1].as_str(), Some("asc" | "desc"))
                    })
                })
            }) {
                projected.insert("sorting".to_owned(), json!({"effective_keys":keys.iter().map(|pair| format!("{} {}",pair[0].as_str().unwrap_or_default(),pair[1].as_str().unwrap_or_default())).collect::<Vec<_>>().join(", ")}));
            } else if sorting.get("effective_keys").is_some() {
                projected.insert("sorting".to_owned(), sorting.clone());
            }
        } else {
            projected.insert("sorting".to_owned(), sorting.clone());
        }
    }
    if let Some(filters) = value["filters"].as_object() {
        let mut filters = filters.clone();
        let applied = value["applied_limit"].as_str().map_or_else(
            || crate::stringify_json(&value["applied_limit"], false),
            str::to_owned,
        );
        if filters.get("limit").is_some_and(|limit| {
            limit
                .as_str()
                .map_or_else(|| crate::stringify_json(limit, false), str::to_owned)
                == applied
        }) {
            filters.shift_remove("limit");
        }
        if filters
            .get("fields")
            .and_then(Value::as_str)
            .is_some_and(|fields| {
                json!(
                    fields
                        .split(',')
                        .map(str::trim)
                        .filter(|field| !field.is_empty())
                        .collect::<Vec<_>>()
                ) == value["projection"]["fields"]
            })
        {
            filters.shift_remove("fields");
        }
        if filters
            .get("runtime_filters")
            .is_some_and(|value| value.as_object().is_some_and(Map::is_empty))
        {
            filters.shift_remove("runtime_filters");
        }
        if !filters.is_empty() {
            projected.insert("filters".to_owned(), Value::Object(filters));
        }
    }
    if value["projection"]["mode"] == "fields"
        && value["items"].as_array().is_some_and(Vec::is_empty)
    {
        projected.insert("projection".to_owned(), value["projection"].clone());
    }
}

/// Projects the standard terminal or paged search envelope through the same brief policy.
fn complete_search(value: &Value) -> Value {
    let standard = value.get("projection").is_none()
        && value["omission_receipt"]
            == json!({"has_omissions":true,"omitted_field_group_count":1,"omitted_field_groups":[{"name":"projection_metadata","restore_with":"--full"}]});
    let fields = standard_brief(&value["projection"])
        && value["projection"]["mode"] == "fields"
        && rows_match(value)
        && value["now"].is_string()
        && value["omission_receipt"]
            == json!({"has_omissions":true,"omitted_field_group_count":1,"omitted_field_groups":[{"name":"full_item_fields","restore_with":"--full"}]});
    let count = value["items"].as_array().map_or(0, Vec::len);
    let terminal = value["count"] == count
        && value.get("has_more").is_none()
        && value.get("truncated").is_none()
        && value["next_cursor"].is_null()
        && (value.get("total").is_none() || value["total"] == count);
    if !value["query"].is_string()
        || !value["items"].is_array()
        || value.get("details").is_some()
        || !(standard || fields)
        || !(terminal || standard_pagination(value, count))
    {
        return value.clone();
    }
    let mut projected = value.as_object().cloned().unwrap_or_default();
    for key in [
        "truncated",
        "omission_receipt",
        "filters",
        "projection",
        "now",
        "mode_source",
    ] {
        projected.shift_remove(key);
    }
    if value["mode_source"] == "settings" || (standard && value.get("mode_source").is_some()) {
        projected.insert("mode_source".to_owned(), value["mode_source"].clone());
    }
    compact_echoes(&mut projected, value);
    projected.insert("details".to_owned(), json!("--full"));
    Value::Object(projected)
}

/// Validates standard pagination before removing redundant delivery declarations.
fn standard_pagination(value: &Value, count: usize) -> bool {
    let Some(more) = value["has_more"].as_bool() else {
        return false;
    };
    let Some(total) = value["total"]
        .as_f64()
        .filter(|total| total.fract() == 0.0 && total.abs() <= 9_007_199_254_740_991.0)
    else {
        return false;
    };
    let count = json!(count).as_f64().unwrap_or_default();
    value["truncated"] == more
        && value["count"].as_f64() == Some(count)
        && if more {
            total > count
                && value["next_cursor"]
                    .as_str()
                    .is_some_and(|cursor| !cursor.is_empty())
        } else {
            total.total_cmp(&count).is_eq() && value["next_cursor"].is_null()
        }
}

/// Removes only redundant brief metadata, preserving partial-page and source evidence.
fn complete_brief(value: &Value) -> Value {
    let source = &value["completeness"];
    if value["items"].is_array()
        && value.get("details").is_none()
        && standard_brief(&value["projection"])
        && rows_match(value)
        && matches!(source["status"].as_str(), Some("complete" | "unchecked"))
        && source["unreadable_item_count"] == 0
        && source["unreadable_directory_count"] == 0
        && standard_pagination(value, value["items"].as_array().map_or(0, Vec::len))
        && value["omission_receipt"]
            == json!({"has_omissions":true,"omitted_field_group_count":1,"omitted_field_groups":[{"name":"full_item_fields","restore_with":"--full"}]})
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
            "filters",
        ] {
            projected.shift_remove(key);
        }
        if source["status"] != "complete"
            || value["has_more"] == true
            || source.as_object().is_some_and(|fields| fields.len() != 3)
        {
            projected.insert("total".to_owned(), value["total"].clone());
            projected.insert("has_more".to_owned(), value["has_more"].clone());
            let mut source = source.as_object().cloned().unwrap_or_default();
            source.shift_remove("unreadable_item_count");
            source.shift_remove("unreadable_directory_count");
            projected.insert("completeness".to_owned(), Value::Object(source));
        }
        if value["has_more"] == true {
            projected.insert("next_cursor".to_owned(), value["next_cursor"].clone());
        }
        compact_echoes(&mut projected, value);
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
                if key == "omission_receipt" && value == &json!({"has_omissions":false,"omitted_field_group_count":0,"omitted_field_groups":[]}) {
                    format!("{indent}{key}: complete")
                } else if value.is_array() {
                    let expanded = encode(&json!({key:value}), depth);
                    if fields.contains_key(&format!("{key}_encoding")) {
                        expanded
                    } else {
                        [false, true].into_iter().filter_map(|compact| table(value, compact)).fold(expanded, |smallest, (rows, encoding)| {
                            let candidate = encode(&json!({key:rows,format!("{key}_encoding"):encoding}),depth);
                            if candidate.len() < smallest.len() { candidate } else { smallest }
                        })
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
fn table(value: &Value, compact: bool) -> Option<(Value, Value)> {
    let rows = value.as_array()?;
    if rows.is_empty() || !rows.iter().all(Value::is_object) {
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
    let nulls: Vec<_> = columns
        .iter()
        .filter(|key| compact && rows.iter().all(|row| row.get(*key) == Some(&Value::Null)))
        .cloned()
        .collect();
    let lists: Vec<_> = columns
        .iter()
        .filter(|key| {
            compact
                && rows.iter().all(|row| {
                    row.get(*key).is_none_or(|value| {
                        value.as_array().is_some_and(|values| {
                            values.iter().all(|value| {
                                value
                                    .as_str()
                                    .is_some_and(|text| !text.is_empty() && !text.contains(';'))
                            })
                        })
                    })
                })
        })
        .cloned()
        .collect();
    let nested: Vec<_> = columns
        .iter()
        .filter(|key| {
            !lists.contains(key)
                && rows
                    .iter()
                    .any(|row| row[*key].is_array() || row[*key].is_object())
        })
        .cloned()
        .collect();
    let mut absent = Vec::new();
    let mut encoded = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let mut fields = Map::new();
        for key in columns.iter().filter(|key| !nulls.contains(key)) {
            let value = if let Some(value) = row.get(key) {
                if lists.contains(key) {
                    json!(
                        value
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(";")
                    )
                } else if nested.contains(key) {
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
    let mut encoding = json!({"json_columns":nested,"absent":absent});
    if !lists.is_empty() {
        encoding["list_columns"] =
            Value::Object(lists.iter().map(|key| (key.clone(), json!(";"))).collect());
    }
    if !nulls.is_empty() {
        encoding["null_columns"] = json!(nulls);
        encoding["null_column_index"] = json!(
            nulls
                .iter()
                .filter_map(|key| columns.iter().position(|column| column == key))
                .collect::<Vec<_>>()
        );
    }
    Some((json!(encoded), encoding))
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
