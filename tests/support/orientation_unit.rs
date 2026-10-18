//! Native history reader integrity and storage-boundary acceptance.
use super::*;

/// Builds a minimal real tracker for private history boundary tests.
fn workspace() -> Result<(tempfile::TempDir, Workspace), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    fs::create_dir_all(root.path().join(".agents/pm/history"))?;
    fs::create_dir_all(root.path().join(".agents/pm/tasks"))?;
    fs::write(
        root.path().join(".agents/pm/settings.json"),
        r#"{"id_prefix":"sample-","item_format":"toon"}"#,
    )?;
    let workspace = Workspace::discover(root.path())?;
    Ok((root, workspace))
}
/// Builds a complete item and isolated history genesis without replacing filesystem calls.
fn genesis() -> Value {
    let metadata = json!({"id":"sample-one","title":"One","description":"","type":"Task","status":"open","priority":2,"tags":[],"created_at":"2026-08-22T10:00:00.000Z","updated_at":"2026-08-22T10:00:00.000Z"});
    json!({"op":"create","author":"fixture","ts":"2026-08-22T10:00:00.000Z","patch":[{"op":"add","path":"/metadata","value":metadata}],"before_hash":digest(&json!({"front_matter":{},"body":""})),"after_hash":digest(&json!({"front_matter":metadata,"body":""})),"item_hash_version":3})
}
/// Record hashes cover all immutable fields and preserve both supported algorithms.
#[test]
fn record_envelopes_and_replay_hashes() {
    for algorithm in ["sha256", "sha512"] {
        let mut entry = genesis();
        entry["hash_algorithm"] = json!(algorithm);
        entry["record_hash_version"] = json!(1);
        entry["record_hash"] = json!(history_digest(&entry, algorithm));
        assert_eq!(record_error(&entry), None);
        entry["author"] = json!("modified");
        assert_eq!(record_error(&entry), Some("record_hash_mismatch"));
    }
    for (entry, expected) in [
        (json!({}), None),
        (
            json!({"record_hash_version":1}),
            Some("incomplete_record_hash_envelope"),
        ),
        (
            json!({"record_hash":"digest"}),
            Some("incomplete_record_hash_envelope"),
        ),
        (
            json!({"record_hash_version":2,"record_hash":"digest"}),
            Some("unsupported_record_hash_version"),
        ),
        (
            json!({"hash_algorithm":"unsupported"}),
            Some("unsupported_history_hash_algorithm"),
        ),
    ] {
        assert_eq!(record_error(&entry), expected);
    }
    assert!(numeric("", true, 0, "bad".to_owned()).is_ok());
    for raw in ["-1", "NaN", "Infinity", "1.5"] {
        assert!(numeric(raw, true, 0, "bad".to_owned()).is_err());
    }
    assert_eq!(numeric("1.9", false, 0, "bad".to_owned()).ok(), Some(1));
    assert!(numeric("1e100", true, 0, "bad".to_owned()).is_err());
    assert_eq!(numeric("0x10", true, 0, "bad".to_owned()).ok(), Some(16));
    for raw in ["0x", "0xwrong", "0b2"] {
        assert!(numeric(raw, true, 0, "bad".to_owned()).is_err());
    }
}
/// Strict patch replay rejects nonexistent paths and preserves arrays and escaped keys.
#[test]
fn strict_patch_operations() {
    assert_eq!(history_pointer("/front_matter/key"), "/metadata/key");
    assert_eq!(history_pointer("/front_matter"), "/metadata");
    assert_eq!(history_pointer("/front_matterish"), "/front_matterish");
    assert_eq!(
        history_pointer("/metadata/front_matter/key"),
        "/metadata/front_matter/key"
    );
    let mut doc = json!({"metadata":{"array":[1,2],"a/b":1,"a~b":2},"body":""});
    assert!(apply_patch(&mut doc,&json!([{}, {"op":"add"},{"op":"add","path":"/metadata/new","value":3},{"op":"replace","path":"/metadata/a~1b","value":4},{"op":"remove","path":"/metadata/a~0b"},{"op":"test","path":"/metadata/new","value":3},{"op":"add","path":"/metadata/array/-","value":3},{"op":"replace","path":"/metadata/array/1","value":9},{"op":"remove","path":"/metadata/array/0"}])).is_some());
    assert_eq!(doc["metadata"]["array"], json!([9, 3]));
    for patch in [
        json!([{"op":"replace","path":"/metadata/absent","value":1}]),
        json!([{"op":"remove","path":"/metadata/absent"}]),
        json!([{"op":"test","path":"/metadata/new","value":0}]),
        json!([{"op":"unsupported","path":"/metadata/new"}]),
        json!([{"op":"add","path":"/metadata/array/50","value":0}]),
        json!([{"op":"replace","path":"/metadata/array/x","value":0}]),
        json!([{"op":"remove","path":"/metadata/array/50"}]),
        json!([{"op":"add","path":"/body/child","value":0}]),
        json!([{"op":"replace","path":"/body","value":0}]),
        json!([{"op":"replace","path":"/absent/child","value":0}]),
        json!([{"op":"add","path":"invalid","value":0}]),
        json!([{"op":"add","path":"/metadata/novalue"}]),
    ] {
        assert!(apply_patch(&mut doc.clone(), &patch).is_none(), "{patch}");
    }
    assert_eq!(changed_field("/metadata/title/x"), "title");
    assert_eq!(changed_field("/front_matter/a~1b"), "a/b");
    assert_eq!(changed_field("/body"), "body");
    assert_eq!(changed_field("/metadata"), "metadata");
}
/// Full chain verification gates record integrity before replay and reports state mismatch.
#[test]
fn verification_failures_and_epochs() -> Result<(), Box<dyn std::error::Error>> {
    let entry = genesis();
    assert_eq!(
        verify_records(std::slice::from_ref(&entry), None)["ok"],
        true
    );
    for (field, value, code) in [
        ("before_hash", json!("wrong"), "before_hash_mismatch"),
        ("after_hash", json!("wrong"), "after_hash_mismatch"),
        (
            "record_hash",
            json!("wrong"),
            "incomplete_record_hash_envelope",
        ),
        (
            "patch",
            json!([{"op":"replace","path":"/absent","value":1}]),
            "patch_apply_failed",
        ),
    ] {
        let mut broken = entry.clone();
        broken[field] = value;
        assert!(
            verify_records(&[broken], None)["errors"][0]
                .as_str()
                .is_some_and(|error| error.contains(code))
        );
    }
    let mut unsupported = entry.clone();
    unsupported["item_hash_version"] = json!(99);
    assert_eq!(verify_records(&[unsupported], None)["ok"], false);
    let current: crate::ItemDocument = serde_json::from_value(entry["patch"][0]["value"].clone())?;
    assert_eq!(
        verify_records(std::slice::from_ref(&entry), Some(&current))["ok"],
        true
    );
    assert_eq!(verify_records(&[], Some(&current))["ok"], false);
    assert_eq!(verify_records(&[], None)["ok"], true);
    let mut changed = current.clone();
    changed.body = "different".to_owned();
    assert_eq!(
        verify_records(std::slice::from_ref(&entry), Some(&changed))["current_matches_latest"],
        false
    );
    assert_eq!(
        verify_records(&[entry.clone(), entry.clone()], None)["ok"],
        false
    );
    for epoch in [1, 2, 3] {
        for algorithm in ["sha256", "sha512"] {
            let mut record = entry.clone();
            record["item_hash_version"] = json!(epoch);
            record["hash_algorithm"] = json!(algorithm);
            let before = json!({"metadata":{},"body":""});
            let after = json!({"metadata":entry["patch"][0]["value"],"body":""});
            record["before_hash"] = json!(replay_hash(&before, epoch, false, algorithm));
            record["after_hash"] = json!(replay_hash(&after, epoch, false, algorithm));
            assert_eq!(verify_records(&[record], Some(&current))["ok"], true);
        }
    }
    let invalid = json!({"metadata":{"not":"an item"},"body":""});
    assert!(!replay_hash(&invalid, 3, false, "sha256").is_empty());
    let legacy = json!({"metadata":{"id":"sample-one","title":"One","type":"Task","status":"open","priority":1,"tags":[],"created_at":"now","updated_at":"now","dependencies":[{"id":"UPPER","kind":"related"}],"tests":[{"command":"z","workspace_context_mode":"isolated","provenance":{}},{"command":"a"}],"test_runs":[{"executions":[]}]},"body":""});
    assert_ne!(
        replay_hash(&legacy, 1, false, "sha256"),
        replay_hash(&legacy, 3, false, "sha256")
    );
    assert_ne!(
        replay_hash(&legacy, 1, false, "sha256"),
        replay_hash(&legacy, 2, true, "sha256")
    );
    Ok(())
}
/// Missing, malformed, empty, detached and compacted streams are read without writes.
#[test]
fn history_storage_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let (_root, workspace) = workspace()?;
    let path = workspace.pm_root().join("history/sample-one.jsonl");
    assert!(records(&workspace, "missing")?.is_empty());
    assert!(
        workspace
            .history("missing", &HistoryOptions::default())
            .is_err()
    );
    fs::write(&path, " \n\r\n")?;
    assert!(records(&workspace, "sample-one")?.is_empty());
    for raw in [
        "{broken}\n",
        "<<<<<<< conflict\n",
        "=======\n",
        ">>>>>>> branch\n",
    ] {
        fs::write(&path, raw)?;
        assert!(records(&workspace, "sample-one").is_err());
    }
    fs::remove_file(&path)?;
    fs::create_dir(&path)?;
    assert!(records(&workspace, "sample-one").is_err());
    fs::remove_dir(&path)?;
    for offset in [Some(10), None] {
        let baseline = json!({"op":"history_compact_baseline","patch":[],"context":{"history_compaction":{"version_offset":offset}}});
        fs::write(&path, format!("{baseline}\n"))?;
        for full in [true, false] {
            let output = workspace.history(
                "one",
                &HistoryOptions {
                    full,
                    ..Default::default()
                },
            )?;
            assert_eq!(output["version_addressing"]["offset"], json!(offset));
        }
    }
    Ok(())
}

