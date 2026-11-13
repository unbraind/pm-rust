//! Black-box dispatch acceptance for every CLI subcommand and refusal path.
//!
//! Each case executes the compiled `pm-rust` binary as a real process and
//! asserts exit codes and stderr text, so the full argument-parsing and
//! dispatch surface is exercised exactly as callers experience it.

use std::fs;
use std::process::Command;

use tempfile::TempDir;

#[allow(dead_code)]
#[path = "support/published_cli.rs"]
mod published_cli;

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

/// Delivery failures retain successful mutations and report failures from read projections.
#[test]
fn round5_read_and_mutation_delivery_failures_retain_commits()
-> Result<(), Box<dyn std::error::Error>> {
    use std::process::Stdio;
    let workspace = workspace()?;
    for args in [
        vec![
            "create",
            "--id",
            "sample-delivery",
            "--title",
            "Delivery",
            "--type",
            "Task",
            "--author",
            "cli-agent",
        ],
        vec![
            "update",
            "sample-delivery",
            "--title",
            "Retained",
            "--author",
            "cli-agent",
        ],
        vec![
            "comment",
            "sample-delivery",
            "Retained comment",
            "--author",
            "cli-agent",
        ],
        vec![
            "close",
            "sample-delivery",
            "Retained close",
            "--validate-close",
            "off",
            "--author",
            "cli-agent",
            "--json",
        ],
        vec!["files", "sample-delivery", "--json"],
        vec!["docs", "sample-delivery", "--json"],
        vec!["context", "--json", "--no-extension-health"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
            .current_dir(workspace.path())
            .args(&args)
            .stdout(Stdio::from(closed_stdout()?))
            .output()?;
        assert!(!output.status.success(), "{args:?}");
        assert!(!output.stderr.is_empty(), "{args:?}");
    }
    let item = pm_rust(&workspace, &["get", "sample-delivery"])?;
    let item: serde_json::Value = serde_json::from_slice(&item.stdout)?;
    assert_eq!(item["title"], "Retained");
    assert_eq!(item["status"], "closed");
    assert!(
        item["comments"]
            .as_array()
            .ok_or("comments")?
            .iter()
            .any(|row| row["text"] == "Retained comment")
    );
    for args in [
        vec!["list", "--timestamp", "invalid"],
        vec!["assurance", "show", "measurement", "missing", "--json"],
    ] {
        let output = pm_rust(&workspace, &args)?;
        assert!(!output.status.success(), "{args:?}");
        assert!(output.stdout.is_empty());
    }
    Ok(())
}

/// Verified history succeeds strictly and propagates a reader closing its output pipe.
#[test]
fn history_strict_success_and_closed_pipe() -> Result<(), Box<dyn std::error::Error>> {
    use std::process::Stdio;
    let workspace = workspace()?;
    seed_item(&workspace)?;
    for (flags, message) in [
        (
            vec!["history", "sample-cli", "--full", "--compact"],
            "mutually exclusive",
        ),
        (
            vec!["history", "sample-cli", "--strict-exit"],
            "requires --verify",
        ),
    ] {
        expect_err(&workspace, &flags, message)?;
    }
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
    let refused = pm_rust(
        &workspace,
        &[
            "history",
            "sample-cli",
            "--verify",
            "--strict-exit",
            "--json",
        ],
    )?;
    assert!(!refused.status.success());
    let verified: serde_json::Value = serde_json::from_slice(&refused.stdout)?;
    assert_eq!(verified["verification"]["ok"], false);
    // Confirm the edited stream loads before isolating the closed-pipe failure.
    expect_ok(&workspace, &["history", "sample-cli", "--full", "--json"])?;
    let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .current_dir(workspace.path())
        .args(["--workspace=.", "history", "sample-cli", "--full", "--json"])
        .stdout(Stdio::from(closed_stdout()?))
        .output()?;
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
    for format in ["json", "toon"] {
        let output = pm_rust(
            &workspace,
            &["deps", "sample-cli", "--output-format", format],
        )?;
        assert!(output.status.success());
        let value: serde_json::Value = if format == "json" {
            serde_json::from_slice(&output.stdout)?
        } else {
            pm_rust::parse_toon(std::str::from_utf8(&output.stdout)?)?
        };
        assert!(value.is_object());
    }
    let changes = pm_rust(&workspace, &["changes"])?;
    assert!(changes.status.success());
    let page: serde_json::Value = pm_rust::parse_toon(std::str::from_utf8(&changes.stdout)?)?;
    assert!(page["events"].is_array());
    let cancelled = workspace.path().join("cancelled");
    fs::write(&cancelled, b"cancel")?;
    let output = pm_rust(
        &workspace,
        &[
            "changes",
            "--cancel-file",
            cancelled.to_str().ok_or("UTF8")?,
        ],
    )?;
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
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
    let human = pm_rust(&workspace, &["context", "--limit", "bad"])?;
    assert_eq!(human.status.code(), Some(2));
    assert!(human.stdout.is_empty());
    assert!(String::from_utf8_lossy(&human.stderr).contains("Error:"));
    assert_eq!((fs::read(&item)?, fs::read(&history)?), before);
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
                "item",
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

/// Startup normalization accepts scope aliases and refuses incomplete or unsupported namespaces.
#[test]
fn startup_argument_boundaries_are_read_only() -> Result<(), Box<dyn std::error::Error>> {
    let workspace = workspace()?;
    let settings = workspace.path().join(".agents/pm/settings.json");
    let before = fs::read(&settings)?;
    for args in [
        vec![],
        vec!["item"],
        vec!["item", "files", "sample-missing"],
        vec!["assurance", "show", "measurement"],
    ] {
        let output = pm_rust(&workspace, &args)?;
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
        assert_eq!(fs::read(&settings)?, before);
    }
    for scope in [
        vec!["--workspace", "."],
        vec!["--pm-path", "."],
        vec!["--path", "."],
        vec!["--workspace=."],
        vec!["--pm-path=."],
        vec!["--path=."],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
            .current_dir(workspace.path())
            .args(&scope)
            .args(["context", "--json", "--no-extension-health"])
            .output()?;
        assert!(
            output.status.success(),
            "{scope:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(serde_json::from_slice::<serde_json::Value>(&output.stdout)?.is_object());
        assert_eq!(fs::read(&settings)?, before);
    }
    Ok(())
}

/// Workspace aliases and canonical dependency commands preserve structured parser refusals.
#[test]
fn scoped_alias_refusals_match_the_native_command() -> Result<(), Box<dyn std::error::Error>> {
    let workspace = workspace()?;
    for flag in ["--workspace", "--pm-path", "--path"] {
        for prefix in [
            vec![flag.to_owned(), ".".to_owned()],
            vec![format!("{flag}=.")],
        ] {
            for command in [
                vec!["context"],
                vec!["deps", "sample-a"],
                vec!["item", "deps", "sample-a"],
            ] {
                let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
                    .current_dir(workspace.path())
                    .args(&prefix)
                    .args(&command)
                    .args(["--bogus=value", "--json"])
                    .output()?;
                assert_eq!(output.status.code(), Some(2));
                let payload: serde_json::Value = serde_json::from_slice(&output.stderr)?;
                assert_eq!(payload["code"], "unsupported_native_option");
                assert_eq!(payload["flag"], "--bogus");
                assert_eq!(payload["recovery"]["normalized_args"][0], "--pm-path");
                assert_eq!(payload["recovery"]["normalized_args"][1], ".");
                assert_eq!(
                    payload["recovery"]["normalized_args"][2],
                    if command == ["context"] {
                        "context"
                    } else {
                        "item"
                    }
                );
            }
        }
    }
    Ok(())
}

/// Parser failures use the same credential projection as structured command refusals.
#[test]
fn parser_diagnostics_redact_credential_bearing_tokens() -> Result<(), Box<dyn std::error::Error>> {
    let directory = workspace()?;
    let output = pm_rust(
        &directory,
        &["https://fixture:test-password@example.invalid"],
    )?;
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr)?;
    assert!(!stderr.contains("test-password"));
    assert!(!stderr.contains("fixture:"));
    assert!(stderr.contains("[redacted]@example.invalid"));
    Ok(())
}

/// Stored credential authorities are withheld in JSON and human foreign-claim refusals.
#[test]
fn persisted_holder_credentials_never_reach_refusal_streams()
-> Result<(), Box<dyn std::error::Error>> {
    for secret in ["stored password", "a b", "quoted\" pass"] {
        let workspace = workspace()?;
        seed_item(&workspace)?;
        let holder = format!("https://fixture:{secret}@example.invalid");
        let claimed = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
            .current_dir(workspace.path())
            .args([
                "claim",
                "sample-cli",
                "--author",
                &holder,
                "--force",
                "--json",
            ])
            .output()?;
        assert!(
            claimed.status.success(),
            "{}",
            String::from_utf8_lossy(&claimed.stderr)
        );
        for format in [vec!["--json"], vec![]] {
            let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
                .current_dir(workspace.path())
                .args(["claim", "sample-cli", "--author", "beta"])
                .args(&format)
                .output()?;
            assert_eq!(output.status.code(), Some(4));
            let text = String::from_utf8_lossy(&output.stderr);
            assert!(!text.contains(secret));
            assert!(text.contains("https://[redacted]@example.invalid"));
            if format.is_empty() {
                assert!(text.contains("Held by: https://[redacted]@example.invalid"));
            } else {
                assert_eq!(
                    serde_json::from_slice::<serde_json::Value>(&output.stderr)?["claimed_by"],
                    "https://[redacted]@example.invalid"
                );
            }
        }
    }
    Ok(())
}

/// Native positional reason text cannot redirect recovery away from the mutated workspace.
#[test]
fn positional_close_text_keeps_actual_recovery_scope() -> Result<(), Box<dyn std::error::Error>> {
    let root = workspace()?;
    seed_item(&root)?;
    let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .current_dir(root.path())
        .args([
            "close",
            "sample-cli",
            "--author",
            "cli-agent",
            "--validate-close",
            "off",
            "--json",
            "--",
            "--pm-path=other",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let hint = value["completion"]["hint"]
        .as_str()
        .ok_or("completion hint")?;
    assert!(hint.contains("pm update sample-cli --completed-at"));
    assert!(!hint.contains("other"));
    assert_eq!(
        pm_rust::Workspace::discover(root.path())?
            .get("sample-cli")?
            .metadata
            .status,
        "closed"
    );
    assert!(!root.path().join("other").exists());
    Ok(())
}

/// All URL credentials are projected in both refusal encodings, while public paths remain intact.
#[test]
fn multiple_url_credentials_never_reach_refusal_streams() -> Result<(), Box<dyn std::error::Error>>
{
    let root = workspace()?;
    seed_item(&root)?;
    expect_ok(&root, &["claim", "sample-cli", "--author", "cli-agent"])?;
    for (input, expected) in [
        (
            "https://first:alpha-secret@one.invalid/p https://second:beta-secret@two.invalid/q",
            "https://[redacted]@one.invalid/p https://[redacted]@two.invalid/q",
        ),
        (
            "https://first:alpha-secret@one.invalid/p,https://second:beta-secret@two.invalid/q",
            "https://[redacted]@one.invalid/p,https://[redacted]@two.invalid/q",
        ),
        (
            "https://first:alpha-secret@one.invalid;https://second:beta-secret@two.invalid",
            "https://[redacted]@one.invalid;https://[redacted]@two.invalid",
        ),
        (
            "https://www.npmjs.com/package/@scope/x https://u:p@host",
            "https://www.npmjs.com/package/@scope/x https://[redacted]@host",
        ),
        (
            "See https://public.invalid/docs and https://fixture:abc def@example.invalid",
            "See https://public.invalid/docs and https://[redacted]@example.invalid",
        ),
        (
            "https://user:abc/def+gh==@host/x",
            "https://[redacted]@host/x",
        ),
        (
            "https://user:long-lived-secret://tail@host.invalid",
            "https://[redacted]@host.invalid",
        ),
        (
            "https://www.npmjs.com/package/@scope/x",
            "https://www.npmjs.com/package/@scope/x",
        ),
    ] {
        for format in [vec![], vec!["--json"]] {
            let mut args = vec![
                "claim",
                "sample-cli",
                "--author",
                "beta",
                "--message",
                input,
            ];
            args.extend(&format);
            let output = pm_rust(&root, &args)?;
            assert_eq!(output.status.code(), Some(4));
            assert!(output.stdout.is_empty());
            let text = String::from_utf8(output.stderr)?;
            for secret in [
                "alpha-secret",
                "beta-secret",
                "abc def",
                "abc/def",
                "long-lived-secret",
            ] {
                assert!(!text.contains(secret), "{args:?}");
            }
            if format.is_empty() {
                assert!(text.contains(expected));
            } else {
                let value: serde_json::Value = serde_json::from_str(&text)?;
                let args = value["recovery"]["normalized_args"]
                    .as_array()
                    .ok_or("normalized args")?;
                let message = args
                    .iter()
                    .position(|arg| arg == "--message")
                    .ok_or("message flag")?;
                assert_eq!(args[message + 1], expected);
                if input == expected {
                    assert!(!text.contains("restore them before replaying"));
                    assert!(text.contains(input));
                }
            }
        }
    }
    Ok(())
}

/// A stored blocker title enrolls its credentials before either refusal stream is rendered.
#[test]
fn round5_stored_blocker_credentials_never_reach_refusal_streams()
-> Result<(), Box<dyn std::error::Error>> {
    let workspace = workspace()?;
    seed_item(&workspace)?;
    let root = workspace.path().join(".agents/pm");
    let mut own = pm_rust::Workspace::discover(workspace.path())?.get("sample-cli")?;
    let mut gate = own.clone();
    gate.metadata.id = "sample-gate".to_owned();
    gate.metadata.title =
        "https://user:long-lived-secret://tail@host.invalid copied long-lived-secret://tail"
            .to_owned();
    own.metadata
        .extra
        .insert("blocked_by".to_owned(), serde_json::json!("sample-gate"));
    let paths = [
        root.join("tasks/sample-cli.toon"),
        root.join("tasks/sample-gate.toon"),
    ];
    for (path, item) in paths.iter().zip([&own, &gate]) {
        fs::write(path, toon_format::encode_default(item)?)?;
    }
    let before = paths.iter().map(fs::read).collect::<Result<Vec<_>, _>>()?;
    for format in [vec!["--json"], vec![]] {
        let mut args = vec!["claim", "sample-cli", "--start", "--author", "beta"];
        args.extend(format);
        let output = pm_rust(&workspace, &args)?;
        assert_eq!(output.status.code(), Some(4));
        assert!(output.stdout.is_empty());
        let text = String::from_utf8(output.stderr)?;
        assert!(!text.contains("long-lived-secret"));
        assert!(text.contains("https://[redacted]@host.invalid copied [redacted]"));
        for (path, bytes) in paths.iter().zip(&before) {
            assert_eq!(&fs::read(path)?, bytes);
        }
    }
    Ok(())
}

/// Parser diagnostics project credentials on both help stdout and refusal stderr.
#[test]
fn round5_parser_credential_projection_covers_both_streams()
-> Result<(), Box<dyn std::error::Error>> {
    let workspace = workspace()?;
    let help = pm_rust(
        &workspace,
        &[
            "claim",
            "--help",
            "--author",
            "https://u:author@host.invalid",
        ],
    )?;
    assert!(help.status.success());
    assert!(help.stderr.is_empty());
    assert!(String::from_utf8(help.stdout)?.contains("[redacted]"));
    let refusal = pm_rust(&workspace, &["https://u:long-secret@host.invalid"])?;
    assert_eq!(refusal.status.code(), Some(2));
    assert!(refusal.stdout.is_empty());
    let text = String::from_utf8(refusal.stderr)?;
    assert!(text.contains("[redacted]"));
    assert!(!text.contains("long-secret"));
    Ok(())
}

/// Both legacy intent and canonical cost omissions keep their machine receipt and exit two.
#[test]
fn round5_list_budget_omissions_keep_their_exit_contract() -> Result<(), Box<dyn std::error::Error>>
{
    let workspace = workspace()?;
    seed_item(&workspace)?;
    let native = pm_rust::Workspace::discover(workspace.path())?;
    let mut item = native.get("sample-cli")?;
    item.metadata.title = "x".repeat(20_000);
    let path = workspace.path().join(".agents/pm/tasks/sample-cli.toon");
    fs::write(&path, toon_format::encode_default(&item)?)?;
    let before = fs::read(&path)?;
    for (args, key) in [
        (
            vec!["list", "--json", "--output-budget", "1"],
            "output_budget_exceeded",
        ),
        (
            vec!["list", "--json", "--for", "triage", "--token-budget", "256"],
            "budget_exceeded",
        ),
    ] {
        let output = pm_rust(&workspace, &args)?;
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stderr.is_empty());
        let receipt: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(receipt[key]["omitted_result"], true);
        assert_eq!(fs::read(&path)?, before);
    }
    Ok(())
}

/// Creates a pipe with no readers before the child process exists.
fn closed_stdout() -> std::io::Result<std::io::PipeWriter> {
    let (reader, writer) = std::io::pipe()?;
    drop(reader);
    Ok(writer)
}

/// The delivery-failure invariant exists before scheduling any child.
#[test]
fn round7_closed_pipe_precedes_child_creation() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write as _;
    let mut writer = closed_stdout()?;
    assert_eq!(
        writer
            .write_all(b"receipt")
            .err()
            .ok_or("pipe remained readable")?
            .kind(),
        std::io::ErrorKind::BrokenPipe
    );
    Ok(())
}

/// Stored titles and positional comments/reasons share decorated-userinfo privacy on both streams.
#[test]
fn round10_decorated_credentials_never_reach_refusal_streams()
-> Result<(), Box<dyn std::error::Error>> {
    let workspace = workspace()?;
    seed_item(&workspace)?;
    let root = workspace.path().join(".agents/pm");
    let own_path = root.join("tasks/sample-cli.toon");
    let mut own = pm_rust::Workspace::discover(workspace.path())?.get("sample-cli")?;
    let mut gate = own.clone();
    gate.metadata.id = "sample-gate".to_owned();
    own.metadata
        .extra
        .insert("blocked_by".to_owned(), serde_json::json!("sample-gate"));
    own.metadata
        .extra
        .insert("assignee".to_owned(), serde_json::json!("alpha"));
    fs::write(&own_path, toon_format::encode_default(&own)?)?;
    // Governance refuses comment/close before persisting their decorated input.
    let settings = root.join("settings.json");
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&settings)?)?;
    value["governance"] = serde_json::json!({"preset":"custom","ownership_enforcement":"strict"});
    fs::write(settings, serde_json::to_vec(&value)?)?;
    for (source, expected) in [
        (
            "Gate: https://stored user@host.invalid",
            "Gate: https://[redacted]@host.invalid",
        ),
        (
            "Gate: https://host.invalid and user@example.invalid",
            "Gate: https://host.invalid and user@example.invalid",
        ),
    ] {
        gate.metadata.title = source.to_owned();
        fs::write(
            root.join("tasks/sample-gate.toon"),
            toon_format::encode_default(&gate)?,
        )?;
        for json in [false, true] {
            let mut formats = vec![];
            if json {
                formats.push("--json");
            }
            for mut args in [
                vec!["claim", "sample-cli", "--start", "--author", "alpha"],
                vec!["comment", "sample-cli", source, "--author", "beta"],
                vec!["close", "sample-cli", source, "--author", "beta"],
            ] {
                if json && args[0] == "comment" {
                    continue;
                }
                args.extend(&formats);
                let before = fs::read(&own_path)?;
                let output = pm_rust(&workspace, &args)?;
                assert!(!output.status.success(), "{args:?}");
                assert!(output.stdout.is_empty());
                let text = String::from_utf8(output.stderr)?;
                assert!(text.contains(expected), "{args:?}: {text}");
                if source.contains("stored user") {
                    assert!(!text.contains("stored user"));
                }
                assert_eq!(fs::read(&own_path)?, before);
            }
        }
    }
    Ok(())
}

