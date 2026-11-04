//! Real native invocations, no-runtime discovery and seeded SDK state preservation.
use pm_rust::{
    Workspace,
    discovery::{DISCOVERY_MAX_BYTES, DiscoveryRequest, DiscoveryResponse, discover_capabilities},
};
use proptest::{
    prelude::*,
    test_runner::{Config, RngSeed, TestRunner},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};
#[path = "support/process_environment.rs"]
mod process_environment;

/// Fallible acceptance result.
type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Runs an actual native process with neither Node nor Git on its PATH.
fn native(
    root: &Path,
    args: &[&str],
    stdin: Option<&str>,
) -> Result<Output, Box<dyn std::error::Error>> {
    let empty = tempfile::tempdir()?;
    let mut command = Command::new(env!("CARGO_BIN_EXE_pm-rust"));
    process_environment::isolate_environment(&mut command);
    command
        .current_dir(root)
        .args(args)
        .env("PATH", empty.path())
        .env("GIT_CEILING_DIRECTORIES", root.parent().ok_or("parent")?)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    child
        .stdin
        .take()
        .ok_or("stdin")?
        .write_all(stdin.unwrap_or_default().as_bytes())?;
    Ok(child.wait_with_output()?)
}

/// Initializes a tracker through the pinned public host, outside any Git repository.
fn fixture() -> Result<(tempfile::TempDir, Workspace), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let mut command = Command::new("node");
    process_environment::isolate_environment(&mut command);
    let initialized = command
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle/launch.mts"))
        .current_dir(directory.path())
        .args(["init", "sample", "--defaults", "--agent-guidance", "skip"])
        .env(
            "GIT_CEILING_DIRECTORIES",
            directory.path().parent().ok_or("parent")?,
        )
        .output()?;
    assert!(
        initialized.status.success(),
        "{}",
        String::from_utf8_lossy(&initialized.stderr)
    );
    fs::create_dir_all(directory.path().join(".agents/pm/tasks"))?;
    let workspace = Workspace::discover(directory.path())?;
    Ok((directory, workspace))
}

/// Complete file/directory snapshot detects incidental cache, lock or journal writes.
fn snapshot(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>, std::io::Error> {
    let mut result = BTreeMap::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            result.insert(entry.file_name().into(), Vec::new());
            for (path, bytes) in snapshot(&entry.path())? {
                result.insert(Path::new(&entry.file_name()).join(path), bytes);
            }
        } else {
            result.insert(entry.file_name().into(), fs::read(entry.path())?);
        }
    }
    Ok(result)
}

/// One call supplies the complete exact inventory even without a tracker or runtimes.
#[test]
fn discovery_is_complete_bounded_and_tracker_free() -> TestResult {
    let directory = tempfile::tempdir()?;
    let expected = serde_json::to_value(discover_capabilities(&DiscoveryRequest::default())?)?;
    for json_output in [false, true] {
        let args = if json_output {
            vec!["--workspace", "missing", "capabilities", "--json"]
        } else {
            vec!["--workspace", "missing", "capabilities"]
        };
        let output = native(directory.path(), &args, None)?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.len() <= DISCOVERY_MAX_BYTES as usize);
        let decoded: Value = if json_output {
            serde_json::from_slice(&output.stdout)?
        } else {
            toon_format::decode_default(&String::from_utf8(output.stdout)?)?
        };
        assert_eq!(decoded, expected);
        assert!(output.stderr.is_empty());
    }
    assert!(snapshot(directory.path())?.is_empty());
    for args in [
        vec!["capabilities", "--operation", "sync", "--json"],
        vec![
            "capabilities",
            "--protocol",
            "pm-rust/capabilities/v999",
            "--json",
        ],
        vec!["capabilities", "--output-bytes", "0", "--json"],
        vec!["capabilities", "--output-bytes", "1024", "--json"],
        vec!["capabilities", "--adapter", "{}", "--json"],
    ] {
        let output = native(directory.path(), &args, None)?;
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let refusal: Value = serde_json::from_slice(&output.stderr)?;
        assert_eq!(refusal["ok"], false);
        assert_eq!(refusal["exit_code"], 2);
        let retry = refusal["refusal"]["retry_args"]
            .as_array()
            .ok_or("retry")?
            .iter()
            .map(|v| v.as_str().ok_or("argument"))
            .collect::<Result<Vec<_>, _>>()?;
        assert!(native(directory.path(), &retry, None)?.status.success());
    }
    Ok(())
}

