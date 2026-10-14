//! Ownership conformance against the locked published CLI, including real races.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Barrier};

#[path = "support/mutation_differential.rs"]
mod mutation_differential;
#[path = "support/published_cli.rs"]
mod published_cli;
use mutation_differential::{copy_directory, run_minimal, write_driver};
use published_cli::published_cli_or_skip;
const CLOCK: &str = "2026-08-22T10:00:00.000Z";

/// Two identical disposable trackers plus the fixed-clock published driver.
struct Fixture {
    node: tempfile::TempDir,
    rust: tempfile::TempDir,
    scratch: tempfile::TempDir,
    driver: PathBuf,
}
impl Fixture {
    /// Initializes a numeric-looking title with a published record and chosen policy.
    fn new(policy: &str) -> Result<Option<Self>, Box<dyn std::error::Error>> {
        let Some(published) = published_cli_or_skip("Ownership differential") else {
            return Ok(None);
        };
        let scratch = tempfile::tempdir()?;
        let driver = write_driver(scratch.path(), &published)?;
        let node = tempfile::tempdir()?;
        for args in [
            vec!["init", "sample-", "--defaults"],
            vec![
                "create",
                "--id",
                "sample-own",
                "--title",
                "05",
                "--type",
                "Task",
                "--author",
                "alpha",
            ],
        ] {
            let output = published_run(&driver, node.path(), &args)?;
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let settings = node.path().join(".agents/pm/settings.json");
        let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&settings)?)?;
        value["telemetry"]["enabled"] = serde_json::json!(false);
        value["governance"]["preset"] = serde_json::json!("custom");
        value["governance"]["ownership_enforcement"] = serde_json::json!(policy);
        fs::write(settings, serde_json::to_vec_pretty(&value)?)?;
        let rust = tempfile::tempdir()?;
        copy_directory(&node.path().join(".agents"), &rust.path().join(".agents"))?;
        Ok(Some(Self {
            node,
            rust,
            scratch,
            driver,
        }))
    }

    /// Executes a step and compares complete envelopes, exit codes and durable bytes.
    fn step(&self, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
        let node = published_run(&self.driver, self.node.path(), args)?;
        let rust = native_command(self.rust.path(), args).output()?;
        assert_eq!(
            rust.status.code(),
            node.status.code(),
            "{args:?}: {}",
            String::from_utf8_lossy(&rust.stderr)
        );
        for (stream, native_bytes, node_bytes) in [
            ("stdout", &rust.stdout, &node.stdout),
            ("stderr", &rust.stderr, &node.stderr),
        ] {
            assert!(
                native_bytes == node_bytes,
                "{args:?} {stream}: native {}, oracle {}",
                String::from_utf8_lossy(native_bytes),
                String::from_utf8_lossy(node_bytes)
            );
        }
        self.compare_bytes()?;
        Ok(())
    }

    /// Asserts exact item and history bytes, without normalization.
    fn compare_bytes(&self) -> Result<(), Box<dyn std::error::Error>> {
        for folder in [
            "epics",
            "features",
            "tasks",
            "chores",
            "issues",
            "decisions",
            "milestones",
            "events",
            "reminders",
            "meetings",
            "plans",
            "history",
        ] {
            let relative = format!(".agents/pm/{folder}");
            let entries = |root: &Path| -> Result<
                std::collections::BTreeSet<String>,
                Box<dyn std::error::Error>,
            > {
                if !root.join(&relative).is_dir() {
                    return Ok(std::collections::BTreeSet::new());
                }
                Ok(fs::read_dir(root.join(&relative))?
                    .filter_map(Result::ok)
                    .filter(|entry| entry.path().is_file())
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .collect())
            };
            let expected = entries(self.node.path())?;
            assert_eq!(entries(self.rust.path())?, expected);
            for name in expected {
                let artifact = format!("{relative}/{name}");
                assert_eq!(
                    fs::read_to_string(self.rust.path().join(&artifact))?,
                    fs::read_to_string(self.node.path().join(&artifact))?,
                    "{artifact}"
                );
            }
        }
        let native_session = self.rust.path().join(".agents/pm/runtime/session.json");
        let oracle_session = self.node.path().join(".agents/pm/runtime/session.json");
        assert_eq!(native_session.is_file(), oracle_session.is_file());
        if oracle_session.is_file() {
            assert_eq!(fs::read(native_session)?, fs::read(oracle_session)?);
        }
        Ok(())
    }

    /// Verifies native history with the independent published verifier.
    fn verify(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Keep the driver's owning directory alive until verification finishes.
        assert!(self.scratch.path().is_dir());
        let result = published_run(
            &self.driver,
            self.rust.path(),
            &[
                "history",
                "sample-own",
                "--verify",
                "--strict-exit",
                "--json",
            ],
        )?;
        assert!(
            result.status.success(),
            "{} {}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        Ok(())
    }
}