/// An executable blocker-removal step targets the selected tracker even when IDs overlap.
#[test]
fn round10_scoped_next_step_cannot_mutate_default_tracker() -> Result<(), Box<dyn std::error::Error>>
{
    let default = workspace()?;
    let selected = workspace()?;
    for tracker in [&default, &selected] {
        seed_item(tracker)?;
        let store = pm_rust::Workspace::discover(tracker.path())?;
        let mut own = store.get("sample-cli")?;
        let mut gate = own.clone();
        gate.metadata.id = "sample-gate".to_owned();
        own.metadata
            .extra
            .insert("blocked_by".to_owned(), serde_json::json!("sample-gate"));
        for item in [&own, &gate] {
            fs::write(
                store
                    .pm_root()
                    .join(format!("tasks/{}.toon", item.metadata.id)),
                toon_format::encode_default(item)?,
            )?;
        }
    }
    let target = selected.path().join(".agents/pm");
    let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .current_dir(default.path())
        .arg("--pm-path")
        .arg(&target)
        .args([
            "claim",
            "sample-cli",
            "--start",
            "--author",
            "beta",
            "--json",
        ])
        .output()?;
    assert_eq!(output.status.code(), Some(4));
    let refusal: serde_json::Value = serde_json::from_slice(&output.stderr)?;
    let expected = format!(
        "pm --pm-path {} update sample-cli --unset blocked_by",
        pm_rust::quote_command_arg(&target.to_string_lossy())
    );
    assert!(
        refusal["next_steps"]
            .as_array()
            .ok_or("steps")?
            .iter()
            .any(|s| s == &expected)
    );
    let default_item = default.path().join(".agents/pm/tasks/sample-cli.toon");
    let default_history = default.path().join(".agents/pm/history/sample-cli.jsonl");
    let before = (fs::read(&default_item)?, fs::read(&default_history)?);
    let Some(published) = published_cli::published_cli_or_skip("scoped executable next step")
    else {
        return Ok(());
    };
    let mut command = published_cli::oracle_command(
        std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned()),
    );
    let output = command
        .env("DO_NOT_TRACK", "1")
        .arg(published.entry)
        .current_dir(default.path())
        .arg("--pm-path")
        .arg(&target)
        .args([
            "update",
            "sample-cli",
            "--unset",
            "blocked_by",
            "--author",
            "beta",
            "--json",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        (fs::read(default_item)?, fs::read(default_history)?),
        before
    );
    assert!(
        pm_rust::Workspace::discover(selected.path())?
            .get("sample-cli")?
            .metadata
            .extra
            .get("blocked_by")
            .is_none()
    );
    Ok(())
}

