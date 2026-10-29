//! Native-only admission, published oracle parity and seeded state-preservation evidence.
use pm_rust::{
    Workspace,
    admission::{ADMISSION_LIMITS, AdmissionBatch},
};
use proptest::{
    prelude::*,
    test_runner::{Config, RngSeed, TestRunner},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
#[path = "support/process_environment.rs"]
mod process_environment;

/// Fallible acceptance result.
type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Real published initialization without Git; product execution has an empty PATH.
fn fixture() -> Result<(tempfile::TempDir, Workspace), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let mut command = Command::new("node");
    process_environment::isolate_environment(&mut command);
    let output = command
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle/launch.mts"))
        .current_dir(directory.path())
        .args(["init", "sample", "--defaults", "--agent-guidance", "skip"])
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
    fs::create_dir_all(directory.path().join(".agents/pm/tasks"))?;
    let workspace = Workspace::discover(directory.path())?;
    Ok((directory, workspace))
}

/// Complete byte inventory includes all runtime, lock and history directories.
fn snapshot(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>, std::io::Error> {
    let mut files = BTreeMap::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            files.insert(entry.file_name().into(), Vec::new());
            for (path, bytes) in snapshot(&entry.path())? {
                files.insert(Path::new(&entry.file_name()).join(path), bytes);
            }
        } else {
            files.insert(entry.file_name().into(), fs::read(entry.path())?);
        }
    }
    Ok(files)
}

/// Canonical progressive create with every scope assertion explicit.
fn create(id: &str) -> Value {
    json!({"op":"create","id":id,"options":{"title":"A native admission fixture","type":"Task","status":"open","priority":2,"createMode":"progressive","parent":"none","assignee":"none","allowDuplicate":true}})
}

/// CLI stdin boundary and compact agent rendering work without Node or Git.
#[test]
fn admission_cli_is_native_bounded_and_never_writes() -> TestResult {
    let (directory, _) = fixture()?;
    let empty_path = tempfile::tempdir()?;
    let before = snapshot(directory.path())?;
    for json_output in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_pm-rust"));
        process_environment::isolate_environment(&mut command);
        command
            .current_dir(directory.path())
            .arg("preview-item-mutations")
            .env("PATH", empty_path.path())
            .env(
                "GIT_CEILING_DIRECTORIES",
                directory.path().parent().ok_or("parent")?,
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if json_output {
            command.arg("--json");
        }
        let mut child = command.spawn()?;
        child
            .stdin
            .take()
            .ok_or("stdin")?
            .write_all(&serde_json::to_vec(
                &json!({"author":"fixture-agent","mutations":[create("sample-native")]}),
            )?)?;
        let output = child.wait_with_output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.len() < 2000);
        if json_output {
            let receipt: Value = serde_json::from_slice(&output.stdout)?;
            assert_eq!(receipt["diagnostics"][0]["state"], "admitted");
            assert_eq!(receipt["limits"]["intents"], 100);
            assert_eq!(String::from_utf8_lossy(&output.stdout).lines().count(), 1);
        } else {
            assert!(String::from_utf8(output.stdout)?.contains("pm-rust/item-admission/v1"));
        }
    }
    assert_eq!(before, snapshot(directory.path())?);
    for (input, code) in [
        (json!({"author":"fixture-agent","mutations":[{"op":"delete","id":"sample-native","options":{}}]}).to_string(), "admission_unsupported_intent"),
        (" ".repeat(ADMISSION_LIMITS.input_bytes + 1), "admission_invalid_request"),
        ("{\"author\":\"one\",\"author\":\"two\",\"mutations\":[]}".to_owned(), "admission_invalid_request"),
    ] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_pm-rust")).current_dir(empty_path.path())
            .arg("preview-item-mutations").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
        // A bounded reader can close stdin after its detection byte.
        let _ = child.stdin.take().ok_or("stdin")?.write_all(input.as_bytes());
        let output = child.wait_with_output()?;
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let receipt: Value = serde_json::from_slice(&output.stderr)?;
        assert_eq!(receipt["code"], code);
        assert_eq!(receipt["limits"]["output_bytes"], 65_536);
        assert!(receipt["required"].as_str().is_some_and(|s| !s.is_empty()));
    }
    Ok(())
}

