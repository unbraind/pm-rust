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
                    fs::read(self.rust.path().join(&artifact))?,
                    fs::read(self.node.path().join(&artifact))?,
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
    mutation_differential::isolate_environment(&mut command);
    command.env("DO_NOT_TRACK", "1");
    // Match the oracle's invocation scope so recovery bytes compare the same operation.
    command
        .current_dir(root)
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
        // The loser must wait for publication to observe the committed claim.
        for root in [fixture.node.path(), fixture.rust.path()] {
            let path = root.join(".agents/pm/settings.json");
            let mut settings: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
            settings["locks"]["wait_ms"] = serde_json::json!(30_000);
            fs::write(path, serde_json::to_vec_pretty(&settings)?)?;
        }
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
    compare_ranking_result(&fixture, &rust, &node)?;
    let receipt: serde_json::Value = serde_json::from_slice(&node.stdout)?;
    assert_eq!(receipt["id"], "sample-urgent");
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
        let mut oracle = published_cli::oracle_command(interpreter);
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
    fixture.step(&["claim", "sample-own", "--start", "--author", "alpha"])?;
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
        start: false,
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
    // Both writers are expected to succeed even when runner I/O exceeds the default wait.
    for root in [fixture.node.path(), fixture.rust.path()] {
        let path = root.join(".agents/pm/settings.json");
        let mut settings: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
        settings["locks"]["wait_ms"] = serde_json::json!(30_000);
        fs::write(path, serde_json::to_vec_pretty(&settings)?)?;
    }
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

/// Node decodes OS environment strings lossily, including known identity values.
#[cfg(unix)]
#[test]
fn non_unicode_environment_matches_published_identity() -> Result<(), Box<dyn std::error::Error>> {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(());
    };
    for (key, value) in [
        (
            OsString::from_vec(b"UNRELATED_\xff".to_vec()),
            OsString::from("value"),
        ),
        (
            OsString::from("UNRELATED"),
            OsString::from_vec(b"value_\xff".to_vec()),
        ),
        (
            OsString::from("PM_AUTHOR"),
            OsString::from_vec(b"alpha_\xff".to_vec()),
        ),
    ] {
        for action in ["claim", "release"] {
            let args = [action, "sample-own", "--json"];
            let mut node = published_cli::oracle_command(
                std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned()),
            );
            node.env_clear();
            for name in ["PATH", "HOME"] {
                if let Some(value) = std::env::var_os(name) {
                    node.env(name, value);
                }
            }
            let oracle = node
                .current_dir(fixture.node.path())
                .env("DO_NOT_TRACK", "1")
                .env("FIXED_CLOCK", CLOCK)
                .env(&key, &value)
                .arg(&fixture.driver)
                .args(args)
                .output()?;
            let native = native_command(fixture.rust.path(), &args)
                .env(&key, &value)
                .output()?;
            assert!(
                oracle.status.success(),
                "{}",
                String::from_utf8_lossy(&oracle.stderr)
            );
            assert_eq!(native.status.code(), oracle.status.code());
            assert_eq!(native.stdout, oracle.stdout);
            assert_eq!(native.stderr, oracle.stderr);
            fixture.compare_bytes()?;
        }
    }
    fixture.verify()?;
    Ok(())
}

/// The oracle requires force for expired session locks, even under minimal policy.
#[test]
fn stale_session_lock_refusal_matches_published_policy() -> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("none")? else {
        return Ok(());
    };
    let lock = serde_json::to_vec(&serde_json::json!({
        "id":"session-state", "pid":1, "owner":"session-state",
        "created_at":"2000-01-01T00:00:00.000Z", "ttl_seconds":1800,
        "token":"stale-session-fixture"
    }))?;
    for root in [fixture.node.path(), fixture.rust.path()] {
        fs::write(root.join(".agents/pm/locks/session-state.lock"), &lock)?;
    }
    let workspace = pm_rust::Workspace::discover(fixture.rust.path())?;
    let request = pm_rust::OwnershipItem {
        id: "sample-own".to_owned(),
        author: "alpha".to_owned(),
        timestamp: Some(CLOCK.to_owned()),
        message: None,
        force: false,
        if_available: false,
        provenance_role: Some("implementer".to_owned()),
    };
    for action in ["claim", "release"] {
        let oracle = published_run(
            &fixture.driver,
            fixture.node.path(),
            &["__ownership_error", action, "sample-own"],
        )?;
        assert!(oracle.status.success());
        let refusal: serde_json::Value = serde_json::from_slice(&oracle.stdout)?;
        if action == "claim" {
            assert_eq!(refusal, serde_json::json!({"ok":true}));
            let receipt =
                workspace.claim_with_options(&request, &pm_rust::OwnershipOptions::default())?;
            assert_eq!(receipt.warnings, ["claim_semantic_attribution_unrecorded"]);
        } else {
            assert_eq!(refusal["exit_code"], 4);
            assert_eq!(
                refusal["detail"],
                "Item session-state lock is stale; rerun with --force when supported for this command"
            );
            assert!(
                matches!(workspace.release_with_options(&request,&pm_rust::OwnershipOptions::default()),Err(pm_rust::PmRustError::CommandRefusal{detail,exit_code:4,context}) if detail == refusal["detail"].as_str().unwrap_or_default() && context == refusal["context"])
            );
        }
        for root in [fixture.node.path(), fixture.rust.path()] {
            assert_eq!(
                fs::read(root.join(".agents/pm/locks/session-state.lock"))?,
                lock
            );
        }
        fixture.compare_bytes()?;
    }
    fixture.verify()?;
    Ok(())
}

/// SDK start rejects ifAvailable before claiming, regardless of owner or policy.
#[test]
fn sdk_start_if_available_refuses_without_mutation() -> Result<(), Box<dyn std::error::Error>> {
    for policy in ["none", "warn", "strict"] {
        let Some(fixture) = Fixture::new(policy)? else {
            return Ok(());
        };
        fixture.step(&["claim", "sample-own", "--author", "alpha", "--json"])?;
        let workspace = pm_rust::Workspace::discover(fixture.rust.path())?;
        let paths = [
            "tasks/sample-own.toon",
            "history/sample-own.jsonl",
            "runtime/session.json",
        ];
        let before = paths
            .iter()
            .map(|path| fs::read(fixture.rust.path().join(".agents/pm").join(path)))
            .collect::<Result<Vec<_>, _>>()?;
        for (id, author) in [
            ("sample-own", "beta"),
            ("sample-own", "alpha"),
            ("sample-absent", "beta"),
        ] {
            let output = published_run(
                &fixture.driver,
                fixture.node.path(),
                &["__start_if_available", id, author],
            )?;
            assert!(output.status.success());
            let oracle: serde_json::Value = serde_json::from_slice(&output.stdout)?;
            let request = pm_rust::OwnershipItem {
                id: id.to_owned(),
                author: author.to_owned(),
                timestamp: Some(CLOCK.to_owned()),
                message: None,
                force: false,
                if_available: true,
                provenance_role: None,
            };
            let error = workspace
                .start_task(&request, &pm_rust::OwnershipOptions::default())
                .err()
                .ok_or("SDK start must refuse ifAvailable")?;
            let pm_rust::PmRustError::CommandRefusal {
                detail,
                exit_code,
                mut context,
            } = error
            else {
                return Err(format!("unexpected native refusal: {error}").into());
            };
            assert_eq!(detail, oracle["detail"]);
            assert_eq!(exit_code, oracle["exit_code"]);
            let steps = context
                .as_object_mut()
                .ok_or("missing native context")?
                .remove("next_steps")
                .ok_or("missing native next steps")?;
            context["nextSteps"] = steps;
            assert_eq!(context, oracle["context"]);
            fixture.compare_bytes()?;
            for (path, bytes) in paths.iter().zip(&before) {
                assert_eq!(
                    &fs::read(fixture.rust.path().join(".agents/pm").join(path))?,
                    bytes
                );
            }
        }
        fixture.verify()?;
    }
    Ok(())
}

/// Indexing lineage preserves the oracle's ancestor evidence and session bytes.
#[test]
fn deep_lineage_session_bytes_match_published() -> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(());
    };
    for level in 0..12 {
        let id = format!("sample-a{level:02}");
        let parent = format!("sample-a{:02}", level - 1);
        let mut args = vec![
            "create",
            "--id",
            &id,
            "--type",
            "Task",
            "--title",
            "Lineage fixture",
            "--author",
            "alpha",
            "--json",
        ];
        if level > 0 {
            args.extend(["--parent", &parent]);
        }
        prepare(&fixture, &args)?;
    }
    fixture.step(&["claim", "sample-a11", "--author", "alpha", "--json"])?;
    fixture.step(&["release", "sample-a11", "--author", "alpha", "--json"])?;
    fixture.step(&[
        "claim",
        "--next",
        "--parent",
        "SAMPLE-A00",
        "--author",
        "alpha",
        "--json",
    ])?;
    fixture.verify()?;
    Ok(())
}

/// Compares every observable ranking outcome and all ownership-derived state.
fn compare_ranking_result(
    fixture: &Fixture,
    native: &Output,
    oracle: &Output,
) -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(native.status.code(), oracle.status.code());
    assert_eq!(native.stderr, oracle.stderr);
    assert_eq!(native.stdout, oracle.stdout);
    fixture.compare_bytes()
}

