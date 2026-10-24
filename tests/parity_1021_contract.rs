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
fn tracker() -> Result<Option<tempfile::TempDir>, Box<dyn std::error::Error>> {
    let Some(published) = published_cli::published_cli_or_skip("October 21 parity") else {
        return Ok(None);
    };
    let dir = tempfile::tempdir()?;
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
    Ok(Some(dir))
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
    let Some(directory) = tracker()? else {
        return Ok(());
    };
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
    let Some(directory) = tracker()? else {
        return Ok(());
    };
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
/// Fresh empty and occupied gates preserve the oracle's complete refusal receipt and bytes.
fn fresh_gate_refusals_match_oracle_and_preserve_authoritative_bytes() -> TestResult {
    let Some(directory) = tracker()? else {
        return Ok(());
    };
    let published = published_cli::locate_published_cli().ok_or("oracle")?;
    let root = directory.path().join(".agents/pm");
    let locks = root.join("locks");
    fs::create_dir_all(&locks)?;
    let lock = locks.join("sample-probe.lock");
    let gate = locks.join("sample-probe.lock.stale-cleanup");
    fs::create_dir(&gate)?;
    let receipt = serde_json::to_vec(&json!({
        "id":"sample-probe", "pid":std::process::id(), "owner":"test-holder",
        "token":"test-held", "ttl_seconds":1800,
        "created_at":time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?
    }))?;
    fs::write(&lock, &receipt)?;
    let item = root.join("tasks/sample-probe.toon");
    let history = root.join("history/sample-probe.jsonl");
    let before = (fs::read(&item)?, fs::read(&history)?);
    let args = ["claim", "sample-probe", "--author", "test-agent", "--json"];
    for contents in [None, Some("foreign owner evidence")] {
        if let Some(contents) = contents {
            fs::write(gate.join("owner.json"), contents)?;
        }
        #[cfg(unix)]
        fs::File::open(&gate)?.set_times(
            fs::FileTimes::new()
                .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(60)),
        )?;
        let expected = oracle_command()
            .arg(&published.entry)
            .current_dir(directory.path())
            .env("PM_LOCK_WAIT_MS", "0")
            .args(args)
            .output()?;
        let actual = native_command(directory.path())
            .env("PM_LOCK_WAIT_MS", "0")
            .args(args)
            .output()?;
        assert_eq!(expected.status.code(), Some(4));
        assert_eq!(actual.status.code(), expected.status.code());
        let expected: Value = serde_json::from_slice(&expected.stderr)?;
        let actual: Value = serde_json::from_slice(&actual.stderr)?;
        assert_eq!(actual, expected);
        assert_eq!(actual["code"], "lock_conflict");
        assert_eq!(fs::read(&lock)?, receipt);
        assert_eq!((fs::read(&item)?, fs::read(&history)?), before);
        assert!(gate.is_dir());
    }
    assert_eq!(
        fs::read_to_string(gate.join("owner.json"))?,
        "foreign owner evidence"
    );
    Ok(())
}

#[test]
/// Retry advice retains the operation and workspace without introducing a force override.
fn lock_contention_preserves_the_actual_invocation_without_force_advice() -> TestResult {
    let Some(directory) = tracker()? else {
        return Ok(());
    };
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
            "item",
            "comments",
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
            "item",
            "comments",
            "sample-probe",
            "scoped note",
            "--author",
            "test-agent"
        ])
    );

    Ok(())
}

