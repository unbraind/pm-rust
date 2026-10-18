//! Complete close field, history and output parity over a real disposable tracker.
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
#[path = "support/mutation_differential.rs"]
mod mutation_differential;
#[path = "support/published_cli.rs"]
mod published_cli;
use mutation_differential::{copy_directory, run_minimal, write_driver};
const CLOCK: &str = "2026-08-22T10:00:00.000Z";
type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Executes the pinned CLI under the existing fixed-clock recipe.
fn oracle(driver: &Path, root: &Path, args: &[&str]) -> Result<Output, Box<dyn std::error::Error>> {
    let mut arguments = vec![driver.to_string_lossy().into_owned()];
    arguments.extend(args.iter().map(ToString::to_string));
    run_minimal(
        &PathBuf::from(std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned())),
        &arguments,
        root,
    )
}

/// Compares canonical item/history/session artifacts without masking any differences.
fn compare_artifacts(expected: &Path, actual: &Path) -> TestResult {
    let entries =
        |root: &Path| -> Result<std::collections::BTreeSet<std::ffi::OsString>, std::io::Error> {
            fs::read_dir(root)?
                .map(|entry| entry.map(|entry| entry.file_name()))
                .filter(|name| {
                    !matches!(
                        name.as_ref().ok().and_then(|name| name.to_str()),
                        Some("cache" | "locks" | "runtime")
                    )
                })
                .collect()
        };
    assert_eq!(entries(expected)?, entries(actual)?);
    if expected.join("runtime/session.json").is_file() {
        assert_eq!(
            fs::read(expected.join("runtime/session.json"))?,
            fs::read(actual.join("runtime/session.json"))?
        );
    }
    for entry in fs::read_dir(expected)? {
        let entry = entry?;
        let path = actual.join(entry.file_name());
        if entry.path().is_dir() {
            if !matches!(
                entry.file_name().to_str(),
                Some("cache" | "locks" | "runtime")
            ) {
                compare_artifacts(&entry.path(), &path)?;
            }
        } else {
            assert_eq!(
                fs::read_to_string(entry.path())?,
                fs::read_to_string(path)?,
                "{}",
                entry.file_name().to_string_lossy()
            );
        }
    }
    Ok(())
}

