//! Ownership conformance against the locked published CLI, including real races.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Barrier};

#[path = "support/mutation_differential.rs"]
mod mutation_differential;
#[path = "support/published_cli.rs"]
mod published_cli;
use mutation_differential::{copy_directory, minimal_command, run_minimal, write_driver};
use published_cli::published_cli_or_skip;
const CLOCK: &str = "2026-08-22T10:00:00.000Z";

/// Two identical disposable trackers plus the fixed-clock published driver.
struct Fixture {
    node: tempfile::TempDir,
    rust: tempfile::TempDir,
    scratch: tempfile::TempDir,
    driver: PathBuf,
}
type TrackerSnapshot = std::collections::BTreeMap<PathBuf, Vec<u8>>;

impl Fixture {
    /// Captures both trackers independently, including missing items and session state.
    fn snapshots(&self) -> Result<[TrackerSnapshot; 2], Box<dyn std::error::Error>> {
        Ok([
            start_snapshot(self.node.path())?,
            start_snapshot(self.rust.path())?,
        ])
    }

    /// Initializes a numeric-looking title with a published record and chosen policy.
    fn new(policy: &str) -> Result<Option<Self>, Box<dyn std::error::Error>> {
        Self::new_at(policy, CLOCK)
    }

    /// Seeds the initial published history using the requested recipe clock.
    fn new_at(policy: &str, clock: &str) -> Result<Option<Self>, Box<dyn std::error::Error>> {
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
            let output = published_command(&driver, node.path(), &args)
                .env("FIXED_CLOCK", clock)
                .output()?;
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
        self.step_with_exit(args).map(|_| ())
    }