/// Five hundred seeded sequences replay old full records, including foreign fields and CRLF.
#[test]
fn admission_seeded_model_preserves_sdk_state_and_all_relationships() -> TestResult {
    let (_directory, workspace) = fixture()?;
    let strategy = prop::collection::vec(
        ("[ a-zA-Z0-9]{1,25}", 0_u8..4, 0_usize..32, any::<bool>()),
        2..6,
    );
    let mut runner = TestRunner::new(Config {
        cases: 500,
        rng_seed: RngSeed::Fixed(20_261_029),
        failure_persistence: None,
        ..Config::default()
    });
    runner.run(&strategy, |operations| {
        let mut old = Vec::new();
        for (text, variant, replay, crlf) in operations {
            let record = json!({
                "id":"sample-model","title":format!("  Model {text}  "),"type":"Task","status":format!("custom-{variant}"),"priority":variant,
                "description":format!(" \r\n{text}\r\n "),"tags":[text,"custom"],"created_at":"2026-10-21T00:00:00.000Z","updated_at":"2026-10-25T00:00:00.000Z",
                "parent":"sample-parent","deadline":"2026-11-01","dependencies":[{"id":"sample-peer","kind":"relates_to"}],"blocked_by":"sample-peer",
                "order":variant,"author":"fixture-agent","assignee":"fixture-agent","risk":"low","release":"test","comments":[{"text":text,"author":"fixture-agent"}],
                "notes":[{"text":text}],"learnings":[{"text":text}],"test_runs":[{"passed":true}],"reminders":[{"text":text}],"events":[{"title":text}],
                "files":[{"path":"src/lib.rs"}],"tests":[{"command":"cargo test"}],"docs":[{"path":"README.md"}],
                "blocked_reason":text,"unblock_note":text,"body":format!(" \r\n{text}\n  body  "),"foreign_extension":{"preserve":text}
            });
            let expected: pm_rust::ItemDocument = serde_json::from_value(record.clone()).map_err(|e| TestCaseError::fail(e.to_string()))?;
            let bytes = toon_format::encode_default(&record).map_err(|e| TestCaseError::fail(e.to_string()))?;
            old.push((expected, if crlf { bytes.replace('\n', "\r\n") } else { bytes }));
            let (model, source) = &old[replay % old.len()];
            let path = workspace.pm_root().join("tasks/sample-model.toon");
            fs::write(&path, source).map_err(|e| TestCaseError::fail(e.to_string()))?;
            let roles = ["active","blocked","terminal_done","terminal_canceled"];
            fs::write(workspace.pm_root().join("schema/statuses.json"), serde_json::to_vec(&json!({"statuses":(0..4).map(|i| json!({"id":format!("custom-{i}"),"roles":[roles[(i + usize::from(variant)) % 4]]})).collect::<Vec<_>>()})).map_err(|e| TestCaseError::fail(e.to_string()))?).map_err(|e| TestCaseError::fail(e.to_string()))?;
            // Random ambient focus never becomes an implicit parent or scope.
            let runtime = workspace.pm_root().join("runtime");
            fs::create_dir_all(&runtime).map_err(|e| TestCaseError::fail(e.to_string()))?;
            fs::write(runtime.join("session.json"), json!({"focused_item":format!("sample-focus-{variant}")}).to_string()).map_err(|e| TestCaseError::fail(e.to_string()))?;
            let batch = AdmissionBatch::from_json(&serde_json::to_vec(&json!({"author":"fixture-agent","mutations":[
                {"op":"update","id":"sample-model","options":{"title":format!("Revised {text}"),"description":format!(" \r\n{text}\r\n"),"body":text,"priority":variant,"tags":format!(" {text}, CUSTOM ")}},
                {"op":"update","id":"sample-missing","options":{"title":"missing"}},
                create("sample-created")
            ]})).map_err(|e| TestCaseError::fail(e.to_string()))?).map_err(|e| TestCaseError::fail(e.to_string()))?;
            let before = snapshot(workspace.pm_root()).map_err(|e| TestCaseError::fail(e.to_string()))?;
            let preview = workspace.preview_item_mutations(&batch).map_err(|e| TestCaseError::fail(e.to_string()))?;
            prop_assert!(!preview.ok);
            prop_assert_eq!(preview.diagnostics[0].state, "admitted");
            prop_assert_eq!(preview.diagnostics[1].code, "item_not_found");
            prop_assert_eq!(preview.diagnostics[2].state, "admitted");
            prop_assert_eq!(&workspace.get("sample-model").map_err(|e| TestCaseError::fail(e.to_string()))?, model);
            prop_assert_eq!(snapshot(workspace.pm_root()).map_err(|e| TestCaseError::fail(e.to_string()))?, before);
        }
        Ok(())
    })?;
    Ok(())
}

/// Runs the real installed SDK oracle; never resolves it from argv or executable symlinks.
#[test]
fn admission_published_preview_oracle_matches_the_declared_slice() -> TestResult {
    let mut command = Command::new("node");
    process_environment::isolate_environment(&mut command);
    let output = command
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .arg("tests/support/admission-oracle.mjs")
        .env("PM_RUST_ADMISSION_BINARY", env!("CARGO_BIN_EXE_pm-rust"))
        .output()?;
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("oracle cases="));
    eprintln!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}