/// Negative controls demonstrate detection of each former ranking blind spot.
#[test]
fn ranking_comparison_rejects_exit_stderr_and_session_divergence()
-> Result<(), Box<dyn std::error::Error>> {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(());
    };
    let args = ["claim", "--next", "--author", "alpha", "--json"];
    let oracle = published_run(&fixture.driver, fixture.node.path(), &args)?;
    let native = native_command(fixture.rust.path(), &args).output()?;
    assert!(oracle.status.success());
    compare_ranking_result(&fixture, &native, &oracle)?;
    let mut divergent = native.clone();
    divergent.status = native_command(fixture.rust.path(), &["invalid-subcommand"])
        .output()?
        .status;
    assert!(!divergent.status.success());
    assert!(
        catch_unwind(AssertUnwindSafe(|| compare_ranking_result(
            &fixture, &divergent, &oracle
        )))
        .is_err(),
        "ranking comparison must reject differing exit codes"
    );
    divergent = native.clone();
    divergent.stderr.extend(b"synthetic divergence");
    assert!(
        catch_unwind(AssertUnwindSafe(|| compare_ranking_result(
            &fixture, &divergent, &oracle
        )))
        .is_err(),
        "ranking comparison must reject differing stderr"
    );
    let session = fixture.rust.path().join(".agents/pm/runtime/session.json");
    let original = fs::read(&session)?;
    fs::write(&session, "{}")?;
    let result = catch_unwind(AssertUnwindSafe(|| {
        compare_ranking_result(&fixture, &native, &oracle)
    }));
    fs::write(session, original)?;
    assert!(
        result.is_err(),
        "ranking comparison must reject differing session bytes"
    );
    fixture.compare_bytes()?;
    Ok(())
}

/// Claim attribution failures remain warnings while composed starts finish durably.
#[test]
fn start_semantic_recording_warns_and_matches_published_state()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("none")? else {
        return Ok(());
    };
    let lock = serde_json::to_vec(
        &serde_json::json!({"id":"session-state","pid":1,"owner":"session-state","created_at":"2000-01-01T00:00:00.000Z","ttl_seconds":1800,"token":"stale-session-fixture"}),
    )?;
    for root in [fixture.node.path(), fixture.rust.path()] {
        fs::write(root.join(".agents/pm/locks/session-state.lock"), &lock)?;
    }
    let oracle = published_run(
        &fixture.driver,
        fixture.node.path(),
        &["__ownership_error", "start", "sample-own"],
    )?;
    assert!(oracle.status.success());
    assert_eq!(oracle.stderr, b"");
    assert_eq!(oracle.stdout, b"{\"ok\":true}\n");
    let workspace = pm_rust::Workspace::discover(fixture.rust.path())?;
    let request = pm_rust::OwnershipItem {
        id: "sample-own".to_owned(),
        author: "alpha".to_owned(),
        timestamp: Some(CLOCK.to_owned()),
        message: None,
        force: false,
        if_available: false,
        provenance_role: None,
    };
    let result = workspace.start_task(&request, &pm_rust::OwnershipOptions::default())?;
    assert_eq!(result["status"], "in_progress");
    assert_eq!(result["changed_field_count"], 3);
    assert_eq!(
        result["claim"]["warnings"],
        serde_json::json!(["claim_semantic_attribution_unrecorded"])
    );
    fixture.compare_bytes()?;
    fixture.step(&[
        "claim",
        "sample-own",
        "--start",
        "--author",
        "alpha",
        "--json",
    ])?;
    let item = workspace.get("sample-own")?;
    assert_eq!(item.metadata.status, "in_progress");
    assert_eq!(item.metadata.extra["assignee"], "alpha");
    assert_eq!(item.metadata.extra["claim_principal"], "alpha");
    assert_eq!(
        fs::read_to_string(workspace.pm_root().join("history/sample-own.jsonl"))?
            .lines()
            .count(),
        3
    );
    for root in [fixture.node.path(), fixture.rust.path()] {
        assert_eq!(
            fs::read(root.join(".agents/pm/locks/session-state.lock"))?,
            lock
        );
        assert!(!root.join(".agents/pm/runtime/session.json").exists());
    }
    fixture.verify()?;
    Ok(())
}

/// Expired derived-state locks retain exact CLI diagnostics after durable writes.
#[test]
fn stale_session_lock_json_matches_published_bytes() -> Result<(), Box<dyn std::error::Error>> {
    for action in ["claim", "release", "start-task", "pause-task"] {
        let Some(fixture) = Fixture::new("none")? else {
            return Ok(());
        };
        let lock = r#"{"id":"session-state","pid":1,"token":"session-fixture","ttl_seconds":1800,"created_at":"2000-01-01T00:00:00.000Z","owner":"session-state"}"#;
        for root in [fixture.node.path(), fixture.rust.path()] {
            fs::write(root.join(".agents/pm/locks/session-state.lock"), lock)?;
        }
        fixture.step(&[action, "sample-own", "--author", "alpha", "--json"])?;
    }
    Ok(())
}

/// Cyclic containers include the active node reached again through a parent cycle.
#[test]
fn cyclic_parent_readiness_matches_published() -> Result<(), Box<dyn std::error::Error>> {
    for statuses in [
        vec!["open"],
        vec!["open", "closed"],
        vec!["open", "open"],
        vec!["closed", "closed"],
    ] {
        let Some(fixture) = Fixture::new("none")? else {
            return Ok(());
        };
        if statuses.len() > 1 {
            prepare(
                &fixture,
                &[
                    "create",
                    "--id",
                    "sample-other",
                    "--title",
                    "Cycle",
                    "--type",
                    "Task",
                    "--author",
                    "alpha",
                ],
            )?;
        }
        for root in [fixture.node.path(), fixture.rust.path()] {
            for (index, status) in statuses.iter().enumerate() {
                let id = if index == 0 {
                    "sample-own"
                } else {
                    "sample-other"
                };
                let parent = if statuses.len() == 1 || index == 1 {
                    "sample-own"
                } else {
                    "sample-other"
                };
                let path = root.join(format!(".agents/pm/tasks/{id}.toon"));
                let bytes = fs::read_to_string(&path)?
                    .replace("status: open", &format!("status: {status}"));
                fs::write(path, format!("{bytes}parent: {parent}\n"))?;
            }
        }
        fixture.step(&["claim", "--next", "--author", "alpha", "--json"])?;
        fixture.step(&[
            "claim",
            "--next",
            "--include-containers",
            "--if-available",
            "--author",
            "alpha",
            "--json",
        ])?;
    }
    Ok(())
}

/// Semantic lineage stops at sixteen resolved items, missing ancestors and cycles.
#[test]
fn lineage_cap_and_unresolved_stop_match_published() -> Result<(), Box<dyn std::error::Error>> {
    for shape in ["deep", "missing", "cycle"] {
        let Some(fixture) = Fixture::new("none")? else {
            return Ok(());
        };
        for level in 0..19 {
            let id = format!("sample-l{level:02}");
            prepare(
                &fixture,
                &[
                    "create", "--id", &id, "--title", "Lineage", "--type", "Task", "--author",
                    "alpha",
                ],
            )?;
            for root in [fixture.node.path(), fixture.rust.path()] {
                let path = root.join(format!(".agents/pm/tasks/{id}.toon"));
                let parent = if level > 0 {
                    format!("sample-l{:02}", level - 1)
                } else if shape == "cycle" {
                    "sample-l18".to_owned()
                } else {
                    "sample-missing".to_owned()
                };
                let bytes = fs::read_to_string(&path)?;
                fs::write(path, format!("{bytes}parent: {parent}\n"))?;
            }
        }
        let id = if shape == "missing" {
            "sample-l01"
        } else {
            "sample-l18"
        };
        fixture.step(&["claim", id, "--author", "alpha", "--json"])?;
        fixture.step(&["release", id, "--author", "alpha", "--json"])?;
    }
    Ok(())
}

/// Kernel directory events detect escaped transient locks even after their removal.
#[cfg(target_os = "linux")]
#[test]
fn malformed_mutation_ids_never_create_escaped_locks() -> Result<(), Box<dyn std::error::Error>> {
    use rustix::fs::inotify;
    use std::mem::MaybeUninit;
    let Some(fixture) = Fixture::new("none")? else {
        return Ok(());
    };
    for root in [fixture.node.path(), fixture.rust.path()] {
        let watcher =
            inotify::init(inotify::CreateFlags::NONBLOCK | inotify::CreateFlags::CLOEXEC)?;
        for path in [
            root.to_path_buf(),
            root.join(".agents"),
            root.join(".agents/pm"),
            root.join(".agents/pm/locks"),
        ] {
            inotify::add_watch(
                &watcher,
                path,
                inotify::WatchFlags::CREATE
                    | inotify::WatchFlags::DELETE
                    | inotify::WatchFlags::CLOSE_WRITE
                    | inotify::WatchFlags::MOVED_TO,
            )?;
        }
        for id in [
            "../escaped",
            "../../escaped",
            "bad/id",
            r"bad\id",
            "bad:id",
            "bad id",
            ".",
            "..",
        ] {
            for action in [
                "claim",
                "release",
                "start-task",
                "pause-task",
                "update",
                "comments",
                "close",
            ] {
                let mut args = vec![action, id, "--author", "alpha"];
                match action {
                    "update" => args.extend(["--title", "Invalid"]),
                    "comments" => args.push("Invalid"),
                    "close" => args.extend(["--reason", "Invalid"]),
                    _ => {}
                }
                let output = if root == fixture.node.path() {
                    published_run(&fixture.driver, root, &args)?
                } else {
                    if action == "comments" {
                        args[0] = "comment";
                    }
                    native_command(root, &args).output()?
                };
                assert!(!output.status.success(), "accepted {action} {id}");
            }
        }
        let mut buffer = [MaybeUninit::uninit(); 4096];
        let mut events = inotify::Reader::new(watcher, &mut buffer);
        assert!(
            matches!(events.next(), Err(rustix::io::Errno::WOULDBLOCK)),
            "malformed identifiers produced filesystem mutation events"
        );
    }
    fixture.step(&["claim", "sample-own", "--author", "alpha", "--json"])?;
    fixture.step(&["release", "sample-own", "--author", "alpha", "--json"])?;
    Ok(())
}