/// Executes the real published CLI with asserted identity and a fixed clock.
fn published_run(
    driver: &Path,
    root: &Path,
    args: &[&str],
) -> Result<Output, Box<dyn std::error::Error>> {
    let interpreter =
        PathBuf::from(std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned()));
    let mut arguments = vec![driver.to_string_lossy().into_owned()];
    arguments.extend(args.iter().map(ToString::to_string));
    run_minimal(&interpreter, &arguments, root)
}

/// Builds a native process over the same fixed-clock inputs.
fn native_command(root: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pm-rust"));
    command.env_clear();
    for name in [
        "PATH",
        "HOME",
        "SystemRoot",
        "SystemDrive",
        "TEMP",
        "TMP",
        // Preserve instrumentation across the otherwise isolated native process.
        "LLVM_PROFILE_FILE",
    ] {
        if let Ok(value) = std::env::var(name) {
            command.env(name, value);
        }
    }
    command.env("DO_NOT_TRACK", "1");
    command
        .arg("--workspace")
        .arg(root)
        .args(args)
        .arg(format!("--timestamp={CLOCK}"));
    command
}

#[test]
fn claim_release_sequences_match_published_bytes_and_envelopes()
-> Result<(), Box<dyn std::error::Error>> {
    for policy in ["none", "warn", "strict"] {
        let Some(fixture) = Fixture::new(policy)? else {
            return Ok(());
        };
        for args in [
            vec!["release", "sample-own", "--author", "alpha", "--json"],
            vec![
                "claim",
                "sample-own",
                "--author",
                "alpha",
                "--message",
                "0.",
                "--json",
            ],
            vec!["claim", "sample-own", "--author", "alpha", "--json"],
            vec!["claim", "sample-own", "--author", "beta", "--json"],
            vec![
                "claim",
                "sample-own",
                "--author",
                "beta",
                "--if-available",
                "--json",
            ],
            vec!["release", "sample-own", "--author", "beta", "--json"],
            vec!["claim", "sample-own", "--author", "alpha", "--json"],
            vec![
                "claim",
                "sample-own",
                "--author",
                "beta",
                "--force",
                "--json",
            ],
            vec![
                "release",
                "sample-own",
                "--author",
                "alpha",
                "--force",
                "--message",
                "05",
                "--json",
            ],
            vec!["release", "sample-own", "--author", "alpha", "--json"],
        ] {
            fixture.step(&args)?;
        }
        fixture.verify()?;
    }
    Ok(())
}

