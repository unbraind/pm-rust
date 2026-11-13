//! Item data that deliberately resembles the table-control vocabulary.
use serde_json::{Value, json};

/// Every JSON kind is legal for extension fields, including metadata-looking names.
pub fn items() -> Vec<Value> {
    [
        Value::Null,
        json!(7),
        json!(true),
        json!("custom"),
        json!(""),
        json!([]),
        json!({}),
        json!(["custom"]),
        json!({"format":"custom"}),
    ]
    .into_iter()
    .map(|value| {
        json!({
            "id":"sample-data", "labels":["alpha"], "labels_encoding":value,
            "list_columns":value, "null_columns":value, "null_column_index":value,
            "x":["alpha"], "x_encoding":value,
            "nested":{"labels":["alpha"],"labels_encoding":{"format":"custom"}}
        })
    })
    .collect()
}

/// A valid table directive is still ordinary data once an array element is entered.
pub fn instruction_shaped_item() -> Value {
    json!({"id":"sample-shaped",
        "labels":[{"value":"[\"literal\"]","keep":"public"}],
        "labels_encoding":{"json_columns":["value"],"absent":[[0,"keep"]],"null_columns":["injected"],"null_column_index":[0]},
        "list_columns":{"x":";"},"null_columns":["public"],
        "x":[{"value":"alpha;beta"}],
        "x_encoding":{"json_columns":[],"absent":[],"list_columns":{"value":";"}}
    })
}
