//! Black-box dispatch acceptance for every CLI subcommand and refusal path.
//!
//! Each case executes the compiled `pm-rust` binary as a real process and
//! asserts exit codes and stderr text, so the full argument-parsing and
//! dispatch surface is exercised exactly as callers experience it.

use std::fs;
use std::process::Command;

use tempfile::TempDir;

/// Builds a tracker fixture and returns its workspace directory.
fn workspace() -> Result<TempDir, Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(&root)?;
    fs::write(
        root.join("settings.json"),
        r#"{"id_prefix":"sample-","item_format":"toon","locks":{"ttl_seconds":1800,"wait_ms":1000}}"#,
    )?;
    Ok(directory)
}

/// Explicit lifecycle commands reject missing IDs before creating any locks.
#[test]
fn explicit_lifecycle_commands_require_item_identifiers() -> Result<(), Box<dyn std::error::Error>>
{
    let workspace = workspace()?;
    for command in ["release", "start-task", "pause-task"] {
        let output = pm_rust(&workspace, &[command, "--author", "cli-agent", "--json"])?;
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).contains("<ID>"));
        assert!(!workspace.path().join(".agents/pm/locks").exists());
    }
    Ok(())
}

/// Ownership dispatch refuses corrupt configuration and missing alias targets.
#[test]
fn ownership_dispatch_preserves_configuration_and_alias_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let workspace = workspace()?;
    let root = workspace.path().join(".agents/pm");
    fs::write(root.join("settings.json"), "invalid")?;
    for command in ["claim", "release", "start-task", "pause-task"] {
        let output = pm_rust(
            &workspace,
            &[command, "sample-missing", "--author", "cli-agent", "--json"],
        )?;
        assert!(!output.status.success());
        assert_eq!(output.stdout, Vec::<u8>::new());
        assert!(String::from_utf8_lossy(&output.stderr).contains("expected value"));
        assert!(!root.join("locks").exists());
        assert_eq!(fs::read_to_string(root.join("settings.json"))?, "invalid");
    }
    fs::write(root.join("settings.json"), "{}")?;
    for command in ["start-task", "pause-task"] {
        expect_err(
            &workspace,
            &[command, "sample-missing", "--author", "cli-agent", "--json"],
            "pm item not found",
        )?;
    }
    Ok(())
}

/// Runs the binary with arguments inside one fixture workspace.
fn pm_rust(
    workspace: &TempDir,
    arguments: &[&str],
) -> Result<std::process::Output, Box<dyn std::error::Error>> {
    Ok(Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .current_dir(workspace.path())
        .args(["--workspace=."])
        .args(arguments)
        .output()?)
}

/// Parser refusals identify the rejected short option or extra positional token.
#[test]
fn orientation_refusals_name_the_rejected_argument() -> Result<(), Box<dyn std::error::Error>> {
    let workspace = workspace()?;
    for (command, positional) in [
        ("context", vec![]),
        ("history", vec!["sample-a"]),
        ("deps", vec!["sample-a"]),
    ] {
        for rejected in ["-v", "extra", "--bogus=value"] {
            for json in [false, true] {
                let mut args = vec![command];
                args.extend(&positional);
                // This allowed option must never be reported as the rejected token.
                args.extend(["--limit", "1"].into_iter().filter(|_| command != "deps"));
                if json {
                    args.push("--json");
                }
                args.push(rejected);
                let output = pm_rust(&workspace, &args)?;
                let stderr = String::from_utf8(output.stderr)?;
                let token = rejected.split('=').next().unwrap_or_default();
                assert_eq!(output.status.code(), Some(2), "{args:?}: {stderr}");
                assert!(output.stdout.is_empty());
                assert!(
                    stderr.contains(&format!("Native {command} does not support {token};")),
                    "{args:?}: {stderr}"
                );
                if json {
                    let payload: serde_json::Value = serde_json::from_str(&stderr)?;
                    assert_eq!(payload["code"], "unsupported_native_option");
                    assert_eq!(payload["flag"], token);
                    assert_eq!(payload["value"], token);
                    assert_eq!(payload["refusal"]["exit_code"], 2);
                }
            }
        }
    }
    assert!(!workspace.path().join(".agents/pm/locks").exists());
    Ok(())
}