#[test]
fn terminal_claims_and_in_progress_releases_match_published()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(());
    };
    for status in ["in_progress", "closed", "canceled"] {
        // Prepare identical fixture state with the real published update.
        let args = [
            "update",
            "sample-own",
            "--status",
            status,
            "--message",
            "fixture lifecycle transition",
            "--author",
            "alpha",
            "--json",
        ];
        let result = published_run(&fixture.driver, fixture.node.path(), &args)?;
        assert!(
            result.status.success(),
            "{} {}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        copy_directory(
            &fixture.node.path().join(".agents"),
            &fixture.rust.path().join(".agents"),
        )?;
        fixture.step(&["claim", "sample-own", "--author", "alpha", "--json"])?;
        fixture.step(&[
            "claim",
            "sample-own",
            "--author",
            "alpha",
            "--force",
            "--json",
        ])?;
        fixture.step(&["release", "sample-own", "--author", "alpha", "--json"])?;
        fixture.verify()?;
    }
    Ok(())
}

#[test]
fn two_process_claim_race_has_one_winner_and_published_refusal()
-> Result<(), Box<dyn std::error::Error>> {
    for _ in 0..6 {
        let Some(fixture) = Fixture::new("strict")? else {
            return Ok(());
        };
        let barrier = Arc::new(Barrier::new(3));
        let mut workers = Vec::new();
        for author in ["alpha", "beta"] {
            let gate = Arc::clone(&barrier);
            let root = fixture.rust.path().to_owned();
            workers.push(std::thread::spawn(move || {
                let mut process = native_command(
                    &root,
                    &["claim", "sample-own", "--author", author, "--json"],
                );
                process.stdout(Stdio::piped()).stderr(Stdio::piped());
                gate.wait();
                process
                    .spawn()?
                    .wait_with_output()
                    .map(|output| (author, output))
            }));
        }
        barrier.wait();
        let mut winners = Vec::new();
        let mut losers = Vec::new();
        for worker in workers {
            let (author, result) = worker.join().map_err(|_| "claim worker panicked")??;
            if result.status.success() {
                winners.push((author, result));
            } else {
                losers.push((author, result));
            }
        }
        assert_eq!(winners.len(), 1);
        assert_eq!(losers.len(), 1);
        let expected_win = published_run(
            &fixture.driver,
            fixture.node.path(),
            &["claim", "sample-own", "--author", winners[0].0, "--json"],
        )?;
        assert!(expected_win.status.success());
        let expected_loss = published_run(
            &fixture.driver,
            fixture.node.path(),
            &["claim", "sample-own", "--author", losers[0].0, "--json"],
        )?;
        assert_eq!(losers[0].1.status.code(), Some(4));
        assert_eq!(losers[0].1.status.code(), expected_loss.status.code());
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&losers[0].1.stderr)?,
            serde_json::from_slice::<serde_json::Value>(&expected_loss.stderr)?
        );
        fixture.compare_bytes()?;
        let history = fs::read_to_string(
            fixture
                .rust
                .path()
                .join(".agents/pm/history/sample-own.jsonl"),
        )?;
        assert_eq!(
            history.lines().count(),
            2,
            "only create and the winning claim may persist"
        );
        fixture.verify()?;
    }
    Ok(())
}

#[test]
fn active_and_stale_lock_refusals_and_force_match_published()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(());
    };
    for root in [fixture.node.path(), fixture.rust.path()] {
        let path = root.join(".agents/pm/settings.json");
        let mut settings: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
        settings["locks"]["wait_ms"] = serde_json::json!(0);
        fs::write(path, serde_json::to_vec_pretty(&settings)?)?;
    }
    for clock in [CLOCK, "2000-01-01T00:00:00.000Z"] {
        for root in [fixture.node.path(), fixture.rust.path()] {
            fs::write(
                root.join(".agents/pm/locks/sample-own.lock"),
                serde_json::to_vec(&serde_json::json!({
                    "id":"sample-own", "pid":1, "owner":"lock-holder", "created_at":clock,
                    "ttl_seconds":1800, "token":"fixture-token"
                }))?,
            )?;
        }
        fixture.step(&["claim", "sample-own", "--author", "alpha", "--json"])?;
        if clock == CLOCK {
            fixture.step(&["claim", "--next", "--author", "alpha", "--json"])?;
        }
        fixture.step(&[
            "claim",
            "sample-own",
            "--author",
            "alpha",
            "--force",
            "--json",
        ])?;
    }
    fixture.step(&["release", "sample-own", "--author", "alpha", "--json"])?;
    // Custom no-force expiry policy permits ordinary stale-lock reclamation.
    for root in [fixture.node.path(), fixture.rust.path()] {
        let path = root.join(".agents/pm/settings.json");
        let mut settings: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
        settings["governance"]["force_required_for_stale_lock"] = serde_json::json!(false);
        fs::write(path, serde_json::to_vec_pretty(&settings)?)?;
        fs::write(
            root.join(".agents/pm/locks/sample-own.lock"),
            serde_json::to_vec(&serde_json::json!({
                "id":"sample-own", "pid":1, "owner":"lock-holder", "created_at":"2000-01-01T00:00:00.000Z",
                "ttl_seconds":1800, "token":"fixture-token"
            }))?,
        )?;
    }
    fixture.step(&["claim", "sample-own", "--author", "alpha", "--json"])?;
    fixture.verify()?;
    Ok(())
}

