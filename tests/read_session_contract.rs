//! Public CLI and SDK session contracts against genuine non-git trackers.
use pm_rust::{ReadSessionLimits, SessionCancellation, Workspace};
use serde_json::Value;
use std::{fs, path::Path, process::Command};

/// Fallible test result.
type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Initializes a real tracker with the pinned CLI, outside any Git repository.
fn fixture() -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let launcher = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle/launch.mts");
    let initialized = Command::new("node")
        .arg(launcher)
        .current_dir(directory.path())
        .args(["init", "--defaults", "--agent-guidance", "skip"])
        .env(
            "GIT_CEILING_DIRECTORIES",
            directory.path().parent().ok_or("temp parent")?,
        )
        .output()?;
    assert!(
        initialized.status.success(),
        "{}",
        String::from_utf8_lossy(&initialized.stderr)
    );
    fs::create_dir_all(directory.path().join(".agents/pm/tasks"))?;
    for index in 0..3 {
        fs::write(
            directory
                .path()
                .join(format!(".agents/pm/tasks/demo-{index}.toon")),
            format!(
                "id: demo-{index}\ntitle: Example {index}\ntype: Task\nstatus: open\npriority: 1\ncreated_at: \"2026-10-21T12:00:00.000Z\"\nupdated_at: \"2026-10-21T12:00:00.000Z\"\nbody: retained\n"
            ),
        )?;
    }
    Ok(directory)
}

#[test]
fn read_session_cli_needs_no_git_or_node_and_emits_bounded_frames() -> TestResult {
    let directory = fixture()?;
    let no_executables = tempfile::tempdir()?;
    for json in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_pm-rust"));
        command
            .current_dir(directory.path())
            .args(["read-session", "--page-items", "1", "--cancel-file"])
            .arg(directory.path().join("absent-cancel-marker"));
        if json {
            command.arg("--json");
        }
        let result = command
            .env("PATH", no_executables.path())
            .env(
                "GIT_CEILING_DIRECTORIES",
                directory.path().parent().ok_or("temp parent")?,
            )
            .output()?;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let output = String::from_utf8(result.stdout)?;
        if json {
            let pages = output
                .lines()
                .map(serde_json::from_str::<Value>)
                .collect::<Result<Vec<_>, _>>()?;
            assert_eq!(pages.len(), 3);
            for (index, page) in pages.iter().enumerate() {
                assert_eq!(page["output"]["items"].as_array().map(Vec::len), Some(1));
                assert_eq!(page["receipt"]["complete"], index == 2);
            }
        } else {
            assert_eq!(output.matches("---\n").count(), 3);
            assert!(output.contains("protocol: pm-rust/read-session/v1"));
        }
    }
    let marker = directory.path().join("cancel");
    fs::write(&marker, "")?;
    for controls in [
        vec!["--memory-bytes", "1"],
        vec!["--output-tokens", "0"],
        vec!["--cancel-file", marker.to_str().ok_or("marker")?],
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
            .current_dir(directory.path())
            .arg("read-session")
            .args(controls)
            .output()?;
        assert_eq!(result.status.code(), Some(2));
        assert!(result.stdout.is_empty());
        let refusal: Value = serde_json::from_slice(&result.stderr)?;
        assert!(
            refusal["code"]
                .as_str()
                .is_some_and(|code| code.starts_with("session_"))
        );
        assert!(
            refusal["required"]
                .as_str()
                .is_some_and(|text| text.contains(';'))
        );
    }
    Ok(())
}

#[test]
fn canonical_output_budget_continuations_do_not_skip_context_rows() -> TestResult {
    let directory = fixture()?;
    let workspace = Workspace::discover(directory.path())?;
    // Large titles force the existing output cursor rather than only a producer cursor.
    for entry in fs::read_dir(workspace.pm_root().join("tasks"))? {
        let path = entry?.path();
        let text = fs::read_to_string(&path)?.replace(
            "title: Example",
            &format!("title: {} Example", "x".repeat(3000)),
        );
        fs::write(path, text)?;
    }
    for toon in [false, true] {
        let mut session = workspace.read_session(
            &ReadSessionLimits {
                output_tokens: 1500,
                page_items: 3,
                toon,
                ..ReadSessionLimits::default()
            },
            SessionCancellation::default(),
            "2026-10-21T12:00:00.000Z",
        )?;
        let mut ids = std::collections::BTreeSet::new();
        let mut pages = 0;
        while let Some(page) = session.next_page()? {
            pages += 1;
            assert!(pages <= 3);
            for row in page.output["items"].as_array().ok_or("missing rows")? {
                assert!(ids.insert(row["id"].as_str().ok_or("missing id")?.to_owned()));
            }
        }
        assert!(session.receipt().complete);
        assert_eq!(ids.len(), 3);
    }
    Ok(())
}