/// Invalid history names have identical refusals regardless of an escaped file's existence.
#[test]
fn history_invalid_id_refuses_before_filesystem_lookup() -> Result<(), Box<dyn std::error::Error>> {
    let workspace = workspace()?;
    let root = workspace.path().join(".agents/pm");
    fs::create_dir_all(root.join("history"))?;
    let outside = workspace.path().join(".agents/outside-x.jsonl");
    for json in [false, true] {
        let mut args = vec!["history", "../../outside-x"];
        if json {
            args.push("--json");
        }
        let missing = pm_rust(&workspace, &args)?;
        fs::write(&outside, "outside bytes must never be read\n")?;
        let existing = pm_rust(&workspace, &args)?;
        assert_eq!(missing.status.code(), Some(2));
        assert_eq!(existing.status.code(), Some(2));
        assert!(missing.stdout.is_empty());
        assert!(existing.stdout.is_empty());
        assert_eq!(missing.stderr, existing.stderr);
        assert!(
            String::from_utf8(missing.stderr)?
                .contains("Invalid history item id: expected a single filename.")
        );
        assert_eq!(fs::read(&outside)?, b"outside bytes must never be read\n");
        fs::remove_file(&outside)?;
    }
    assert!(!root.join("locks").exists());
    Ok(())
}

/// Orientation words in other commands retain the selected command's Clap refusal.
#[test]
fn orientation_values_do_not_reclassify_unrelated_commands()
-> Result<(), Box<dyn std::error::Error>> {
    let workspace = workspace()?;
    for (arguments, control) in [
        (
            vec!["search", "history", "--bogus"],
            vec!["search", "fixture", "--bogus"],
        ),
        (
            vec!["create", "--title", "deps", "--bogus"],
            vec!["create", "--title", "fixture", "--bogus"],
        ),
        (
            vec!["get", "context", "--bogus"],
            vec!["get", "fixture", "--bogus"],
        ),
    ] {
        let expected = pm_rust(&workspace, &control)?;
        let actual = pm_rust(&workspace, &arguments)?;
        assert_eq!(actual.status.code(), Some(2));
        assert!(actual.stdout.is_empty());
        assert_eq!(actual.stderr, expected.stderr, "{arguments:?}");
        assert!(!String::from_utf8_lossy(&actual.stderr).contains("unsupported_native_option"));
    }
    assert!(!workspace.path().join(".agents/pm/locks").exists());
    Ok(())
}

