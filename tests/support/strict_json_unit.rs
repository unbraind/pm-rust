//! Error contract for unsupported non-JSON serde inputs.

use super::StrictValue;
use serde::{Deserialize, de::value};

/// A binary serde value must produce the strict reader's actionable expectation.
#[test]
fn strict_json_rejects_non_json_serde_values() -> Result<(), Box<dyn std::error::Error>> {
    let input = value::BytesDeserializer::<value::Error>::new(b"opaque bytes");
    let error = StrictValue::deserialize(input)
        .err()
        .ok_or("must refuse bytes")?;
    assert_eq!(
        error.to_string(),
        "invalid type: byte array, expected JSON with unique object keys"
    );
    Ok(())
}
