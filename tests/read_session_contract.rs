//! Public CLI and SDK session contracts against genuine non-git trackers.
use pm_rust::{ItemFilter, ListOptions, ReadSessionLimits, SessionCancellation, Workspace};
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
        .arg("--path")
        .arg(directory.path().join(".agents/pm"))
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
    assert!(
        directory.path().join(".agents/pm/settings.json").is_file(),
        "init must create the explicitly selected tracker"
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
            command.args(["--json", "--timestamp", "2026-10-21T12:00:00.000Z"]);
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
            assert!(output.contains("pm-rust/read-session/v1"));
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

/// Public embedding admission refuses early and owns a source-independent snapshot.
#[test]
fn embedding_admission_and_snapshot_lifetime_are_enforced() -> TestResult {
    let directory = fixture()?;
    let workspace = Workspace::discover(directory.path())?;
    assert!(matches!(
        workspace.read_session(
            &ReadSessionLimits {
                memory_bytes: 1,
                ..ReadSessionLimits::default()
            },
            SessionCancellation::default(),
            "2026-10-21T12:00:00.000Z"
        ),
        Err(pm_rust::SessionRefusal::MemoryLimit)
    ));
    let mut session = workspace.read_session(
        &ReadSessionLimits::default(),
        SessionCancellation::default(),
        "2026-10-21T12:00:00.000Z",
    )?;
    fs::remove_dir_all(workspace.pm_root())?;
    let page = session.next_page()?.ok_or("snapshot page missing")?;
    assert_eq!(page.output["items"].as_array().map(Vec::len), Some(3));
    assert!(page.receipt.complete);
    assert!(session.next_page()?.is_none());
    Ok(())
}

/// The CLI polls cancellation at delivery boundaries and removes its scratch snapshot.
#[test]
fn cli_cancellation_and_success_remove_private_storage() -> TestResult {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    let directory = fixture()?;
    let tasks = directory.path().join(".agents/pm/tasks");
    let template = fs::read_to_string(tasks.join("demo-0.toon"))?;
    for index in 3..200 {
        fs::write(
            tasks.join(format!("demo-{index}.toon")),
            template.replace("demo-0", &format!("demo-{index}")),
        )?;
    }
    let scratch = tempfile::tempdir()?;
    let marker = directory.path().join("cancel-after-page");
    let mut child = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .current_dir(directory.path())
        .args([
            "read-session",
            "--page-items",
            "1",
            "--json",
            "--cancel-file",
        ])
        .arg(&marker)
        .env("TMPDIR", scratch.path())
        .env("TMP", scratch.path())
        .env("TEMP", scratch.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdout = BufReader::new(child.stdout.take().ok_or("missing stdout")?);
    let mut first = String::new();
    stdout.read_line(&mut first)?;
    let frame: Value = serde_json::from_str(&first)?;
    assert_eq!(frame["receipt"]["complete"], false);
    fs::write(&marker, "cancel")?;
    // Drain after setting the marker, so even a full pipe cannot block cancellation.
    let mut remaining = String::new();
    std::io::Read::read_to_string(&mut stdout, &mut remaining)?;
    let result = child.wait_with_output()?;
    assert_eq!(result.status.code(), Some(2));
    let refusal: Value = serde_json::from_slice(&result.stderr)?;
    assert_eq!(refusal["code"], "session_cancelled");
    assert_eq!(fs::read_dir(scratch.path())?.count(), 0);
    Ok(())
}

/// A disconnected native consumer terminates delivery and removes private storage.
#[test]
fn disconnected_consumer_releases_session_and_storage_refusals_are_typed() -> TestResult {
    use std::process::Stdio;
    let directory = fixture()?;
    let scratch = tempfile::tempdir()?;
    for json in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_pm-rust"));
        command
            .current_dir(directory.path())
            .args(["read-session", "--page-items", "1"])
            .env("TMPDIR", scratch.path())
            .env("TMP", scratch.path())
            .env("TEMP", scratch.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if json {
            command.arg("--json");
        }
        let mut child = command.spawn()?;
        drop(child.stdout.take());
        let result = child.wait_with_output()?;
        assert!(!result.status.success());
        assert_eq!(fs::read_dir(scratch.path())?.count(), 0);
    }
    let blocker = scratch.path().join("blocked-temp");
    fs::write(&blocker, "not a directory")?;
    let result = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .current_dir(directory.path())
        .arg("read-session")
        .env("TMPDIR", &blocker)
        .env("TMP", &blocker)
        .env("TEMP", &blocker)
        .output()?;
    assert_eq!(result.status.code(), Some(2));
    let refusal: Value = serde_json::from_slice(&result.stderr)?;
    assert_eq!(refusal["code"], "session_storage_failed");
    let source = directory.path().join(".agents/pm/tasks/demo-0.toon");
    for (bytes, code) in [
        (&b"invalid"[..], "session_source_refused"),
        (&b"\xff"[..], "session_unsupported_source"),
    ] {
        fs::write(&source, bytes)?;
        let result = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
            .current_dir(directory.path())
            .arg("read-session")
            .output()?;
        assert_eq!(result.status.code(), Some(2));
        let refusal: Value = serde_json::from_slice(&result.stderr)?;
        assert_eq!(refusal["code"], code);
    }
    Ok(())
}

/// Unix schema lookup errors refuse delivery and remove the partially copied snapshot.
#[cfg(unix)]
#[test]
fn schema_parent_file_never_certifies_cli_completion() -> TestResult {
    let directory = fixture()?;
    let root = directory.path().join(".agents/pm");
    fs::remove_dir_all(root.join("schema"))?;
    fs::write(root.join("schema"), "schema is a regular file")?;
    let scratch = tempfile::tempdir()?;
    for json in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_pm-rust"));
        command
            .current_dir(directory.path())
            .arg("read-session")
            .env("TMPDIR", scratch.path())
            .env("TMP", scratch.path())
            .env("TEMP", scratch.path());
        if json {
            command.arg("--json");
        }
        let result = command.output()?;
        assert_eq!(result.status.code(), Some(2));
        assert!(
            result.stdout.is_empty(),
            "refused source must emit no certificate"
        );
        let refusal: Value = serde_json::from_slice(&result.stderr)?;
        assert_eq!(refusal["code"], "session_source_refused");
        assert_eq!(fs::read_dir(scratch.path())?.count(), 0);
    }
    assert_eq!(
        fs::read_to_string(root.join("schema"))?,
        "schema is a regular file"
    );
    Ok(())
}

/// Windows canonical `NotFound` fallback also completes through the CLI and cleans up.
#[cfg(windows)]
#[test]
fn schema_parent_file_cli_matches_windows_canonical_defaults() -> TestResult {
    let directory = fixture()?;
    let root = directory.path().join(".agents/pm");
    fs::remove_dir_all(root.join("schema"))?;
    fs::write(root.join("schema"), "schema is a regular file")?;
    let scratch = tempfile::tempdir()?;
    for json in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_pm-rust"));
        command
            .current_dir(directory.path())
            .arg("read-session")
            .env("TMPDIR", scratch.path())
            .env("TMP", scratch.path())
            .env("TEMP", scratch.path());
        if json {
            command.arg("--json");
        }
        let result = command.output()?;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        if json {
            let page: Value = serde_json::from_slice(&result.stdout)?;
            assert_eq!(page["receipt"]["complete"], true);
            assert_eq!(page["output"]["items"].as_array().map(Vec::len), Some(3));
        } else {
            assert!(String::from_utf8(result.stdout)?.contains("complete: true"));
        }
        assert_eq!(fs::read_dir(scratch.path())?.count(), 0);
    }
    Ok(())
}