/// Closes identical before-images at the same root so recovery paths compare exactly.
fn compare_close(setup: &[Vec<&str>], fields: &[&str]) -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("Close differential") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let tracker = tempfile::tempdir()?;
    for args in [
        vec!["init", "sample-", "--defaults"],
        vec![
            "create",
            "--id",
            "sample-close",
            "--title",
            "Closure fixture",
            "--type",
            "Issue",
            "--author",
            "alpha",
        ],
    ]
    .iter()
    .chain(setup)
    {
        if args[0] == "__stale_session" {
            fs::write(
                tracker.path().join(".agents/pm/locks/session-state.lock"),
                r#"{"id":"session-state","pid":1,"owner":"fixture","created_at":"2000-01-01T00:00:00Z","ttl_seconds":1800,"token":"fixture"}"#,
            )?;
            continue;
        }
        let output = oracle(&driver, tracker.path(), args)?;
        assert!(
            output.status.success(),
            "setup {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let snapshot = tempfile::tempdir()?;
    copy_directory(
        &tracker.path().join(".agents"),
        &snapshot.path().join(".agents"),
    )?;
    let mut args = vec!["close", "sample-close", "--author", "alpha"];
    args.extend(fields);
    let expected = oracle(&driver, tracker.path(), &args)?;
    let artifacts = tempfile::tempdir()?;
    copy_directory(
        &tracker.path().join(".agents"),
        &artifacts.path().join(".agents"),
    )?;
    fs::remove_dir_all(tracker.path().join(".agents"))?;
    copy_directory(
        &snapshot.path().join(".agents"),
        &tracker.path().join(".agents"),
    )?;
    let mut command = Command::new(env!("CARGO_BIN_EXE_pm-rust"));
    mutation_differential::isolate_environment(&mut command);
    let actual = command
        .current_dir(tracker.path())
        .env_remove("PM_PATH")
        .env("DO_NOT_TRACK", "1")
        .args(&args)
        .args(["--timestamp", CLOCK])
        .output()?;
    assert_eq!(
        actual.status.code(),
        expected.status.code(),
        "{fields:?}: {}",
        String::from_utf8_lossy(&actual.stderr)
    );
    compare_artifacts(
        &artifacts.path().join(".agents/pm"),
        &tracker.path().join(".agents/pm"),
    )?;
    assert_eq!(
        String::from_utf8_lossy(&actual.stdout),
        String::from_utf8_lossy(&expected.stdout),
        "{fields:?} stdout"
    );
    assert_eq!(
        String::from_utf8_lossy(&actual.stderr),
        String::from_utf8_lossy(&expected.stderr),
        "{fields:?} stderr"
    );
    Ok(())
}

/// Every published single-item close flag persists exact fields and canonical history.
#[test]
fn close_fields_aliases_and_formats_match_oracle() -> TestResult {
    for output in [vec!["--json"], vec![]] {
        for fields in [
            vec![
                "Done",
                "--resolution",
                "  Fixed  ",
                "--expected-result",
                "  Works  ",
                "--actual-result",
                "  Works  ",
                "--validate-close",
                "strict",
                "--completed-at",
                "2026-08-21T08:00:00+02:00",
                "--message",
                "Evidence",
            ],
            vec![
                "--close-reason",
                "Alias",
                "--resolution",
                "Fixed",
                "--expected",
                "Works",
                "--actual",
                "Works",
                "--validate-close",
            ],
            vec![
                "-r",
                "Short reason",
                "-m",
                "History",
                "--validate-close",
                "disabled",
                "--completed-at",
                "-1d",
            ],
            vec![
                "--resolution",
                "Reason fallback",
                "--expected",
                "",
                "--actual",
                "  ",
                "--validate-close",
                "off",
            ],
            vec!["-m", "Message fallback", "--completed-at", "now"],
            vec!["Done", "--completed-at", "2026-08-20"],
            vec![
                "",
                "--reason",
                "Canonical",
                "--close-reason",
                "Alias",
                "--expected-result",
                "Canonical",
                "--expected",
                "Alias",
                "--actual-result",
                "Canonical",
                "--actual",
                "Alias",
            ],
            vec![
                "--close-reason",
                "Alias",
                "--reason",
                "Canonical",
                "--expected",
                "Alias",
                "--expected-result",
                "Canonical",
                "--actual",
                "Alias",
                "--actual-result",
                "Canonical",
            ],
            vec![
                "--reason",
                " ",
                "--close-reason",
                "Alias",
                "--resolution",
                "First",
                "--resolution",
                "Last",
            ],
        ] {
            let mut flags = fields;
            flags.extend(&output);
            compare_close(&[], &flags)?;
        }
    }
    Ok(())
}

/// Missing evidence, invalid validation modes and completion input refuse identically.
#[test]
fn close_validation_errors_match_oracle() -> TestResult {
    for fields in [
        vec!["Done", "--validate-close", "strict", "--json"],
        vec![
            "Done",
            "--validate-close",
            "strict",
            "--resolution",
            "Fixed",
            "--expected",
            "Works",
            "--actual",
            " ",
            "--json",
        ],
        vec!["Done", "--validate-close", "invalid", "--json"],
        vec!["Done", "--completed-at", "invalid", "--json"],
        vec!["Done", "--completed-at", "2026-02-30", "--json"],
        vec!["Done", "--completed-at", "2026-13-01", "--json"],
        vec!["Done", "--completed-at", "+3d+1h", "--json"],
        vec!["Done", "--completed-at", "not-a-date", "--json"],
        vec!["Done", "--completed-at", "2026-xx-01", "--json"],
        vec!["Done", "--completed-at", "+1200000000m", "--json"],
        vec!["Done", "--completed-at", "+1q", "--json"],
        vec!["Done", "--validate-close", "warn", "--json"],
        vec!["Done", "--validate-close", "warn"],
    ] {
        compare_close(&[], &fields)?;
    }
    let absent = format!("sample-{}", "x".repeat(130));
    compare_close(&[], &["Done", "--duplicate-of", &absent, "--json"])?;
    Ok(())
}

/// Duplicate targets fill missing evidence and preserve explicit resolution overrides.
#[test]
fn close_duplicate_and_repeat_controls_match_oracle() -> TestResult {
    let canonical = vec![
        "create",
        "--id",
        "sample-canonical",
        "--title",
        "Canonical",
        "--type",
        "Task",
        "--author",
        "alpha",
    ];
    for fields in [
        vec![
            "--duplicate-of",
            "sample-canonical",
            "--validate-close",
            "strict",
            "--json",
        ],
        vec!["-d", "sample-canonical", "--resolution", "Custom", "--json"],
        vec!["--duplicate-of", "sample-close", "--json"],
        vec!["--duplicate-of", "sample-absent", "--json"],
    ] {
        compare_close(std::slice::from_ref(&canonical), &fields)?;
    }
    for fields in [vec!["Again", "--json"], vec!["Again", "--force", "--json"]] {
        compare_close(&[vec!["close", "sample-close", "First"]], &fields)?;
    }
    compare_close(
        &[vec!["claim", "sample-close", "--author", "alpha"]],
        &["Done", "--release-assignment", "--json"],
    )?;
    compare_close(
        &[
            vec!["claim", "sample-close", "--author", "alpha"],
            vec!["close", "sample-close", "First"],
        ],
        &["Again", "--force", "--release-assignment", "--json"],
    )?;
    Ok(())
}

/// Stored evidence survives blank options; strict governance validates author-controlled reasons.
#[test]
fn close_existing_evidence_children_and_governance_match_oracle() -> TestResult {
    let evidence = vec![
        "update",
        "sample-close",
        "--resolution",
        "Stored",
        "--expected-result",
        "Expected",
        "--actual-result",
        "Actual",
        "--author",
        "alpha",
    ];
    compare_close(
        std::slice::from_ref(&evidence),
        &[
            "Done",
            "--resolution",
            " ",
            "--expected-result",
            "",
            "--actual-result",
            " ",
            "--validate-close",
            "strict",
            "--json",
        ],
    )?;
    for mode in ["off", "warn", "strict"] {
        compare_close(
            &[
                evidence.clone(),
                vec![
                    "create",
                    "--id",
                    "sample-child",
                    "--title",
                    "Child",
                    "--type",
                    "Task",
                    "--parent",
                    "sample-close",
                    "--author",
                    "alpha",
                ],
            ],
            &["Done", "--validate-close", mode, "--json"],
        )?;
    }
    compare_close(
        &[vec![
            "config",
            "set",
            "governance-require-close-reason",
            "--policy",
            "enabled",
        ]],
        &["--json"],
    )?;
    compare_close(&[], &["--json"])?;
    compare_close(
        &[vec![
            "close",
            "sample-close",
            "First",
            "--completed-at",
            "2026-08-19T00:00:00.000Z",
        ]],
        &["Again", "--force", "--json"],
    )?;
    for completion in ["+2h", "-1w", "+6m", "20260820", "2026-08-21T10:00:00.123Z"] {
        compare_close(&[], &["Done", "--completed-at", completion, "--json"])?;
    }
    Ok(())
}

/// Builds an ordinary task through the oracle's create command.
fn task_arguments(id: &str) -> Vec<&str> {
    vec![
        "create",
        "--id",
        id,
        "--title",
        "Related fixture",
        "--type",
        "Task",
        "--author",
        "alpha",
    ]
}

/// Multiple children, predecessor edges and foreign ownership retain exact receipts.
#[test]
fn close_container_predecessor_and_ownership_receipts_match_oracle() -> TestResult {
    let mut children = vec![
        task_arguments("sample-child-z"),
        task_arguments("sample-child-a"),
    ];
    for child in &mut children {
        child.extend(["--parent", "sample-close"]);
    }
    compare_close(&children, &["Done", "--validate-close", "warn", "--json"])?;
    compare_close(
        &[
            task_arguments("sample-prior"),
            task_arguments("sample-other"),
            vec![
                "update",
                "sample-close",
                "--blocked-by",
                "sample-prior",
                "--blocked-reason",
                "Waiting",
                "--dep",
                "id=sample-other,kind=blocked_by",
                "--author",
                "alpha",
            ],
        ],
        &["Done", "--json"],
    )?;
    for preset in ["default", "strict"] {
        for forced in [false, true] {
            let mut fields = vec!["Done", "--json"];
            if forced {
                fields.push("--force");
            }
            compare_close(
                &[
                    vec!["config", "set", "governance-preset", "--policy", preset],
                    vec!["claim", "sample-close", "--author", "beta"],
                ],
                &fields,
            )?;
        }
    }
    Ok(())
}

/// Assignment release preserves no-op counts and reports durable partial failures.
#[test]
fn close_assignment_release_boundaries_match_oracle() -> TestResult {
    compare_close(&[], &["Done", "--release-assignment", "--json"])?;
    compare_close(&[], &["Done", "--release-assignment"])?;
    compare_close(
        &[
            vec!["claim", "sample-close", "--author", "alpha"],
            vec!["__stale_session"],
        ],
        &["Done", "--release-assignment", "--json"],
    )?;
    Ok(())
}
