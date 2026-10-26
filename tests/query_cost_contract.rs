//! SDK/CLI acceptance on a real initialized non-git tracker, including seeded replay.
use pm_rust::{ItemDocument, QueryCostRequest, QueryProjection, Workspace};
use proptest::{
    prelude::*,
    test_runner::{Config, RngSeed, TestRunner},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
};

/// Fallible acceptance result.
type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Uses the pinned core CLI only to initialize the genuine non-git test backend.
fn fixture() -> Result<(tempfile::TempDir, Workspace), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle/launch.mts"))
        .current_dir(directory.path())
        .arg("--path")
        .arg(&root)
        .args(["init", "--defaults", "--agent-guidance", "skip"])
        .env(
            "GIT_CEILING_DIRECTORIES",
            directory.path().parent().ok_or("parent")?,
        )
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::create_dir_all(root.join("tasks"))?;
    Ok((directory, Workspace::discover(&root)?))
}

/// Captures every relative filename and byte; includes history, locks and runtime state.
fn snapshot(
    root: &Path,
) -> Result<std::collections::BTreeMap<std::path::PathBuf, Vec<u8>>, std::io::Error> {
    let mut output = std::collections::BTreeMap::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            output.insert(entry.file_name().into(), Vec::new());
            for (path, bytes) in snapshot(&entry.path())? {
                output.insert(Path::new(&entry.file_name()).join(path), bytes);
            }
        } else {
            output.insert(entry.file_name().into(), fs::read(entry.path())?);
        }
    }
    Ok(output)
}

/// Compact plans and typed remedies need neither Node nor Git on the runtime path.
#[test]
fn cli_json_toon_refusals_and_zero_writes() -> TestResult {
    let (directory, workspace) = fixture()?;
    let empty_path = tempfile::tempdir()?;
    fs::write(
        workspace.pm_root().join("tasks/raw.toon"),
        "body: context\n",
    )?;
    let before = snapshot(directory.path())?;
    for full in [false, true] {
        for json in [false, true] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_pm-rust"));
            command
                .current_dir(directory.path())
                .args(["explain", "list"])
                .env("PATH", empty_path.path())
                .env(
                    "GIT_CEILING_DIRECTORIES",
                    directory.path().parent().ok_or("parent")?,
                );
            if full {
                command.arg("--full");
            }
            if json {
                command.arg("--json");
            }
            let output = command.output()?;
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(output.stdout.len() < 2000);
            if json {
                assert_eq!(String::from_utf8_lossy(&output.stdout).lines().count(), 1);
                let value: Value = serde_json::from_slice(&output.stdout)?;
                assert_eq!(value["observed_candidate_files"], 1);
                assert_eq!(value["projection"], if full { "full" } else { "brief" });
            } else {
                assert!(String::from_utf8(output.stdout)?.contains("pm-rust/query-cost/v1"));
            }
        }
    }
    for (flags, code) in [
        (vec!["--max-bytes", "0"], "query_cost_invalid_request"),
        (vec!["--max-entries", "0"], "query_cost_invalid_request"),
        (vec!["--max-bytes", "1"], "query_cost_scan_limit"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
            .current_dir(directory.path())
            .args(["explain", "list"])
            .args(flags)
            .output()?;
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let refusal: Value = serde_json::from_slice(&output.stderr)?;
        assert_eq!(refusal["code"], code);
        assert!(
            refusal["required"]
                .as_str()
                .is_some_and(|text| text.contains(';'))
        );
    }
    for json in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_pm-rust"));
        command
            .current_dir(directory.path())
            .args(["explain", "list"])
            .env("PATH", empty_path.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if json {
            command.arg("--json");
        }
        let mut child = command.spawn()?;
        drop(child.stdout.take());
        let output = child.wait_with_output()?;
        assert_eq!(
            output.status.code(),
            Some(2),
            "disconnected consumers must refuse delivery"
        );
    }
    assert_eq!(before, snapshot(directory.path())?);
    let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .current_dir(empty_path.path())
        .args(["explain", "list"])
        .output()?;
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stderr)?["code"],
        "query_cost_source"
    );
    Ok(())
}