/// Runs a fixture preparation through the real oracle and mirrors its durable tracker.
fn prepare(fixture: &Fixture, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let output = published_run(&fixture.driver, fixture.node.path(), args)?;
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    copy_directory(
        &fixture.node.path().join(".agents"),
        &fixture.rust.path().join(".agents"),
    )?;
    Ok(())
}

/// Start, pause and deprecated aliases preserve bytes under all ownership policies.
#[test]
fn composed_lifecycle_matches_published_bytes() -> Result<(), Box<dyn std::error::Error>> {
    for policy in ["none", "warn", "strict"] {
        let Some(fixture) = Fixture::new(policy)? else {
            return Ok(());
        };
        for args in [
            vec![
                "claim",
                "sample-own",
                "--start",
                "--author",
                "alpha",
                "--json",
            ],
            vec![
                "claim",
                "sample-own",
                "--start",
                "--author",
                "alpha",
                "--json",
            ],
            vec![
                "release",
                "sample-own",
                "--pause",
                "--author",
                "beta",
                "--json",
            ],
            vec![
                "release",
                "sample-own",
                "--pause",
                "--author",
                "alpha",
                "--json",
            ],
            vec![
                "start-task",
                "sample-own",
                "--author",
                "alpha",
                "--message",
                "start fixture",
                "--json",
            ],
            vec![
                "pause-task",
                "sample-own",
                "--author",
                "alpha",
                "--message",
                "pause fixture",
                "--json",
            ],
        ] {
            fixture.step(&args)?;
        }
        fixture.verify()?;
    }
    Ok(())
}

/// Repeated selection, exact filters and empty selections retain published receipts.
#[test]
fn selection_filters_and_no_candidate_receipts_match_published()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(());
    };
    fixture.step(&["claim", "--next", "--author", "alpha", "--json"])?;
    fixture.step(&["claim", "--next", "--author", "alpha", "--json"])?;
    fixture.step(&[
        "claim",
        "--next",
        "--author",
        "beta",
        "--if-available",
        "--json",
    ])?;
    fixture.step(&["claim", "--next", "--author", "beta", "--json"])?;
    fixture.step(&["release", "sample-own", "--author", "alpha", "--json"])?;
    for filter in [
        ["--type", "Issue"],
        ["--tag", "missing"],
        ["--priority", "0"],
        ["--sprint", "missing"],
        ["--release", "missing"],
    ] {
        fixture.step(&[
            "claim",
            "--next",
            filter[0],
            filter[1],
            "--if-available",
            "--author",
            "alpha",
            "--json",
        ])?;
    }
    fixture.step(&[
        "claim",
        "--next",
        "--type",
        "task",
        "--priority",
        "2",
        "--author",
        "alpha",
        "--json",
    ])?;
    fixture.verify()?;
    Ok(())
}