/// Retained rewrite proof is checked independently of the current immutable record seal.
#[test]
fn retained_rewrite_evidence() {
    let mut original = genesis();
    original["record_hash_version"] = json!(1);
    original["record_hash"] = json!(digest(&original));
    let proof = json!({"before_hash":original["before_hash"],"after_hash":original["after_hash"],"item_hash_version":original["item_hash_version"],"patch_hash":digest(&original["patch"]),"record_hash_version":original["record_hash_version"],"record_hash":original["record_hash"],"record":original});
    let mut rewritten = original.clone();
    rewritten["reanchor_evidence"] = json!([proof]);
    assert_eq!(rewrite_error(&rewritten), None);
    let mut bad = rewritten.clone();
    bad["reanchor_evidence"][0]["record"]["author"] = json!("tampered");
    assert_eq!(
        rewrite_error(&bad),
        Some("rewrite_evidence_record_hash_mismatch")
    );
    let mut bad = rewritten.clone();
    bad["reanchor_evidence"][0]["after_hash"] = json!("wrong");
    assert_eq!(
        rewrite_error(&bad),
        Some("rewrite_evidence_record_hash_mismatch")
    );
    let mut bad = rewritten.clone();
    bad["reanchor_evidence"][0]["patch"] = json!([]);
    assert_eq!(
        rewrite_error(&bad),
        Some("rewrite_evidence_patch_hash_mismatch")
    );
    for evidence in [
        Value::Null,
        json!([{}]),
        json!([{"before_hash":"before","after_hash":"after","patch_hash":"wrong"}]),
    ] {
        let mut bad = original.clone();
        bad["reanchor_evidence"] = evidence;
        assert_eq!(rewrite_error(&bad), Some("rewrite_evidence_invalid"));
    }
    let mut reconstructed = rewritten.clone();
    reconstructed["reanchor_evidence"][0]
        .as_object_mut()
        .map(|record| record.shift_remove("record"));
    assert_eq!(rewrite_error(&reconstructed), None);
    let mut legacy = reconstructed.clone();
    for key in ["record_hash", "record_hash_version"] {
        legacy["reanchor_evidence"][0]
            .as_object_mut()
            .map(|row| row.shift_remove(key));
    }
    assert_eq!(rewrite_error(&legacy), None);
    let mut digest_only = legacy.clone();
    digest_only["patch"] = json!([]);
    assert_eq!(rewrite_error(&digest_only), None);
    assert_eq!(rewrite_error(&json!({"reanchor_evidence":[]})), None);
    for valid in [
        json!([{"op":"copy","from":"/metadata/a","path":"/metadata/b"}]),
        json!([{"op":"remove","path":"/metadata/a"}]),
    ] {
        assert!(patch_shape(&valid));
    }
    for invalid in [
        Value::Null,
        json!([{"op":"copy","path":"/metadata/a"}]),
        json!([{"op":"unknown","path":"/metadata/a"}]),
        json!([{"op":"add","path":"/metadata/a"}]),
    ] {
        assert!(!patch_shape(&invalid));
    }
    assert!(!hex_digest(&json!("X".repeat(64)), &[64]));
}

