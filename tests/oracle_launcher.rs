//! Observes the actual oracle child's environment through a disposable shim.

use std::{fs, process::Command};

#[path = "support/published_cli.rs"]
#[allow(dead_code)]
mod published_cli;

/// The just recipe preserves argument boundaries and enforces the child telemetry boundary.
#[test]
fn just_oracle_preserves_argument_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let shim = directory.path().join("echo shim.mjs");
    fs::write(
        &shim,
        "console.log(JSON.stringify([process.env.PM_SENTRY_DISABLED,process.env.PM_TELEMETRY_SOURCE_CONTEXT,...process.argv.slice(2)]));process.exit(7);",
    )?;
    let output = Command::new("just")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("PM_SENTRY_DISABLED", "0")
        .env("PM_TELEMETRY_SOURCE_CONTEXT", "production")
        .args(["oracle", "--entry"])
        .arg(&shim)
        .args(["argument with spaces", "", "$PM_SENTRY_DISABLED", "--json"])
        .output()?;
    assert_eq!(
        output.status.code(),
        Some(7),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout)?,
        serde_json::json!([
            "1",
            "test",
            "argument with spaces",
            "",
            "$PM_SENTRY_DISABLED",
            "--json"
        ])
    );
    Ok(())
}

/// Hostile inherited settings and a cleared parent environment cannot enable Sentry.
#[test]
fn oracle_launcher_sets_child_telemetry_environment() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let shim = directory.path().join("shim.mjs");
    fs::write(
        &shim,
        "console.log(JSON.stringify([process.env.PM_SENTRY_DISABLED,process.env.PM_TELEMETRY_SOURCE_CONTEXT,...process.argv.slice(2)]));process.exit(7);",
    )?;
    let interpreter = std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned());
    for (cleared, tool) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut command = if tool {
            published_cli::oracle_tool(&interpreter)
        } else {
            published_cli::oracle_command(&interpreter)
        };
        if cleared {
            command.env_clear();
            for name in ["PATH", "SystemRoot", "SystemDrive", "TEMP", "TMP"] {
                if let Some(value) = std::env::var_os(name) {
                    command.env(name, value);
                }
            }
        }
        let output = command
            .env("PM_SENTRY_DISABLED", "0")
            .env("PM_TELEMETRY_SOURCE_CONTEXT", "production")
            .arg(&shim)
            .args(["argument with spaces", "--json"])
            .output()?;
        assert_eq!(
            output.status.code(),
            Some(7),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.stdout)?,
            serde_json::json!(["1", "test", "argument with spaces", "--json"])
        );
    }
    // Exercise the launcher's default entry selection, too.
    let output = Command::new(&interpreter)
        .arg("tests/oracle/launch.mts")
        .arg("--version")
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