/// An urgent ready leaf outranks ordinary work while drafts and blocked work wait.
#[test]
fn selection_ranks_ready_work_and_excludes_live_blockers() -> Result<(), Box<dyn std::error::Error>>
{
    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(());
    };
    for args in [
        vec![
            "create",
            "--id",
            "sample-urgent",
            "--type",
            "Task",
            "--title",
            "Urgent",
            "--priority",
            "0",
            "--author",
            "alpha",
            "--json",
        ],
        vec![
            "create",
            "--id",
            "sample-blocked",
            "--type",
            "Issue",
            "--title",
            "Blocked urgent",
            "--priority",
            "0",
            "--blocked-by",
            "sample-urgent",
            "--author",
            "alpha",
            "--json",
        ],
        vec![
            "create",
            "--id",
            "sample-draft",
            "--type",
            "Task",
            "--title",
            "Draft",
            "--status",
            "draft",
            "--priority",
            "0",
            "--author",
            "alpha",
            "--json",
        ],
    ] {
        prepare(&fixture, &args)?;
    }
    let args = ["claim", "--next", "--author", "alpha", "--json"];
    let node = published_run(&fixture.driver, fixture.node.path(), &args)?;
    let rust = native_command(fixture.rust.path(), &args).output()?;
    assert_eq!(
        String::from_utf8_lossy(&rust.stdout),
        String::from_utf8_lossy(&node.stdout)
    );
    let receipt: serde_json::Value = serde_json::from_slice(&node.stdout)?;
    assert_eq!(receipt["id"], "sample-urgent");
    for id in [
        "sample-own",
        "sample-urgent",
        "sample-blocked",
        "sample-draft",
    ] {
        let folder = if id == "sample-blocked" {
            "issues"
        } else {
            "tasks"
        };
        for path in [
            format!(".agents/pm/{folder}/{id}.toon"),
            format!(".agents/pm/history/{id}.jsonl"),
        ] {
            assert_eq!(
                fs::read(fixture.rust.path().join(&path))?,
                fs::read(fixture.node.path().join(&path))?,
                "{path}"
            );
        }
    }
    Ok(())
}

/// Ambiguous flags and invalid budgets fail without durable ownership writes.
#[test]
fn lifecycle_rejects_ambiguous_start_inputs_without_writes()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(());
    };
    fixture.step(&["claim", "--author", "alpha", "--json"])?;
    fixture.step(&["claim", "--start", "--author", "alpha", "--json"])?;
    fixture.step(&[
        "claim",
        "sample-own",
        "--start",
        "--if-available",
        "--author",
        "alpha",
        "--json",
    ])?;
    fixture.step(&["claim", "--next", "--start", "--author", "alpha", "--json"])?;
    fixture.step(&[
        "claim",
        "sample-own",
        "--next",
        "--start",
        "--author",
        "alpha",
        "--json",
    ])?;
    fixture.step(&[
        "claim",
        "--next",
        "--max-attempts",
        "0",
        "--author",
        "alpha",
        "--json",
    ])?;
    fixture.step(&[
        "claim",
        "sample-own",
        "--next",
        "--author",
        "alpha",
        "--json",
    ])?;
    Ok(())
}

/// Equal-clock annotations and locale-sorted tags match complete canonical history.
#[test]
fn equal_timestamp_comments_and_punctuation_tags_match_published_storage()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(());
    };
    for args in [
        vec![
            "update",
            "sample-own",
            "--tags",
            "+1,-0,.5,Alpha,alpha,é,e",
            "--author",
            "alpha",
        ],
        vec!["comment", "sample-own", "05", "--author", "00"],
        vec!["comment", "sample-own", "0.", "--author", "fixture-agent"],
        vec!["comment", "sample-own", "0.", "--author", "00"],
    ] {
        let node = published_run(&fixture.driver, fixture.node.path(), &args)?;
        let rust = native_command(fixture.rust.path(), &args).output()?;
        assert!(
            node.status.success(),
            "{}",
            String::from_utf8_lossy(&node.stderr)
        );
        assert!(
            rust.status.success(),
            "{}",
            String::from_utf8_lossy(&rust.stderr)
        );
        fixture.compare_bytes()?;
    }
    fixture.verify()?;
    Ok(())
}