/// Filename boundaries and historical patch namespaces remain independent.
#[test]
fn history_ids_and_pointer_aliases() -> Result<(), Box<dyn std::error::Error>> {
    let (_root, workspace) = workspace()?;
    for id in [
        "../outside",
        "with:colon",
        ".",
        "..",
        "back\\slash",
        "null\0byte",
    ] {
        assert!(records(&workspace, id).is_err());
    }
    let mut document = json!({"metadata":{"original":"value","array":[0,1]},"body":""});
    assert!(apply_patch(&mut document,&json!([{"op":"copy","from":"/front_matter/original","path":"/metadata/copied"},{"op":"move","from":"/metadata/copied","path":"/metadata/moved"},{"op":"test","path":"","value":{"metadata":{"original":"value","array":[0,1],"moved":"value"},"body":""}}])).is_some());
    assert_eq!(document["metadata"]["moved"], "value");
    assert!(
        apply_patch(
            &mut document,
            &json!([{"op":"replace","path":"","value":{"metadata":{},"body":"new"}}])
        )
        .is_some()
    );
    assert!(apply_patch(&mut document, &json!([{"op":"test","path":"","value":{}}])).is_none());
    assert!(apply_patch(&mut document, &json!([{"op":"remove","path":""}])).is_none());
    assert!(
        apply_patch(
            &mut document,
            &json!([{"op":"add","path":"/metadata/__proto__","value":1}])
        )
        .is_none()
    );
    Ok(())
}

