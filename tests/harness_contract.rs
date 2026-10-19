//! Execute behavioral harness regressions on every release-gate platform.
use std::process::Command;

/// Harness argument, root selection and latency aggregation stay in the release gate.
#[test]
fn harness_behavior() -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("node")
        .args(["--test", "scripts/harness.test.mts"])
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