/// An incumbent gate without a published payload produces the exact oracle conflict.
#[test]
fn unpublished_lock_payload_refusals_match_oracle() -> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(());
    };
    for root in [fixture.node.path(), fixture.rust.path()] {
        let path = root.join(".agents/pm/settings.json");
        let mut settings: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
        settings["locks"]["wait_ms"] = serde_json::json!(0);
        fs::write(path, serde_json::to_vec_pretty(&settings)?)?;
        fs::create_dir(root.join(".agents/pm/locks/sample-own.lock.stale-cleanup"))?;
    }
    let lease = fs::File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(
            fixture
                .rust
                .path()
                .join(".agents/pm/locks/sample-own.lock.gate-lease"),
        )?;
    lease.try_lock()?;
    for payload in [None, Some(""), Some(" \n\t")] {
        for root in [fixture.node.path(), fixture.rust.path()] {
            let path = root.join(".agents/pm/locks/sample-own.lock");
            if let Some(raw) = payload {
                fs::write(path, raw)?;
            }
        }
        let args = ["claim", "sample-own", "--author", "alpha", "--json"];
        let expected = published_run(&fixture.driver, fixture.node.path(), &args)?;
        assert!(!expected.status.success());
        assert!(String::from_utf8_lossy(&expected.stderr).contains("is locked"));
        fixture.step(&args)?;
    }
    fs::File::unlock(&lease)?;
    Ok(())
}

/// All ownership refusals preserve explicit scope and redact URL credentials centrally.
#[test]
fn scoped_refusals_redact_credentials_without_changing_durable_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(());
    };
    fixture.step(&["claim", "sample-own", "--author", "alpha", "--json"])?;
    for message in [
        "ordinary message",
        "contact@example.invalid",
        "https://example.invalid/public",
        "https://fixture:test-password@example.invalid/path",
        "https://fixture:quoted$pass`word@example.invalid/path",
    ] {
        for action in ["claim", "release"] {
            let args = [
                "--pm-path",
                ".agents/pm",
                action,
                "sample-own",
                "--author",
                "beta",
                "--message",
                message,
                "--json",
            ];
            fixture.step(&args)?;
            fixture.step(&args[..args.len() - 1])?;
            let output = native_command(fixture.rust.path(), &args).output()?;
            assert_eq!(output.status.code(), Some(4));
            let envelope: serde_json::Value = serde_json::from_slice(&output.stderr)?;
            assert_eq!(envelope["recovery"]["normalized_args"][0], "--pm-path");
            assert_eq!(envelope["recovery"]["normalized_args"][1], ".agents/pm");
            assert!(String::from_utf8(output.stderr)?.contains("--json"));
        }
    }
    fixture.step(&[
        "--pm-path",
        ".agents/pm",
        "close",
        "--author",
        "alpha",
        "sample-own",
        "--reason",
        "Done",
        "--validate-close",
        "strict",
        "--json",
    ])?;
    fixture.step(&[
        "claim",
        "sample-own",
        "--author",
        "https://fixture:stored-password@example.invalid",
        "--force",
        "--json",
    ])?;
    fixture.step(&["claim", "sample-own", "--author", "beta", "--json"])?;
    fixture.verify()?;
    Ok(())
}

/// Refusal provenance distinguishes an explicit assignment from unmatched stored ownership.
#[test]
fn claim_refusal_uses_matching_latest_ownership_evidence() -> Result<(), Box<dyn std::error::Error>>
{
    let Some(fixture) = Fixture::new("none")? else {
        return Ok(());
    };
    prepare(
        &fixture,
        &[
            "update",
            "sample-own",
            "--assignee",
            "alpha",
            "--author",
            "alpha",
            "--json",
        ],
    )?;
    fixture.step(&["claim", "sample-own", "--author", "beta", "--json"])?;
    for root in [fixture.node.path(), fixture.rust.path()] {
        let path = root.join(".agents/pm/tasks/sample-own.toon");
        let bytes = fs::read_to_string(&path)?;
        let updated = bytes
            .replace("assignee: alpha", "assignee: gamma")
            .replace("assignee: \"alpha\"", "assignee: \"gamma\"");
        assert_ne!(
            updated, bytes,
            "the unmatched-owner fixture must change the stored assignee"
        );
        fs::write(path, updated)?;
    }
    fixture.step(&["claim", "sample-own", "--author", "beta", "--json"])?;
    Ok(())
}

/// Registry-defined terminal states refuse atomic starts under the claim lock without writes.
#[test]
fn atomic_start_rejects_configured_terminal_status_like_oracle()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("none")? else {
        return Ok(());
    };
    for root in [fixture.node.path(), fixture.rust.path()] {
        let path = root.join(".agents/pm/settings.json");
        let mut settings: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
        settings["schema"]["workflow"]["close_status"] = serde_json::json!("done");
        settings["schema"]["statuses"]
            .as_array_mut()
            .ok_or("statuses")?
            .push(serde_json::json!({"id":"done","roles":["terminal_done","default_close"]}));
        fs::write(&path, serde_json::to_vec_pretty(&settings)?)?;
        fs::write(
            root.join(".agents/pm/schema/statuses.json"),
            serde_json::to_vec(&serde_json::json!({"statuses":settings["schema"]["statuses"]}))?,
        )?;
        fs::write(
            root.join(".agents/pm/schema/workflows.json"),
            serde_json::to_vec(&serde_json::json!({"workflow":settings["schema"]["workflow"]}))?,
        )?;
    }
    prepare(
        &fixture,
        &[
            "close",
            "sample-own",
            "--reason",
            "Completed",
            "--author",
            "alpha",
            "--validate-close",
            "off",
            "--json",
        ],
    )?;
    let workspace = pm_rust::Workspace::discover(fixture.rust.path())?;
    assert_eq!(workspace.get("sample-own")?.metadata.status, "done");
    for force in [false, true] {
        let output = published_run(
            &fixture.driver,
            fixture.node.path(),
            &[
                "__atomic_claim_error",
                "sample-own",
                if force { "true" } else { "false" },
            ],
        )?;
        assert!(output.status.success());
        let expected: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        let request = pm_rust::OwnershipItem {
            id: "sample-own".to_owned(),
            author: "alpha".to_owned(),
            timestamp: Some(CLOCK.to_owned()),
            message: None,
            force,
            if_available: false,
            provenance_role: None,
        };
        let error = workspace
            .claim_with_options(
                &request,
                &pm_rust::OwnershipOptions {
                    start: true,
                    ..Default::default()
                },
            )
            .err()
            .ok_or("terminal start admitted")?;
        let pm_rust::PmRustError::OwnershipRefusal {
            detail,
            mut context,
        } = error
        else {
            return Err(format!("unexpected error: {error}").into());
        };
        if let Some(steps) = context
            .as_object_mut()
            .and_then(|fields| fields.shift_remove("next_steps"))
        {
            context["nextSteps"] = steps;
        }
        assert_eq!(detail, expected["detail"]);
        assert_eq!(context, expected["context"]);
        assert_eq!(expected["exit_code"], 4);
        fixture.compare_bytes()?;
    }
    fixture.verify()?;
    Ok(())
}