/// Detected sessions remain distinct during explicit and selected claims.
#[test]
fn detected_sessions_and_asserted_overrides_match_published_history()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(());
    };
    for (session, args) in [
        ("fixture-session-one", vec!["claim", "--next", "--json"]),
        (
            "fixture-session-one",
            vec!["release", "sample-own", "--force", "--json"],
        ),
        ("fixture-session-one", vec!["claim", "sample-own", "--json"]),
        ("fixture-session-two", vec!["claim", "sample-own", "--json"]),
        (
            "fixture-session-two",
            vec!["claim", "--next", "--if-available", "--json"],
        ),
        ("fixture-session-one", vec!["claim", "--next", "--json"]),
        (
            "fixture-session-two",
            vec!["claim", "sample-own", "--if-available", "--json"],
        ),
        (
            "fixture-session-one",
            vec!["claim", "sample-own", "--start", "--json"],
        ),
        (
            "fixture-session-one",
            vec!["release", "sample-own", "--pause", "--json"],
        ),
        (
            "fixture-session-one",
            vec![
                "claim",
                "sample-own",
                "--author",
                "asserted-agent",
                "--json",
            ],
        ),
        (
            "fixture-session-two",
            vec![
                "release",
                "sample-own",
                "--author",
                "asserted-agent",
                "--json",
            ],
        ),
    ] {
        let mut native = native_command(fixture.rust.path(), &args);
        let interpreter =
            std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned());
        let mut oracle = Command::new(interpreter);
        oracle
            .env_clear()
            .env("PATH", std::env::var("PATH")?)
            .current_dir(fixture.node.path())
            .arg(&fixture.driver)
            .args(&args)
            .env("FIXED_CLOCK", CLOCK);
        for command in [&mut native, &mut oracle] {
            for name in ["HOME", "SystemRoot", "SystemDrive", "TEMP", "TMP"] {
                if let Ok(value) = std::env::var(name) {
                    command.env(name, value);
                }
            }
            command
                .env("DO_NOT_TRACK", "1")
                .env("PI_AGENT", "1")
                .env("PI_SESSION_ID", session)
                .env("PI_MODEL", "fixture-model");
        }
        let native = native.output()?;
        let oracle = oracle.output()?;
        assert_eq!(
            native.status.code(),
            oracle.status.code(),
            "{session} {args:?}: native {}, oracle {}",
            String::from_utf8_lossy(&native.stderr),
            String::from_utf8_lossy(&oracle.stderr)
        );
        assert!(
            native.stdout == oracle.stdout,
            "native {}, oracle {}",
            String::from_utf8_lossy(&native.stdout),
            String::from_utf8_lossy(&oracle.stdout)
        );
        assert!(
            native.stderr == oracle.stderr,
            "native {}, oracle {}",
            String::from_utf8_lossy(&native.stderr),
            String::from_utf8_lossy(&oracle.stderr)
        );
        fixture.compare_bytes()?;
    }
    fixture.verify()?;
    Ok(())
}

/// A schema-file transition refusal leaves the completed claim intact.
#[test]
fn failed_start_preserves_claim_and_published_recovery_receipt()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(());
    };
    for root in [fixture.node.path(), fixture.rust.path()] {
        let path = root.join(".agents/pm/settings.json");
        let mut settings: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
        settings["governance"]["workflow_enforcement"] = serde_json::json!("strict");
        fs::write(path, serde_json::to_vec_pretty(&settings)?)?;
        fs::write(
            root.join(".agents/pm/schema/workflows.json"),
            serde_json::to_vec_pretty(
                &serde_json::json!({"type_workflows":[{"type":"Task","allowed_transitions":[]}]}),
            )?,
        )?;
    }
    fixture.step(&[
        "claim",
        "sample-own",
        "--start",
        "--author",
        "alpha",
        "--json",
    ])?;
    let history = fs::read_to_string(
        fixture
            .rust
            .path()
            .join(".agents/pm/history/sample-own.jsonl"),
    )?;
    assert_eq!(history.lines().count(), 2);
    let item = pm_rust::Workspace::discover(fixture.rust.path())?.get("sample-own")?;
    assert_eq!(item.metadata.status, "open");
    assert_eq!(item.metadata.extra["assignee"], "alpha");
    fixture.verify()?;
    Ok(())
}

