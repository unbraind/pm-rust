//! Ordered JSON output with the published JavaScript number representation.

use crate::history::javascript_float;
use serde::{
    Serialize, Serializer,
    ser::{SerializeMap, SerializeSeq},
};
use serde_json::Value;
use serde_json::ser::Formatter;
use std::io;
use std::{collections::BTreeMap, ops::Deref};

/// Bounded list envelope with lossless JSON serialization of UTF-16 cuts.
///
/// Rust strings cannot contain an unpaired surrogate. The immutable `Value`
/// view uses U+FFFD for that code unit; serialization retains the original
/// code unit as an ASCII JSON escape. Use [`stringify_json`],
/// [`write_pretty_json`] or a serde JSON serializer for exact output.
#[derive(Clone, Debug)]
pub struct ListOutput {
    /// Unicode-safe view of the response.
    pub(crate) value: Value,
    /// Raw JSON strings for split surrogate pairs, indexed by JSON pointer.
    pub(crate) cuts: BTreeMap<String, String>,
}

impl ListOutput {
    /// Measures the original projected rows with sorted keys and preserved cuts.
    pub(crate) fn row_fingerprint(&self) -> String {
        let mut sorted = self.value["items"].clone();
        sorted.sort_all_objects();
        let view = OutputView {
            value: &sorted,
            cuts: &self.cuts,
            pointer: "/items".to_owned(),
        };
        crate::pagination::collection_bytes(
            format!(
                "{{\"path\":\"items\",\"value\":{}}}",
                stringify_json(&view, false)
            )
            .as_bytes(),
        )
    }

    /// Moves lossless row-string locations when an output continuation skips rows.
    pub(crate) fn skip_rows(&mut self, offset: usize) {
        self.cuts = std::mem::take(&mut self.cuts)
            .into_iter()
            .filter_map(|(pointer, raw)| {
                if let Some(tail) = pointer.strip_prefix("/items/") {
                    let boundary = tail.find('/').unwrap_or(tail.len());
                    let index = tail[..boundary].parse::<usize>().ok()?;
                    let index = index.checked_sub(offset)?;
                    Some((format!("/items/{index}{}", &tail[boundary..]), raw))
                } else {
                    Some((pointer, raw))
                }
            })
            .collect();
    }

    /// Views a subtree while retaining its original pointer for wire overrides.
    pub(crate) fn view(&self, pointer: &str) -> OutputView<'_> {
        OutputView {
            value: self.value.pointer(pointer).unwrap_or(&Value::Null),
            cuts: &self.cuts,
            pointer: pointer.to_owned(),
        }
    }
}

impl From<Value> for ListOutput {
    fn from(value: Value) -> Self {
        Self {
            value,
            cuts: BTreeMap::new(),
        }
    }
}

impl Deref for ListOutput {
    type Target = Value;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl Serialize for ListOutput {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        OutputView {
            value: &self.value,
            cuts: &self.cuts,
            pointer: String::new(),
        }
        .serialize(serializer)
    }
}

/// One response subtree with access to the lossless string overrides.
pub(crate) struct OutputView<'a> {
    /// Current Unicode-safe subtree.
    value: &'a Value,
    /// Escaped strings retained during compaction.
    cuts: &'a BTreeMap<String, String>,
    /// Location of the subtree within the serialized response.
    pointer: String,
}

impl Serialize for OutputView<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let Some(raw) = self.cuts.get(&self.pointer) {
            return serialize_raw(raw.clone(), serializer);
        }
        match self.value {
            Value::Array(rows) => {
                let mut array = serializer.serialize_seq(Some(rows.len()))?;
                for (index, value) in rows.iter().enumerate() {
                    array.serialize_element(&OutputView {
                        value,
                        cuts: self.cuts,
                        pointer: format!("{}/{index}", self.pointer),
                    })?;
                }
                array.end()
            }
            Value::Object(rows) => {
                let mut map = serializer.serialize_map(Some(rows.len()))?;
                for (key, value) in rows {
                    map.serialize_entry(
                        key,
                        &OutputView {
                            value,
                            cuts: self.cuts,
                            pointer: format!(
                                "{}/{}",
                                self.pointer,
                                key.replace('~', "~0").replace('/', "~1")
                            ),
                        },
                    )?;
                }
                map.end()
            }
            Value::Number(number) => {
                serialize_raw(crate::history::javascript_number(number), serializer)
            }
            scalar => scalar.serialize(serializer),
        }
    }
}

