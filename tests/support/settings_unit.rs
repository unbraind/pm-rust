//! Whole-document supplied-field validation, legacy compatibility and read boundaries.
use super::*;

/// Constructs a valid supplied value for each published declarative rule.
fn example(rule: &Value) -> Value {
    match rule["kind"].as_str().unwrap_or_default() {
        "string" | "normalized" => json!("fixture"),
        "boolean" => json!(true),
        "number" => json!(rule["min"].as_f64().unwrap_or(1.0)),
        "literal" => rule["allowed"][0].clone(),
        "object" => Value::Object(
            rule["fields"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(key, rule)| (key.clone(), example(rule)))
                .collect(),
        ),
        "record" => json!({"fixture":example(&rule["item"])}),
        "array" => json!([example(&rule["item"])]),
        "pair" => json!(["open", "closed"]),
        "union" => example(&rule["variants"][0]),
        _ => Value::Null,
    }
}

/// Every supplied setting is checked before a valid context override can survive.
#[test]
fn whole_settings_validation_preserves_legacy_documents() -> Result<(), Box<dyn std::error::Error>>
{
    let complete = example(&SHAPE);
    assert!(valid(&complete));
    for key in [
        "id_prefix",
        "author_default",
        "locks",
        "extensions",
        "providers",
    ] {
        let mut missing = complete.clone();
        missing
            .as_object_mut()
            .map(|fields| fields.shift_remove(key));
        assert!(!valid(&missing), "missing required {key}");
    }
    for key in complete
        .as_object()
        .into_iter()
        .flatten()
        .map(|(key, _)| key)
    {
        let mut settings = complete.clone();
        settings[key] = Value::Null;
        assert!(!valid(&settings), "{key}");
        let read = validated(settings);
        assert!(read.fallback);
        assert_eq!(read.value, json!({}));
    }
    for settings in [
        json!({}),
        json!({"id_prefix":"sample-","context":{"activity_limit":1}}),
        json!({"extension_package":{"future":null},"context":{"future":null}}),
    ] {
        assert!(valid(&settings));
        let read = validated(settings.clone());
        assert!(!read.fallback);
        assert!(!read.unsupported_context);
        assert_eq!(read.value, settings);
    }
    assert!(validated(json!({"context":null})).unsupported_context);
    for partial in [
        json!({"schema":{"statuses":[{"id":"queued","aliases":[null,"waiting"]}]}}),
        json!({"item_types":{"definitions":[{"name":"Task","aliases":[null,"work"]}]}}),
        json!({"search":{"max_results":1.9}}),
    ] {
        assert!(valid(&partial));
        let mut versioned = partial;
        versioned["version"] = json!(1);
        assert!(!valid(&versioned));
    }
    assert!(conforms(&Value::Null, &json!({"kind":"null"})));
    assert!(!conforms(&json!(false), &json!({"kind":"null"})));
    let root = tempfile::tempdir()?;
    let tracker = root.path().join(".agents/pm");
    std::fs::create_dir_all(&tracker)?;
    let path = tracker.join("settings.json");
    std::fs::write(&path, r#"{"extensions":42,"context":{"activity_limit":1}}"#)?;
    let workspace = Workspace::discover(root.path())?;
    let read = workspace.read_settings()?;
    assert_eq!(*read, json!({}));
    assert!(read.settings_warning().is_some());
    std::fs::remove_file(path)?;
    assert!(workspace.read_settings().is_err());
    Ok(())
}

/// Scalar, collection and range boundaries use the same validator as stored reads.
#[test]
fn settings_rule_boundaries() {
    for (rule, accepted, rejected) in [
        (json!({"kind":"string"}), json!(""), json!(1)),
        (json!({"kind":"boolean"}), json!(false), Value::Null),
        (json!({"kind":"number"}), json!(-1.5), json!("1")),
        (
            json!({"kind":"number","int":true}),
            json!(9_007_199_254_740_991u64),
            json!(1.5),
        ),
        (
            json!({"kind":"number","int":true}),
            json!(1),
            json!(9_007_199_254_740_992u64),
        ),
        (json!({"kind":"number","positive":true}), json!(1), json!(0)),
        (
            json!({"kind":"number","min":0,"max":2}),
            json!(0),
            json!(-1),
        ),
        (json!({"kind":"number","min":0,"max":2}), json!(2), json!(3)),
        (
            json!({"kind":"literal","allowed":["yes"]}),
            json!("yes"),
            json!("no"),
        ),
        (
            json!({"kind":"object","fields":{"x":{"kind":"string"}}}),
            json!({}),
            json!({"x":1}),
        ),
        (
            json!({"kind":"record","item":{"kind":"string"}}),
            json!({"x":""}),
            json!({"x":false}),
        ),
        (
            json!({"kind":"array","item":{"kind":"string"}}),
            json!([""]),
            json!([false]),
        ),
        (
            json!({"kind":"array","item":{"kind":"string"},"maxLength":1}),
            json!([""]),
            json!(["", ""]),
        ),
        (json!({"kind":"pair"}), json!(["a", "b"]), json!(["a"])),
        (
            json!({"kind":"pair"}),
            json!(["a", "b"]),
            json!(["a", false]),
        ),
        (
            json!({"kind":"union","variants":[{"kind":"string"},{"kind":"boolean"}]}),
            json!(false),
            json!(1),
        ),
        (
            json!({"kind":"normalized","maxLength":8}),
            json!("Name"),
            json!(""),
        ),
        (
            json!({"kind":"normalized","maxLength":8}),
            json!("a"),
            json!("abcdefghi"),
        ),
        (
            json!({"kind":"normalized","maxLength":8}),
            json!("a"),
            json!(" a"),
        ),
        (
            json!({"kind":"normalized","maxLength":8,"token":true}),
            json!("a-1"),
            json!("a--1"),
        ),
        (
            json!({"kind":"normalized","maxLength":8,"token":true}),
            json!("a1"),
            json!("A"),
        ),
    ] {
        assert!(conforms(&accepted, &rule), "{rule}, {accepted}");
        assert!(!conforms(&rejected, &rule), "{rule}, {rejected}");
        assert!(!conforms(&Value::Null, &rule));
    }
    for rule in [
        json!({}),
        json!({"kind":"unknown"}),
        json!({"kind":"literal"}),
        json!({"kind":"object"}),
        json!({"kind":"union"}),
        json!({"kind":"normalized"}),
    ] {
        assert!(!conforms(&json!({}), &rule));
        assert!(!conforms(&json!("a"), &rule));
    }
}
