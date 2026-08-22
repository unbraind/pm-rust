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

#[test]
/// Covers the read commands over stored items and their failure paths.
fn list_and_get_dispatch_over_real_processes() -> Result<(), Box<dyn std::error::Error>> {
    let workspace = workspace()?;
    let output = pm_rust(&workspace, &["list"])?;
    assert!(output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("\"count\": 0"),
        "empty list projection missing"
    );
    seed_item(&workspace)?;
    let output = pm_rust(&workspace, &["list", "--status", "open", "--type", "task"])?;
    assert!(String::from_utf8_lossy(&output.stdout).contains("sample-cli"));
    let output = pm_rust(&workspace, &["get", "sample-cli"])?;
    assert!(String::from_utf8_lossy(&output.stdout).contains("Dispatch target"));
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
    );
    expect_err(
        &workspace,
        &["update", "sample-cli", "--title=  ", "--author=cli-agent"],
        "must not be empty",
    );
    expect_err(
        &workspace,
        &[
            "update",
            "sample-missing",
            "--title=X",
            "--author=cli-agent",
        ],
        "not found",
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
    );
    expect_err(
        &workspace,
        &["comment", "sample-missing", "text", "--author=cli-agent"],
        "not found",
    );
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
    );
    expect_err(
        &workspace,
        &["close", "sample-cli", "--reason=  ", "--author=cli-agent"],
        "closing summary",
    );
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

    // A relative workspace that exists but holds no tracker fails identically.
    expect_err(&workspace, &["get", "sample-missing"], "pm item not found")?;
    Ok(())
}