/// Validates raw scalar JSON and forwards it through the caller's serializer.
fn serialize_raw<S: Serializer>(raw: String, serializer: S) -> Result<S::Ok, S::Error> {
    serde_json::value::RawValue::from_string(raw)
        .map_err(serde::ser::Error::custom)?
        .serialize(serializer)
}

/// Encodes an SDK envelope with JavaScript-compatible floating-point numbers.
///
/// Object insertion order is preserved. Pretty output uses two-space indentation;
/// neither encoding adds a trailing newline. Use this when SDK JSON must match
/// the published CLI's bytes and token receipts. Returns an empty string if a
/// custom serializer fails.
#[must_use]
pub fn stringify_json<T: ?Sized + Serialize>(value: &T, pretty: bool) -> String {
    let mut output = Vec::new();
    let result = if pretty {
        write_pretty_json(&mut output, value)
    } else {
        value.serialize(&mut serde_json::Serializer::with_formatter(
            &mut output as &mut dyn io::Write,
            JsCompactFormatter,
        ))
    };
    result
        .map(|()| String::from_utf8(output).unwrap_or_default())
        .unwrap_or_default()
}

/// Compact formatter with the same JavaScript number rendering as pretty output.
struct JsCompactFormatter;

impl Formatter for JsCompactFormatter {
    fn write_f64<W: ?Sized + io::Write>(&mut self, writer: &mut W, value: f64) -> io::Result<()> {
        writer.write_all(javascript_float(value).as_bytes())
    }
}

/// Serializes any response to a writer with JavaScript-compatible numbers.
///
/// The layout matches `to_writer_pretty`: two-space indentation, `": "`
/// object separation and the same string escaping, with every `f64` rendered
/// by the shared JavaScript number renderer instead of `serde_json`'s own.
/// Serialization errors surface at the write stage, exactly where the
/// reference reports them.
///
/// # Errors
///
/// Returns the serializer error when the value cannot be encoded or the
/// writer fails.
pub fn write_pretty_json<W, T>(writer: &mut W, value: &T) -> Result<(), serde_json::Error>
where
    W: ?Sized + io::Write,
    T: ?Sized + Serialize,
{
    let mut borrowed = writer;
    write_pretty_inner(&mut borrowed, value)
}

/// Uses one serializer writer type so all sinks share error-handling paths.
fn write_pretty_inner<T: ?Sized + Serialize>(
    writer: &mut dyn io::Write,
    value: &T,
) -> Result<(), serde_json::Error> {
    let mut serializer = serde_json::Serializer::with_formatter(writer, JsFormatter::new());
    value.serialize(&mut serializer)
}

/// Pretty JSON formatter carrying the published JavaScript number renderer.
struct JsFormatter {
    current_indent: usize,
    has_value: bool,
}

impl JsFormatter {
    fn new() -> Self {
        JsFormatter {
            current_indent: 0,
            has_value: false,
        }
    }

    /// Builds one collection entry's separator and indentation.
    fn prefix(&self, first: bool) -> String {
        format!(
            "{}{}",
            if first { "\n" } else { ",\n" },
            "  ".repeat(self.current_indent)
        )
    }

    /// Builds a closed collection's delimiter, indented when it has entries.
    fn closing(&self, delimiter: &str) -> String {
        format!(
            "{}{delimiter}",
            if self.has_value {
                format!("\n{}", "  ".repeat(self.current_indent))
            } else {
                String::new()
            }
        )
    }
}

impl Formatter for JsFormatter {
    fn begin_array<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.current_indent += 1;
        self.has_value = false;
        writer.write_all(b"[")
    }

    fn end_array<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.current_indent -= 1;
        writer.write_all(self.closing("]").as_bytes())
    }

    fn begin_array_value<W>(&mut self, writer: &mut W, first: bool) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        writer.write_all(self.prefix(first).as_bytes())
    }

    fn end_array_value<W>(&mut self, _writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.has_value = true;
        Ok(())
    }

    fn begin_object<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.current_indent += 1;
        self.has_value = false;
        writer.write_all(b"{")
    }

    fn end_object<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.current_indent -= 1;
        writer.write_all(self.closing("}").as_bytes())
    }

    fn begin_object_key<W>(&mut self, writer: &mut W, first: bool) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        writer.write_all(self.prefix(first).as_bytes())
    }

    fn begin_object_value<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        writer.write_all(b": ")
    }

    fn end_object_value<W>(&mut self, _writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.has_value = true;
        Ok(())
    }

    fn write_f64<W>(&mut self, writer: &mut W, value: f64) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        writer.write_all(javascript_float(value).as_bytes())
    }
}

#[cfg(test)]
#[path = "../tests/support/json_output_unit.rs"]
mod tests;