/// Runs the binary expecting success.
fn expect_ok(workspace: &TempDir, arguments: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let output = pm_rust(workspace, arguments)?;
    assert!(
        output.status.success(),
        "pm-rust {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

/// Runs the binary expecting a typed failure with stable stderr text.
fn expect_err(
    workspace: &TempDir,
    arguments: &[&str],
    needle: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let output = pm_rust(workspace, arguments)?;
    assert!(
        !output.status.success(),
        "pm-rust {arguments:?} unexpectedly succeeded"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(needle),
        "pm-rust stderr missing {needle:?}: {stderr}"
    );
    Ok(())
}

/// Seeds one canonical item into the fixture workspace.
fn seed_item(workspace: &TempDir) -> Result<(), Box<dyn std::error::Error>> {
    expect_ok(
        workspace,
        &[
            "create",
            "--id=sample-cli",
            "--title=Dispatch target",
            "--type=Task",
            "--author=cli-agent",
        ],
    )
}

/// Verified history succeeds strictly and propagates a reader closing its output pipe.
#[test]
fn history_strict_success_and_closed_pipe() -> Result<(), Box<dyn std::error::Error>> {
    use std::process::Stdio;
    let workspace = workspace()?;
    seed_item(&workspace)?;
    expect_ok(
        &workspace,
        &[
            "history",
            "sample-cli",
            "--verify",
            "--strict-exit",
            "--json",
        ],
    )?;
    let path = workspace.path().join(".agents/pm/history/sample-cli.jsonl");
    let mut entry: serde_json::Value = serde_json::from_str(fs::read_to_string(&path)?.trim())?;
    entry["author"] = serde_json::json!("fixture".repeat(100_000));
    fs::write(&path, format!("{}\n", serde_json::to_string(&entry)?))?;
    let before = fs::read(&path)?;
    // Confirm the edited stream loads before isolating the closed-pipe failure.
    expect_ok(&workspace, &["history", "sample-cli", "--full", "--json"])?;
    let mut child = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .current_dir(workspace.path())
        .args(["--workspace=.", "history", "sample-cli", "--full", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    drop(child.stdout.take());
    let output = child.wait_with_output()?;
    assert_eq!(output.status.code(), Some(2));
    assert!(!output.stderr.is_empty());
    assert_eq!(fs::read(path)?, before);
    Ok(())
}

#[test]
/// Covers the read commands over stored items and their failure paths.
fn list_and_get_dispatch_over_real_processes() -> Result<(), Box<dyn std::error::Error>> {
    let workspace = workspace()?;
    let output = pm_rust(&workspace, &["list"])?;
    assert!(output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("count: 0"),
        "empty list projection missing"
    );
    seed_item(&workspace)?;
    let output = pm_rust(&workspace, &["list", "--status", "open", "--type", "task"])?;
    assert!(String::from_utf8_lossy(&output.stdout).contains("sample-cli"));
    let output = pm_rust(&workspace, &["get", "sample-cli"])?;
    assert!(String::from_utf8_lossy(&output.stdout).contains("Dispatch target"));
    let unknown = pm_rust(&workspace, &["list", "--native-boundary-probe"])?;
    assert_eq!(unknown.status.code(), Some(2));
    assert!(unknown.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&unknown.stderr)
            .contains("unexpected argument '--native-boundary-probe'")
    );
    for args in [
        &["--help"][..],
        &["--version"][..],
        &["context", "--help"][..],
    ] {
        let output = pm_rust(&workspace, args)?;
        assert!(output.status.success());
        assert!(!output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
    expect_err(&workspace, &["get", "sample-missing"], "pm item not found")?;
    Ok(())
}

#[test]
/// Covers create dispatch including its duplicate-id refusal.
fn create_dispatch_succeeds_then_refuses_duplicates() -> Result<(), Box<dyn std::error::Error>> {
    let workspace = workspace()?;
    seed_item(&workspace)?;
    expect_err(
        &workspace,
        &[
            "create",
            "--id=sample-cli",
            "--title=Duplicate",
            "--type=Task",
            "--author=cli-agent",
        ],
        "already exists",
    )?;
    Ok(())
}

#[test]
/// Covers update dispatch across field kinds, tag CSV handling, and refusals.
#[allow(clippy::too_many_lines)]
fn update_dispatch_covers_fields_and_refusals() -> Result<(), Box<dyn std::error::Error>> {
    let workspace = workspace()?;
    seed_item(&workspace)?;
    expect_ok(
        &workspace,
        &[
            "update",
            "sample-cli",
            "--title=Renamed by CLI",
            "--description=New description",
            "--status=in_progress",
            "--priority=1",
            "--tags=z,a,a",
            "--body=New body",
            "--author=cli-agent",
            "--message=bulk field update",
        ],
    )?;
    let item = fs::read_to_string(workspace.path().join(".agents/pm/tasks/sample-cli.toon"))?;
    assert!(item.contains("title: Renamed by CLI"));
    assert!(item.contains("description: New description"));
    assert!(item.contains("status: in_progress"));
    assert!(item.contains("priority: 1"));
    // Tags arrive through the CSV value and are canonicalized before storage.
    assert!(item.contains("tags[2]: a,z"));
    assert!(item.contains("body: New body"));
    let history = fs::read_to_string(workspace.path().join(".agents/pm/history/sample-cli.jsonl"))?;
    assert!(history.contains(r#""message":"bulk field update""#));

    expect_err(
        &workspace,
        &["update", "sample-cli", "--author=cli-agent"],
        "at least one field",
    )?;
    expect_err(
        &workspace,
        &["update", "sample-cli", "--title=  ", "--author=cli-agent"],
        "must not be empty",
    )?;
    expect_err(
        &workspace,
        &[
            "update",
            "sample-missing",
            "--title=X",
            "--author=cli-agent",
        ],
        "not found",
    )?;
    Ok(())
}

#[test]
/// Proves the CLI trims whitespace from each `--tags` CSV segment before storing
/// it, so `--tags "alpha, beta"` stores `alpha,beta` rather than ` beta`.
fn update_dispatch_trims_tag_csv_segments() -> Result<(), Box<dyn std::error::Error>> {
    let workspace = workspace()?;
    seed_item(&workspace)?;
    expect_ok(
        &workspace,
        &[
            "update",
            "sample-cli",
            "--tags=alpha, beta",
            "--author=cli-agent",
        ],
    )?;
    let item = fs::read_to_string(workspace.path().join(".agents/pm/tasks/sample-cli.toon"))?;
    assert!(
        item.contains("tags[2]: alpha,beta"),
        "trailing/leading whitespace was not trimmed from a tag CSV segment: {item}"
    );
    Ok(())
}

#[test]
/// Covers comment dispatch including its empty-text refusal.
fn comment_dispatch_appends_and_refuses_empty_text() -> Result<(), Box<dyn std::error::Error>> {
    let workspace = workspace()?;
    seed_item(&workspace)?;
    expect_ok(
        &workspace,
        &[
            "comment",
            "sample-cli",
            "note from dispatch test",
            "--author=cli-agent",
        ],
    )?;
    let item = fs::read_to_string(workspace.path().join(".agents/pm/tasks/sample-cli.toon"))?;
    assert!(item.contains("note from dispatch test"));
    expect_err(
        &workspace,
        &["comment", "sample-cli", "   ", "--author=cli-agent"],
        "must not be empty",
    )?;
    expect_err(
        &workspace,
        &["comment", "sample-missing", "text", "--author=cli-agent"],
        "not found",
    )?;
    Ok(())
}

#[test]
/// Covers close dispatch including its terminal-item refusal.
fn close_dispatch_closes_once_and_refuses_repeats() -> Result<(), Box<dyn std::error::Error>> {
    let workspace = workspace()?;
    seed_item(&workspace)?;
    expect_ok(
        &workspace,
        &[
            "close",
            "sample-cli",
            "--reason=dispatch complete",
            "--author=cli-agent",
        ],
    )?;
    let item = fs::read_to_string(workspace.path().join(".agents/pm/tasks/sample-cli.toon"))?;
    assert!(item.contains("status: closed"));
    assert!(item.contains("close_reason: dispatch complete"));
    expect_err(
        &workspace,
        &[
            "close",
            "sample-cli",
            "--reason=again",
            "--author=cli-agent",
        ],
        "already terminal",
    )?;
    expect_err(
        &workspace,
        &["close", "sample-cli", "--reason=  ", "--author=cli-agent"],
        "Close reason text",
    )?;
    Ok(())
}

/// A non-UTF-8 workspace argument must not panic a cursor refusal.
/// Linux permits these directory names; macOS rejects them before dispatch.
/// The argument-publisher unit test exercises invalid bytes on every Unix OS.
#[cfg(target_os = "linux")]
#[test]
fn non_utf8_workspace_argument_refuses_a_bad_cursor_without_panic()
-> Result<(), Box<dyn std::error::Error>> {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let parent = tempfile::tempdir()?;
    let mut name = OsString::from("ws-");
    name.push(OsString::from_vec(vec![0xff, 0xfe]));
    let workspace = parent.path().join(name);
    let root = workspace.join(".agents/pm");
    fs::create_dir_all(&root)?;
    fs::write(
        root.join("settings.json"),
        r#"{"id_prefix":"sample-","item_format":"toon"}"#,
    )?;
    let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .arg("--workspace")
        .arg(&workspace)
        .args(["list", "--json", "--after", "not-a-cursor"])
        .output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
    let payload: serde_json::Value = serde_json::from_str(stderr.trim())?;
    assert_eq!(payload["code"], "invalid_query_cursor");
    assert_eq!(payload["exit_code"], 2);
    assert_eq!(payload["refusal"]["exit_code"], 2);
    let attempted = payload["recovery"]["attempted_command"]
        .as_str()
        .ok_or("missing attempted command")?;
    assert!(attempted.contains("list"));
    assert!(!attempted.contains("--workspace"));
    Ok(())
}

#[test]
/// Covers parser-level refusals: unknown commands, missing args, bad workspaces.
fn parser_refusals_exit_with_diagnostics() -> Result<(), Box<dyn std::error::Error>> {
    let workspace = workspace()?;
    let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .current_dir(workspace.path())
        .args(["frobnicate"])
        .output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unrecognized subcommand") || stderr.contains("Usage"),
        "unknown-command diagnostics missing: {stderr}"
    );

    let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .current_dir(workspace.path())
        .arg("create")
        .output()?;
    assert!(!output.status.success());

    let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .args(["--workspace=/tmp/definitely-not-a-pm-tracker-42", "list"])
        .output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no pm tracker found") || stderr.contains("filesystem operation failed"),
        "tracker-not-found diagnostic missing: {stderr}"
    );

    // A tracked workspace whose requested item is absent fails with
    // item-not-found. The no-tracker case is covered by the absolute
    // `--workspace` invocation above.
    expect_err(&workspace, &["get", "sample-missing"], "pm item not found")?;
    for controls in [
        vec!["--status", "open,closed"],
        vec!["--type", "Task,Issue"],
        vec!["--sort", "invalid"],
        vec!["--order", "desc"],
        vec!["--sort", "title", "--order", "sideways"],
        vec!["--output-limit", "bad"],
        vec!["--output-budget", "bad"],
    ] {
        for json in [false, true] {
            let mut args = vec!["list"];
            if json {
                args.push("--json");
            }
            args.extend(&controls);
            expect_err(&workspace, &args, "invalid read request")?;
        }
    }
    Ok(())
}

/// Each command family refuses invalid preflight input without changing canonical item/history bytes.
#[test]
fn structured_preflight_refusals_preserve_stored_state() -> Result<(), Box<dyn std::error::Error>> {
    let workspace = workspace()?;
    expect_ok(
        &workspace,
        &[
            "create",
            "--id",
            "sample-preflight",
            "--title",
            "Preflight",
            "--type",
            "Task",
            "--author",
            "cli-agent",
        ],
    )?;
    let root = workspace.path().join(".agents/pm");
    let item = root.join("tasks/sample-preflight.toon");
    let history = root.join("history/sample-preflight.jsonl");
    let before = (fs::read(&item)?, fs::read(&history)?);
    for (args, code) in preflight_refusal_cases() {
        let output = pm_rust(&workspace, &args)?;
        assert_eq!(
            output.status.code(),
            Some(2),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        let payload: serde_json::Value =
            serde_json::from_slice(&output.stderr).map_err(|error| {
                format!(
                    "{args:?}: {error}: {}",
                    String::from_utf8_lossy(&output.stderr)
                )
            })?;
        assert_eq!(payload["code"], code, "{args:?}: {payload}");
        assert_eq!(payload["exit_code"], 2);
        assert_eq!((fs::read(&item)?, fs::read(&history)?), before);
    }
    Ok(())
}

/// Lists invalid input contracts shared by native unit and process coverage.
fn preflight_refusal_cases() -> [(Vec<&'static str>, &'static str); 7] {
    [
        (
            vec!["context", "--limit", "bad", "--json"],
            "invalid_argument_value",
        ),
        (
            vec!["search", "Preflight", "--limit", "bad", "--json"],
            "invalid_argument_value",
        ),
        (
            vec![
                "test",
                "sample-preflight",
                "--remove-index",
                "bad",
                "--json",
            ],
            "invalid_argument_value",
        ),
        (
            vec![
                "close",
                "sample-preflight",
                "--resolution",
                "Fixed",
                "--expected-result",
                "Expected",
                "--actual-result",
                "Observed",
                "--completed-at",
                "bad",
                "--author",
                "cli-agent",
                "--json",
            ],
            "invalid_argument_value",
        ),
        (
            vec![
                "claim",
                "sample-preflight",
                "--start",
                "--next",
                "--author",
                "cli-agent",
                "--json",
            ],
            "invalid_argument_value",
        ),
        (
            vec![
                "deps",
                "sample-preflight",
                "--format",
                "context",
                "--cursor",
                "broken",
                "--json",
            ],
            "invalid_query_cursor",
        ),
        (
            vec![
                "deps",
                "sample-preflight",
                "--format",
                "context",
                "--token-budget",
                "1",
                "--json",
            ],
            "invalid_argument_value",
        ),
    ]
}