/// Seeds two non-git trackers with the same issue ID for recovery execution checks.
fn overlapping_issue_trackers() -> Result<(TempDir, TempDir), Box<dyn std::error::Error>> {
    let default = workspace()?;
    let selected = workspace()?;
    for tracker in [&default, &selected] {
        expect_ok(
            tracker,
            &[
                "create",
                "--id",
                "sample-one",
                "--title",
                "Scope collision control",
                "--type",
                "Issue",
                "--author",
                "cli-agent",
            ],
        )?;
    }
    Ok((default, selected))
}

/// Executing the scoped retry closes the selected overlapping ID and leaves the default untouched.
#[test]
fn round11_scoped_close_retry_executes_only_selected_tracker()
-> Result<(), Box<dyn std::error::Error>> {
    let (default, selected) = overlapping_issue_trackers()?;
    let untouched = default.path().join(".agents/pm");
    let before = (
        fs::read(untouched.join("issues/sample-one.toon"))?,
        fs::read(untouched.join("history/sample-one.jsonl"))?,
    );
    let scope = selected
        .path()
        .join(".agents/pm")
        .to_string_lossy()
        .into_owned();
    let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .current_dir(default.path())
        .args([
            "--pm-path",
            &scope,
            "close",
            "sample-one",
            "--validate-close",
            "strict",
            "--json",
            "--",
            "--pm-path=literal",
        ])
        .output()?;
    assert_eq!(
        output.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let refusal: serde_json::Value = serde_json::from_slice(&output.stderr)?;
    for key in ["normalized_args", "suggested_retry_args"] {
        let values = refusal["recovery"][key]
            .as_array()
            .ok_or("missing recovery argv")?;
        assert_eq!(
            &values[..2],
            &[serde_json::json!("--pm-path"), serde_json::json!(scope)]
        );
        assert_eq!(values.last(), Some(&serde_json::json!("--pm-path=literal")));
    }
    let expected = format!("pm --pm-path {} close ", pm_rust::quote_command_arg(&scope));
    for key in ["attempted_command", "suggested_retry"] {
        assert!(
            refusal["recovery"][key]
                .as_str()
                .ok_or("command")?
                .starts_with(&expected)
        );
    }
    let retry = refusal["recovery"]["suggested_retry_args"]
        .as_array()
        .ok_or("retry")?
        .iter()
        .map(|value| {
            value.as_str().ok_or("argument").map(|text| {
                if text == "<value>" {
                    "Verified evidence".to_owned()
                } else {
                    text.to_owned()
                }
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let closed = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .current_dir(default.path())
        .args(&retry)
        .output()?;
    assert!(
        closed.status.success(),
        "{}",
        String::from_utf8_lossy(&closed.stderr)
    );
    assert_eq!(
        pm_rust::Workspace::discover(selected.path())?
            .get("sample-one")?
            .metadata
            .status,
        "closed"
    );
    assert_eq!(
        pm_rust::Workspace::discover(selected.path())?
            .get("sample-one")?
            .metadata
            .extra["close_reason"],
        "--pm-path=literal"
    );
    assert_eq!(
        (
            fs::read(untouched.join("issues/sample-one.toon"))?,
            fs::read(untouched.join("history/sample-one.jsonl"))?
        ),
        before
    );
    Ok(())
}

#[path = "support/refusal_corpus.rs"]
mod refusal_corpus;

/// Stored holders and close retries mirror the shared authority corpus in both refusal formats.
#[test]
fn round11_stored_holder_and_public_retry_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    exercise_round11_holder_boundaries(None)
}

/// Public URL prose keeps the stored display and the same-close retry advice.
#[test]
fn round11_public_prose_process_keeps_retry() -> Result<(), Box<dyn std::error::Error>> {
    exercise_round11_holder_boundaries(Some(false))
}

/// Runs the shared corpus against real persisted holders without Git or SDK mocks.
fn exercise_round11_holder_boundaries(
    private_only: Option<bool>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut mismatches = Vec::new();
    for &(input, expected) in refusal_corpus::AUTHORITIES
        .iter()
        .chain(refusal_corpus::COPIED_PASSWORDS)
    {
        for &prefix in refusal_corpus::DECORATIONS {
            let tracker = workspace()?;
            expect_ok(
                &tracker,
                &[
                    "create",
                    "--id",
                    "sample-holder",
                    "--title",
                    "Stored holder fixture",
                    "--type",
                    "Issue",
                    "--author",
                    "fixture",
                ],
            )?;
            let holder = format!("{prefix}{input}");
            let expected = format!("{prefix}{expected}");
            let private = holder != expected;
            if private_only.is_some_and(|wanted| wanted != private) {
                continue;
            }

            expect_ok(
                &tracker,
                &[
                    "claim",
                    "sample-holder",
                    "--author",
                    &holder,
                    "--force",
                    "--json",
                ],
            )?;
            let root = tracker.path().join(".agents/pm");
            let item = root.join("issues/sample-holder.toon");
            let history = root.join("history/sample-holder.jsonl");
            let before = (fs::read(&item)?, fs::read(&history)?);
            for json in [false, true] {
                let mut claim = vec!["claim", "sample-holder", "--author", "beta"];
                if json {
                    claim.push("--json");
                }
                let output = pm_rust(&tracker, &claim)?;
                assert_eq!(output.status.code(), Some(4));
                assert!(output.stdout.is_empty());
                let text = String::from_utf8(output.stderr)?;
                if json {
                    if serde_json::from_str::<serde_json::Value>(&text)?["claimed_by"] != expected {
                        mismatches.push(format!("JSON holder: {holder}"));
                    }
                } else if !text.contains(&format!("Held by: {expected}")) {
                    mismatches.push(format!("human holder: {holder}"));
                }
                let mut close = vec![
                    "close",
                    "sample-holder",
                    "--author",
                    holder.as_str(),
                    "--validate-close",
                    "strict",
                ];
                if json {
                    close.push("--json");
                }
                close.extend(["--", "Closing reason"]);
                let output = pm_rust(&tracker, &close)?;
                assert_eq!(output.status.code(), Some(2));
                assert!(output.stdout.is_empty());
                let text = String::from_utf8(output.stderr)?;
                let encoded = if json {
                    serde_json::to_string(&expected)?
                } else {
                    pm_rust::quote_command_arg(&expected)
                };
                if !text.contains(&expected) && !text.contains(&encoded) {
                    mismatches.push(format!("close projection json={json}: {holder}"));
                }
                if json {
                    let payload: serde_json::Value = serde_json::from_str(&text)?;
                    if payload["recovery"].get("suggested_retry").is_none() != private {
                        mismatches.push(format!("JSON retry: {holder}"));
                    }
                } else if text.contains("Run: pm ") == private {
                    mismatches.push(format!("human retry: {holder}"));
                }
                assert_eq!((fs::read(&item)?, fs::read(&history)?), before);
            }
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    Ok(())
}

/// Quoted message arguments register slash passwords before projecting copied values.
#[test]
fn round12_quoted_message_copies_are_redacted() -> Result<(), Box<dyn std::error::Error>> {
    let tracker = workspace()?;
    seed_item(&tracker)?;
    expect_ok(
        &tracker,
        &["claim", "sample-cli", "--author", "alpha", "--force"],
    )?;
    for &(input, _) in refusal_corpus::COPIED_PASSWORDS {
        for json in [false, true] {
            let mut args = vec![
                "claim",
                "sample-cli",
                "--author",
                "beta",
                "--message",
                input,
            ];
            if json {
                args.push("--json");
            }
            let output = pm_rust(&tracker, &args)?;
            assert_eq!(output.status.code(), Some(4));
            let text = String::from_utf8(output.stderr)?;
            assert!(text.contains("[redacted]"));
            assert!(!text.contains("443/secret"), "{text}");
        }
    }
    Ok(())
}

/// Real stored-row diagnostics redact enrolled copies in both requested refusal modes.
#[test]
fn round15_stored_duplicate_diagnostics_never_leak() -> Result<(), Box<dyn std::error::Error>> {
    use refusal_corpus as corpus;
    let tracker = workspace()?;
    for id in ["sample-first", "sample-second"] {
        expect_ok(
            &tracker,
            &[
                "create", "--id", id, "--title", "Fixture", "--type", "Task", "--author", "alpha",
            ],
        )?;
    }
    let paths = ["sample-first", "sample-second"]
        .map(|id| tracker.path().join(format!(".agents/pm/tasks/{id}.toon")));
    let baseline: serde_json::Value = toon_format::decode_default(&fs::read_to_string(&paths[0])?)?;
    for (text, secret) in corpus::DIAGNOSTICS
        .iter()
        .map(|(text, secret)| (*text, Some(*secret)))
        .chain(corpus::PUBLIC_DIAGNOSTICS.iter().map(|text| (*text, None)))
    {
        let mut document = baseline.clone();
        document["id"] = serde_json::json!(text);
        for path in &paths {
            fs::write(path, toon_format::encode_default(&document)?)?;
        }
        for json in [false, true] {
            let args = if json {
                vec!["deps", "sample-first", "--json"]
            } else {
                vec!["deps", "sample-first"]
            };
            let output = pm_rust(&tracker, &args)?;
            let diagnostic = String::from_utf8(output.stderr)?;
            assert_eq!(output.status.code(), Some(2), "{diagnostic}");
            assert!(diagnostic.contains("duplicate pm item id"));
            if let Some(secret) = secret {
                assert!(
                    !diagnostic.contains(secret),
                    "stored credential copy must not reach stderr"
                );
                assert!(diagnostic.contains("[redacted]"));
            } else {
                assert!(diagnostic.contains(text));
            }
            for path in &paths {
                assert_eq!(
                    fs::read_to_string(path)?,
                    toon_format::encode_default(&document)?
                );
            }
        }
    }
    Ok(())
}
