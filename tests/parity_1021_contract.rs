//! Behavioural admission and early lock recovery against the October 21 contract.
use pm_rust::{CloseItem, CommentItem, CreateItem, OwnershipItem, UpdateItem};
use serde_json::{Value, json};
use std::{fs, process::Command};
#[path = "support/published_cli.rs"]
#[allow(dead_code)]
mod published_cli;
type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Seeds a plain directory tracker through the published initialization surface.
fn tracker() -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let published = published_cli::locate_published_cli().ok_or("published oracle required")?;
    let result = published_cli::oracle_command("node")
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
    let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .current_dir(dir.path())
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
        let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
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
fn dead_same_host_owner_is_recovered_before_ttl_with_audit() -> TestResult {
    let directory = tracker()?;
    let published = published_cli::locate_published_cli().ok_or("oracle")?;
    let lock_module = published.package_root.join("dist/core/lock/lock.js");
    let script = "const {pathToFileURL}=await import('node:url');const {acquireLock}=await import(pathToFileURL(process.argv[1]));await acquireLock(process.argv[2],'sample-probe',1800,'dead-agent');process.exit(0);";
    let output = published_cli::oracle_tool("node")
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
    let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .current_dir(directory.path())
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
fn lock_contention_preserves_the_actual_invocation_without_force_advice() -> TestResult {
    let directory = tracker()?;
    let root = directory.path().join(".agents/pm");
    let lock = root.join("locks/sample-probe.lock");
    fs::create_dir_all(root.join("locks"))?;
    fs::write(&lock, "unverified owner")?;
    let before = fs::read(root.join("history/sample-probe.jsonl"))?;
    let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .current_dir(directory.path())
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
    let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .current_dir(elsewhere.path())
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
fn unknown_declaration_reads_match_typed_oracle_refusals() -> TestResult {
    let directory = tracker()?;
    let published = published_cli::locate_published_cli().ok_or("oracle")?;
    for kind in ["measurement", "assertion", "gate"] {
        let args = ["assurance", "show", kind, "missing", "--json"];
        let expected = published_cli::oracle_command("node")
            .arg(&published.entry)
            .current_dir(directory.path())
            .args(args)
            .output()?;
        let actual = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
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
        let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
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