/// Explicit workspace and tracker retry paths retain the oracle's literal spelling.
#[test]
#[allow(clippy::too_many_lines)]
fn explicit_lock_retry_scopes_match_oracle() -> TestResult {
    let Some(directory) = tracker()? else {
        return Ok(());
    };
    let published = published_cli::locate_published_cli().ok_or("oracle")?;
    let workspace = directory.path().join("selected workspace");
    fs::create_dir(&workspace)?;
    fs::rename(directory.path().join(".agents"), workspace.join(".agents"))?;
    let root = workspace.join(".agents/pm");
    let lock = root.join("locks/sample-probe.lock");
    fs::create_dir_all(root.join("locks"))?;
    let receipt = serde_json::to_vec(&json!({
        "id":"sample-probe", "pid":std::process::id(), "owner":"test-holder",
        "token":"test-held", "ttl_seconds":1800,
        "created_at":time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?
    }))?;
    fs::write(&lock, &receipt)?;
    let before = (
        fs::read(root.join("tasks/sample-probe.toon"))?,
        fs::read(root.join("history/sample-probe.jsonl"))?,
    );
    let operation = [
        "comment",
        "sample-probe",
        "scoped note",
        "--author",
        "test-agent",
    ];
    for scope in [
        workspace.to_string_lossy().into_owned(),
        root.to_string_lossy().into_owned(),
        "selected workspace".to_owned(),
        "selected workspace/.agents/pm".to_owned(),
    ] {
        let expected = oracle_command()
            .arg(&published.entry)
            .current_dir(directory.path())
            .env("PM_LOCK_WAIT_MS", "0")
            .args(["--pm-path", &scope])
            .args(operation)
            .arg("--json")
            .output()?;
        assert_eq!(
            expected.status.code(),
            Some(4),
            "{}",
            String::from_utf8_lossy(&expected.stderr)
        );
        let mut expected: Value = serde_json::from_slice(&expected.stderr)?;
        // Native comment refusals already use JSON; remove the oracle-only encoding switch.
        for field in ["normalized_args", "suggested_retry_args"] {
            assert_eq!(
                expected["recovery"][field]
                    .as_array_mut()
                    .ok_or("retry args")?
                    .pop(),
                Some(json!("--json"))
            );
        }
        for field in ["attempted_command", "suggested_retry"] {
            let command = expected["recovery"][field]
                .as_str()
                .ok_or("retry command")?
                .strip_suffix(" --json")
                .ok_or("encoding switch")?
                .to_owned();
            expected["recovery"][field] = json!(command);
        }
        expected["examples"] = json!([expected["recovery"]["suggested_retry"]]);
        // The published CLI keeps the supplied directory even after discovering its tracker.
        assert_eq!(expected["recovery"]["suggested_retry_args"][1], scope);
        for flag in ["--workspace", "--pm-path", "--path"] {
            for prefix in [
                vec![flag.to_owned(), scope.clone()],
                vec![format!("{flag}={scope}")],
            ] {
                let actual = native_command(directory.path())
                    .env("PM_LOCK_WAIT_MS", "0")
                    .args(&prefix)
                    .args(operation)
                    .output()?;
                assert_eq!(
                    actual.status.code(),
                    Some(4),
                    "{prefix:?}: {}",
                    String::from_utf8_lossy(&actual.stderr)
                );
                let actual: Value = serde_json::from_slice(&actual.stderr)?;
                for field in [
                    "attempted_command",
                    "normalized_args",
                    "suggested_retry",
                    "suggested_retry_args",
                ] {
                    assert_eq!(
                        actual["recovery"][field], expected["recovery"][field],
                        "{prefix:?}: {field}"
                    );
                }
                assert_eq!(actual["examples"], expected["examples"]);
                let retry = actual["recovery"]["suggested_retry_args"]
                    .as_array()
                    .ok_or("retry args")?
                    .iter()
                    .map(|arg| arg.as_str().ok_or("retry string"))
                    .collect::<Result<Vec<_>, _>>()?;
                let replay = native_command(directory.path())
                    .env("PM_LOCK_WAIT_MS", "0")
                    .args(retry)
                    .output()?;
                assert_eq!(
                    replay.status.code(),
                    Some(4),
                    "{}",
                    String::from_utf8_lossy(&replay.stderr)
                );
                let replay: Value = serde_json::from_slice(&replay.stderr)?;
                assert_eq!(
                    replay["recovery"]["suggested_retry_args"],
                    actual["recovery"]["suggested_retry_args"]
                );
                assert_eq!(fs::read(&lock)?, receipt);
                assert_eq!(
                    (
                        fs::read(root.join("tasks/sample-probe.toon"))?,
                        fs::read(root.join("history/sample-probe.jsonl"))?
                    ),
                    before
                );
            }
        }
    }
    Ok(())
}