/// Creates a real open prerequisite and a tagged target, preserving published history.
fn blocked_start_fixture(mode: &str) -> Result<Option<Fixture>, Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(None);
    };
    prepare(
        &fixture,
        &[
            "create",
            "--id",
            "sample-gate",
            "--title",
            "Gate",
            "--type",
            "Task",
            "--author",
            "alpha",
            "--json",
        ],
    )?;
    prepare(
        &fixture,
        &[
            "update",
            "sample-own",
            "--blocked-by",
            "sample-gate",
            "--tags",
            "start-target",
            "--author",
            "alpha",
            "--json",
        ],
    )?;
    for root in [fixture.node.path(), fixture.rust.path()] {
        let path = root.join(".agents/pm/settings.json");
        let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
        value["governance"]["start_blocker_enforcement"] = serde_json::json!(mode);
        fs::write(path, serde_json::to_vec_pretty(&value)?)?;
    }
    Ok(Some(fixture))
}
/// Captures canonical files and session state independently of derived read caches and locks.
fn start_snapshot(
    root: &Path,
) -> Result<std::collections::BTreeMap<PathBuf, Vec<u8>>, Box<dyn std::error::Error>> {
    fn visit(
        root: &Path,
        path: &Path,
        out: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path.strip_prefix(root)?;
            if path.is_dir() {
                if relative.components().count() == 1
                    && [
                        "runtime",
                        "cache",
                        "locks",
                        "search",
                        "checkpoints",
                        "transactions",
                    ]
                    .contains(&entry.file_name().to_string_lossy().as_ref())
                {
                    continue;
                }
                visit(root, &path, out)?;
            } else {
                out.insert(relative.to_owned(), fs::read(path)?);
            }
        }
        Ok(())
    }
    let root = root.join(".agents/pm");
    let mut out = std::collections::BTreeMap::new();
    visit(&root, &root, &mut out)?;
    let session = root.join("runtime/session.json");
    if session.is_file() {
        out.insert(PathBuf::from("runtime/session.json"), fs::read(session)?);
    }
    Ok(out)
}
/// Exercises the actual atomic SDK transaction, including its no-write refusal boundary.
fn assert_atomic_blocker_refusal(fixture: &Fixture) -> Result<(), Box<dyn std::error::Error>> {
    let before = start_snapshot(fixture.node.path())?;
    let output = published_run(
        &fixture.driver,
        fixture.node.path(),
        &["__atomic_claim_error", "sample-own", "false"],
    )?;
    assert!(output.status.success());
    let expected: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(expected["exit_code"], 4);
    let workspace = pm_rust::Workspace::discover(fixture.rust.path())?;
    let request = pm_rust::OwnershipItem {
        id: "sample-own".to_owned(),
        author: "alpha".to_owned(),
        timestamp: Some(CLOCK.to_owned()),
        message: None,
        force: false,
        if_available: false,
        provenance_role: None,
    };
    let error = workspace
        .claim_with_options(
            &request,
            &pm_rust::OwnershipOptions {
                start: true,
                ..Default::default()
            },
        )
        .err()
        .ok_or("blocked atomic start admitted")?;
    let pm_rust::PmRustError::OwnershipRefusal {
        detail,
        mut context,
    } = error
    else {
        return Err(format!("unexpected refusal: {error}").into());
    };
    if let Some(steps) = context
        .as_object_mut()
        .and_then(|fields| fields.shift_remove("next_steps"))
    {
        context["nextSteps"] = steps;
    }
    assert_eq!(detail, expected["detail"]);
    assert_eq!(context, expected["context"]);
    assert_eq!(start_snapshot(fixture.node.path())?, before);
    assert_eq!(start_snapshot(fixture.rust.path())?, before);
    fixture.compare_bytes()?;
    Ok(())
}
/// Explicit claim/start judges the blocker before publishing ownership or semantic attribution.
#[test]
fn explicit_start_blocker_refusal_matches_oracle_without_writes()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = blocked_start_fixture("strict")? else {
        return Ok(());
    };
    let before = start_snapshot(fixture.node.path())?;
    fixture.step(&[
        "claim",
        "sample-own",
        "--start",
        "--author",
        "alpha",
        "--json",
    ])?;
    assert_eq!(start_snapshot(fixture.node.path())?, before);
    assert_eq!(start_snapshot(fixture.rust.path())?, before);
    fixture.step(&["claim", "sample-own", "--start", "--author", "alpha"])?;
    assert_eq!(start_snapshot(fixture.rust.path())?, before);
    assert_atomic_blocker_refusal(&fixture)?;
    Ok(())
}
/// Ranked starts exclude current blockers, and their atomic writer refuses a stale ready candidate.
#[test]
fn ranked_start_blocker_refusal_matches_oracle_without_writes()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = blocked_start_fixture("strict")? else {
        return Ok(());
    };
    let before = start_snapshot(fixture.node.path())?;
    fixture.step(&[
        "claim",
        "--next",
        "--start",
        "--tag",
        "start-target",
        "--author",
        "alpha",
        "--json",
    ])?;
    assert_eq!(start_snapshot(fixture.node.path())?, before);
    assert_eq!(start_snapshot(fixture.rust.path())?, before);
    assert_atomic_blocker_refusal(&fixture)?;
    Ok(())
}
/// Configures coincident workflow statuses through both supported schema layers.
fn progress_fallback_fixture() -> Result<Option<Fixture>, Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(None);
    };
    for root in [fixture.node.path(), fixture.rust.path()] {
        let path = root.join(".agents/pm/settings.json");
        let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
        value["schema"]["workflow"]["open_status"] = serde_json::json!("open");
        value["schema"]["workflow"]["in_progress_status"] = serde_json::json!("open");
        fs::write(&path, serde_json::to_vec_pretty(&value)?)?;
        fs::write(
            root.join(".agents/pm/schema/workflows.json"),
            serde_json::to_vec(&serde_json::json!({"workflow":value["schema"]["workflow"]}))?,
        )?;
    }
    Ok(Some(fixture))
}
/// Explicit compositions use the registered progress fallback rather than an open no-op.
#[test]
fn explicit_start_progress_fallback_matches_published_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = progress_fallback_fixture()? else {
        return Ok(());
    };
    fixture.step(&[
        "claim",
        "sample-own",
        "--start",
        "--author",
        "alpha",
        "--json",
    ])?;
    assert_eq!(
        pm_rust::Workspace::discover(fixture.rust.path())?
            .get("sample-own")?
            .metadata
            .status,
        "in_progress"
    );
    fixture.verify()?;
    Ok(())
}
/// The selected atomic transition resolves the identical fallback in its single claim write.
#[test]
fn ranked_start_progress_fallback_matches_published_bytes() -> Result<(), Box<dyn std::error::Error>>
{
    let Some(fixture) = progress_fallback_fixture()? else {
        return Ok(());
    };
    fixture.step(&["claim", "--next", "--start", "--author", "alpha", "--json"])?;
    assert_eq!(
        pm_rust::Workspace::discover(fixture.rust.path())?
            .get("sample-own")?
            .metadata
            .status,
        "in_progress"
    );
    fixture.verify()?;
    Ok(())
}
/// Explicit and atomic starts share strict-force, warning and off policy without duplicate warnings.
#[test]
fn start_blocker_enforcement_modes_match_published_bytes() -> Result<(), Box<dyn std::error::Error>>
{
    for (mode, force) in [("warn", false), ("off", false), ("strict", true)] {
        let Some(fixture) = blocked_start_fixture(mode)? else {
            return Ok(());
        };
        let mut args = vec![
            "claim",
            "sample-own",
            "--start",
            "--author",
            "alpha",
            "--json",
        ];
        if force {
            args.push("--force");
        }
        fixture.step(&args)?;
        fixture.verify()?;
    }
    Ok(())
}

/// The pause composition also runs start admission when its open target is the progress fallback.
#[test]
fn pause_progress_fallback_admission_matches_published_no_write()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = blocked_start_fixture("strict")? else {
        return Ok(());
    };
    prepare(
        &fixture,
        &[
            "update",
            "sample-own",
            "--status",
            "blocked",
            "--author",
            "alpha",
            "--json",
        ],
    )?;
    for root in [fixture.node.path(), fixture.rust.path()] {
        let path = root.join(".agents/pm/settings.json");
        let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
        value["schema"]["workflow"]["in_progress_status"] = serde_json::json!("open");
        value["schema"]["statuses"]
            .as_array_mut()
            .ok_or("statuses")?
            .retain(|row| row["id"] != "in_progress");
        fs::write(&path, serde_json::to_vec_pretty(&value)?)?;
        fs::write(
            root.join(".agents/pm/schema/statuses.json"),
            serde_json::to_vec(&serde_json::json!({"statuses":value["schema"]["statuses"]}))?,
        )?;
        fs::write(
            root.join(".agents/pm/schema/workflows.json"),
            serde_json::to_vec(&serde_json::json!({"workflow":value["schema"]["workflow"]}))?,
        )?;
    }
    let before = start_snapshot(fixture.node.path())?;
    fixture.step(&[
        "release",
        "sample-own",
        "--pause",
        "--author",
        "alpha",
        "--json",
    ])?;
    fixture.step(&["pause-task", "sample-own", "--author", "alpha", "--json"])?;
    assert_eq!(start_snapshot(fixture.node.path())?, before);
    assert_eq!(start_snapshot(fixture.rust.path())?, before);
    Ok(())
}
/// Restores a supported historical alias without a scalar or canonical edge masking it.
fn historical_alias_only(fixture: &Fixture) -> Result<(), Box<dyn std::error::Error>> {
    // Current writes canonicalize aliases; change only the persisted relationship kind.
    let mut item = pm_rust::Workspace::discover(fixture.node.path())?.get("sample-own")?;
    item.metadata.extra["dependencies"][0]["kind"] = serde_json::json!("depends_on");
    for root in [fixture.node.path(), fixture.rust.path()] {
        fs::write(
            root.join(".agents/pm/tasks/sample-own.toon"),
            toon_format::encode_default(&item)?,
        )?;
        let stored = pm_rust::Workspace::discover(root)?.get("sample-own")?;
        assert!(stored.metadata.extra.get("blocked_by").is_none());
        assert_eq!(
            stored.metadata.extra["dependencies"],
            serde_json::json!([{"id":"sample-gate","kind":"depends_on","created_at":CLOCK,"author":"alpha","source_kind":"cli:update:dep","author_source":"detected"}])
        );
    }
    Ok(())
}