/// Strict missing-stream policy and compacted version arithmetic retain their published boundaries.
#[test]
fn history_missing_stream_policy() -> Result<(), Box<dyn std::error::Error>> {
    let (_root, workspace) = workspace()?;
    let metadata = genesis()["patch"][0]["value"].clone();
    fs::write(
        workspace.pm_root().join("tasks/sample-one.toon"),
        toon_format::encode_default(&metadata)?,
    )?;
    let path = workspace.pm_root().join("history/sample-one.jsonl");
    assert_eq!(
        workspace.history("one", &HistoryOptions::default())?["count"],
        0
    );
    assert!(!path.exists());
    fs::write(
        workspace.pm_root().join("settings.json"),
        r#"{"id_prefix":"sample-","history":{"missing_stream":"strict_error"}}"#,
    )?;
    assert!(
        workspace
            .history("one", &HistoryOptions::default())
            .is_err()
    );
    for offset in [json!(9_007_199_254_740_991u64), json!("bad")] {
        fs::write(
            &path,
            format!(
                "{}\n",
                json!({"op":"history_compact_baseline","patch":[],"context":{"history_compaction":{"version_offset":offset}}})
            ),
        )?;
        assert_eq!(
            workspace.history("one", &HistoryOptions::default())?["version_addressing"]["offset"],
            Value::Null
        );
    }
    Ok(())
}

