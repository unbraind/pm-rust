//! Published JSON.stringify number and indentation examples.

use super::stringify_json;
use serde_json::json;

/// Brief list wire bytes are compact; other reads retain pretty JSON and explicit facts.
#[test]
fn read_json_uses_the_published_brief_list_encoding() {
    let brief = super::ListOutput::from(
        json!({"items":[{"id":"a","priority":null}],"projection":{"mode":"brief"}}),
    );
    let mut bytes = Vec::new();
    assert!(brief.write_read_json(&mut bytes).is_ok());
    assert_eq!(
        format!("{}\n", String::from_utf8_lossy(&bytes)),
        brief.stringify_read_json()
    );
    assert_eq!(
        brief.stringify_read_json(),
        "{\"items\":[{\"id\":\"a\",\"priority\":null}],\"projection\":{\"mode\":\"brief\"}}\n"
    );
    for value in [
        json!({"items":[],"projection":{"mode":"full"}}),
        json!({"item":null,"projection":{"mode":"brief"}}),
    ] {
        let mut bytes = Vec::new();
        assert!(
            super::ListOutput::from(value.clone())
                .write_read_json(&mut bytes)
                .is_ok()
        );
        assert_eq!(
            super::ListOutput::from(value.clone()).stringify_read_json(),
            format!("{}\n", stringify_json(&value, true))
        );
    }
    let invalid = super::ListOutput::with_json_overrides(
        json!({"items":["safe"],"projection":{"mode":"brief"}}),
        std::collections::BTreeMap::from([("/items/0".to_owned(), "{".to_owned())]),
    );
    assert!(invalid.write_read_json(&mut Vec::new()).is_err());
}

#[test]
fn numbers_use_javascript_thresholds_and_zero() {
    let value = json!([30.0, 30.5, 1e20, 1e-6, -0.0, 1e-7, 1e21]);
    assert_eq!(
        stringify_json(&value, false),
        "[30,30.5,100000000000000000000,0.000001,0,1e-7,1e+21]"
    );
    assert_eq!(
        stringify_json(&value, true),
        "[\n  30,\n  30.5,\n  100000000000000000000,\n  0.000001,\n  0,\n  1e-7,\n  1e+21\n]"
    );
}

#[test]
fn ordered_collections_preserve_strings_and_empty_values() -> Result<(), Box<dyn std::error::Error>>
{
    let value = json!({"z":[{}, [], null, true, "line\n\"quoted\""],"a":false});
    for pretty in [true, false] {
        let expected = if pretty {
            serde_json::to_string_pretty(&value)?
        } else {
            value.to_string()
        };
        assert_eq!(stringify_json(&value, pretty), expected);
    }
    assert_eq!(stringify_json(&json!(false), false), "false");
    Ok(())
}

/// Envelope-level cuts survive row slicing, while row-string cuts move with rows.
#[test]
fn lossless_views_keep_cuts_and_javascript_numbers() -> Result<(), Box<dyn std::error::Error>> {
    // Pair-preserving compaction never manufactures a lone surrogate (GH-1478):
    // every safe view retains the prefix before the astral character.
    let cut = format!("{}😀suffix", "x".repeat(239));
    let mut output =
        super::ListOutput::from(json!({"items":[cut, cut],"notice":cut,"number":1e20}));
    assert!(crate::read_output::compact_strings(&mut output.value));
    assert_eq!(output["notice"], format!("{}…", "x".repeat(239)));
    let compacted = serde_json::to_string(&output)?;
    assert_eq!(compacted.matches("…").count(), 3);
    assert_eq!(compacted.matches("\\ud83d").count(), 0);
    assert!(compacted.contains("100000000000000000000"));
    // Retained raw overrides replace their safe views verbatim, including the
    // lone-surrogate escape a malformed value keeps for the encoder to reject.
    let raw = format!("\"{}\\ud83d…\"", "x".repeat(239));
    for pointer in ["/items/0", "/items/1", "/notice"] {
        output.cuts.insert(pointer.to_owned(), raw.clone());
    }
    let before = serde_json::to_string(&output)?;
    assert_eq!(before.matches("\\ud83d…").count(), 3);
    output
        .cuts
        .insert("/items/invalid/title".to_owned(), "null".to_owned());
    output.skip_rows(1);
    assert!(!output.cuts.contains_key("/items/invalid/title"));
    output.value["items"] = json!([output["items"][1]]);
    let after = serde_json::to_string(&output)?;
    assert_eq!(after.matches("\\ud83d…").count(), 2);
    assert_eq!(stringify_json(&output.view("/absent"), false), "null");
    assert_eq!(stringify_json(&output.clone(), false), after);
    // Invalid internal raw data must surface as an error instead of malformed JSON.
    output.cuts.insert(String::new(), "invalid".to_owned());
    assert!(super::write_pretty_json(&mut Vec::new(), &output).is_err());
    assert_eq!(stringify_json(&output, true), "");
    Ok(())
}

/// A finite output sink exercises errors at collection headers and entries.
#[test]
fn lossless_serialization_propagates_collection_write_failures() {
    struct LimitedWriter(usize);
    impl std::io::Write for LimitedWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.0 {
                return Err(std::io::Error::other("synthetic sink full"));
            }
            self.0 -= bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    for value in [json!(["entry"]), json!({"key":"entry"})] {
        for limit in [0, 1] {
            let result = super::write_pretty_json(
                &mut LimitedWriter(limit),
                &super::ListOutput::from(value.clone()),
            );
            assert!(result.is_err());
        }
    }
}