/// Blocker aliases, closed prerequisites and absent/external targets share one admission policy.
#[test]
fn start_declared_blocker_variants_match_published_bytes() -> Result<(), Box<dyn std::error::Error>>
{
    for variant in ["closed", "missing", "external", "alias"] {
        let Some(fixture) = blocked_start_fixture("strict")? else {
            return Ok(());
        };
        match variant {
            "closed" => prepare(
                &fixture,
                &[
                    "close",
                    "sample-gate",
                    "Done",
                    "--validate-close",
                    "off",
                    "--author",
                    "alpha",
                    "--json",
                ],
            )?,
            "missing" => prepare(
                &fixture,
                &[
                    "update",
                    "sample-own",
                    "--blocked-by",
                    "sample-missing",
                    "--author",
                    "alpha",
                    "--json",
                ],
            )?,
            "external" => prepare(
                &fixture,
                &[
                    "update",
                    "sample-own",
                    "--unset",
                    "blocked_by",
                    "--dep",
                    "id=github:fixture/issue,kind=blocked_by,source_kind=external",
                    "--author",
                    "alpha",
                    "--json",
                ],
            )?,
            _ => {
                prepare(
                    &fixture,
                    &[
                        "update",
                        "sample-own",
                        "--unset",
                        "blocked_by",
                        "--dep-remove",
                        "id=sample-gate,kind=blocked_by",
                        "--author",
                        "alpha",
                        "--json",
                    ],
                )?;
                prepare(
                    &fixture,
                    &[
                        "update",
                        "sample-own",
                        "--dep",
                        "id=sample-gate,kind=depends_on",
                        "--author",
                        "alpha",
                        "--json",
                    ],
                )?;
            }
        }
        if variant == "alias" {
            historical_alias_only(&fixture)?;
        }
        let unchanged = start_snapshot(fixture.node.path())?;
        fixture.step(&[
            "claim",
            "sample-own",
            "--start",
            "--author",
            "alpha",
            "--json",
        ])?;
        if variant == "alias" {
            assert_eq!(start_snapshot(fixture.node.path())?, unchanged);
            assert_eq!(start_snapshot(fixture.rust.path())?, unchanged);
        }
        fixture.compare_bytes()?;
    }
    Ok(())
}
/// Removed and aliased workflow progress selectors use the same registered fallback for both paths.
#[test]
fn configured_progress_resolution_matches_published_registry()
-> Result<(), Box<dyn std::error::Error>> {
    for selector in ["missing-progress", "OPEN-ALIAS"] {
        for ranked in [false, true] {
            let Some(fixture) = progress_fallback_fixture()? else {
                return Ok(());
            };
            for root in [fixture.node.path(), fixture.rust.path()] {
                let path = root.join(".agents/pm/settings.json");
                let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
                value["schema"]["workflow"]["in_progress_status"] = serde_json::json!(selector);
                for status in value["schema"]["statuses"]
                    .as_array_mut()
                    .ok_or("statuses")?
                {
                    if status["id"] == "open" {
                        status["aliases"] = serde_json::json!(["open_alias"]);
                    }
                }
                fs::write(&path, serde_json::to_vec_pretty(&value)?)?;
                fs::write(
                    root.join(".agents/pm/schema/statuses.json"),
                    serde_json::to_vec(
                        &serde_json::json!({"statuses":value["schema"]["statuses"]}),
                    )?,
                )?;
                fs::write(
                    root.join(".agents/pm/schema/workflows.json"),
                    serde_json::to_vec(
                        &serde_json::json!({"workflow":value["schema"]["workflow"]}),
                    )?,
                )?;
            }
            let target = if ranked { "--next" } else { "sample-own" };
            fixture.step(&["claim", target, "--start", "--author", "alpha", "--json"])?;
            assert_eq!(
                pm_rust::Workspace::discover(fixture.rust.path())?
                    .get("sample-own")?
                    .metadata
                    .status,
                "in_progress"
            );
        }
    }
    Ok(())
}

/// Additive status files retain built-in progress and closed-blocker semantics.
#[test]
fn round5_additive_status_admission_matches_published_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = blocked_start_fixture("strict")? else {
        return Ok(());
    };
    for root in [fixture.node.path(), fixture.rust.path()] {
        fs::write(
            root.join(".agents/pm/schema/statuses.json"),
            r#"{"statuses":[{"id":"done","label":"Done","roles":["terminal","terminal_done"]}]}"#,
        )?;
    }
    let before = start_snapshot(fixture.node.path())?;
    fixture.step(&[
        "claim",
        "sample-own",
        "--start",
        "--author",
        "alpha",
        "--json",
    ])?;
    assert_eq!(start_snapshot(fixture.node.path())?, before);
    assert_eq!(start_snapshot(fixture.rust.path())?, before);
    assert_atomic_blocker_refusal(&fixture)?;
    prepare(
        &fixture,
        &[
            "close",
            "sample-gate",
            "Done",
            "--validate-close",
            "off",
            "--author",
            "alpha",
            "--json",
        ],
    )?;
    fixture.step(&[
        "claim",
        "sample-own",
        "--start",
        "--author",
        "alpha",
        "--json",
    ])?;
    assert_eq!(
        pm_rust::Workspace::discover(fixture.rust.path())?
            .get("sample-own")?
            .metadata
            .status,
        "in_progress"
    );
    Ok(())
}

/// Compares physical item identities while retaining every other refusal context field.
/// The SDK preserves caller spelling; native discovery resolves Windows short names and links.
fn canonical_item_path_context(
    context: &serde_json::Value,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let path = context["item_path"]
        .as_str()
        .ok_or("refusal item_path must be a string")?;
    let mut canonical = context.clone();
    canonical["item_path"] = serde_json::json!(fs::canonicalize(path)?);
    Ok(canonical)
}

/// Filesystem aliases compare equally without changing other fields or accepting unresolved paths.
#[test]
fn round6_item_path_comparison_preserves_other_refusal_fields()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    fs::create_dir(directory.path().join("nested"))?;
    let item = directory.path().join("item.toon");
    fs::write(&item, [0xff, 0xfe, 0x61])?;
    let alias = directory.path().join("nested/../item.toon");
    let original = serde_json::json!({"code":"item_document_encoding_invalid","item_id":"sample-gate","item_path":alias,"byte_length":3,"empty":false,"detail":alias});
    let mut expected = original.clone();
    expected["item_path"] = serde_json::json!(fs::canonicalize(&item)?);
    assert_eq!(canonical_item_path_context(&original)?, expected);
    assert_eq!(canonical_item_path_context(&expected)?, expected);
    assert_eq!(expected["detail"], original["detail"]);
    let other = directory.path().join("other.toon");
    fs::write(&other, [0xff, 0xfe, 0x61])?;
    let mut different_item = expected.clone();
    different_item["item_path"] = serde_json::json!(other);
    assert_ne!(
        canonical_item_path_context(&original)?,
        canonical_item_path_context(&different_item)?
    );
    for (key, value) in [
        ("code", serde_json::json!("different_code")),
        ("item_id", serde_json::json!("different-item")),
        ("byte_length", serde_json::json!(4)),
        ("empty", serde_json::json!(true)),
        ("detail", serde_json::json!("different detail")),
    ] {
        let mut changed = expected.clone();
        changed[key] = value;
        assert_ne!(
            canonical_item_path_context(&original)?,
            canonical_item_path_context(&changed)?,
            "{key} must remain exact"
        );
    }
    for invalid in [
        serde_json::json!({}),
        serde_json::json!({"item_path":false}),
        serde_json::json!({"item_path":directory.path().join("missing.toon")}),
    ] {
        assert!(canonical_item_path_context(&invalid).is_err());
    }
    assert_eq!(fs::read(item)?, [0xff, 0xfe, 0x61]);
    Ok(())
}

/// Unverifiable UTF-8 uses strict refusal or the exact warning/force bypass without decoding bytes.
#[test]
fn round5_invalid_utf8_blocker_policy_matches_published_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    for (mode, force) in [
        ("strict", false),
        ("warn", false),
        ("strict", true),
        ("off", false),
    ] {
        let Some(fixture) = blocked_start_fixture(mode)? else {
            return Ok(());
        };
        for root in [fixture.node.path(), fixture.rust.path()] {
            fs::write(
                root.join(".agents/pm/tasks/sample-gate.toon"),
                [0xff, 0xfe, 0x61],
            )?;
        }
        if mode == "strict" && !force {
            let before = start_snapshot(fixture.node.path())?;
            let output = published_run(
                &fixture.driver,
                fixture.node.path(),
                &["__ownership_error", "start", "sample-own"],
            )?;
            let expected: serde_json::Value = serde_json::from_slice(&output.stdout)?;
            let workspace = pm_rust::Workspace::discover(fixture.node.path())?;
            let request = pm_rust::OwnershipItem {
                id: "sample-own".to_owned(),
                author: "alpha".to_owned(),
                timestamp: Some(CLOCK.to_owned()),
                message: None,
                force: false,
                if_available: false,
                provenance_role: None,
            };
            let error = workspace
                .start_task(&request, &pm_rust::OwnershipOptions::default())
                .err()
                .ok_or("invalid blocker admitted")?;
            let pm_rust::PmRustError::CommandRefusal {
                detail,
                exit_code,
                context,
            } = error
            else {
                return Err(format!("unexpected error {error}").into());
            };
            assert_eq!(expected["exit_code"], exit_code);
            assert_eq!(expected["detail"], detail);
            assert_eq!(
                canonical_item_path_context(&expected["context"])?,
                canonical_item_path_context(&context)?
            );
            assert_eq!(start_snapshot(fixture.node.path())?, before);
            let args = [
                "claim",
                "sample-own",
                "--start",
                "--author",
                "alpha",
                "--json",
            ];
            let oracle = published_run(&fixture.driver, fixture.node.path(), &args)?;
            let native = native_command(fixture.node.path(), &args).output()?;
            assert_eq!(native.status.code(), oracle.status.code());
            assert_eq!(native.stdout, oracle.stdout);
            assert_eq!(native.stderr, oracle.stderr);
            assert_eq!(start_snapshot(fixture.node.path())?, before);
        } else {
            let mut args = vec![
                "claim",
                "sample-own",
                "--start",
                "--author",
                "alpha",
                "--json",
            ];
            if force {
                args.push("--force");
            }
            fixture.step(&args)?;
            let item = pm_rust::Workspace::discover(fixture.rust.path())?.get("sample-own")?;
            assert_eq!(item.metadata.status, "in_progress");
            assert_eq!(
                fs::read(
                    fixture
                        .rust
                        .path()
                        .join(".agents/pm/tasks/sample-gate.toon")
                )?,
                [0xff, 0xfe, 0x61]
            );
        }
    }
    Ok(())
}