/// Every advertised operation executes on a genuine tracker, rather than merely parsing help.
#[test]
#[allow(clippy::too_many_lines)]
fn every_advertised_operation_is_invoked() -> TestResult {
    let (directory, workspace) = fixture()?;
    let mut invoked = BTreeSet::new();
    let mut invoke = |args: &[&str], input: Option<&str>| -> TestResult {
        let output = native(directory.path(), args, input)?;
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!output.stdout.is_empty());
        invoked.insert(args[0].to_owned());
        Ok(())
    };
    invoke(&["capabilities", "--json"], None)?;
    invoke(
        &[
            "create",
            "--id",
            "sample-surface",
            "--title",
            "Surface example",
            "--type",
            "Task",
            "--author",
            "fixture-agent",
        ],
        None,
    )?;
    assert_eq!(
        workspace.get("sample-surface")?.metadata.title,
        "Surface example"
    );
    invoke(
        &[
            "update",
            "sample-surface",
            "--title",
            "Surface updated",
            "--author",
            "fixture-agent",
        ],
        None,
    )?;
    assert_eq!(
        workspace.get("sample-surface")?.metadata.title,
        "Surface updated"
    );
    invoke(
        &[
            "comment",
            "sample-surface",
            "Surface comment",
            "--author",
            "fixture-agent",
        ],
        None,
    )?;
    for args in [
        vec!["get", "sample-surface"],
        vec!["list", "--all", "--json"],
        vec!["search", "Surface", "--json"],
        vec!["context", "--json"],
        vec!["history", "sample-surface", "--verify", "--json"],
        vec!["deps", "sample-surface", "--json"],
        vec!["assurance", "list", "measurement", "--json"],
        vec!["read-session", "--json"],
        vec!["explain", "list", "--json"],
        vec!["changes", "--json"],
        vec![
            "files",
            "sample-surface",
            "--add",
            "src/lib.rs",
            "--author",
            "fixture-agent",
            "--json",
        ],
        vec![
            "docs",
            "sample-surface",
            "--add",
            "README.md",
            "--author",
            "fixture-agent",
            "--json",
        ],
        vec![
            "test",
            "sample-surface",
            "--add",
            "command=cargo test",
            "--author",
            "fixture-agent",
            "--json",
        ],
        vec![
            "claim",
            "sample-surface",
            "--author",
            "fixture-agent",
            "--json",
        ],
        vec![
            "release",
            "sample-surface",
            "--author",
            "fixture-agent",
            "--json",
        ],
        vec![
            "start-task",
            "sample-surface",
            "--author",
            "fixture-agent",
            "--json",
        ],
        vec![
            "pause-task",
            "sample-surface",
            "--author",
            "fixture-agent",
            "--json",
        ],
    ] {
        invoke(&args, None)?;
    }
    invoke(
        &["preview-item-mutations", "--json"],
        Some(
            r#"{"author":"fixture-agent","mutations":[{"op":"update","id":"sample-surface","options":{"title":"Preview title"}}]}"#,
        ),
    )?;
    let request = json!({"abi":"pm-native-adapter/1","request_id":"discovery-receipt","operation":{"action":"get","id":"sample-surface"}});
    let reply = json!({"abi":"pm-native-adapter/1","request_id":"discovery-receipt","outcome":{"status":"success","output":workspace.get("sample-surface")?}});
    let transcript = json!({"manifest":{"abi":"pm-native-adapter/1","capabilities":["get"],"max_frame_bytes":1_048_576},"request":request,"reply_bytes":serde_json::to_vec(&reply)?});
    invoke(&["adapter-check", "--json"], Some(&transcript.to_string()))?;
    invoke(
        &[
            "close",
            "sample-surface",
            "Surface validation complete",
            "--author",
            "fixture-agent",
            "--json",
        ],
        None,
    )?;
    invoke(&["help", "list"], None)?;
    let discovered = discover_capabilities(&DiscoveryRequest::default())?;
    assert_eq!(
        invoked,
        discovered
            .operations
            .iter()
            .map(|op| op.name.clone())
            .collect()
    );
    for op in &discovered.operations {
        if let Some(receipt) = &op.parity_receipt {
            let (file, function) = receipt.split_once("::").ok_or("exact parity test")?;
            assert!(
                fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(file))?
                    .contains(&format!("fn {function}(")),
                "{receipt}"
            );
        }
    }
    // Canonical recovery namespace aliases are also executable.
    for args in [
        vec!["item", "deps", "sample-surface", "--json"],
        vec![
            "item",
            "comments",
            "sample-surface",
            "Alias comment",
            "--author",
            "fixture-agent",
        ],
    ] {
        assert!(native(directory.path(), &args, None)?.status.success());
    }
    Ok(())
}