/// Claim-next follows status roles from the configured schema file.
#[test]
fn selection_honors_external_status_schema() -> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(());
    };
    for root in [fixture.node.path(), fixture.rust.path()] {
        let path = root.join(".agents/pm/schema/statuses.json");
        let mut statuses: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
        let open = statuses["statuses"]
            .as_array_mut()
            .ok_or("missing statuses")?
            .iter_mut()
            .find(|status| status["id"] == "open")
            .ok_or("missing open status")?;
        open["roles"] = serde_json::json!(["draft"]);
        fs::write(path, serde_json::to_vec_pretty(&statuses)?)?;
    }
    fixture.step(&[
        "claim",
        "--next",
        "--if-available",
        "--author",
        "alpha",
        "--json",
    ])?;
    let item = pm_rust::Workspace::discover(fixture.rust.path())?.get("sample-own")?;
    assert!(!item.metadata.extra.contains_key("assignee"));
    Ok(())
}

/// Parent scopes compose with exact metadata and ownership filters.
#[test]
fn selection_parent_and_ownership_filters_compose() -> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(());
    };
    prepare(
        &fixture,
        &[
            "create",
            "--id",
            "sample-parent",
            "--title",
            "Parent",
            "--type",
            "Epic",
            "--author",
            "alpha",
            "--json",
        ],
    )?;
    prepare(
        &fixture,
        &[
            "create",
            "--id",
            "sample-child",
            "--title",
            "Child",
            "--type",
            "Task",
            "--parent",
            "sample-parent",
            "--tags",
            "native",
            "--sprint",
            "S1",
            "--release",
            "R1",
            "--author",
            "alpha",
            "--json",
        ],
    )?;
    for filters in [
        vec![
            "--type",
            "Task",
            "--tag",
            "native",
            "--sprint",
            "S1",
            "--release",
            "R1",
            "--max-attempts",
            "2",
        ],
        vec!["--assignee-filter", "unassigned", "--if-available"],
        vec!["--assignee-filter", "mine"],
        vec!["--assignee-filter", "assigned"],
        vec!["--include-containers", "--type", "Epic"],
    ] {
        let mut args = vec!["claim", "--next", "--parent", "sample-parent"];
        args.extend(filters);
        args.extend(["--author", "alpha", "--json"]);
        fixture.step(&args)?;
    }
    fixture.step(&[
        "claim",
        "--next",
        "--parent",
        "sample-absent",
        "--author",
        "alpha",
        "--json",
    ])?;
    Ok(())
}

/// Approved SDK handoff bypass applies only to the ownership-release operation.
#[test]
fn sdk_release_bypass_matches_published_without_authorizing_pause()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(());
    };
    fixture.step(&["claim", "sample-own", "--author", "alpha", "--json"])?;
    let request = pm_rust::OwnershipItem {
        id: "sample-own".to_owned(),
        author: "handoff-agent".to_owned(),
        timestamp: Some(CLOCK.to_owned()),
        message: None,
        force: false,
        if_available: false,
        provenance_role: None,
    };
    let options = pm_rust::OwnershipOptions {
        identity: None,
        release_bypass: true,
    };
    let workspace = pm_rust::Workspace::discover(fixture.rust.path())?;
    assert!(workspace.pause_task(&request, &options).is_err());
    fixture.compare_bytes()?;
    let oracle = published_run(
        &fixture.driver,
        fixture.node.path(),
        &["__release_bypass", "sample-own", "handoff-agent"],
    )?;
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let native = workspace.release_with_options(&request, &options)?;
    assert_eq!(native.previous_assignee.as_deref(), Some("alpha"));
    assert!(!native.forced);
    fixture.compare_bytes()?;
    fixture.verify()?;
    Ok(())
}