/// Every retained-proof envelope member is validated before trusting the reconstructed record.
#[test]
fn malformed_retained_envelopes() {
    let original = genesis();
    let valid = json!({"before_hash":"before","after_hash":"after","patch_hash":digest(&original["patch"]),"record":original});
    for (field, value) in [
        ("record", json!(42)),
        ("record", json!({"ts":42})),
        ("before_hash", json!(42)),
        ("after_hash", Value::Null),
        ("patch", json!(42)),
        ("record_hash_version", json!("bad")),
        ("record_hash", json!("wrong")),
        ("hash_algorithm", json!("other")),
    ] {
        let mut proof = valid.clone();
        proof[field] = value;
        if field == "record_hash_version" {
            proof["record_hash"] = json!("a".repeat(64));
        }
        if field == "record_hash" {
            proof["record_hash_version"] = json!(1);
        }
        assert_eq!(
            rewrite_error(&json!({"reanchor_evidence":[proof]})),
            Some("rewrite_evidence_invalid"),
            "{field}"
        );
    }
    assert!(!hex_digest(&json!(42), &[64]));
    assert!(!hex_digest(&json!("short"), &[64]));
    for invalid in [json!([42]), json!([{"op":"remove","path":42}])] {
        assert!(!patch_shape(&invalid));
    }
}

/// Root tests and unsupported operations refuse without accepting malformed final documents.
#[test]
fn patch_root_and_array_boundaries() {
    for change in [
        json!({"op":"test","path":"","value":{}}),
        json!({"op":"remove","path":""}),
        json!({"op":"test","path":"/metadata/rows/0","value":1}),
        json!({"op":"replace","path":"/metadata/rows/01","value":0}),
        json!({"op":"add","path":"/metadata/rows/9","value":0}),
        json!({"op":"unknown","path":"/metadata/value"}),
    ] {
        let mut document = json!({"metadata":{"rows":[0],"value":1},"body":""});
        assert!(apply_patch(&mut document, &json!([change])).is_none());
    }
    let mut document = json!({"metadata":{"rows":[0]},"body":""});
    assert!(
        apply_patch(
            &mut document,
            &json!([{"op":"test","path":"/metadata/rows/0","value":0}])
        )
        .is_some()
    );
    assert!(
        apply_patch(
            &mut document,
            &json!([{"op":"remove","path":"/metadata/rows/0"}])
        )
        .is_some()
    );
    assert_eq!(document["metadata"]["rows"], json!([]));
    assert!(
        apply_patch(
            &mut document,
            &json!([{"op":"replace","path":"","value":{"body":""}}])
        )
        .is_none()
    );
}

/// Undeclared hash epochs are detected across records and unsupported textual versions are disclosed.
#[test]
fn history_detected_hash_epochs() -> Result<(), Box<dyn std::error::Error>> {
    let mut entry = genesis();
    entry
        .as_object_mut()
        .ok_or("record")?
        .shift_remove("item_hash_version");
    assert_eq!(
        verify_records(std::slice::from_ref(&entry), None)["ok"],
        true
    );
    let mut unsupported = entry.clone();
    unsupported["item_hash_version"] = json!("future");
    assert!(
        verify_records(&[unsupported], None)["errors"][0]
            .as_str()
            .is_some_and(|error| error.contains("future"))
    );
    let mut baseline = entry.clone();
    baseline["op"] = json!("history_compact_baseline");
    assert!(
        verify_records(&[baseline, entry], None)["errors"]
            .as_array()
            .is_some_and(|errors| errors.iter().any(|error| error
                .as_str()
                .is_some_and(|text| text.contains("duplicate_create"))))
    );
    Ok(())
}

