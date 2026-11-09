//! Whole-document validation before native settings projections.
use crate::{ListOutput, PmRustError, Workspace};
use serde_json::{Value, json};
use std::{path::Path, sync::LazyLock};

/// Supplied-field schema from the published 2026.11.5 settings validator.
/// Missing fields remain allowed for legacy native partial settings documents;
/// unknown extension sections and future keys remain readable.
static SHAPE: LazyLock<Value> =
    LazyLock::new(|| serde_json::from_str(include_str!("settings_shape.json")).unwrap_or_default());

/// Legacy native partial documents permit ignored null aliases and fractional
/// search limits. Preserve those existing reader contracts without relaxing
/// validation of versioned published settings documents.
static LEGACY_SHAPE: LazyLock<Value> = LazyLock::new(|| {
    let mut shape = SHAPE.clone();
    allow_partial(&mut shape);
    let aliases = json!({"kind":"union","variants":[{"kind":"string"},{"kind":"null"}]});
    shape["fields"]["schema"]["fields"]["statuses"]["item"]["fields"]["aliases"]["item"] =
        aliases.clone();
    shape["fields"]["item_types"]["fields"]["definitions"]["item"]["fields"]["aliases"]["item"] =
        aliases;
    shape["fields"]["search"]["fields"]["max_results"]["int"] = json!(false);
    shape
});

/// Retains the historical native contract for omitted fields in partial documents.
fn allow_partial(rule: &mut Value) {
    match rule {
        Value::Object(fields) => {
            fields.shift_remove("required");
            for value in fields.values_mut() {
                allow_partial(value);
            }
        }
        Value::Array(items) => {
            for item in items {
                allow_partial(item);
            }
        }
        _ => {}
    }
}

/// A validated document and the diagnostic policy captured before defaults.
pub(crate) struct SettingsRead {
    /// Only validated values, or an empty document selecting native defaults.
    pub(crate) value: Value,
    /// Whether the complete stored document required fallback.
    pub(crate) fallback: bool,
    /// Existing native boundary for a non-object context section.
    pub(crate) unsupported_context: bool,
}

/// Validates every supplied core field before any consumer applies a projection.
pub(crate) fn valid(value: &Value) -> bool {
    conforms(
        value,
        if value.get("version").is_some() {
            &SHAPE
        } else {
            &LEGACY_SHAPE
        },
    )
}

/// Tests one declarative shape without rejecting unknown persisted keys.
fn conforms(value: &Value, shape: &Value) -> bool {
    match shape["kind"].as_str().unwrap_or_default() {
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        "number" => value.as_f64().is_some_and(|n| {
            (shape["int"] != true || (n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_991.0))
                && (shape["positive"] != true || n > 0.0)
                && shape["min"].as_f64().is_none_or(|min| n >= min)
                && shape["max"].as_f64().is_none_or(|max| n <= max)
        }),
        "literal" => shape["allowed"]
            .as_array()
            .is_some_and(|allowed| allowed.contains(value)),
        "object" => value.as_object().is_some_and(|fields| {
            shape["fields"].as_object().is_some_and(|rules| {
                rules.iter().all(|(key, rule)| {
                    fields.get(key).map_or_else(
                        || {
                            !shape["required"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .any(|required| required == key)
                        },
                        |value| conforms(value, rule),
                    )
                })
            })
        }),
        "record" => value
            .as_object()
            .is_some_and(|fields| fields.values().all(|value| conforms(value, &shape["item"]))),
        "array" => value.as_array().is_some_and(|items| {
            shape["maxLength"]
                .as_u64()
                .is_none_or(|max| items.len() as u64 <= max)
                && items.iter().all(|value| conforms(value, &shape["item"]))
        }),
        "pair" => value
            .as_array()
            .is_some_and(|items| items.len() == 2 && items.iter().all(Value::is_string)),
        "union" => shape["variants"]
            .as_array()
            .is_some_and(|variants| variants.iter().any(|rule| conforms(value, rule))),
        "normalized" => value.as_str().is_some_and(|text| {
            !text.is_empty()
                && text.encode_utf16().count() as u64
                    <= shape["maxLength"].as_u64().unwrap_or_default()
                && text == crate::orientation::trim_control(text)
                && (shape["token"] != true
                    || text.split('-').all(|part| {
                        !part.is_empty()
                            && part
                                .bytes()
                                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
                    }))
        }),
        _ => false,
    }
}

/// Captures validation before discarding any invalid document as a whole.
pub(crate) fn validated(value: Value) -> SettingsRead {
    let fallback = !valid(&value);
    let unsupported_context = value
        .get("context")
        .is_some_and(|context| !context.is_object());
    SettingsRead {
        value: if fallback { json!({}) } else { value },
        fallback,
        unsupported_context,
    }
}

/// Reads and validates once before callers inspect any stored settings value.
pub(crate) fn read(root: &Path) -> Result<SettingsRead, PmRustError> {
    Ok(validated(crate::ownership::read_json(
        &root.join("settings.json"),
    )?))
}

impl Workspace {
    /// Reads whole-document-validated settings, falling back to native defaults.
    ///
    /// Unknown keys and legacy partial documents remain readable. Invalid supplied
    /// fields discard all overrides, so callers cannot apply a partial configuration.
    /// # Errors
    /// Returns storage or JSON decoding errors without applying any settings.
    pub fn read_settings(&self) -> Result<ListOutput, PmRustError> {
        let settings = read(self.pm_root())?;
        let mut output = ListOutput::from(settings.value);
        output.settings_warning = settings.fallback;
        Ok(output)
    }
}

#[cfg(test)]
#[path = "../tests/support/settings_unit.rs"]
mod tests;