/// Invalid filenames refuse admission with source diagnostics rather than snapshot paths.
#[test]
fn schema_invalid_path_preserves_cli_source_diagnostics() -> TestResult {
    for name in ["statuses", "workflows"] {
        let directory = fixture()?;
        let root = directory.path().join(".agents/pm");
        let mut settings: Value = serde_json::from_slice(&fs::read(root.join("settings.json"))?)?;
        settings["schema"]["files"][name] = serde_json::json!(format!("schema/{name}\0.json"));
        fs::write(root.join("settings.json"), serde_json::to_vec(&settings)?)?;
        let expected = Workspace::discover(directory.path())?
            .list_page(
                &ItemFilter::default(),
                &ListOptions::default(),
                "2026-10-21T12:00:00.000Z",
            )
            .err()
            .ok_or("canonical list must refuse the invalid filename")?
            .to_string();
        let scratch = tempfile::tempdir()?;
        for json in [false, true] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_pm-rust"));
            command
                .current_dir(directory.path())
                .arg("--workspace")
                .arg(directory.path())
                .arg("read-session")
                .env("TMPDIR", scratch.path())
                .env("TMP", scratch.path())
                .env("TEMP", scratch.path());
            if json {
                command.arg("--json");
            }
            let result = command.output()?;
            assert_eq!(result.status.code(), Some(2));
            assert!(
                result.stdout.is_empty(),
                "refused source must emit no certificate"
            );
            let refusal: Value = serde_json::from_slice(&result.stderr)?;
            assert_eq!(refusal["code"], "session_source_refused");
            assert!(
                refusal["detail"]
                    .as_str()
                    .ok_or("missing detail")?
                    .contains(&expected),
                "admission must retain the canonical source diagnostic: {refusal}"
            );
            assert_eq!(fs::read_dir(scratch.path())?.count(), 0);
        }
    }
    Ok(())
}