/// Compact changed fields preserve root namespaces and reject malformed patch rows.
#[test]
fn compact_history_field_names() -> Result<(), Box<dyn std::error::Error>> {
    for (path, expected) in [
        ("/body/line", "body"),
        ("/body", "body"),
        ("/metadata", "metadata"),
        ("/front_matter", "metadata"),
        ("/metadatax/title", "metadatax"),
        ("/front_matter/tag~1name", "tag/name"),
        ("/other~0field/value", "other~field"),
        ("/", "root"),
        ("", "root"),
    ] {
        assert_eq!(changed_field(path), expected);
    }
    for raw in ["0x", "0b", "0o", "0xgg"] {
        assert!(numeric(raw, true, 0, "invalid".to_owned()).is_err());
    }
    let (_root, workspace) = workspace()?;
    fs::write(workspace.pm_root().join("settings.json"), "{}")?;
    fs::write(
        workspace.pm_root().join("history/plain.jsonl"),
        format!(
            "{}\n",
            json!({"patch":[{"op":"copy","path":"/metadata/title","from":"/body"},{"op":42,"path":"/body"},{"op":"remove","path":42}]})
        ),
    )?;
    assert_eq!(
        workspace.history("plain", &HistoryOptions::default())?["compact_history"][0]["patch_ops"],
        1
    );
    Ok(())
}

/// Legacy epoch-two current state accepts its historical test projection while preserving the stream.
#[test]
fn legacy_epoch_two_current_state() -> Result<(), Box<dyn std::error::Error>> {
    let mut entry = genesis();
    entry["patch"][0]["value"]["tests"] = json!([{"command":"z","workspace_context_mode":"isolated","provenance":{}},{"command":"a"}]);
    let metadata = entry["patch"][0]["value"].clone();
    let current: crate::ItemDocument = serde_json::from_value(metadata.clone())?;
    entry["item_hash_version"] = json!(2);
    entry["before_hash"] = json!(replay_hash(
        &json!({"metadata":{},"body":""}),
        2,
        true,
        "sha256"
    ));
    entry["after_hash"] = json!(replay_hash(
        &json!({"metadata":metadata,"body":""}),
        2,
        true,
        "sha256"
    ));
    assert_eq!(verify_records(&[entry], Some(&current))["ok"], true);
    Ok(())
}

/// Malformed RFC 6902 operations refuse at each required source, target and value boundary.
#[test]
fn incomplete_patch_operands() {
    for change in [
        json!({"op":"copy","path":"/metadata/copy"}),
        json!({"op":"copy","from":"/missing","path":"/metadata/copy"}),
        json!({"op":"move","from":"","path":"/metadata/copy"}),
        json!({"op":"copy","from":"/metadata/value","path":"/missing/copy"}),
        json!({"op":"replace","path":""}),
        json!({"op":"test","path":""}),
        json!({"op":"replace","path":"/metadata/value"}),
        json!({"op":"test","path":"/metadata/missing","value":1}),
        json!({"op":"test","path":"/metadata/value"}),
        json!({"op":"add","path":"/metadata/rows/0"}),
        json!({"op":"replace","path":"/metadata/rows/9","value":1}),
        json!({"op":"replace","path":"/metadata/rows/0"}),
        json!({"op":"test","path":"/metadata/rows/9","value":1}),
        json!({"op":"test","path":"/metadata/rows/0"}),
    ] {
        let mut document = json!({"metadata":{"value":1,"rows":[0]},"body":""});
        assert!(apply_patch(&mut document, &json!([change])).is_none());
    }
}

/// History propagates real settings and corpus storage failures without synthesizing a timeline.
#[test]
fn history_configuration_failures() -> Result<(), Box<dyn std::error::Error>> {
    let (_root, workspace) = workspace()?;
    fs::write(workspace.pm_root().join("settings.json"), "invalid")?;
    assert!(
        workspace
            .history("one", &HistoryOptions::default())
            .is_err()
    );
    fs::write(workspace.pm_root().join("settings.json"), "{}")?;
    fs::write(
        workspace.pm_root().join("tasks/invalid.toon"),
        "invalid item",
    )?;
    assert!(
        workspace
            .history("one", &HistoryOptions::default())
            .is_err()
    );
    fs::remove_file(workspace.pm_root().join("tasks/invalid.toon"))?;
    fs::write(
        workspace.pm_root().join("history/sample-one.jsonl"),
        "{broken}",
    )?;
    assert!(
        workspace
            .history("sample-one", &HistoryOptions::default())
            .is_err()
    );
    Ok(())
}