/// A prefix change does not orphan an older complete blocker ID; shorthand still resolves.
#[test]
fn round5_complete_blocker_identity_matches_published_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = blocked_start_fixture("strict")? else {
        return Ok(());
    };
    for root in [fixture.node.path(), fixture.rust.path()] {
        let path = root.join(".agents/pm/settings.json");
        let mut settings: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
        settings["id_prefix"] = serde_json::json!("new-");
        fs::write(path, serde_json::to_vec_pretty(&settings)?)?;
    }
    prepare(
        &fixture,
        &[
            "close",
            "sample-gate",
            "Done",
            "--validate-close",
            "off",
            "--author",
            "alpha",
            "--json",
        ],
    )?;
    fixture.step(&[
        "claim",
        "sample-own",
        "--start",
        "--author",
        "alpha",
        "--json",
    ])?;
    assert_eq!(
        pm_rust::Workspace::discover(fixture.rust.path())?
            .get("sample-own")?
            .metadata
            .status,
        "in_progress"
    );
    Ok(())
}

/// Mixed open and undecodable blockers preserve the published single unverifiable warning.
#[test]
fn round7_mixed_blocker_warnings_match_published() -> Result<(), Box<dyn std::error::Error>> {
    for (mode, force) in [("warn", false), ("strict", true)] {
        let Some(fixture) = blocked_start_fixture(mode)? else {
            return Ok(());
        };
        for id in ["sample-zbad", "sample-zzlater"] {
            prepare(
                &fixture,
                &[
                    "create", "--id", id, "--title", id, "--type", "Task", "--author", "alpha",
                    "--json",
                ],
            )?;
        }
        for root in [fixture.node.path(), fixture.rust.path()] {
            fs::write(root.join(".agents/pm/tasks/sample-zbad.toon"), [0xff])?;
            let path = root.join(".agents/pm/tasks/sample-own.toon");
            let mut item: serde_json::Value =
                toon_format::decode_default(&fs::read_to_string(&path)?)?;
            item["dependencies"] = serde_json::json!([{"id":"sample-zbad","kind":"blocked_by"},{"id":"sample-zzlater","kind":"blocked_by"}]);
            fs::write(path, toon_format::encode_default(&item)?)?;
        }
        let mut args = vec![
            "claim",
            "sample-own",
            "--start",
            "--author",
            "alpha",
            "--json",
        ];
        if force {
            args.push("--force");
        }
        let expected = published_run(&fixture.driver, fixture.node.path(), &args)?;
        let actual = native_command(fixture.rust.path(), &args).output()?;
        assert_eq!(actual.status.code(), expected.status.code());
        assert!(expected.status.success());
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
        let receipt: serde_json::Value = serde_json::from_slice(&expected.stdout)?;
        assert_eq!(
            receipt["claim"]["warnings"],
            serde_json::json!([
                "start_blocker_unverifiable:sample-own:item_document_encoding_invalid"
            ])
        );
        fixture.compare_bytes()?;
    }
    Ok(())
}

/// Persisted receipt indices are measured independently against the published comment CLI.
#[test]
fn round7_stored_comment_index_matches_published() -> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("none")? else {
        return Ok(());
    };
    for root in [fixture.node.path(), fixture.rust.path()] {
        let path = root.join(".agents/pm/tasks/sample-own.toon");
        let mut item: serde_json::Value = toon_format::decode_default(&fs::read_to_string(&path)?)?;
        item["comments"] =
            serde_json::json!([{"text":"Earlier","author":"alpha","created_at":CLOCK,"index":99}]);
        fs::write(path, toon_format::encode_default(&item)?)?;
    }
    let args = ["comment", "sample-own", "Newer", "--author", "alpha"];
    let mut oracle_args = args.to_vec();
    oracle_args.extend(["--json", "--full-history"]);
    let expected = published_run(&fixture.driver, fixture.node.path(), &oracle_args)?;
    let actual = native_command(fixture.rust.path(), &args).output()?;
    assert_eq!(
        actual.status.code(),
        expected.status.code(),
        "native stderr {}",
        String::from_utf8_lossy(&actual.stderr)
    );
    assert!(expected.status.success());
    let v: serde_json::Value = serde_json::from_slice(&expected.stdout)?;
    assert_eq!(v["comments"][0]["index"], 1);
    assert_eq!(v["comments"][1]["index"], 2);
    let actual_value: serde_json::Value = serde_json::from_slice(&actual.stdout)?;
    assert_eq!(actual_value["item"]["comments"], v["comments"]);
    assert_eq!(actual.stderr, expected.stderr);
    fixture.compare_bytes()?;
    let stored = pm_rust::Workspace::discover(fixture.rust.path())?.get("sample-own")?;
    assert_eq!(stored.metadata.extra["comments"][0]["index"], 99);
    Ok(())
}

/// Atomic starts use the same default-file registry as the preceding claim check.
#[test]
fn round7_terminal_source_contract() -> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("none")? else {
        return Ok(());
    };
    for root in [fixture.node.path(), fixture.rust.path()] {
        let path = root.join(".agents/pm/settings.json");
        let mut settings: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
        settings["schema"]["files"]
            .as_object_mut()
            .ok_or("files")?
            .shift_remove("statuses");
        fs::write(&path, serde_json::to_vec_pretty(&settings)?)?;
        fs::write(
            root.join(".agents/pm/schema/statuses.json"),
            br#"{"statuses":[{"id":"done","roles":["terminal_done"]}]}"#,
        )?;
        let item = root.join(".agents/pm/tasks/sample-own.toon");
        fs::write(
            &item,
            fs::read_to_string(&item)?.replace("status: open", "status: done"),
        )?;
    }
    for force in [false, true] {
        let before = start_snapshot(fixture.node.path())?;
        let expected = published_run(
            &fixture.driver,
            fixture.node.path(),
            &[
                "__atomic_claim_error",
                "sample-own",
                if force { "true" } else { "false" },
            ],
        )?;
        let expected: serde_json::Value = serde_json::from_slice(&expected.stdout)?;
        let request = pm_rust::OwnershipItem {
            id: "sample-own".to_owned(),
            author: "alpha".to_owned(),
            timestamp: Some(CLOCK.to_owned()),
            message: None,
            force,
            if_available: false,
            provenance_role: None,
        };
        let actual = pm_rust::Workspace::discover(fixture.rust.path())?
            .claim_with_options(
                &request,
                &pm_rust::OwnershipOptions {
                    start: true,
                    ..Default::default()
                },
            )
            .err()
            .ok_or("terminal start admitted")?;
        let pm_rust::PmRustError::OwnershipRefusal {
            detail,
            mut context,
        } = actual
        else {
            return Err(format!("unexpected refusal {actual}").into());
        };
        if let Some(steps) = context
            .as_object_mut()
            .and_then(|fields| fields.shift_remove("next_steps"))
        {
            context["nextSteps"] = steps;
        }
        assert_eq!(expected["exit_code"], 4);
        assert_eq!(expected["detail"], detail);
        assert_eq!(expected["context"], context);
        assert_eq!(start_snapshot(fixture.node.path())?, before);
        assert_eq!(start_snapshot(fixture.rust.path())?, before);
    }
    Ok(())
}

/// A configured close target without a terminal role remains startable in the published policy.
#[test]
fn round7_roleless_close_status_matches_published_policy() -> Result<(), Box<dyn std::error::Error>>
{
    for (legacy, force) in [(false, false), (false, true), (true, false), (true, true)] {
        let Some(fixture) = Fixture::new("none")? else {
            return Ok(());
        };
        for root in [fixture.node.path(), fixture.rust.path()] {
            let path = root.join(".agents/pm/settings.json");
            let mut settings: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
            if legacy {
                settings["workflow"]["close_status"] = serde_json::json!("done");
            } else {
                settings["schema"]["workflow"]["close_status"] = serde_json::json!("done");
            }
            settings["schema"]["statuses"]
                .as_array_mut()
                .ok_or("statuses")?
                .push(serde_json::json!({"id":"done","roles":["active"]}));
            fs::write(path, serde_json::to_vec_pretty(&settings)?)?;
            fs::write(
                root.join(".agents/pm/schema/statuses.json"),
                serde_json::to_vec(
                    &serde_json::json!({"statuses":settings["schema"]["statuses"]}),
                )?,
            )?;
            fs::write(
                root.join(".agents/pm/schema/workflows.json"),
                serde_json::to_vec(
                    &serde_json::json!({"workflow":settings["schema"]["workflow"]}),
                )?,
            )?;
            let item = root.join(".agents/pm/tasks/sample-own.toon");
            fs::write(
                &item,
                fs::read_to_string(&item)?.replace("status: open", "status: done"),
            )?;
        }
        let expected = published_run(
            &fixture.driver,
            fixture.node.path(),
            &[
                "__atomic_claim_error",
                "sample-own",
                if force { "true" } else { "false" },
            ],
        )?;
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&expected.stdout)?,
            serde_json::json!({"ok":true})
        );
        assert_eq!(
            pm_rust::Workspace::discover(fixture.node.path())?
                .get("sample-own")?
                .metadata
                .status,
            "in_progress"
        );
        let request = pm_rust::OwnershipItem {
            id: "sample-own".to_owned(),
            author: "alpha".to_owned(),
            timestamp: Some(CLOCK.to_owned()),
            message: None,
            force,
            if_available: false,
            provenance_role: None,
        };
        let started = pm_rust::Workspace::discover(fixture.rust.path())?.claim_with_options(
            &request,
            &pm_rust::OwnershipOptions {
                start: true,
                ..Default::default()
            },
        )?;
        assert_eq!(started.status, "in_progress");
        fixture.compare_bytes()?;
        assert_eq!(
            pm_rust::Workspace::discover(fixture.rust.path())?
                .get("sample-own")?
                .metadata
                .status,
            "in_progress"
        );
    }
    Ok(())
}