#[test]
/// Expected declaration misses match the oracle and leave registry state absent.
fn unknown_declaration_reads_match_typed_oracle_refusals() -> TestResult {
    let Some(directory) = tracker()? else {
        return Ok(());
    };
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
    let Some(directory) = tracker()? else {
        return Ok(());
    };
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
    let Some(directory) = tracker()? else {
        return Ok(());
    };
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
    let Some(directory) = tracker()? else {
        return Ok(());
    };
    let root = directory.path().join(".agents/pm");
    let item = root.join("tasks/sample-probe.toon");
    let history = root.join("history/sample-probe.jsonl");
    let before = (fs::read(&item)?, fs::read(&history)?);
    let args = [
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
    ];
    for json in [false, true] {
        let mut command = native_command(directory.path());
        command.args(args);
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
    // Wait for the only reader to exit before giving its write end to stderr.
    let mut reader = native_command(directory.path())
        .arg("--version")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let closed_pipe = reader.stdin.take().ok_or("stdin pipe")?;
    assert!(reader.wait()?.success());
    let output = native_command(directory.path())
        .args(args)
        .stderr(std::process::Stdio::from(closed_pipe))
        .output()?;
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!((fs::read(&item)?, fs::read(&history)?), before);
    Ok(())
}

/// Fixture children ignore hostile developer Git configuration and keep neutral CI values.
#[test]
fn fixture_git_configuration_is_neutral() -> TestResult {
    const PROBE: &str = "PM_RUST_GIT_ISOLATION_PROBE";
    if std::env::var_os(PROBE).is_some() {
        let mut command = Command::new("git");
        process_environment::isolate_environment(&mut command);
        let output = command
            .args(["config", "--global", "--get", "review.inherited"])
            .output()?;
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(
            command
                .get_envs()
                .any(|(key, value)| key == "GIT_CONFIG_NOSYSTEM"
                    && value == Some(std::ffi::OsStr::new("1")))
        );
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    let config = directory.path().join("hostile.gitconfig");
    fs::write(&config, "[review]\n inherited = yes\n")?;
    let output = Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "fixture_git_configuration_is_neutral",
            "--nocapture",
        ])
        .env(PROBE, "1")
        .env("GIT_CONFIG_GLOBAL", config)
        .env("GIT_CONFIG_NOSYSTEM", "0")
        .output()?;
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

/// The pinned oracle preserves aged recovery receipts through actual lock garbage collection.
#[test]
fn published_lock_gc_retains_recovery_receipts() -> TestResult {
    let Some(directory) = tracker()? else {
        return Ok(());
    };
    let root = directory.path().join(".agents/pm");
    let receipts = root.join("locks/recoveries");
    fs::create_dir_all(&receipts)?;
    let receipt = receipts.join("retention-probe.json");
    let raw = br#"{"event":"owner_dead_recovery_attempt","recorded_at":"2000-01-01T00:00:00.000Z","lock":{}}"#;
    fs::write(&receipt, raw)?;
    fs::File::options()
        .write(true)
        .open(&receipt)?
        .set_times(fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH))?;
    let published = published_cli::locate_published_cli().ok_or("oracle")?;
    let output = oracle_command()
        .arg(published.entry)
        .current_dir(directory.path())
        .args(["gc", "--scope", "locks", "--json"])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(result["locks"]["scanned"], 0);
    assert_eq!(result["locks"]["removed"], 0);
    assert_eq!(fs::read(&receipt)?, raw);
    Ok(())
}

/// The active certification task body agrees with its title and oracle acceptance criteria.
#[test]
fn certification_tracker_body_matches_the_active_oracle() -> TestResult {
    let task: Value =
        toon_format::decode_default(include_str!("../.agents/pm/tasks/pm-rust-qhem.toon"))?;
    let body = task["body"].as_str().ok_or("missing body")?;
    assert!(body.contains("2026.10.21") && body.contains("PR 101"));
    assert!(!body.contains("Keep PR 100 scoped"));
    Ok(())
}

/// The optional oracle skips fixture tests, while required mode still fails closed.
#[test]
fn absent_oracle_obeys_optional_and_required_modes() -> TestResult {
    let directory = tempfile::tempdir()?;
    for required in [false, true] {
        let mut command = Command::new(std::env::current_exe()?);
        command
            .args([
                "--exact",
                "unsupported_test_execution_is_refused_before_link_edits",
                "--nocapture",
            ])
            .env("PM_NODE_CLI", directory.path().join("absent-oracle"));
        if required {
            command.env("PM_RUST_REQUIRE_PUBLISHED_CLI", "1");
        } else {
            command.env_remove("PM_RUST_REQUIRE_PUBLISHED_CLI");
        }
        let output = command.output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.success(), !required, "{stdout}{stderr}");
        if required {
            assert!(
                stderr.contains("requires the published Node pm CLI"),
                "{stdout}{stderr}"
            );
        } else {
            assert!(
                stdout.contains("skip: no published Node pm CLI found"),
                "{stdout}{stderr}"
            );
        }
    }
    Ok(())
}