/// Multiple retained proofs reconstruct earlier envelopes in reverse chronological order.
#[test]
fn retained_proof_generations() {
    let original = genesis();
    let proof = json!({"before_hash":original["before_hash"],"after_hash":original["after_hash"],"patch_hash":digest(&original["patch"]),"patch":original["patch"]});
    let entry = json!({"patch":original["patch"],"reanchor_evidence":[proof.clone(),proof]});
    assert_eq!(rewrite_error(&entry), None);
    assert_eq!(
        rewrite_error(&json!({"reanchor_evidence":[42]})),
        Some("rewrite_evidence_invalid")
    );
    assert_eq!(
        rewrite_error(&json!({"reanchor_evidence":[{"record_hash_version":1}]})),
        Some("rewrite_evidence_invalid")
    );
}

/// Legacy hashing retains malformed optional rows while normalizing only supported object members.
#[test]
fn legacy_optional_row_shapes() {
    let mut metadata = genesis()["patch"][0]["value"].clone();
    metadata["dependencies"] = json!([{"id":42,"kind":"related"}]);
    metadata["tests"] = json!([42,{"command":"same","path":"same","scope":"same"},{"command":"same","path":"same","scope":"same"}]);
    metadata["test_runs"] = json!([42]);
    let document = json!({"metadata":metadata,"body":""});
    assert_eq!(
        replay_hash(&document, 1, false, "sha256"),
        replay_hash(&document, 3, false, "sha256")
    );
    let original = genesis();
    let proof = json!({"before_hash":original["before_hash"],"after_hash":original["after_hash"],"patch_hash":digest(&original["patch"]),"record":original,"hash_algorithm":"sha512"});
    assert_eq!(
        rewrite_error(&json!({"reanchor_evidence":[proof]})),
        Some("rewrite_evidence_record_hash_mismatch")
    );
}

/// Reserved prototype traversal is refused for writes, tests and copy/move sources.
#[test]
fn history_prototype_paths() {
    for path in [
        "/metadata/__proto__/value",
        "/metadata/constructor/prototype/value",
    ] {
        assert!(protected_history_pointer(path));
        for op in ["add", "copy", "move"] {
            let mut document = json!({"metadata":{"__proto__":{"value":1},"constructor":{"prototype":{"value":1}},"ordinary":1},"body":""});
            let patch = if op == "add" {
                json!({"op":op,"path":path,"value":2})
            } else {
                json!({"op":op,"from":path,"path":"/metadata/copied"})
            };
            assert!(apply_patch(&mut document, &json!([patch])).is_none());
        }
    }
    for path in ["", "/metadata/prototype", "/metadata/constructor/ordinary"] {
        assert!(!protected_history_pointer(path));
    }
}

/// The public history reader verifies the full stream while limiting only rendered rows.
#[test]
fn history_api_verified_projection() -> Result<(), Box<dyn std::error::Error>> {
    let (_root, workspace) = workspace()?;
    let entry = genesis();
    let item_path = workspace.pm_root().join("tasks/sample-one.toon");
    let history_path = workspace.pm_root().join("history/sample-one.jsonl");
    fs::write(
        &item_path,
        toon_format::encode_default(&entry["patch"][0]["value"])?,
    )?;
    fs::write(&history_path, format!("{entry}\n"))?;
    let before = (fs::read(&item_path)?, fs::read(&history_path)?);
    for limit in ["0", "1"] {
        let output = workspace.history(
            "sample-one",
            &HistoryOptions {
                limit: Some(limit.to_owned()),
                verify: true,
                ..Default::default()
            },
        )?;
        assert_eq!(output["verification"]["ok"], true);
        assert_eq!(output["verification"]["entries"], 1);
        assert_eq!(output["count"], limit.parse::<usize>()?);
    }
    assert!(
        workspace
            .history(
                "sample-one",
                &HistoryOptions {
                    limit: Some("bad".to_owned()),
                    ..Default::default()
                }
            )
            .is_err()
    );
    let current: crate::ItemDocument = serde_json::from_value(entry["patch"][0]["value"].clone())?;
    let mut broken = entry;
    broken["after_hash"] = json!("wrong");
    assert_eq!(verify_records(&[broken], Some(&current))["ok"], false);
    assert_eq!((fs::read(item_path)?, fs::read(history_path)?), before);
    Ok(())
}
