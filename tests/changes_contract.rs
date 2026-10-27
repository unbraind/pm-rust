//! Genuine non-git SDK fixtures prove canonical transport, restart and CLI behavior.
use pm_rust::{
    SessionCancellation, Workspace,
    changes::{ChangeReader, ChangeRequest, LocalChangeAdapter},
};
use serde_json::Value;
use std::{fs, path::Path, process::Command};

/// Fallible integration test result.
type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Initializes a tracker through the pinned published CLI, outside Git.
fn fixture() -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle/launch.mts"))
        .current_dir(dir.path())
        .arg("--path")
        .arg(dir.path().join(".agents/pm"))
        .args(["init", "--defaults", "--agent-guidance", "skip"])
        .env(
            "GIT_CEILING_DIRECTORIES",
            dir.path().parent().ok_or("parent")?,
        )
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(dir)
}

/// No external executable is available on either native CLI encoding's read path.
#[test]
fn cli_resumes_without_git_or_node() -> TestResult {
    let dir = fixture()?;
    let empty = tempfile::tempdir()?;
    let workspace = Workspace::discover(dir.path())?;
    let cancellation = SessionCancellation::default();
    let adapter = LocalChangeAdapter::new(workspace, cancellation.clone());
    let mut reader = ChangeReader::new(adapter, ChangeRequest::default(), cancellation)?;
    let sdk = reader.pull()?;
    assert!(!sdk.events.is_empty());
    for json in [true, false] {
        let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
            .current_dir(dir.path())
            .args(["changes", "--limit", "1"])
            .args(if json { vec!["--json"] } else { vec![] })
            .env("PATH", empty.path())
            .env(
                "GIT_CEILING_DIRECTORIES",
                dir.path().parent().ok_or("parent")?,
            )
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.len() <= 65_536);
        if json {
            let page: Value = serde_json::from_slice(&output.stdout)?;
            assert_eq!(page["events"][0], serde_json::to_value(&sdk.events[0])?);
            let resumed = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
                .current_dir(dir.path())
                .args([
                    "changes",
                    "--json",
                    "--cursor",
                    page["next_cursor"].as_str().ok_or("cursor")?,
                ])
                .env("PATH", empty.path())
                .output()?;
            assert!(resumed.status.success());
        } else {
            assert!(String::from_utf8(output.stdout)?.contains("pm-rust/changes/1"));
        }
    }
    let marker = dir.path().join("cancel");
    fs::write(&marker, "")?;
    for controls in [
        vec!["--limit", "0"],
        vec!["--output-bytes", "1"],
        vec!["--cursor", "stale"],
        vec!["--cancel-file", marker.to_str().ok_or("marker")?],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
            .current_dir(dir.path())
            .arg("changes")
            .args(controls)
            .output()?;
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let error: Value = serde_json::from_slice(&output.stderr)?;
        assert!(
            error["code"]
                .as_str()
                .is_some_and(|code| code.starts_with("changes_"))
        );
        assert!(
            error["required"]
                .as_str()
                .is_some_and(|remedy| remedy.contains(';'))
        );
    }
    Ok(())
}