/// Human decisions and milestone gates require their explicit selection opt-ins.
#[test]
fn selection_respects_human_and_gate_execution_roles() -> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(());
    };
    for (id, item_type) in [
        ("sample-decision", "Decision"),
        ("sample-gate", "Milestone"),
    ] {
        prepare(
            &fixture,
            &[
                "create",
                "--id",
                id,
                "--type",
                item_type,
                "--title",
                "Governance fixture",
                "--author",
                "alpha",
                "--json",
            ],
        )?;
    }
    for (item_type, flag) in [
        ("Decision", "--include-decisions"),
        ("Milestone", "--include-gates"),
    ] {
        fixture.step(&[
            "claim",
            "--next",
            "--type",
            item_type,
            "--if-available",
            "--author",
            "alpha",
            "--json",
        ])?;
        fixture.step(&[
            "claim", "--next", "--type", item_type, flag, "--author", "alpha", "--json",
        ])?;
    }
    Ok(())
}

/// Concurrent selection converges on distinct items and compatible session state.
#[test]
fn competing_claim_next_processes_select_distinct_items() -> Result<(), Box<dyn std::error::Error>>
{
    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(());
    };
    prepare(
        &fixture,
        &[
            "create",
            "--id",
            "sample-backup",
            "--type",
            "Task",
            "--title",
            "Second candidate",
            "--priority",
            "3",
            "--author",
            "alpha",
            "--json",
        ],
    )?;
    let barrier = Arc::new(Barrier::new(3));
    let workers: Vec<_> = ["alpha", "beta"]
        .into_iter()
        .map(|author| {
            let gate = Arc::clone(&barrier);
            let root = fixture.rust.path().to_owned();
            std::thread::spawn(move || {
                let mut command =
                    native_command(&root, &["claim", "--next", "--author", author, "--json"]);
                gate.wait();
                command.output()
            })
        })
        .collect();
    barrier.wait();
    let mut ids = std::collections::BTreeSet::new();
    for worker in workers {
        let result = worker.join().map_err(|_| "selection worker panicked")??;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let receipt: serde_json::Value = serde_json::from_slice(&result.stdout)?;
        ids.insert(
            receipt["id"]
                .as_str()
                .ok_or("missing selected id")?
                .to_owned(),
        );
        assert_eq!(receipt["available"], true);
    }
    assert_eq!(ids.len(), 2);
    let state: serde_json::Value = serde_json::from_slice(&fs::read(
        fixture.rust.path().join(".agents/pm/runtime/session.json"),
    )?)?;
    // Mirror each successful atomic claim in the actual session publication order.
    for semantic in state["semantic_attribution"]
        .as_object()
        .ok_or("missing semantic state")?
        .values()
    {
        let id = semantic["active_item_ids"][0]
            .as_str()
            .ok_or("missing active item")?;
        let item = pm_rust::Workspace::discover(fixture.rust.path())?.get(id)?;
        let author = item.metadata.extra["assignee"]
            .as_str()
            .ok_or("missing owner")?;
        let output = published_run(
            &fixture.driver,
            fixture.node.path(),
            &["claim", id, "--author", author, "--json"],
        )?;
        assert!(output.status.success());
    }
    fixture.compare_bytes()?;
    for id in ids {
        let history = fs::read_to_string(
            fixture
                .rust
                .path()
                .join(format!(".agents/pm/history/{id}.jsonl")),
        )?;
        assert_eq!(history.lines().count(), 2);
    }
    Ok(())
}
