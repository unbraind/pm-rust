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
