//! JSON admission shared by wire frames, tracker files and opaque receipts.
//!
//! Object keys are compared after JSON unescaping at every nesting depth.
//! Typed deserialization only sees the already-validated value, so serde's
//! default last-key-wins object decoding cannot erase an authoritative field.

use serde::{Deserialize, Deserializer, de};
use serde_json::{Map, Value};
use std::fmt;

/// Decodes one complete JSON document, rejecting duplicate keys recursively.
///
/// # Errors
/// Returns a JSON error for invalid syntax, duplicate keys, excessive nesting,
/// trailing data or a value that does not match the requested type.
pub fn from_slice<T: de::DeserializeOwned>(bytes: &[u8]) -> serde_json::Result<T> {
    let mut reader = serde_json::Deserializer::from_slice(bytes);
    let StrictValue(value) = StrictValue::deserialize(&mut reader)?;
    reader.end()?;
    serde_json::from_value(value)
}

/// Decodes JSON text through the same strict byte reader as wire frames.
///
/// # Errors
/// Returns the same syntax, duplicate-key and typed decoding errors as
/// [`from_slice`].
pub fn from_str<T: de::DeserializeOwned>(text: &str) -> serde_json::Result<T> {
    from_slice(text.as_bytes())
}

/// Recursive value wrapper that refuses a repeated decoded object key.
struct StrictValue(Value);

impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D: Deserializer<'de>>(reader: D) -> Result<Self, D::Error> {
        reader.deserialize_any(StrictVisitor)
    }
}

/// Builds JSON values while keeping serde's parser and recursion limits.
struct StrictVisitor;

impl<'de> de::Visitor<'de> for StrictVisitor {
    type Value = StrictValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("JSON with unique object keys")
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::Null))
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::Bool(value)))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::from(value)))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::from(value)))
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::from(value)))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::String(value.into())))
    }

    fn visit_seq<A: de::SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
        let mut values = Vec::new();
        while let Some(StrictValue(value)) = sequence.next_element()? {
            values.push(value);
        }
        Ok(StrictValue(Value::Array(values)))
    }

    fn visit_map<A: de::MapAccess<'de>>(self, mut object: A) -> Result<Self::Value, A::Error> {
        let mut values = Map::new();
        while let Some(key) = object.next_key::<String>()? {
            if values.contains_key(&key) {
                // Do not echo potentially sensitive object keys into diagnostics.
                return Err(de::Error::custom("duplicate JSON object key"));
            }
            let StrictValue(value) = object.next_value()?;
            values.insert(key, value);
        }
        Ok(StrictValue(Value::Object(values)))
    }
}

#[cfg(test)]
#[path = "../tests/support/strict_json_unit.rs"]
mod tests;
