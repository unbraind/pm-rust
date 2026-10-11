//! Published JSON.stringify number and indentation examples.

use super::stringify_json;
use serde_json::json;

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
    let cut = format!("{}😀suffix", "x".repeat(239));
    let mut output =
        super::ListOutput::from(json!({"items":[cut, cut],"notice":cut,"number":1e20}));
    assert!(crate::read_output::compact_strings(
        &mut output.value,
        "",
        &mut output.cuts
    ));
    let before = serde_json::to_string(&output)?;
    assert_eq!(before.matches("\\ud83d…").count(), 3);
    assert!(before.contains("100000000000000000000"));
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
    assert!(stringify_json(&output, true).is_empty());
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