/// Ordinary native CLI writes share strict authorization with SDK and previews.
#[test]
fn admission_governance_also_protects_native_cli_writes() -> TestResult {
    let (directory, workspace) = fixture()?;
    workspace.create(serde_json::from_value(json!({"id":"sample-owned","title":"Owned","description":"","type":"Task","status":"open","priority":2,"tags":[],"body":"","author":"owner-agent","timestamp":null,"message":null,"provenance_role":null,"force_stale_lock":false}))?)?;
    workspace.claim(&pm_rust::OwnershipItem {
        id: "sample-owned".into(),
        author: "owner-agent".into(),
        timestamp: None,
        message: None,
        force: false,
        if_available: false,
        provenance_role: None,
    })?;
    let settings_path = workspace.pm_root().join("settings.json");
    let mut settings: Value = serde_json::from_slice(&fs::read(&settings_path)?)?;
    settings["governance"]["preset"] = json!("strict");
    fs::write(&settings_path, serde_json::to_vec(&settings)?)?;
    let before = workspace.get("sample-owned")?;
    let bytes = snapshot(workspace.pm_root())?;
    for args in [
        vec![
            "update",
            "sample-owned",
            "--title",
            "Owned",
            "--author",
            "other-agent",
        ],
        vec![
            "comment",
            "sample-owned",
            "Annotation",
            "--author",
            "other-agent",
        ],
        vec![
            "update",
            "sample-owned",
            "--title",
            "Owned",
            "--author",
            "other-agent",
            "--force-stale-lock",
        ],
        vec![
            "comment",
            "sample-owned",
            "Annotation",
            "--author",
            "other-agent",
            "--force-stale-lock",
        ],
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_pm-rust"));
        process_environment::isolate_environment(&mut command);
        let output = command
            .current_dir(directory.path())
            .args(args)
            .env("PATH", "")
            .env(
                "GIT_CEILING_DIRECTORIES",
                directory.path().parent().ok_or("parent")?,
            )
            .output()?;
        assert_eq!(
            output.status.code(),
            Some(4),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let error: Value = serde_json::from_slice(&output.stderr)?;
        assert_eq!(error["code"], "ownership_conflict");
        assert_eq!(workspace.get("sample-owned")?, before);
        assert_eq!(snapshot(workspace.pm_root())?, bytes);
    }
    Ok(())
}

/// Both preview renderers and ordinary CLI writes refuse an unresolvable item file.
#[test]
fn admission_physical_identity_is_enforced_without_git_or_node() -> TestResult {
    let (directory, workspace) = fixture()?;
    workspace.create(serde_json::from_value(json!({"id":"sample-identity","title":"Identity","description":"","type":"Task","status":"open","priority":2,"tags":[],"body":"","author":"fixture-agent","timestamp":null,"message":null,"provenance_role":null,"force_stale_lock":false}))?)?;
    let canonical = workspace.pm_root().join("tasks/sample-identity.toon");
    let alias = workspace.pm_root().join("tasks/sample-other.toon");
    fs::rename(&canonical, &alias)?;
    let before = snapshot(workspace.pm_root())?;
    let input = serde_json::to_vec(
        &json!({"author":"fixture-agent","mutations":[{"op":"update","id":"sample-identity","options":{"title":"Must refuse"}}]}),
    )?;
    for json_output in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_pm-rust"));
        process_environment::isolate_environment(&mut command);
        command
            .current_dir(directory.path())
            .arg("preview-item-mutations")
            .env("PATH", "")
            .env(
                "GIT_CEILING_DIRECTORIES",
                directory.path().parent().ok_or("parent")?,
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if json_output {
            command.arg("--json");
        }
        let mut child = command.spawn()?;
        child.stdin.take().ok_or("stdin")?.write_all(&input)?;
        let output = child.wait_with_output()?;
        assert_eq!(output.status.code(), Some(2));
        let refusal: Value = serde_json::from_slice(&output.stderr)?;
        assert_eq!(refusal["code"], "admission_source");
        assert_eq!(
            refusal["limits"]["source_rows"],
            ADMISSION_LIMITS.source_rows
        );
        assert_eq!(snapshot(workspace.pm_root())?, before);
    }
    for args in [
        vec![
            "update",
            "sample-other",
            "--title",
            "Must refuse",
            "--author",
            "fixture-agent",
        ],
        vec![
            "comment",
            "sample-other",
            "Must refuse",
            "--author",
            "fixture-agent",
        ],
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_pm-rust"));
        process_environment::isolate_environment(&mut command);
        let output = command
            .current_dir(directory.path())
            .args(args)
            .env("PATH", "")
            .env(
                "GIT_CEILING_DIRECTORIES",
                directory.path().parent().ok_or("parent")?,
            )
            .output()?;
        assert!(!output.status.success());
        let mut after = snapshot(workspace.pm_root())?;
        // Target validation runs under the lease; retain its shared rendezvous inode.
        if let Some(lease) = after.remove(Path::new("locks/sample-other.lock.gate-lease")) {
            assert!(lease.is_empty());
        }
        assert_eq!(after, before);
    }
    assert!(workspace.get("sample-identity").is_err());
    assert!(workspace.get("sample-other").is_err());
    fs::rename(alias, canonical)?;
    assert_eq!(
        workspace.get("sample-identity")?.metadata.id,
        "sample-identity"
    );
    assert!(
        workspace
            .preview_item_mutations(&AdmissionBatch::from_json(&input)?)?
            .ok
    );
    Ok(())
}
