//! Ordered JSON output with the published JavaScript number representation.

use crate::history::javascript_float;
use serde::Serialize;
use serde_json::Value;
use serde_json::ser::Formatter;
use std::io;

/// Encodes an SDK envelope with JavaScript-compatible floating-point numbers.
///
/// Object insertion order is preserved. Pretty output uses two-space indentation;
/// neither encoding adds a trailing newline. Use this when SDK JSON must match
/// the published CLI's bytes and token receipts.
#[must_use]
pub fn stringify_json(value: &Value, pretty: bool) -> String {
    let mut output = String::new();
    append(value, &mut output, 0, pretty);
    output
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

/// Separates a collection entry and applies its requested indentation.
fn separate(output: &mut String, index: usize, depth: usize, pretty: bool) {
    if index > 0 {
        output.push(',');
    }
    if pretty {
        output.push('\n');
        output.push_str(&"  ".repeat(depth));
    }
}

/// Indents a nonempty collection's closing delimiter.
fn close(output: &mut String, empty: bool, depth: usize, pretty: bool) {
    if pretty && !empty {
        output.push('\n');
        output.push_str(&"  ".repeat(depth));
    }
}

/// Appends ordered collections and scalar values without changing string data.
fn append(value: &Value, output: &mut String, depth: usize, pretty: bool) {
    match value {
        Value::Array(rows) => {
            output.push('[');
            for (index, row) in rows.iter().enumerate() {
                separate(output, index, depth + 1, pretty);
                append(row, output, depth + 1, pretty);
            }
            close(output, rows.is_empty(), depth, pretty);
            output.push(']');
        }
        Value::Object(rows) => {
            output.push('{');
            for (index, (key, row)) in rows.iter().enumerate() {
                separate(output, index, depth + 1, pretty);
                output.push_str(&serde_json::to_string(key).unwrap_or_default());
                output.push_str(if pretty { ": " } else { ":" });
                append(row, output, depth + 1, pretty);
            }
            close(output, rows.is_empty(), depth, pretty);
            output.push('}');
        }
        Value::Number(number) => output.push_str(&crate::history::javascript_number(number)),
        scalar => output.push_str(&serde_json::to_string(scalar).unwrap_or_default()),
    }
}

#[cfg(test)]
#[path = "../tests/support/json_output_unit.rs"]
mod tests;