/// Five hundred seeded operation sequences replay old complete inputs and compare SDK state.
#[test]
fn seeded_model_preserves_every_field_and_relationship_across_explanations() -> TestResult {
    let (_directory, workspace) = fixture()?;
    let strategy = prop::collection::vec(
        ("[ a-zA-Z0-9]{1,30}", 0_u8..4, 0_usize..32, any::<bool>()),
        3..10,
    );
    let mut runner = TestRunner::new(Config {
        cases: 500,
        rng_seed: RngSeed::Fixed(20_261_026),
        failure_persistence: None,
        ..Config::default()
    });
    runner.run(&strategy, |operations| {
        let mut prior = Vec::<(ItemDocument, String)>::new();
        for (text, variant, replay, full) in operations {
            let value = json!({
                "id":"demo-model", "title":format!("  Model {text}  "), "type":"Task", "status":format!("custom-{variant}"),
                "priority":variant, "description":format!(" \r\n{text}\r\n "), "tags":["custom",text],
                "created_at":"2026-10-21T00:00:00.000Z", "updated_at":"2026-10-25T00:00:00.000Z",
                "parent":"demo-parent", "deadline":"2026-11-01", "dependencies":[{"id":"demo-peer","kind":"relates_to"}],
                "blocked_by":"demo-peer", "order":variant, "assignee":"fixture-agent", "risk":"low", "release":"test",
                "comments":[{"text":text,"author":"fixture-agent"}], "notes":[{"text":text}], "learnings":[{"text":text}],
                "test_runs":[{"passed":true}], "reminders":[{"text":text}], "events":[{"title":text}],
                "files":[{"path":"src/lib.rs"}], "tests":[{"command":"cargo test"}], "docs":[{"path":"README.md"}],
                "blocked_reason":text,"unblock_note":text, "body":format!(" \r\n{text}\n  body  "), "foreign_extension":{"preserve":text}
            });
            let expected: ItemDocument = serde_json::from_value(value.clone()).map_err(|error| TestCaseError::fail(error.to_string()))?;
            let mut bytes = toon_format::encode_default(&value).map_err(|error| TestCaseError::fail(error.to_string()))?;
            if variant % 2 == 1 { bytes = bytes.replace('\n', "\r\n"); }
            prior.push((expected, bytes));
            // A simple last-input-wins reference model: replay old complete inputs,
            // not hashes or a test-side imitation of the core normalization rules.
            let (model, source) = &prior[replay % prior.len()];
            let path = workspace.pm_root().join("tasks/demo-model.toon");
            fs::write(&path, source).map_err(|error| TestCaseError::fail(error.to_string()))?;
            let before = workspace.get("demo-model").map_err(|error| TestCaseError::fail(error.to_string()))?;
            prop_assert_eq!(&before, model);
            let roles = ["active", "blocked", "terminal_done", "terminal_canceled"];
            let statuses = json!({"statuses": (0..4).map(|index| json!({
                "id":format!("custom-{index}"), "roles":[roles[(index + usize::from(variant)) % 4]]
            })).collect::<Vec<_>>()});
            let registry = workspace.pm_root().join("schema/statuses.json");
            let registry_bytes = serde_json::to_vec(&statuses).map_err(|error| TestCaseError::fail(error.to_string()))?;
            fs::write(&registry, &registry_bytes).map_err(|error| TestCaseError::fail(error.to_string()))?;
            let plan = workspace.explain_list(&QueryCostRequest { projection: if full { QueryProjection::Full } else { QueryProjection::Brief }, ..QueryCostRequest::default() }).map_err(|error| TestCaseError::fail(error.to_string()))?;
            prop_assert_eq!(fs::read(&registry).map_err(|error| TestCaseError::fail(error.to_string()))?, registry_bytes);
            prop_assert_eq!(plan.candidate_rows.upper, 1);
            prop_assert_eq!(plan.item_read_bytes.upper, source.len() as u64);
            prop_assert!(plan.decoder_input_bytes.upper <= 2 * source.len() as u64);
            let after = workspace.get("demo-model").map_err(|error| TestCaseError::fail(error.to_string()))?;
            prop_assert_eq!(&after, model);
            prop_assert_eq!(fs::read_to_string(&path).map_err(|error| TestCaseError::fail(error.to_string()))?, source.clone());
        }
        Ok(())
    })?;
    Ok(())
}