/// Five hundred seeded random request/replay sequences preserve all SDK-read tracker state.
#[test]
fn seeded_discovery_model_replays_inputs_without_touching_native_state() -> TestResult {
    let (directory, workspace) = fixture()?;
    let value = json!({"id":"sample-model","title":"  Surface model  ","type":"Task","status":"custom-active","priority":2,
        "description":" \r\n retained \r\n ","created_at":"2026-10-21T00:00:00.000Z","updated_at":"2026-10-25T00:00:00.000Z",
        "tags":[" alpha ","β"],"body":" \r\n body \n ","parent":"sample-parent","dependencies":[{"id":"sample-peer","kind":"relates_to"}],
        "blocked_by":"sample-peer","order":2,"assignee":"fixture-agent","risk":"low","sprint":"iteration","release":"test",
        "comments":[{"text":" comment \r\n "}],"notes":[{"text":"note"}],"learnings":[{"text":"learning"}],
        "test_runs":[{"passed":true}],"reminders":[{"text":"reminder"}],"events":[{"title":"event"}],"files":[{"path":"src/lib.rs"}],
        "tests":[{"command":"cargo test"}],"docs":[{"path":"README.md"}],"blocked_reason":"reason","unblock_note":"unblock","foreign_extension":{"preserve":"yes"}});
    fs::write(
        workspace.pm_root().join("tasks/sample-model.toon"),
        toon_format::encode_default(&value)?.replace('\n', "\r\n"),
    )?;
    let model = workspace.get("sample-model")?;
    let bytes = snapshot(directory.path())?;
    let all = discover_capabilities(&DiscoveryRequest::default())?;
    let names: Vec<_> = all.operations.iter().map(|op| op.name.clone()).collect();
    let strategy = prop::collection::vec(
        (
            0_usize..names.len(),
            0_usize..32,
            any::<bool>(),
            any::<bool>(),
        ),
        2..7,
    );
    let mut runner = TestRunner::new(Config {
        cases: 500,
        rng_seed: RngSeed::Fixed(20_261_104),
        failure_persistence: None,
        ..Config::default()
    });
    runner.run(&strategy, |steps| {
        let mut prior = Vec::new();
        for (index, replay, crlf, json_output) in steps {
            let request = DiscoveryRequest {
                operation: Some(names[index].clone()),
                ..DiscoveryRequest::default()
            };
            let text = serde_json::to_string_pretty(&request)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            prior.push(if crlf {
                format!(" \r\n{}\r\n ", text.replace('\n', "\r\n"))
            } else {
                text
            });
            let input = &prior[replay % prior.len()];
            let admitted = DiscoveryRequest::from_json(input.as_bytes())
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            let response = discover_capabilities(&admitted)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            prop_assert_eq!(response.operations.len(), 1);
            prop_assert_eq!(
                Some(&response.operations[0].name),
                admitted.operation.as_ref()
            );
            let encoded = response
                .render(json_output)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            let decoded: Value = if json_output {
                serde_json::from_str(&encoded)
                    .map_err(|error| TestCaseError::fail(error.to_string()))?
            } else {
                toon_format::decode_default(&encoded)
                    .map_err(|error| TestCaseError::fail(error.to_string()))?
            };
            let roundtrip: DiscoveryResponse = serde_json::from_value(decoded)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            prop_assert_eq!(
                serde_json::to_value(roundtrip)
                    .map_err(|error| TestCaseError::fail(error.to_string()))?,
                serde_json::to_value(&response)
                    .map_err(|error| TestCaseError::fail(error.to_string()))?
            );
            prop_assert_eq!(
                workspace
                    .get("sample-model")
                    .map_err(|error| TestCaseError::fail(error.to_string()))?,
                model.clone()
            );
        }
        Ok(())
    })?;
    assert_eq!(snapshot(directory.path())?, bytes);
    Ok(())
}
