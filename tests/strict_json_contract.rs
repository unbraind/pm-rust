//! Recursive JSON admission and real non-git tracker decode boundaries.

use pm_rust::{HistoryOptions, ItemFilter, ListOptions, Workspace, strict_json};
use serde::Deserialize;
use serde_json::{Value, json};
use std::fs;

/// Fallible assertion result.
type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Keeps types, order, integer precision and valid repeated keys in separate objects.
#[test]
fn strict_json_preserves_values_and_rejects_invalid_documents() -> TestResult {
    let raw = r#"{"z":[null,true,false,-42,18446744073709551615,1.25,"雪\n",{},[],{"a":1},{"a":2}],"a":0}"#;
    let value: Value = strict_json::from_str(raw)?;
    assert_eq!(value, serde_json::from_str::<Value>(raw)?);
    assert_eq!(
        value.as_object().ok_or("object")?.keys().next(),
        Some(&"z".to_owned())
    );
    assert_eq!(value["z"][4].as_u64(), Some(u64::MAX));
    for invalid in [
        b"".as_slice(),
        b"{",
        b"[1,]",
        b"{} {}",
        b"{} trailing",
        b"\xff",
        br#"{"a":1,"a":2}"#,
        br#"[{"a":1,"\u0061":2}]"#,
        br#"{"outer":{"a":1,"a":1}}"#,
        br#"{"a":1,2:3}"#,
        br#"{"a":"\ud800"}"#,
        b"1e9999",
    ] {
        assert!(strict_json::from_slice::<Value>(invalid).is_err());
    }
    let too_deep = format!("{}0{}", "[".repeat(140), "]".repeat(140));
    assert!(strict_json::from_str::<Value>(&too_deep).is_err());
    assert!(strict_json::from_str::<Vec<String>>("[1]").is_err());
    // Decoding a typed map must not allow serde's default to discard duplicates.
    assert!(
        strict_json::from_str::<std::collections::BTreeMap<String, bool>>(
            r#"{"skipped":true,"skipped":false}"#
        )
        .is_err()
    );
    Ok(())
}

/// Even ignored typed fields must pass strict recursive validation first.
#[test]
fn strict_json_validates_before_typed_fields_are_ignored() -> TestResult {
    /// Forward-compatible consumer that deliberately ignores unknown fields.
    #[derive(Debug, Deserialize, PartialEq)]
    struct Receipt {
        /// Authoritative ownership result.
        skipped: bool,
    }
    let valid = r#"{"skipped":true,"future":[{"a":1},{"a":2}]}"#;
    assert_eq!(
        strict_json::from_str::<Receipt>(valid)?,
        Receipt { skipped: true }
    );
    let invalid = r#"{"skipped":true,"future":[{"a":1,"\u0061":2}]}"#;
    assert!(strict_json::from_str::<Receipt>(invalid).is_err());
    Ok(())
}

/// Settings, history and registry inputs reject ambiguous data without rewriting it.
#[test]
fn strict_json_tracker_decodes_refuse_duplicates_without_git() -> TestResult {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(root.join("history"))?;
    fs::create_dir_all(root.join("tasks"))?;
    fs::write(root.join("settings.json"), "{}")?;
    fs::write(
        root.join("tasks/demo-1.toon"),
        "id: demo-1\ntitle: Demo\ntype: Task\nstatus: open\npriority: 2\n",
    )?;
    let workspace = Workspace::discover(&root)?;
    let settings = r#"{"schema":{"workflow":{"open_status":"open","open\u005fstatus":"closed"}}}"#;
    fs::write(root.join("settings.json"), settings)?;
    assert!(
        workspace
            .list_page(
                &ItemFilter::default(),
                &ListOptions::default(),
                "2026-10-24T12:00:00Z"
            )
            .is_err()
    );
    assert_eq!(fs::read_to_string(root.join("settings.json"))?, settings);
    fs::write(root.join("settings.json"), "{}")?;
    let history = "{\"action\":\"claim\",\"changes\":[{\"skipped\":true,\"skipped\":false}]}\n";
    fs::write(root.join("history/demo-1.jsonl"), history)?;
    assert!(
        workspace
            .history("demo-1", &HistoryOptions::default())
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(root.join("history/demo-1.jsonl"))?,
        history
    );
    let registry = r#"{"measurements":[{"id":"m1","receipt":{"a":1,"\u0061":2}}]}"#;
    fs::write(root.join("assurance.json"), registry)?;
    assert!(workspace.assurance_declarations("measurement").is_err());
    assert_eq!(fs::read_to_string(root.join("assurance.json"))?, registry);
    fs::write(root.join("assurance.json"), r#"{"measurements":[]}"#)?;
    assert_eq!(
        workspace.assurance_declarations("measurement")?["items"],
        json!([])
    );
    assert!(!directory.path().join(".git").exists());
    Ok(())
}
