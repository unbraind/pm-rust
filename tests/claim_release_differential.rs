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
    /// Initializes the fixture with a published create record and chosen policy.
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
                "Ownership fixture",
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
        let (native_bytes, node_bytes) = if node.status.success() {
            (&rust.stdout, &node.stdout)
        } else {
            (&rust.stderr, &node.stderr)
        };
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(native_bytes)?,
            serde_json::from_slice::<serde_json::Value>(node_bytes)?,
            "{args:?}"
        );
        self.compare_bytes()?;
        Ok(())
    }

    /// Asserts exact item and history bytes, without normalization.
    fn compare_bytes(&self) -> Result<(), Box<dyn std::error::Error>> {
        for artifact in [
            ".agents/pm/tasks/sample-own.toon",
            ".agents/pm/history/sample-own.jsonl",
        ] {
            assert_eq!(
                fs::read(self.rust.path().join(artifact))?,
                fs::read(self.node.path().join(artifact))?,
                "{artifact}"
            );
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
                "take ownership",
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
                "handoff",
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