    /// Returns the common exit code after comparing both command results.
    fn step_with_exit(&self, args: &[&str]) -> Result<Option<i32>, Box<dyn std::error::Error>> {
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
        Ok(node.status.code())
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

/// Prepares a published invocation whose environment a test needs to vary.
fn published_command(driver: &Path, root: &Path, args: &[&str]) -> Command {
    let interpreter =
        PathBuf::from(std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned()));
    let mut command = minimal_command(&interpreter, root);
    command.arg(driver).args(args);
    command
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
    let active = pm_rust::current_timestamp();
    for clock in [active.as_str(), "2000-01-01T00:00:00.000Z"] {
        for root in [fixture.node.path(), fixture.rust.path()] {
            fs::write(
                root.join(".agents/pm/locks/sample-own.lock"),
                serde_json::to_vec(&serde_json::json!({
                    "id":"sample-own", "pid":1, "owner":"lock-holder", "created_at":clock,
                    "ttl_seconds":1800, "token":"fixture-token"
                }))?,
            )?;
        }
        let args = ["claim", "sample-own", "--author", "alpha", "--json"];
        if clock == active {
            fixture.step(&args)?;
            fixture.step(&["claim", "--next", "--author", "alpha", "--json"])?;
        } else {
            assert_expired_lock_cleanup_scope(&fixture, &args)?;
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

/// The native cleanup command can preview expired receipts in the explicitly selected tracker.
fn assert_expired_lock_cleanup_scope(
    fixture: &Fixture,
    args: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let target = fixture.rust.path().join(".agents/pm");
    let target = target.to_str().ok_or("tracker path")?;
    let oracle = published_run(&fixture.driver, fixture.node.path(), args)?;
    let native = native_command(
        fixture.node.path(),
        &["--pm-path", target]
            .into_iter()
            .chain(args.iter().copied())
            .collect::<Vec<_>>(),
    )
    .output()?;
    let scoped = pm_rust::render_pm_command(["--pm-path", target, "gc", "--scope", "locks"]);
    assert_eq!(oracle.status.code(), Some(4));
    assert_eq!(native.status.code(), Some(4));
    let expected: serde_json::Value = serde_json::from_slice(&oracle.stderr)?;
    let actual: serde_json::Value = serde_json::from_slice(&native.stderr)?;
    for key in ["code", "detail", "required", "details"] {
        assert_eq!(actual[key], expected[key], "{key}");
    }
    assert_eq!(actual["code"], "stale_lock_requires_override");
    assert!(
        expected["next_steps"][0]
            .as_str()
            .ok_or("oracle cleanup")?
            .contains("pm gc --scope locks")
    );
    assert_eq!(actual["next_steps"][0], scoped);
    assert!(
        actual["next_steps"][1]
            .as_str()
            .ok_or("preview")?
            .contains("--dry-run")
    );
    assert!(
        actual["next_steps"][2]
            .as_str()
            .ok_or("retry")?
            .contains("retry the same command")
    );
    let mut preview = pm_rust::parse_command_args(&scoped).ok_or("scoped cleanup command")?;
    preview.extend(["--dry-run".to_owned(), "--json".to_owned()]);
    let preview: Vec<_> = preview.iter().skip(1).map(String::as_str).collect();
    let working_lock = fixture.node.path().join(".agents/pm/locks/sample-own.lock");
    let other_lock = working_lock.with_file_name("sample-other.lock");
    let original = fs::read(&working_lock)?;
    let mut other: serde_json::Value = serde_json::from_slice(&original)?;
    other["id"] = serde_json::json!("sample-other");
    let other_raw = serde_json::to_vec(&other)?;
    fs::write(&other_lock, &other_raw)?;
    fs::remove_file(&working_lock)?;
    let target_lock = fixture.rust.path().join(".agents/pm/locks/sample-own.lock");
    let target_raw = fs::read(&target_lock)?;
    let output = published_run(&fixture.driver, fixture.node.path(), &preview)?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let preview = String::from_utf8_lossy(&output.stdout);
    assert!(preview.contains("sample-own"), "{preview}");
    assert!(!preview.contains("sample-other"), "{preview}");
    assert_eq!(fs::read(&other_lock)?, other_raw);
    assert_eq!(fs::read(&target_lock)?, target_raw);
    fs::write(&working_lock, original)?;
    fs::remove_file(other_lock)?;
    fixture.compare_bytes()
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

/// Composed close removes the active workset entry under the claim's identity.
#[test]
fn close_release_clears_scoped_and_asserted_claim_attribution()
-> Result<(), Box<dyn std::error::Error>> {
    for asserted in [false, true] {
        let Some(fixture) = Fixture::new("strict")? else {
            return Ok(());
        };
        let mut claim = vec!["claim", "sample-own", "--json"];
        let mut close = vec![
            "close",
            "sample-own",
            "Done",
            "--validate-close",
            "off",
            "--release-assignment",
            "--json",
        ];
        if asserted {
            for args in [&mut claim, &mut close] {
                args.extend(["--author", "asserted-agent"]);
            }
        }
        let state = fixture.rust.path().join(".agents/pm/runtime/session.json");
        let mut oracle = published_command(&fixture.driver, fixture.node.path(), &claim);
        let mut native = native_command(fixture.rust.path(), &claim);
        for command in [&mut oracle, &mut native] {
            command
                .env("PI_AGENT", "1")
                .env("PI_SESSION_ID", "fixture-close-session")
                .env("PI_MODEL", "fixture-model");
            let output = command.output()?;
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        fixture.compare_bytes()?;
        let before: serde_json::Value = serde_json::from_slice(&fs::read(&state)?)?;
        let entries = before["semantic_attribution"]
            .as_object()
            .ok_or("active attribution")?;
        let (key, entry) = entries.iter().next().ok_or("claim attribution")?;
        assert_eq!(entries.len(), 1);
        assert_eq!(key.starts_with("author-"), asserted);
        assert_eq!(entry["active_item_ids"], serde_json::json!(["sample-own"]));
        let mut oracle = published_command(&fixture.driver, fixture.node.path(), &close);
        let mut native = native_command(fixture.rust.path(), &close);
        for command in [&mut oracle, &mut native] {
            command
                .env("PI_AGENT", "1")
                .env("PI_SESSION_ID", "fixture-close-session")
                .env("PI_MODEL", "fixture-model");
        }
        let oracle = oracle.output()?;
        let native = native.output()?;
        for output in [&oracle, &native] {
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        assert_eq!(
            String::from_utf8(native.stdout)?,
            String::from_utf8(oracle.stdout)?
        );
        let tracker = |root: &Path| root.join(".agents/pm");
        let published = tracker(fixture.node.path());
        let native = tracker(fixture.rust.path());
        for relative in ["tasks/sample-own.toon", "runtime/session.json"] {
            assert_eq!(
                fs::read(native.join(relative))?,
                fs::read(published.join(relative))?
            );
        }
        let after: serde_json::Value = serde_json::from_slice(&fs::read(&state)?)?;
        assert!(after["semantic_attribution"].get(key).is_none());
        // Compare the release record independently of the earlier close record.
        let read_history = |root: &Path| fs::read_to_string(root.join("history/sample-own.jsonl"));
        let native_history = read_history(&native)?;
        let published_history = read_history(&published)?;
        let release = native_history.lines().last().ok_or("release record")?;
        let record: serde_json::Value = serde_json::from_str(release)?;
        assert_eq!(record["op"], "release");
        assert_eq!(Some(release), published_history.lines().last());
        fixture.verify()?;
    }
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

/// Claim and release reclaim expired attribution locks in normal operation.
#[test]
fn stale_session_lock_reclamation_matches_published_policy()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("none")? else {
        return Ok(());
    };
    let lock = serde_json::to_vec(&serde_json::json!({
        "id":"session-state", "pid":1, "owner":"session-state",
        "created_at":"2000-01-01T00:00:00.000Z", "ttl_seconds":1800,
        "token":"stale-session-fixture"
    }))?;
    for action in ["claim", "release"] {
        for root in [fixture.node.path(), fixture.rust.path()] {
            fs::write(root.join(".agents/pm/locks/session-state.lock"), &lock)?;
        }
        assert_eq!(
            fixture.step_with_exit(&[action, "sample-own", "--author", "alpha", "--json"])?,
            Some(0),
            "{action}"
        );
        for root in [fixture.node.path(), fixture.rust.path()] {
            assert!(!root.join(".agents/pm/locks/session-state.lock").exists());
            assert!(root.join(".agents/pm/runtime/session.json").exists());
        }
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

/// The oracle saves a new claim under a busy session lease and leaves attribution unchanged.
#[test]
fn saved_claim_with_busy_session_matches_published_state() -> Result<(), Box<dyn std::error::Error>>
{
    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(());
    };
    let lock = ".agents/pm/locks/session-state.lock";
    let session = ".agents/pm/runtime/session.json";
    let initial_session = fs::read(fixture.node.path().join(session)).ok();
    let lease = r#"{"id":"session-state","pid":1,"owner":"fixture","created_at":"2126-01-01T00:00:00Z","ttl_seconds":1800,"token":"fixture-token"}"#;
    for root in [fixture.node.path(), fixture.rust.path()] {
        fs::write(root.join(lock), lease)?;
    }
    let oracle = published_run(
        &fixture.driver,
        fixture.node.path(),
        &["__ownership_error", "claim", "sample-own"],
    )?;
    assert!(oracle.status.success());
    assert_eq!(oracle.stdout, b"{\"ok\":true}\n");
    let workspace = pm_rust::Workspace::discover(fixture.rust.path())?;
    let receipt = workspace.claim(&pm_rust::OwnershipItem {
        id: "sample-own".to_owned(),
        author: "alpha".to_owned(),
        timestamp: Some(CLOCK.to_owned()),
        message: None,
        force: false,
        if_available: false,
        provenance_role: Some("implementer".to_owned()),
    })?;
    assert_eq!(receipt.changed_field_count, 2);
    assert_eq!(receipt.warnings, ["claim_semantic_attribution_unrecorded"]);
    assert!(receipt.suggestions[0].contains("check the session's active-item attribution"));
    fixture.compare_bytes()?;
    for root in [fixture.node.path(), fixture.rust.path()] {
        assert_eq!(fs::read_to_string(root.join(lock))?, lease);
        fs::remove_file(root.join(lock))?;
        assert_eq!(fs::read(root.join(session)).ok(), initial_session);
    }
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
        &serde_json::json!({"id":"session-state","pid":1,"owner":"session-state","created_at":"2126-01-01T00:00:00.000Z","ttl_seconds":1800,"token":"active-session-fixture"}),
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
    let args = [
        "claim",
        "sample-own",
        "--start",
        "--author",
        "alpha",
        "--json",
    ];
    let oracle = published_run(&fixture.driver, fixture.node.path(), &args)?;
    let native = native_command(fixture.rust.path(), &args).output()?;
    assert!(oracle.status.success());
    assert_eq!(native.status.code(), oracle.status.code());
    assert_eq!(native.stderr, oracle.stderr);
    let oracle: serde_json::Value = serde_json::from_slice(&oracle.stdout)?;
    let mut native: serde_json::Value = serde_json::from_slice(&native.stdout)?;
    let oracle_advice = oracle["claim"]["suggestions"][0]
        .as_str()
        .ok_or("oracle advice")?;
    let native_advice = native["claim"]["suggestions"][0]
        .as_str()
        .ok_or("native advice")?;
    assert!(oracle_advice.contains("nothing needs a retry"));
    assert!(native_advice.contains("The claim of sample-own is saved"));
    assert!(native_advice.contains("active-item attribution against current ownership"));
    assert!(!native_advice.contains("retry"));
    // This sentence is native recovery guidance. The remaining receipt matches the oracle.
    let suggestions = native["claim"]["suggestions"]
        .as_array_mut()
        .ok_or("native suggestions")?;
    for suggestion in &suggestions[1..] {
        let suggestion = suggestion
            .as_str()
            .ok_or("native suggestion")?
            .to_ascii_lowercase();
        assert!(
            ["retry", "replay", "rerun", "re-run"]
                .iter()
                .all(|word| !suggestion.contains(word))
        );
    }
    suggestions[0] = oracle["claim"]["suggestions"][0].clone();
    assert_eq!(native, oracle);
    fixture.compare_bytes()?;
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

/// Composed and explicit lifecycle commands reclaim expired derived-state locks.
#[test]
fn stale_session_lock_lifecycle_matches_published_bytes() -> Result<(), Box<dyn std::error::Error>>
{
    for (action, owned) in [
        ("claim", false),
        ("release", true),
        ("start-task", false),
        ("pause-task", true),
        ("pause-task", false),
    ] {
        let Some(fixture) = Fixture::new("none")? else {
            return Ok(());
        };
        if owned {
            prepare(
                &fixture,
                &["start-task", "sample-own", "--author", "alpha", "--json"],
            )?;
        }
        let lock = r#"{"id":"session-state","pid":1,"token":"session-fixture","ttl_seconds":1800,"created_at":"2000-01-01T00:00:00.000Z","owner":"session-state"}"#;
        for root in [fixture.node.path(), fixture.rust.path()] {
            fs::write(root.join(".agents/pm/locks/session-state.lock"), lock)?;
        }
        assert_eq!(
            fixture.step_with_exit(&[action, "sample-own", "--author", "alpha", "--json"])?,
            Some(0),
            "{action}, owned={owned}"
        );
        for root in [fixture.node.path(), fixture.rust.path()] {
            assert!(
                !root.join(".agents/pm/locks/session-state.lock").exists(),
                "{action}, owned={owned}"
            );
        }
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
    for payload in [
        None,
        Some(""),
        Some(" \n\t"),
        Some(r#"{"created_at":"2000-01-01T00:00:00Z","owner":"fixture"}"#),
    ] {
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
        let actual = native_command(fixture.rust.path(), &args).output()?;
        for output in [&expected, &actual] {
            assert_eq!(output.status.code(), Some(4));
            let refusal: serde_json::Value = serde_json::from_slice(&output.stderr)?;
            assert_eq!(refusal["code"], "lock_conflict");
        }
        if payload.is_none_or(|raw| raw.trim().is_empty()) {
            assert_eq!(expected.stderr, actual.stderr);
        } else {
            let refusal: serde_json::Value = serde_json::from_slice(&actual.stderr)?;
            let required = refusal["required"].as_str().ok_or("required")?;
            assert!(
                required.contains("then retry the same operation"),
                "{refusal}"
            );
            assert!(required.contains("PM_LOCK_WAIT_MS"), "{refusal}");
            assert_eq!(
                refusal["recovery"]["suggested_retry_args"],
                serde_json::json!(args)
            );
            assert!(!refusal.to_string().contains("--force"));
        }
    }
    fs::File::unlock(&lease)?;
    Ok(())
}

/// An abandoned receipt requires force regardless of whether its payload was written.
#[test]
fn abandoned_blank_lock_receipts_match_oracle() -> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(());
    };
    let args = ["claim", "sample-own", "--author", "alpha", "--json"];
    for raw in ["", " \n\t"] {
        for project in [fixture.node.path(), fixture.rust.path()] {
            fs::write(project.join(".agents/pm/locks/sample-own.lock"), raw)?;
        }
        let before = fixture.snapshots()?;
        let oracle = published_run(&fixture.driver, fixture.node.path(), &args)?;
        let native = native_command(fixture.rust.path(), &args).output()?;
        for output in [oracle, native] {
            assert_eq!(output.status.code(), Some(4));
            let refusal: serde_json::Value = serde_json::from_slice(&output.stderr)?;
            assert_eq!(refusal["code"], "stale_lock_requires_override");
            assert_eq!(refusal["details"]["lock_path"], "locks/sample-own.lock");
            let guidance = refusal["next_steps"][0]
                .as_str()
                .ok_or("cleanup guidance")?;
            assert!(guidance.contains("once its owner is confirmed gone"));
            assert!(guidance.contains("pm gc --scope locks keeps"));
        }
        assert_eq!(fixture.snapshots()?, before);
        for project in [fixture.node.path(), fixture.rust.path()] {
            assert_eq!(
                fs::read_to_string(project.join(".agents/pm/locks/sample-own.lock"))?,
                raw
            );
        }
    }
    assert_eq!(
        fixture.step_with_exit(&[
            "claim",
            "sample-own",
            "--author",
            "alpha",
            "--force",
            "--json",
        ])?,
        Some(0)
    );
    for root in [fixture.node.path(), fixture.rust.path()] {
        let workspace = pm_rust::Workspace::discover(root)?;
        assert_eq!(
            workspace.get("sample-own")?.metadata.extra["assignee"].as_str(),
            Some("alpha")
        );
    }
    Ok(())
}

/// A malformed timestamp needs owner-verified cleanup because oracle GC retains the lock.
#[test]
fn undated_lock_recovery_matches_oracle_guidance() -> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(());
    };
    let raw = r#"{"id":"sample-own","pid":1,"owner":"fixture","created_at":"unknown","ttl_seconds":1800}"#;
    for root in [fixture.node.path(), fixture.rust.path()] {
        fs::write(root.join(".agents/pm/locks/sample-own.lock"), raw)?;
    }
    let before = fixture.snapshots()?;
    let args = ["claim", "sample-own", "--author", "alpha", "--json"];
    for output in [
        published_run(&fixture.driver, fixture.node.path(), &args)?,
        native_command(fixture.rust.path(), &args).output()?,
    ] {
        assert_eq!(output.status.code(), Some(4));
        let refusal: serde_json::Value = serde_json::from_slice(&output.stderr)?;
        assert_eq!(refusal["code"], "stale_lock_requires_override");
        assert_eq!(refusal["details"]["lock_path"], "locks/sample-own.lock");
        let guidance = refusal["next_steps"][0]
            .as_str()
            .ok_or("cleanup guidance")?;
        assert!(guidance.starts_with("Remove locks/sample-own.lock"));
        assert!(guidance.contains("once its owner is confirmed gone"));
        assert!(guidance.contains("pm gc --scope locks keeps"));
    }
    let gc = published_run(
        &fixture.driver,
        fixture.node.path(),
        &["gc", "--scope", "locks", "--json"],
    )?;
    assert!(
        gc.status.success(),
        "{}",
        String::from_utf8_lossy(&gc.stderr)
    );
    let receipt = String::from_utf8(gc.stdout)?;
    assert!(receipt.contains("invalid_timestamp"), "{receipt}");
    assert_eq!(fixture.snapshots()?, before);
    for root in [fixture.node.path(), fixture.rust.path()] {
        assert_eq!(
            fs::read_to_string(root.join(".agents/pm/locks/sample-own.lock"))?,
            raw
        );
    }
    Ok(())
}

/// The pinned settings loader accepts a missing or null guard and every optional field.
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "the settings matrix and durable write assertions stay together"
)]
fn create_accepts_mutation_guard_shapes() -> Result<(), Box<dyn std::error::Error>> {
    use std::fmt::Write as _;

    let Some(fixture) = Fixture::new("none")? else {
        return Ok(());
    };
    for (guard, id) in [
        (None, "pm-absent"),
        (Some("null"), "pm-null"),
        (Some("{}"), "pm-empty"),
        (Some(r#"{"stale_in_progress_hours":null}"#), "pm-null-hours"),
        (Some(r#"{"stale_in_progress_hours":0}"#), "pm-zero-hours"),
        (
            Some(r#"{"stale_in_progress_hours":1.5}"#),
            "pm-fraction-hours",
        ),
        (
            Some(r#"{"stale_in_progress_hours":"72"}"#),
            "pm-string-hours",
        ),
        (Some(r#"{"require_attributed_author":true}"#), "pm-author"),
        (Some(r#"{"secret_guard":"off"}"#), "pm-off"),
        (Some(r#"{"secret_guard":"advise"}"#), "pm-advise"),
        (Some(r#"{"secret_guard":"block"}"#), "pm-block"),
        (Some(r#"{"stale_in_progress_hours":24}"#), "pm-integer"),
        (Some(r#"{"stale_in_progress_hours":72.0}"#), "pm-decimal"),
        (Some(r#"{"stale_in_progress_hours":24e0}"#), "pm-exponent"),
        (
            Some(
                r#"{"require_attributed_author":false,"secret_guard":"block","stale_in_progress_hours":9007199254740991}"#,
            ),
            "pm-all",
        ),
    ] {
        for root in [fixture.node.path(), fixture.rust.path()] {
            let path = root.join(".agents/pm/settings.json");
            let mut settings: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
            // Explicit null makes the oracle load all default settings, including the prefix.
            settings["id_prefix"] = serde_json::json!("pm-");
            settings
                .as_object_mut()
                .ok_or("settings object")?
                .shift_remove("mutation_guard");
            let mut wire = settings.to_string();
            if let Some(guard) = guard {
                wire.pop();
                write!(wire, ",\"mutation_guard\":{guard}}}")?;
            }
            fs::write(path, wire)?;
        }
        let args = [
            "create",
            "--id",
            id,
            "--title",
            "Threshold settings",
            "--type",
            "Task",
            "--author",
            "alpha",
        ];
        let oracle = published_run(&fixture.driver, fixture.node.path(), &args)?;
        let native = native_command(fixture.rust.path(), &args).output()?;
        for output in [&oracle, &native] {
            assert!(
                output.status.success(),
                "guard {guard:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let invalid = matches!(
            id,
            "pm-null" | "pm-null-hours" | "pm-zero-hours" | "pm-fraction-hours" | "pm-string-hours"
        );
        for output in [&oracle, &native] {
            assert_eq!(
                String::from_utf8_lossy(&output.stderr).contains("settings_read_invalid_schema"),
                invalid,
                "guard {guard:?}"
            );
        }
        if id == "pm-null-hours" {
            let args = ["claim", id, "--author", "alpha", "--json"];
            for output in [
                published_run(&fixture.driver, fixture.node.path(), &args)?,
                native_command(fixture.rust.path(), &args).output()?,
            ] {
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                let warning: serde_json::Value = serde_json::from_slice(&output.stderr)?;
                assert_eq!(
                    warning["warnings"],
                    serde_json::json!(["settings_read_invalid_schema"])
                );
            }
        }
        fixture.compare_bytes()?;
    }
    Ok(())
}

/// Compare invalid field defaults with the metadata API of the exact published package.
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "the complete settings matrix shares one oracle process"
)]
fn invalid_mutation_setting_defaults_match_published() -> Result<(), Box<dyn std::error::Error>> {
    use serde_json::{Value, json};
    let Some(published) = published_cli_or_skip("mutation settings defaults") else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    let driver = root.path().join("settings.mjs");
    fs::write(
        &driver,
        r"
import { readFileSync, writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { join } from 'node:path';
const { readSettingsWithMetadata } = await import(pathToFileURL(process.argv[2]));
const { base, cases } = JSON.parse(readFileSync(process.argv[3], 'utf8'));
const results = [];
for (const [index, [key, raw]] of cases.entries()) {
  const path = join(process.argv[4], String(index));
  const settings = structuredClone(base);
  const parts = key.split('.');
  const leaf = parts.pop();
  const parent = parts.reduce((node, part) => node[part] ??= {}, settings);
  parent[leaf] = raw;
  writeFileSync(join(path, 'settings.json'), JSON.stringify(settings));
  const resolved = await readSettingsWithMetadata(path);
  results.push({
    warnings: resolved.warnings,
    value: [...parts, leaf].reduce((node, part) => node?.[part], resolved.settings)
  });
}
process.stdout.write(JSON.stringify(results));
",
    )?;
    let mut base: Value = serde_json::from_str(include_str!("fixtures/list-settings.json"))?;
    base["governance"]["preset"] = json!("minimal");
    let cases = [
        ("id_prefix", json!("pm-"), true),
        ("author_default", json!(""), true),
        ("item_format", json!("toon"), true),
        ("locks", json!({"ttl_seconds":1800,"wait_ms":3000}), true),
        ("locks.ttl_seconds", json!(1800), true),
        ("locks.wait_ms", json!(3000), true),
        ("workflow", json!({}), true),
        // The published reader ignores the legacy flat close status. Both paths use closed.
        ("workflow.close_status", json!("closed"), false),
        ("governance", json!({"preset":"minimal"}), true),
        ("governance.preset", json!("minimal"), true),
        (
            "governance.force_required_for_stale_lock",
            json!(true),
            true,
        ),
        ("governance.ownership_enforcement", json!("warn"), true),
        ("governance.require_close_reason", json!(true), true),
        ("mutation_guard", json!({}), true),
        ("mutation_guard.stale_in_progress_hours", json!(72), true),
        (
            "mutation_guard.require_attributed_author",
            json!(false),
            true,
        ),
        ("mutation_guard.secret_guard", json!("advise"), true),
    ];
    let checks: Vec<_> = cases
        .iter()
        .flat_map(|case| {
            let mut values = vec![Value::Null, json!([])];
            match case.0 {
                "locks.ttl_seconds" => values.push(json!(1.5)),
                "locks.wait_ms" => values.push(json!(-1)),
                _ => {}
            }
            values.into_iter().map(move |value| (case, value))
        })
        .collect();
    let inputs: Vec<_> = checks
        .iter()
        .map(|((key, ..), value)| json!([key, value]))
        .collect();
    for index in 0..inputs.len() {
        fs::create_dir(root.path().join(index.to_string()))?;
    }
    let input = root.path().join("inputs.json");
    fs::write(&input, json!({"base":base,"cases":inputs}).to_string())?;
    let mut command = published_cli::oracle_command(
        std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned()),
    );
    mutation_differential::isolate_environment(&mut command);
    let output = command
        .arg(driver)
        .arg(published.package_root.join("dist/core/store/settings.js"))
        .arg(input)
        .arg(root.path())
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Vec<Value> = serde_json::from_slice(&output.stdout)?;
    assert_eq!(oracle.len(), checks.len());
    for (((key, expected, oracle_warns), invalid), resolved) in checks.into_iter().zip(oracle) {
        let pointer = format!("/{}", key.replace('.', "/"));
        let (parent, leaf) = pointer.rsplit_once('/').ok_or("setting path")?;
        let mut native = base.clone();
        native.pointer_mut(parent).ok_or("settings parent")?[leaf] = invalid;
        assert_eq!(
            pm_rust::normalize_mutation_settings(&mut native),
            *oracle_warns,
            "{key}"
        );
        assert_eq!(native.pointer(&pointer), Some(expected), "{key}");
        assert_eq!(
            resolved["warnings"] == json!(["settings_read_invalid_schema"]),
            *oracle_warns,
            "{key}"
        );
        match *key {
            "governance" => assert_eq!(resolved["value"]["preset"], "minimal"),
            "governance.force_required_for_stale_lock" => assert_eq!(resolved["value"], false),
            "governance.ownership_enforcement" => assert_eq!(resolved["value"], "none"),
            "mutation_guard" => assert_eq!(resolved["value"]["stale_in_progress_hours"], 72),
            "workflow" => assert_eq!(resolved["value"], json!({"definition_of_done":[]})),
            "workflow.close_status" => assert!(resolved.get("value").is_none()),
            _ => assert_eq!(&resolved["value"], expected, "{key}"),
        }
        if *key != "id_prefix" {
            assert_eq!(native["id_prefix"], "demo-");
        }
    }
    Ok(())
}

/// A stored credential named like an option leaves that option usable in the attempted command.
#[test]
fn stored_holder_cannot_redact_fixed_author_option() -> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(());
    };
    for project in [fixture.node.path(), fixture.rust.path()] {
        let path = project.join(".agents/pm/tasks/sample-own.toon");
        let mut item = fs::read_to_string(&path)?;
        item.push_str("assignee: custom:author%40example.invalid\n");
        fs::write(path, item)?;
    }
    let args = ["claim", "sample-own", "--author", "beta", "--json"];
    for output in [
        published_run(&fixture.driver, fixture.node.path(), &args)?,
        native_command(fixture.rust.path(), &args).output()?,
    ] {
        assert_eq!(output.status.code(), Some(4));
        let payload: serde_json::Value = serde_json::from_slice(&output.stderr)?;
        assert_eq!(payload["claimed_by"], "custom:[redacted]%40example.invalid");
        assert_eq!(
            payload["recovery"]["normalized_args"],
            serde_json::json!(args)
        );
        assert_eq!(
            payload["recovery"]["provided_fields"],
            serde_json::json!(["--author", "--json"])
        );
        assert_eq!(
            payload["recovery"]["attempted_command"],
            "pm claim sample-own --author beta --json"
        );
    }
    fixture.compare_bytes()
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
    let asserted = "https://fixture:stored-password@example.invalid";
    for action in ["claim", "release"] {
        let args = [
            action,
            "sample-own",
            "--author",
            asserted,
            "--force",
            "--json",
        ];
        fixture.step(&args)?;
        fixture.step(&args[..args.len() - 1])?;
        let output = native_command(fixture.rust.path(), &args).output()?;
        assert_eq!(output.status.code(), Some(2));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("stored-password"));
    }
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

/// Stale foreign claims report exact advice and preserve item and history bytes.
#[test]
fn stale_foreign_claim_refusal_matches_published() -> Result<(), Box<dyn std::error::Error>> {
    let earlier = "2026-08-18T10:00:00.000Z";
    let Some(fixture) = Fixture::new_at("none", earlier)? else {
        return Ok(());
    };
    let claim = published_command(
        &fixture.driver,
        fixture.node.path(),
        &["claim", "sample-own", "--author", "alpha", "--json"],
    )
    .env("FIXED_CLOCK", earlier)
    .output()?;
    assert!(
        claim.status.success(),
        "{}",
        String::from_utf8_lossy(&claim.stderr)
    );
    copy_directory(
        &fixture.node.path().join(".agents"),
        &fixture.rust.path().join(".agents"),
    )?;
    fixture.verify()?;
    let args = ["claim", "sample-own", "--author", "beta"];
    let before = fixture.snapshots()?;
    fixture.step(&args)?;
    assert_eq!(fixture.snapshots()?, before);
    let human = native_command(fixture.rust.path(), &args).output()?;
    assert_eq!(fixture.snapshots()?, before);
    assert_eq!(human.status.code(), Some(4));
    assert!(String::from_utf8(human.stderr)?.contains("idle 96h, stale"));
    let args = ["claim", "sample-own", "--author", "beta", "--json"];
    let before = fixture.snapshots()?;
    fixture.step(&args)?;
    assert_eq!(fixture.snapshots()?, before);
    let output = native_command(fixture.rust.path(), &args).output()?;
    assert_eq!(fixture.snapshots()?, before);
    let receipt: serde_json::Value = serde_json::from_slice(&output.stderr)?;
    assert_eq!(receipt["claim_source"], "claim");
    assert_eq!(receipt["claimed_at"], earlier);
    assert_eq!(receipt["claim_activity"]["claim_age_hours"], 96);
    assert_eq!(receipt["claim_activity"]["stale_policy"]["stale"], true);
    assert_eq!(
        receipt["claim_activity"]["stale_policy"]["eligible_at"],
        "2026-08-21T10:00:00.000Z"
    );
    Ok(())
}

/// The CLI refuses private explicit and environment authors before altering a tracker.
#[test]
fn credential_author_sources_refuse_without_writes() -> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("none")? else {
        return Ok(());
    };
    let credential = "https://fixture:synthetic-secret@example.invalid";
    let attached = format!("--author={credential}");
    for args in [
        vec![
            "create",
            "--id",
            "sample-other",
            "--title",
            "Other",
            "--type",
            "Task",
            &attached,
        ],
        vec![
            "files",
            "sample-own",
            "--add",
            "example.rs",
            &attached,
            "--json",
        ],
        vec!["update", "sample-own", "--title", "Changed", &attached],
    ] {
        let before = fixture.snapshots()?;
        let expected = published_run(&fixture.driver, fixture.node.path(), &args)?;
        let actual = native_command(fixture.rust.path(), &args).output()?;
        for output in [&expected, &actual] {
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert_eq!(output.status.code(), Some(2), "{args:?}: {stderr}");
            assert!(output.stdout.is_empty());
            assert!(!stderr.contains("synthetic-secret"));
            assert!(stderr.contains("--author"), "{args:?}: {stderr}");
        }
        if args.contains(&"--json") {
            let refusal: serde_json::Value = serde_json::from_slice(&actual.stderr)?;
            assert_eq!(refusal["code"], "invalid_argument_value");
        }
        assert_eq!(fixture.snapshots()?, before);
        fixture.compare_bytes()?;
    }
    let args = ["claim", "sample-own", "--json"];
    let before = fixture.snapshots()?;
    let actual = native_command(fixture.rust.path(), &args)
        .env("PM_AUTHOR", credential)
        .output()?;
    assert_eq!(actual.status.code(), Some(2));
    let stderr = String::from_utf8(actual.stderr)?;
    assert!(!stderr.contains("synthetic-secret"));
    let refusal: serde_json::Value = serde_json::from_str(&stderr)?;
    assert_eq!(refusal["refusal"]["surface"], "claim");
    assert_eq!(fixture.snapshots()?, before);
    fixture.compare_bytes()
}

/// A path at-sign must not prevent redaction of a short authority password copied into argv.
#[test]
fn environment_identity_projects_short_authority_password() -> Result<(), Box<dyn std::error::Error>>
{
    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(());
    };
    let before = fixture.snapshots()?;
    let args = ["claim", "sample-own", "--message", "pw", "--json"];
    let author = "https://fixture:pw@example.invalid/users/@fixture";
    let published = published_command(&fixture.driver, fixture.node.path(), &args)
        .env("PM_AUTHOR", author)
        .output()?;
    let native = native_command(fixture.rust.path(), &args)
        .env("PM_AUTHOR", author)
        .output()?;
    for output in [&published, &native] {
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
    let refusal: serde_json::Value = serde_json::from_slice(&native.stderr)?;
    assert!(
        refusal["recovery"]["attempted_command"]
            .as_str()
            .ok_or("attempted command")?
            .contains("[redacted]")
    );
    assert!(!String::from_utf8_lossy(&native.stderr).contains("pw"));
    assert_eq!(fixture.snapshots()?, before);
    Ok(())
}

/// A resolved SDK collection identity is subject to the oracle's author admission policy.
#[test]
fn sdk_collection_identity_must_be_credential_free() -> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(());
    };
    let credential = "https://fixture:synthetic-secret@example.invalid";
    let expected = published_run(
        &fixture.driver,
        fixture.node.path(),
        &[
            "files",
            "sample-own",
            "--add",
            "example.rs",
            "--author",
            credential,
            "--json",
        ],
    )?;
    assert_eq!(expected.status.code(), Some(2));
    assert!(!String::from_utf8_lossy(&expected.stderr).contains("synthetic-secret"));
    let before = fixture.snapshots()?;
    let workspace = pm_rust::Workspace::discover(fixture.rust.path())?;
    let env = std::collections::BTreeMap::new();
    let resolved = pm_rust::AuthorIdentity::resolve(Some(credential), None, &env);
    let clean = pm_rust::AuthorIdentity::resolve(Some("fixture"), None, &env);
    let identities = [
        (
            pm_rust::AuthorIdentity {
                principal: clean.principal.clone(),
                ..resolved.clone()
            },
            "asserted",
        ),
        (
            pm_rust::AuthorIdentity {
                principal: resolved.principal,
                ..clean
            },
            "claim_principal",
        ),
    ];
    for (identity, expected_source) in identities {
        let mut options = pm_rust::CollectionOptions {
            add: vec!["example.rs".to_owned()],
            author: "fixture".to_owned(),
            identity: Some(identity),
            ..Default::default()
        };
        assert!(matches!(
            workspace.collection("sample-own", "files", &options, fixture.rust.path()),
            Err(pm_rust::PmRustError::IdentityCredentials { identity_source }) if identity_source == expected_source
        ));
        options.add.clear();
        workspace.collection("sample-own", "files", &options, fixture.rust.path())?;
    }
    assert_eq!(fixture.snapshots()?, before);
    Ok(())
}

/// A forced release only edits the releasing principal's attribution.
#[test]
fn forced_foreign_release_preserves_prior_holder_workset() -> Result<(), Box<dyn std::error::Error>>
{
    let Some(fixture) = Fixture::new("strict")? else {
        return Ok(());
    };
    let session = fixture.rust.path().join(".agents/pm/runtime/session.json");
    fixture.step(&["claim", "sample-own", "--author", "fixture", "--json"])?;
    let before = fs::read(&session)?;
    let state: serde_json::Value = serde_json::from_slice(&before)?;
    let attribution = state["semantic_attribution"]
        .as_object()
        .ok_or("fixture workset")?;
    assert_eq!(attribution.len(), 1);
    assert_eq!(
        attribution.values().next().ok_or("fixture attribution")?["active_item_ids"],
        serde_json::json!(["sample-own"])
    );
    fixture.step(&[
        "release",
        "sample-own",
        "--author",
        "maintainer",
        "--force",
        "--json",
    ])?;
    assert_eq!(fs::read(session)?, before);
    let workspace = pm_rust::Workspace::discover(fixture.rust.path())?;
    assert!(
        workspace
            .get("sample-own")?
            .metadata
            .extra
            .get("assignee")
            .is_none()
    );
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
/// Redacting an external selector preserves diagnostics and withdraws its update command.
#[test]
fn external_blocker_removal_guidance_respects_selector_projection()
-> Result<(), Box<dyn std::error::Error>> {
    for private in [false, true] {
        let Some(fixture) = Fixture::new("none")? else {
            return Ok(());
        };
        let id = if private {
            "https://fixture:synthetic-secret@example.invalid/issue"
        } else {
            "https://example.invalid/issue"
        };
        let selector = format!("id={id},kind=blocked_by");
        let dependency = format!("{selector},source_kind=external");
        prepare(
            &fixture,
            &[
                "update",
                "sample-own",
                "--dep",
                &dependency,
                "--author",
                "alpha",
                "--json",
            ],
        )?;
        let before = fixture.snapshots()?;
        let args = [
            "claim",
            "sample-own",
            "--start",
            "--author",
            "alpha",
            "--json",
        ];
        let oracle = published_run(&fixture.driver, fixture.node.path(), &args)?;
        let native = native_command(fixture.rust.path(), &args).output()?;
        assert_eq!(oracle.status.code(), Some(4));
        assert_eq!(native.status.code(), Some(4));
        let oracle: serde_json::Value = serde_json::from_slice(&oracle.stderr)?;
        let native: serde_json::Value = serde_json::from_slice(&native.stderr)?;
        assert_eq!(oracle["code"], "start_blocked_by_open_dependency");
        assert_eq!(native["code"], oracle["code"]);
        let workspace = pm_rust::Workspace::discover(fixture.rust.path())?;
        let error = workspace
            .claim_with_options(
                &pm_rust::OwnershipItem {
                    id: "sample-own".to_owned(),
                    author: "alpha".to_owned(),
                    timestamp: Some(CLOCK.to_owned()),
                    message: None,
                    force: false,
                    if_available: false,
                    provenance_role: None,
                },
                &pm_rust::OwnershipOptions {
                    start: true,
                    ..Default::default()
                },
            )
            .err()
            .ok_or("blocker admission")?;
        let pm_rust::PmRustError::OwnershipRefusal { detail, context } = error else {
            return Err(format!("unexpected blocker refusal: {error}").into());
        };
        for receipt in [&native, &context] {
            let steps = receipt["next_steps"].to_string();
            if private {
                assert!(!steps.contains("--dep-remove"), "{steps}");
                assert!(steps.contains("original ID"), "{steps}");
                assert!(!receipt.to_string().contains("synthetic-secret"));
                assert_eq!(
                    receipt["details"]["open_blockers"][0]["id"],
                    "https://[redacted]@example.invalid/issue"
                );
            } else {
                assert_eq!(receipt["next_steps"], oracle["next_steps"]);
                assert!(
                    steps.contains("--dep-remove") && steps.contains(&selector),
                    "{steps}"
                );
            }
        }
        if private {
            assert!(!detail.contains("synthetic-secret"));
        }
        assert_eq!(fixture.snapshots()?, before);
        fixture.compare_bytes()?;
    }
    Ok(())
}

/// A second blocker's userinfo cannot corrupt the first selector or redirect an inspection.
#[test]
fn scoped_blocker_inspection_survives_cross_blocker_redaction()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("none")? else {
        return Ok(());
    };
    let selector = "id=github:get,kind=blocked_by";
    for dependency in [
        format!("{selector},source_kind=external"),
        "id=https://get:synthetic-secret@example.invalid/issue,kind=blocked_by,source_kind=external".to_owned(),
    ] {
        prepare(&fixture, &["update", "sample-own", "--dep", &dependency, "--author", "alpha", "--json"])?;
    }
    assert_scoped_blocker_inspection(&fixture)
}

/// A local blocker's title can supply credentials copied by an external selector.
#[test]
fn blocker_title_projection_withdraws_removal_command() -> Result<(), Box<dyn std::error::Error>> {
    let Some(fixture) = Fixture::new("none")? else {
        return Ok(());
    };
    prepare(
        &fixture,
        &[
            "create",
            "--id",
            "sample-gate",
            "--title",
            "Review https://review-token:synthetic-secret@example.invalid",
            "--type",
            "Task",
            "--author",
            "alpha",
            "--json",
        ],
    )?;
    for dependency in [
        "id=github:review-token,kind=blocked_by,source_kind=external",
        "id=sample-gate,kind=blocked_by",
    ] {
        prepare(
            &fixture,
            &[
                "update",
                "sample-own",
                "--dep",
                dependency,
                "--author",
                "alpha",
                "--json",
            ],
        )?;
    }
    assert_scoped_blocker_inspection(&fixture)
}

/// Checks atomic refusal and inspection replay across both tracker selections.
fn assert_scoped_blocker_inspection(fixture: &Fixture) -> Result<(), Box<dyn std::error::Error>> {
    let args = [
        "claim",
        "sample-own",
        "--start",
        "--author",
        "alpha",
        "--json",
    ];
    let before = fixture.snapshots()?;
    let oracle = published_run(&fixture.driver, fixture.node.path(), &args)?;
    assert_eq!(oracle.status.code(), Some(4));
    let expected: serde_json::Value = serde_json::from_slice(&oracle.stderr)?;
    assert_eq!(expected["code"], "start_blocked_by_open_dependency");

    let target = fixture.rust.path().join(".agents/pm");
    let target = target.to_str().ok_or("tracker path")?;
    let scoped_args = ["--pm-path", target]
        .into_iter()
        .chain(args)
        .collect::<Vec<_>>();
    let native = native_command(fixture.node.path(), &scoped_args).output()?;
    assert_eq!(
        native.status.code(),
        Some(4),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    let actual: serde_json::Value = serde_json::from_slice(&native.stderr)?;
    assert_eq!(actual["code"], expected["code"]);
    let steps = actual["next_steps"].as_array().ok_or("guidance")?;
    let inspect = pm_rust::render_pm_command([
        "--pm-path",
        target,
        "get",
        "sample-own",
        "--fields",
        "blocked_by,dependencies",
    ]);
    assert!(
        steps.windows(2).any(|pair| {
            pair[0] == inspect
                && pair[1] == "Use the dependency's original ID to remove the blocked_by edge."
        }),
        "{actual}"
    );
    assert!(!actual.to_string().contains("--dep-remove"));
    assert!(!actual.to_string().contains("synthetic-secret"));
    assert_eq!(
        actual["details"]["open_blockers"][0]["id"],
        "github:[redacted]"
    );
    assert_eq!(fixture.snapshots()?, before);
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
        assert_eq!(actual.stderr, oracle.stderr, "separator {separator:?}");
        let envelope: serde_json::Value = serde_json::from_slice(&actual.stderr)?;
        assert_eq!(envelope["claimed_by"], "https://[redacted]@host.invalid");
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

/// Published blocker advice quotes stored shell syntax, preserving all other bytes and writes.
#[test]
fn round15_blocker_advice_matches_published_bytes() -> Result<(), Box<dyn std::error::Error>> {
    for blocked_by in [
        "sample-missing",
        "sample-missing;id",
        "sample-$(printf injected)",
        "sample-\"quote",
    ] {
        let Some(fixture) = Fixture::new("none")? else {
            return Ok(());
        };
        for root in [fixture.node.path(), fixture.rust.path()] {
            let path = root.join(".agents/pm/tasks/sample-own.toon");
            let mut item: serde_json::Value =
                toon_format::decode_default(&fs::read_to_string(&path)?)?;
            item["blocked_by"] = serde_json::json!(blocked_by);
            fs::write(path, toon_format::encode_default(&item)?)?;
        }
        let before = start_snapshot(fixture.rust.path())?;
        fixture.step(&[
            "claim",
            "sample-own",
            "--start",
            "--author",
            "alpha",
            "--json",
        ])?;
        assert_eq!(start_snapshot(fixture.rust.path())?, before);
    }
    Ok(())
}

/// Published lifecycle admission still refuses stored absent-reference marker strings.
#[test]
fn round17_absent_blocker_starts_match_oracle() -> Result<(), Box<dyn std::error::Error>> {
    for marker in ["none", "NULL", "n/a", "Na"] {
        for atomic in [false, true] {
            let Some(fixture) = Fixture::new("none")? else {
                return Ok(());
            };
            for root in [fixture.node.path(), fixture.rust.path()] {
                let path = root.join(".agents/pm/tasks/sample-own.toon");
                let mut item: serde_json::Value =
                    toon_format::decode_default(&fs::read_to_string(&path)?)?;
                item["blocked_by"] = serde_json::json!(marker);
                fs::write(path, toon_format::encode_default(&item)?)?;
            }
            let args = if atomic {
                vec![
                    "claim",
                    "sample-own",
                    "--start",
                    "--author",
                    "alpha",
                    "--json",
                ]
            } else {
                vec!["start-task", "sample-own", "--author", "alpha", "--json"]
            };
            let native_before = start_snapshot(fixture.rust.path())?;
            let oracle_before = start_snapshot(fixture.node.path())?;
            let published = published_run(&fixture.driver, fixture.node.path(), &args)?;
            assert_eq!(published.status.code(), Some(4));
            let payload: serde_json::Value = serde_json::from_slice(&published.stderr)?;
            assert_eq!(payload["code"], "start_blocked_by_open_dependency");
            fixture.step(&args)?;
            assert_eq!(start_snapshot(fixture.rust.path())?, native_before);
            assert_eq!(start_snapshot(fixture.node.path())?, oracle_before);
        }
    }
    Ok(())
}

/// Ranked ownership uses the oracle's last-definition precedence for external status overlays.
#[test]
fn round17_external_duplicate_status_claim_next_matches_oracle()
-> Result<(), Box<dyn std::error::Error>> {
    for role in ["active", "terminal"] {
        let Some(fixture) = Fixture::new("none")? else {
            return Ok(());
        };
        for root in [fixture.node.path(), fixture.rust.path()] {
            let pm = root.join(".agents/pm");
            let path = pm.join("settings.json");
            let mut settings: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
            settings["schema"]["statuses"] = serde_json::json!([{"id":"open","roles":["active"]},{"id":"OPEN","roles":["terminal"]}]);
            fs::write(path, serde_json::to_vec_pretty(&settings)?)?;
            fs::write(
                pm.join("schema/statuses.json"),
                serde_json::json!({"statuses":[{"id":"open","roles":[role]}]}).to_string(),
            )?;
        }
        let before = start_snapshot(fixture.rust.path())?;
        fixture.step(&["claim", "--next", "--author", "alpha", "--json"])?;
        let item = pm_rust::Workspace::discover(fixture.rust.path())?.get("sample-own")?;
        if role == "active" {
            assert_eq!(item.metadata.extra["assignee"], "alpha");
        } else {
            assert_eq!(start_snapshot(fixture.rust.path())?, before);
        }
    }
    Ok(())
}