/// Default-file status aliases and workflow rules are configured in real non-git trackers.
fn round8_workflow_fixture(
    external_rules: bool,
    allowed: bool,
) -> Result<Option<Fixture>, Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(None);
    };
    for root in [fixture.node.path(), fixture.rust.path()] {
        let path = root.join(".agents/pm/settings.json");
        let mut settings: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
        let files = settings["schema"]["files"]
            .as_object_mut()
            .ok_or("schema files")?;
        files.shift_remove("statuses");
        files.shift_remove("workflows");
        settings["governance"]["workflow_enforcement"] = serde_json::json!("strict");
        settings["schema"]["statuses"] = serde_json::json!([{"id":"open","roles":["active"]}]);
        let rules = serde_json::json!([{"type":"Task","allowed_transitions":if allowed {serde_json::json!([["open","in_progress"]])} else {serde_json::json!([["draft","in_progress"]])}}]);
        settings["schema"]["type_workflows"] = if external_rules {
            serde_json::json!([])
        } else {
            rules.clone()
        };
        fs::write(path, serde_json::to_vec_pretty(&settings)?)?;
        fs::write(
            root.join(".agents/pm/schema/statuses.json"),
            serde_json::to_vec(
                &serde_json::json!({"statuses":[{"id":"doing","roles":["active"],"aliases":["in_progress"]}]}),
            )?,
        )?;
        let workflows = root.join(".agents/pm/schema/workflows.json");
        if external_rules {
            fs::write(
                workflows,
                serde_json::to_vec(&serde_json::json!({"type_workflows":rules}))?,
            )?;
        } else {
            fs::remove_file(workflows)?;
        }
    }
    Ok(Some(fixture))
}

/// Allowed explicit starts finish both commits using default-file aliases and workflow rules.
#[test]
fn round8_explicit_alias_start_matches_published() -> Result<(), Box<dyn std::error::Error>> {
    for external in [false, true] {
        let Some(fixture) = round8_workflow_fixture(external, true)? else {
            return Ok(());
        };
        fixture.step(&[
            "claim",
            "sample-own",
            "--start",
            "--author",
            "alpha",
            "--json",
        ])?;
        let item = pm_rust::Workspace::discover(fixture.rust.path())?.get("sample-own")?;
        assert_eq!(item.metadata.status, "doing");
        let history = fs::read_to_string(
            fixture
                .rust
                .path()
                .join(".agents/pm/history/sample-own.jsonl"),
        )?;
        assert_eq!(history.lines().count(), 3);
        fixture.verify()?;
    }
    Ok(())
}

/// Refusal streams preserve complete nested passwords and their legitimate public-URL twin.
#[test]
fn round8_nested_password_invocation_matches_published() -> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("none")? else {
        return Ok(());
    };
    prepare(
        &fixture,
        &["claim", "sample-own", "--author", "alpha", "--json"],
    )?;
    for message in [
        "https://user:secret,value://tail@host.invalid",
        "https://user:secret;value://tail@host.invalid",
        "https://user:secret value://tail@host.invalid",
        "https://public.invalid:443/p https://second.invalid/path",
        "https://user.invalid:443/secret@host.invalid copied 443/secret",
    ] {
        for json in [false, true] {
            let mut args = vec![
                "claim",
                "sample-own",
                "--author",
                "beta",
                "--message",
                message,
            ];
            if json {
                args.push("--json");
            }
            fixture.step(&args)?;
        }
    }
    Ok(())
}

/// Measures persisted-holder projection without invocation credentials enrolling its password.
#[test]
fn round8_stored_nested_password_projection() -> Result<(), Box<dyn std::error::Error>> {
    for separator in [",", ";", " "] {
        let Some(fixture) = Fixture::new("none")? else {
            return Ok(());
        };
        let holder = format!("https://user:secret{separator}value://tail@host.invalid");
        for root in [fixture.node.path(), fixture.rust.path()] {
            let path = root.join(".agents/pm/tasks/sample-own.toon");
            let mut item: serde_json::Value =
                toon_format::decode_default(&fs::read_to_string(&path)?)?;
            item["assignee"] = serde_json::json!(holder);
            fs::write(path, toon_format::encode_default(&item)?)?;
        }
        let args = ["claim", "sample-own", "--author", "beta", "--json"];
        let oracle = published_run(&fixture.driver, fixture.node.path(), &args)?;
        let actual = native_command(fixture.rust.path(), &args).output()?;
        assert_eq!(actual.status.code(), Some(4));
        assert_eq!(actual.status.code(), oracle.status.code());
        assert!(!String::from_utf8_lossy(&actual.stderr).contains("secret"));
        assert_eq!(actual.stdout, oracle.stdout);
        if separator == " " {
            // Published 2026.11.11 misses stored outer userinfo without invocation enrollment.
            let mut expected: serde_json::Value = serde_json::from_slice(&oracle.stderr)?;
            assert_eq!(
                expected["claimed_by"],
                "https://user:secret value://[redacted]@host.invalid"
            );
            assert_eq!(
                expected["detail"],
                "Item sample-own is already assigned to https://user:secret value://[redacted]@host.invalid. Use --force to take over, or --if-available to skip without failing."
            );
            assert_eq!(
                expected["title"],
                "Item sample-own is already assigned to https://user:secret value://[redacted]@host.invalid. Use --force to take over,..."
            );
            expected["claimed_by"] = serde_json::json!("https://[redacted]@host.invalid");
            expected["detail"] = serde_json::json!(
                "Item sample-own is already assigned to https://[redacted]@host.invalid. Use --force to take over, or --if-available to skip without failing."
            );
            expected["title"] = serde_json::json!(
                "Item sample-own is already assigned to https://[redacted]@host.invalid. Use --force to take over, or --if-available t..."
            );
            assert_eq!(
                String::from_utf8(actual.stderr)?,
                format!("{}\n", pm_rust::stringify_json(&expected, true))
            );
        } else {
            assert_eq!(actual.stderr, oracle.stderr);
        }
        let native_text = native_command(
            fixture.rust.path(),
            &["claim", "sample-own", "--author", "beta"],
        )
        .output()?;
        assert_eq!(native_text.status.code(), Some(4));
        assert!(!String::from_utf8_lossy(&native_text.stderr).contains("secret"));
        fixture.compare_bytes()?;
    }
    Ok(())
}

/// Atomic workflow admission compares the oracle and refuses before either item or history writes.
#[test]
fn round8_atomic_alias_workflow_matches_published() -> Result<(), Box<dyn std::error::Error>> {
    for external in [false, true] {
        for allowed in [false, true] {
            let Some(fixture) = round8_workflow_fixture(external, allowed)? else {
                return Ok(());
            };
            let before = start_snapshot(fixture.node.path())?;
            let oracle = published_run(
                &fixture.driver,
                fixture.node.path(),
                &["__atomic_claim_error", "sample-own", "false"],
            )?;
            let expected: serde_json::Value = serde_json::from_slice(&oracle.stdout)?;
            let request = pm_rust::OwnershipItem {
                id: "sample-own".to_owned(),
                author: "alpha".to_owned(),
                timestamp: Some(CLOCK.to_owned()),
                message: None,
                force: false,
                if_available: false,
                provenance_role: None,
            };
            let actual = pm_rust::Workspace::discover(fixture.rust.path())?.claim_with_options(
                &request,
                &pm_rust::OwnershipOptions {
                    start: true,
                    ..Default::default()
                },
            );
            if allowed {
                assert_eq!(expected, serde_json::json!({"ok":true}));
                assert_eq!(actual?.status, "doing");
                fixture.compare_bytes()?;
            } else {
                let Err(pm_rust::PmRustError::CommandRefusal {
                    detail,
                    exit_code,
                    context,
                }) = actual
                else {
                    return Err("workflow refusal required".into());
                };
                assert_eq!(expected["detail"], detail);
                assert_eq!(expected["exit_code"], exit_code);
                assert_eq!(expected["context"], context);
                assert_eq!(start_snapshot(fixture.node.path())?, before);
                assert_eq!(start_snapshot(fixture.rust.path())?, before);
            }
        }
    }
    Ok(())
}

