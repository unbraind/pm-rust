//! Behavioural admission and early lock recovery against the October 21 contract.
use pm_rust::{CloseItem, CommentItem, CreateItem, OwnershipItem, UpdateItem};
use serde_json::{Value, json};
use std::{fs, process::Command};
#[path = "support/process_environment.rs"]
mod process_environment;
#[path = "support/published_cli.rs"]
#[allow(dead_code)]
mod published_cli;
type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Seeds a plain directory tracker through the published initialization surface.
fn tracker() -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let published = published_cli::locate_published_cli().ok_or("published oracle required")?;
    let result = oracle_command()
        .arg(published.entry)
        .current_dir(dir.path())
        .env(
            "GIT_CEILING_DIRECTORIES",
            dir.path().parent().ok_or("parent")?,
        )
        .args(["init", "sample-", "--defaults", "--agent-guidance", "skip"])
        .output()?;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = native_command(dir.path())
        .args([
            "create",
            "--id",
            "sample-probe",
            "--title",
            "Admission probe",
            "--type",
            "Task",
            "--author",
            "test-agent",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(dir)
}

#[test]
/// Decoding admits known mutation schemas and refuses misspelled options before storage access.
fn unknown_mutation_keys_are_refused_before_storage() -> TestResult {
    let base = json!({"id":"sample-probe","author":"test-agent"});
    let mut create = base.clone();
    create["title"] = json!("A");
    create["type"] = json!("Task");
    let mut comment = base.clone();
    comment["text"] = json!("A");
    let mut ownership = base.clone();
    ownership["force"] = json!(false);
    ownership["if_available"] = json!(false);
    let mut close = base.clone();
    close["resolution"] = json!("Fixed");
    close["expected_result"] = json!("Expected");
    close["actual_result"] = json!("Observed");
    close["force"] = json!(false);
    for (kind, known) in [
        ("create", create),
        ("update", base.clone()),
        ("comment", comment),
        ("close", close),
        ("ownership", ownership),
    ] {
        let decode = |value: Value| match kind {
            "create" => serde_json::from_value::<CreateItem>(value).map(|_| ()),
            "update" => serde_json::from_value::<UpdateItem>(value).map(|_| ()),
            "comment" => serde_json::from_value::<CommentItem>(value).map(|_| ()),
            "close" => serde_json::from_value::<CloseItem>(value).map(|_| ()),
            _ => serde_json::from_value::<OwnershipItem>(value).map(|_| ()),
        };
        decode(known.clone())?;
        for key in ["froce", "authro", "unknown"] {
            let mut invalid = known.clone();
            invalid[key] = json!(true);
            assert!(decode(invalid).is_err(), "{kind} accepted unknown {key}");
        }
    }
    Ok(())
}

#[test]
/// Unsupported execution controls cannot persist linked tests or change history.
fn unsupported_test_execution_is_refused_before_link_edits() -> TestResult {
    let directory = tracker()?;
    let root = directory.path().join(".agents/pm");
    let item = root.join("tasks/sample-probe.toon");
    let history = root.join("history/sample-probe.jsonl");
    let before = (fs::read(&item)?, fs::read(&history)?);
    for options in [
        vec!["--workspace-context", "source"],
        vec!["--pm-context", "none"],
        vec!["--run"],
        vec!["--timeout", "invalid"],
    ] {
        let output = native_command(directory.path())
            .current_dir(directory.path())
            .args([
                "test",
                "sample-probe",
                "--add",
                "command=exit 0",
                "--author",
                "test-agent",
            ])
            .args(options)
            .output()?;
        assert_eq!(output.status.code(), Some(2));
        assert_eq!((fs::read(&item)?, fs::read(&history)?), before);
    }
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
/// A terminated published writer permits audited recovery without waiting for lease expiry.
fn dead_same_host_owner_is_recovered_before_ttl_with_audit() -> TestResult {
    let directory = tracker()?;
    let published = published_cli::locate_published_cli().ok_or("oracle")?;
    let lock_module = published.package_root.join("dist/core/lock/lock.js");
    let script = "const {pathToFileURL}=await import('node:url');const {acquireLock}=await import(pathToFileURL(process.argv[1]));await acquireLock(process.argv[2],'sample-probe',1800,'dead-agent');process.exit(0);";
    let output = oracle_tool()
        .args(["--input-type=module", "-e", script])
        .arg(lock_module)
        .arg(directory.path().join(".agents/pm"))
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let lock = directory.path().join(".agents/pm/locks/sample-probe.lock");
    let old: Value = serde_json::from_slice(&fs::read(&lock)?)?;
    assert!(old["host_identity"].is_string());
    let output = native_command(directory.path())
        .args([
            "comment",
            "sample-probe",
            "Recovered before TTL",
            "--author",
            "test-agent",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!lock.exists());
    let receipts = fs::read_dir(directory.path().join(".agents/pm/locks/recoveries"))?
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(receipts.len(), 1);
    let receipt: Value = serde_json::from_slice(&fs::read(receipts[0].path())?)?;
    assert_eq!(receipt["event"], "owner_dead_recovery_attempt");
    assert_eq!(receipt["lock"], old);
    Ok(())
}

#[test]
/// Retry advice retains the operation and workspace without introducing a force override.
fn lock_contention_preserves_the_actual_invocation_without_force_advice() -> TestResult {
    let directory = tracker()?;
    let root = directory.path().join(".agents/pm");
    let lock = root.join("locks/sample-probe.lock");
    fs::create_dir_all(root.join("locks"))?;
    fs::write(&lock, "unverified owner")?;
    let before = fs::read(root.join("history/sample-probe.jsonl"))?;
    let output = native_command(directory.path())
        .env("PM_LOCK_WAIT_MS", "0")
        .args([
            "comment",
            "sample-probe",
            "contended note",
            "--author",
            "test-agent",
        ])
        .output()?;
    assert_eq!(output.status.code(), Some(4));
    let refusal: Value = serde_json::from_slice(&output.stderr)?;
    assert_eq!(refusal["code"], "lock_conflict");
    assert_eq!(
        refusal["recovery"]["suggested_retry_args"],
        json!([
            "comment",
            "sample-probe",
            "contended note",
            "--author",
            "test-agent"
        ])
    );
    assert!(
        refusal["required"]
            .as_str()
            .ok_or("required")?
            .contains("PM_LOCK_WAIT_MS")
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("--force"));
    assert_eq!(fs::read(root.join("history/sample-probe.jsonl"))?, before);
    assert_eq!(fs::read_to_string(&lock)?, "unverified owner");
    let elsewhere = tempfile::tempdir()?;
    let output = native_command(elsewhere.path())
        .env("PM_LOCK_WAIT_MS", "0")
        .arg("--workspace")
        .arg(directory.path())
        .args([
            "comment",
            "sample-probe",
            "scoped note",
            "--author",
            "test-agent",
        ])
        .output()?;
    assert_eq!(output.status.code(), Some(4));
    let refusal: Value = serde_json::from_slice(&output.stderr)?;
    assert_eq!(
        refusal["recovery"]["suggested_retry_args"],
        json!([
            "--pm-path",
            directory.path().to_string_lossy(),
            "comment",
            "sample-probe",
            "scoped note",
            "--author",
            "test-agent"
        ])
    );

    Ok(())
}

#[test]
/// Expected declaration misses match the oracle and leave registry state absent.
fn unknown_declaration_reads_match_typed_oracle_refusals() -> TestResult {
    let directory = tracker()?;
    let published = published_cli::locate_published_cli().ok_or("oracle")?;
    for kind in ["measurement", "assertion", "gate"] {
        let args = ["assurance", "show", kind, "missing", "--json"];
        let expected = oracle_command()
            .arg(&published.entry)
            .current_dir(directory.path())
            .args(args)
            .output()?;
        let actual = native_command(directory.path())
            .current_dir(directory.path())
            .args(args)
            .output()?;
        assert_eq!(expected.status.code(), Some(2));
        assert_eq!(actual.status.code(), Some(2));
        let expected: Value = serde_json::from_slice(&expected.stderr)?;
        let actual: Value = serde_json::from_slice(&actual.stderr)?;
        assert_eq!(actual, expected);
        assert_eq!(actual["changed"], false);
    }
    assert!(!directory.path().join(".agents/pm/assurance.json").exists());
    Ok(())
}

#[cfg(unix)]
#[test]
/// Plain-directory mutations avoid a poison Git shim and also work without its executable.
fn plain_directory_linked_tests_never_invoke_git() -> TestResult {
    use std::os::unix::fs::PermissionsExt;
    let directory = tracker()?;
    let poison = directory.path().join("poison-path");
    fs::create_dir(&poison)?;
    let git = poison.join("git");
    fs::write(&git, "#!/bin/sh\nprintf called > git-was-invoked\nexit 1\n")?;
    fs::set_permissions(&git, fs::Permissions::from_mode(0o755))?;
    for (path, command) in [
        (&poison, "command=cargo test"),
        (&directory.path().join("empty-path"), "command=cargo check"),
    ] {
        fs::create_dir_all(path)?;
        let output = native_command(directory.path())
            .current_dir(directory.path())
            .env("PATH", path)
            .env(
                "GIT_CEILING_DIRECTORIES",
                directory.path().parent().ok_or("parent")?,
            )
            .env("GITHUB_HEAD_REF", "")
            .env("GITHUB_REF_NAME", "")
            .env_remove("PM_SOURCE_WORKSPACE_ROOT")
            .args([
                "test",
                "sample-probe",
                "--add",
                command,
                "--author",
                "test-agent",
            ])
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!directory.path().join("git-was-invoked").exists());
    }
    Ok(())
}

/// Bounded declaration discovery delivers every row once and refuses changed snapshots.
#[test]
fn bounded_assurance_discovery_resumes_and_refuses_stale_snapshots() -> TestResult {
    let directory = tracker()?;
    let path = directory.path().join(".agents/pm/assurance.json");
    let mut document = json!({"version":1,"measurements":[],"assertions":[],"gates":(0..50).map(|index|json!({"id":format!("gate-{index:03}"),"assertion_ids":[],"triggers":["ci"],"description":"x".repeat(500)})).collect::<Vec<_>>()});
    let before = serde_json::to_vec(&document)?;
    fs::write(&path, &before)?;
    let mut cursor = None::<String>;
    let mut first_cursor = None;
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..50 {
        let mut command = native_command(directory.path());
        command.args([
            "assurance",
            "list",
            "gate",
            "--json",
            "--output-budget",
            "1000",
        ]);
        if let Some(cursor) = &cursor {
            command.args(["--output-cursor", cursor]);
        }
        let output = command.output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let page: Value = serde_json::from_slice(&output.stdout)?;
        for row in page["items"].as_array().ok_or("items")? {
            assert!(seen.insert(row["id"].as_str().ok_or("id")?.to_owned()));
        }
        cursor = page["next_cursor"].as_str().map(str::to_owned);
        if first_cursor.is_none() {
            first_cursor = cursor.clone();
        }
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(seen.len(), 50);
    assert!(cursor.is_none());
    assert_eq!(fs::read(&path)?, before);
    document["gates"]
        .as_array_mut()
        .ok_or("gates")?
        .push(json!({"id":"gate-new","assertion_ids":[],"triggers":["ci"]}));
    let changed = serde_json::to_vec(&document)?;
    fs::write(&path, &changed)?;
    let output = native_command(directory.path())
        .args([
            "assurance",
            "list",
            "gate",
            "--json",
            "--output-budget",
            "1000",
            "--output-cursor",
            &first_cursor.ok_or("expected bounded continuation")?,
        ])
        .output()?;
    assert_eq!(output.status.code(), Some(2));
    let refusal: Value = serde_json::from_slice(&output.stderr)?;
    assert_eq!(refusal["code"], "read_output_cursor_stale");
    assert_eq!(refusal["refusal"]["surface"], "assurance");
    assert_eq!(fs::read(path)?, changed);
    Ok(())
}

/// Starts the published CLI without ambient tracker or agent context.
fn oracle_command() -> Command {
    let mut command = published_cli::oracle_command("node");
    process_environment::isolate_environment(&mut command);
    command
}

/// Starts a native fixture process without ambient tracker or agent context.
fn native_command(root: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pm-rust"));
    process_environment::isolate_environment(&mut command);
    command.current_dir(root);
    command
}

/// Starts a process-level oracle tool with the same deterministic environment boundary.
#[cfg(target_os = "linux")]
fn oracle_tool() -> Command {
    let mut command = published_cli::oracle_tool("node");
    process_environment::isolate_environment(&mut command);
    command
}

#[test]
/// Invalid completion arithmetic keeps its published human/JSON failure and preserves stored bytes.
fn completion_overflow_preserves_refusal_encoding_before_mutation() -> TestResult {
    let directory = tracker()?;
    let root = directory.path().join(".agents/pm");
    let item = root.join("tasks/sample-probe.toon");
    let history = root.join("history/sample-probe.jsonl");
    let before = (fs::read(&item)?, fs::read(&history)?);
    for json in [false, true] {
        let mut command = native_command(directory.path());
        command.args([
            "close",
            "sample-probe",
            "--author",
            "test-agent",
            "--resolution",
            "Fixed",
            "--expected-result",
            "Expected",
            "--actual-result",
            "Observed",
            "--completed-at",
            "+9007199254740991w",
        ]);
        if json {
            command.arg("--json");
        }
        let output = command.output()?;
        assert_eq!(
            output.status.code(),
            Some(1),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        if json {
            let payload: Value = serde_json::from_slice(&output.stderr)?;
            assert_eq!(payload["code"], "unknown_error");
            assert_eq!(payload["detail"], "Invalid time value");
        } else {
            assert_eq!(output.stderr, b"Invalid time value\n");
        }
        assert_eq!((fs::read(&item)?, fs::read(&history)?), before);
    }
    Ok(())
}
