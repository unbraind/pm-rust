//! Execute behavioral harness regressions on every release-gate platform.
use std::process::Command;
#[path = "support/node_harness.rs"]
mod node_harness;

/// Harness argument, root selection and latency aggregation stay in the release gate.
#[test]
fn harness_behavior() -> Result<(), Box<dyn std::error::Error>> {
    let Some(interpreter) = node_harness::interpreter_or_skip()? else {
        return Ok(());
    };
    let output = interpreter
        .test_command()
        .arg("scripts/harness.test.mts")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()?;
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

/// Optional missing tooling skips, but the same configuration fails required runs.
#[test]
fn harness_missing_node_obeys_opt_in() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let interpreter = directory.path().join("missing-node");
    for required in [false, true] {
        let mut child = Command::new(std::env::current_exe()?);
        child
            .args(["--exact", "harness_behavior", "--nocapture"])
            .env("PM_NODE_INTERPRETER", &interpreter)
            .env("PATH", directory.path())
            .env_remove("PM_RUST_REQUIRE_PUBLISHED_CLI");
        if required {
            child.env("PM_RUST_REQUIRE_PUBLISHED_CLI", "1");
        }
        let output = child.output()?;
        assert_eq!(output.status.success(), !required);
        if !required {
            assert!(String::from_utf8_lossy(&output.stdout).contains("skip: Node harness"));
        }
    }
    Ok(())
}

/// A present Node with disabled type stripping follows the same opt-in contract.
#[test]
fn harness_without_type_stripping_obeys_opt_in() -> Result<(), Box<dyn std::error::Error>> {
    let interpreter = std::env::var_os("PM_NODE_INTERPRETER").unwrap_or_else(|| "node".into());
    if !Command::new(&interpreter)
        .args(["-e", "process.exit(process.features.typescript ? 0 : 1)"])
        .output()
        .is_ok_and(|output| output.status.success())
    {
        println!("skip: disabled-type-stripping control needs a capable Node interpreter");
        return Ok(());
    }
    for required in [false, true] {
        let mut child = Command::new(std::env::current_exe()?);
        child
            .args(["--exact", "harness_behavior", "--nocapture"])
            .env("PM_NODE_INTERPRETER", &interpreter)
            .env("NODE_OPTIONS", "--no-experimental-strip-types")
            .env_remove("PM_RUST_REQUIRE_PUBLISHED_CLI");
        if required {
            child.env("PM_RUST_REQUIRE_PUBLISHED_CLI", "1");
        }
        let output = child.output()?;
        assert_eq!(output.status.success(), !required);
        if !required {
            assert!(String::from_utf8_lossy(&output.stdout).contains("skip: Node harness"));
        }
    }
    Ok(())
}
