//! Restoration of the published compact TOON read-table protocol.
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

/// Decodes a read envelope, restoring compact tables to their original JSON values.
///
/// Ordinary TOON decoders parse the wire syntax but leave table encoding metadata intact.
/// This reader restores lists, explicit nulls, nested JSON cells and absent fields recursively.
/// It does not expand fields intentionally omitted by a brief output projection.
///
/// # Errors
/// Returns a read refusal for invalid TOON, encoding metadata or encoded JSON cells.
pub fn parse_toon(input: &str) -> Result<Value, crate::PmRustError> {
    let input = input.strip_suffix("\n---\n").unwrap_or(input);
    let mut value = toon_format::decode_strict(input).map_err(invalid)?;
    restore(&mut value)?;
    Ok(value)
}

/// Attributes malformed wire data to the read boundary.
fn invalid(error: impl std::fmt::Display) -> crate::PmRustError {
    crate::PmRustError::InvalidReadRequest {
        reason: format!("Invalid TOON read output: {error}"),
    }
}

/// Published table metadata; optional compact fields preserve the original column order.
#[derive(Deserialize)]
struct Encoding {
    /// Columns whose cells contain serialized nested JSON.
    json_columns: BTreeSet<String>,
    /// Row/column coordinates that were absent in the original object.
    absent: BTreeSet<(usize, String)>,
    /// Delimiter for each compact string-list column.
    #[serde(default)]
    list_columns: BTreeMap<String, String>,
    /// Explicit null columns omitted from the encoded rows.
    #[serde(default)]
    null_columns: Vec<String>,
    /// Original positions of omitted null columns.
    #[serde(default)]
    null_column_index: Vec<usize>,
    /// Optional explicit order supported by the published restoring reader.
    column_order: Option<Vec<String>>,
}

/// Restores every explicitly encoded table, including tables nested in envelope objects.
fn restore(value: &mut Value) -> Result<(), crate::PmRustError> {
    match value {
        Value::Object(fields) => {
            let tables: Vec<_> = fields
                .iter()
                .filter_map(|(key, metadata)| {
                    key.strip_suffix("_encoding")
                        .filter(|name| fields.get(*name).is_some_and(Value::is_array))
                        .map(|name| (name.to_owned(), metadata.clone()))
                })
                .collect();
            for (name, metadata) in tables {
                let encoding: Encoding = serde_json::from_value(metadata).map_err(invalid)?;
                fields.shift_remove(&format!("{name}_encoding"));
                for (index, row) in fields[&name]
                    .as_array_mut()
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    restore_row(row, index, &encoding)?;
                }
            }
            for child in fields.values_mut() {
                restore(child)?;
            }
        }
        Value::Array(rows) => {
            for row in rows {
                restore(row)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Reconstructs one row without conflating absent fields, nulls and empty lists.
fn restore_row(
    row: &mut Value,
    index: usize,
    encoding: &Encoding,
) -> Result<(), crate::PmRustError> {
    let cells = row
        .as_object_mut()
        .ok_or_else(|| invalid("table row is not an object"))?;
    let mut columns = cells.keys().cloned().collect::<Vec<_>>();
    let mut insertions: Vec<_> = encoding
        .null_columns
        .iter()
        .zip(&encoding.null_column_index)
        .collect();
    insertions.sort_by_key(|(_, position)| *position);
    for (column, position) in insertions {
        columns.insert((*position).min(columns.len()), column.clone());
    }
    for column in &encoding.null_columns {
        if !columns.contains(column) {
            columns.push(column.clone());
        }
        cells.insert(column.clone(), Value::Null);
    }
    for (column, cell) in cells.iter_mut() {
        if encoding.absent.contains(&(index, column.clone())) {
            continue;
        }
        if let Some(delimiter) = encoding.list_columns.get(column) {
            let text = cell
                .as_str()
                .ok_or_else(|| invalid("list cell is not a string"))?;
            if delimiter.is_empty() {
                return Err(invalid("empty list delimiter"));
            }
            *cell = Value::Array(if text.is_empty() {
                vec![]
            } else {
                text.split(delimiter)
                    .map(|part| Value::String(part.to_owned()))
                    .collect()
            });
        } else if encoding.json_columns.contains(column) {
            *cell = crate::strict_json::from_str(
                cell.as_str()
                    .ok_or_else(|| invalid("JSON cell is not a string"))?,
            )
            .map_err(invalid)?;
        }
    }
    let mut restored = Map::new();
    for column in encoding.column_order.as_ref().unwrap_or(&columns) {
        if !encoding.absent.contains(&(index, column.clone()))
            && let Some(cell) = cells.shift_remove(column)
        {
            restored.insert(column.clone(), cell);
        }
    }
    *cells = restored;
    Ok(())
}

#[cfg(test)]
#[path = "../tests/support/toon_input_unit.rs"]
mod tests;