/// Missing workflow sidecars retain inline progress settings, matching the published CLI.
#[test]
fn round10_missing_workflow_start_matches_oracle() -> Result<(), Box<dyn std::error::Error>> {
    for (present, configured) in [(false, false), (false, true), (true, true)] {
        let Some(fixture) = Fixture::new("none")? else {
            return Ok(());
        };
        for root in [fixture.node.path(), fixture.rust.path()] {
            let pm = root.join(".agents/pm");
            let path = pm.join("settings.json");
            let mut settings: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
            settings["schema"]["workflow"]["in_progress_status"] = serde_json::json!("working");
            settings["schema"]["statuses"]
                .as_array_mut()
                .ok_or("statuses")?
                .push(serde_json::json!({"id":"working","roles":["active"]}));
            if !configured {
                settings["schema"]["files"]
                    .as_object_mut()
                    .ok_or("files")?
                    .shift_remove("workflows");
            }
            fs::write(path, serde_json::to_vec(&settings)?)?;
            fs::write(
                pm.join("schema/statuses.json"),
                serde_json::to_vec(
                    &serde_json::json!({"statuses":settings["schema"]["statuses"]}),
                )?,
            )?;
            if present {
                fs::write(
                    pm.join("schema/workflows.json"),
                    serde_json::to_vec(
                        &serde_json::json!({"workflow":settings["schema"]["workflow"]}),
                    )?,
                )?;
            } else {
                fs::remove_file(pm.join("schema/workflows.json"))?;
            }
        }
        let args = [
            "claim",
            "sample-own",
            "--start",
            "--author",
            "alpha",
            "--json",
        ];
        let expected = published_run(&fixture.driver, fixture.node.path(), &args)?;
        assert!(
            expected.status.success(),
            "{}",
            String::from_utf8_lossy(&expected.stderr)
        );
        let oracle_status = pm_rust::Workspace::discover(fixture.node.path())?
            .get("sample-own")?
            .metadata
            .status;
        assert_eq!(oracle_status, "working");
        let actual = native_command(fixture.rust.path(), &args).output()?;
        assert_eq!(
            actual.status.code(),
            expected.status.code(),
            "native: {}; oracle chose {oracle_status}",
            String::from_utf8_lossy(&actual.stderr)
        );
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
        fixture.compare_bytes()?;
        assert_eq!(
            pm_rust::Workspace::discover(fixture.rust.path())?
                .get("sample-own")?
                .metadata
                .status,
            "working"
        );
    }
    Ok(())
}

/// Ranked candidates and terminal prerequisites discover default status sidecars.
#[test]
fn round12_selection_default_status_sidecar_matches_oracle()
-> Result<(), Box<dyn std::error::Error>> {
    for (status, gate) in [
        ("queued", None),
        ("open", Some("done")),
        ("open", Some("closed")),
    ] {
        let Some(fixture) = Fixture::new("none")? else {
            return Ok(());
        };
        for root in [fixture.node.path(), fixture.rust.path()] {
            let pm = root.join(".agents/pm");
            let path = pm.join("settings.json");
            let mut settings: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
            settings["schema"]["files"]
                .as_object_mut()
                .ok_or("files")?
                .shift_remove("statuses");
            fs::write(path, serde_json::to_vec_pretty(&settings)?)?;
            fs::write(
                pm.join("schema/statuses.json"),
                r#"{"statuses":[{"id":"queued","roles":["active"]},{"id":"done","roles":["terminal_done"]}]}"#,
            )?;
            let path = pm.join("tasks/sample-own.toon");
            let mut item: serde_json::Value =
                toon_format::decode_default(&fs::read_to_string(&path)?)?;
            item["status"] = status.into();
            if let Some(gate_status) = gate {
                item["blocked_by"] = "sample-gate".into();
                let mut blocker = item.clone();
                blocker["id"] = "sample-gate".into();
                blocker["status"] = gate_status.into();
                blocker
                    .as_object_mut()
                    .ok_or("blocker")?
                    .shift_remove("blocked_by");
                fs::write(
                    pm.join("tasks/sample-gate.toon"),
                    toon_format::encode_default(&blocker)?,
                )?;
            }
            fs::write(path, toon_format::encode_default(&item)?)?;
        }
        fixture.step(&["claim", "--next", "--author", "alpha", "--json"])?;
        assert_eq!(
            pm_rust::Workspace::discover(fixture.rust.path())?
                .get("sample-own")?
                .metadata
                .extra["assignee"],
            "alpha"
        );
    }
    Ok(())
}

/// Compares explicit and default absence at each real lifecycle CLI boundary.
fn round14_missing_workflow_case(action: &str) -> Result<(), Box<dyn std::error::Error>> {
    for explicit in [false, true] {
        let Some(fixture) = Fixture::new("none")? else {
            return Ok(());
        };
        for root in [fixture.node.path(), fixture.rust.path()] {
            let pm = root.join(".agents/pm");
            let path = pm.join("settings.json");
            let mut settings: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
            if explicit {
                settings["schema"]["files"]["workflows"] = "missing.json".into();
            } else {
                settings["schema"]["files"]
                    .as_object_mut()
                    .ok_or("files")?
                    .shift_remove("workflows");
            }
            fs::write(path, serde_json::to_vec_pretty(&settings)?)?;
            fs::remove_file(pm.join("schema/workflows.json"))?;
        }
        let args = match action {
            "pause" => vec!["pause-task", "sample-own", "--author", "alpha", "--json"],
            "start" => vec!["start-task", "sample-own", "--author", "alpha", "--json"],
            "atomic" => vec![
                "claim",
                "sample-own",
                "--start",
                "--author",
                "alpha",
                "--json",
            ],
            "close" => vec![
                "close",
                "sample-own",
                "Done",
                "--author",
                "alpha",
                "--validate-close",
                "off",
                "--json",
            ],
            _ => return Err("unknown lifecycle action".into()),
        };
        fixture.step(&args)?;
        let item = pm_rust::Workspace::discover(fixture.rust.path())?.get("sample-own")?;
        assert_eq!(
            item.metadata.status,
            match action {
                "pause" => "open",
                "close" => "closed",
                _ => "in_progress",
            }
        );
    }
    round14_minimal_workflow_case(action)
}

/// Checks the exact minimal settings through public SDK mutations and persisted bytes.
fn round14_minimal_workflow_case(action: &str) -> Result<(), Box<dyn std::error::Error>> {
    for explicit in [false, true] {
        let Some(fixture) = Fixture::new("none")? else {
            return Ok(());
        };
        for root in [fixture.node.path(), fixture.rust.path()] {
            let pm = root.join(".agents/pm");
            fs::write(
                pm.join("settings.json"),
                if explicit {
                    r#"{"schema":{"files":{"workflows":"missing.json"}}}"#
                } else {
                    "{}"
                },
            )?;
            fs::remove_file(pm.join("schema/workflows.json"))?;
        }
        let output = published_run(
            &fixture.driver,
            fixture.node.path(),
            &["__workflow_receipt", action, "sample-own"],
        )?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.stdout)?,
            serde_json::json!({"ok":true})
        );
        let workspace = pm_rust::Workspace::discover(fixture.rust.path())?;
        let request = pm_rust::OwnershipItem {
            id: "sample-own".to_owned(),
            author: "alpha".to_owned(),
            timestamp: Some(CLOCK.to_owned()),
            message: None,
            force: false,
            if_available: false,
            provenance_role: None,
        };
        match action {
            "pause" => {
                workspace.pause_task(&request, &pm_rust::OwnershipOptions::default())?;
            }
            "start" => {
                workspace.start_task(&request, &pm_rust::OwnershipOptions::default())?;
            }
            "atomic" => {
                workspace.claim_with_options(
                    &request,
                    &pm_rust::OwnershipOptions {
                        start: true,
                        ..Default::default()
                    },
                )?;
            }
            "close" => {
                workspace.close(pm_rust::CloseItem {
                    id: request.id.clone(),
                    author: request.author.clone(),
                    reason: "Done".to_owned(),
                    timestamp: request.timestamp.clone(),
                    provenance_role: Some("implementer".to_owned()),
                    force_stale_lock: false,
                    fields: pm_rust::CloseFields {
                        validate_close: Some("off".to_owned()),
                        ..Default::default()
                    },
                })?;
            }
            _ => return Err("unknown action".into()),
        }
        fixture.compare_bytes()?;
        assert_eq!(
            workspace.get("sample-own")?.metadata.status,
            match action {
                "pause" => "open",
                "close" => "closed",
                _ => "in_progress",
            }
        );
    }
    Ok(())
}

/// Published pause accepts both forms of absent workflow sidecar.
#[test]
fn round14_missing_workflow_pause_matches_oracle() -> Result<(), Box<dyn std::error::Error>> {
    round14_missing_workflow_case("pause")
}
/// Published start accepts both forms of absent workflow sidecar.
#[test]
fn round14_missing_workflow_start_matches_oracle() -> Result<(), Box<dyn std::error::Error>> {
    round14_missing_workflow_case("start")
}
/// Published atomic start accepts both forms of absent workflow sidecar.
#[test]
fn round14_missing_workflow_atomic_matches_oracle() -> Result<(), Box<dyn std::error::Error>> {
    round14_missing_workflow_case("atomic")
}
/// Published close accepts both forms of absent workflow sidecar.
#[test]
fn round14_missing_workflow_close_matches_oracle() -> Result<(), Box<dyn std::error::Error>> {
    round14_missing_workflow_case("close")
}
