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

/// Optional suites skip a stale package and required suites fail before invoking it.
#[test]
fn published_locator_requires_the_manifest_pin() -> Result<(), Box<dyn std::error::Error>> {
    const CHILD: &str = "PM_RUST_TEST_ORACLE_LOCATOR_EXPECT";
    if let Ok(expected) = std::env::var(CHILD) {
        assert_eq!(
            published_cli::published_cli_or_skip("oracle locator contract").is_some(),
            expected == "found"
        );
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    let entry = root.join("dist/cli.js");
    fs::create_dir(root.join("dist"))?;
    fs::write(
        &entry,
        "throw new Error('the locator must not execute the entry');",
    )?;
    let manifest: serde_json::Value = serde_json::from_str(include_str!("oracle/package.json"))?;
    let pin = manifest["dependencies"]["@unbrained/pm-cli"]
        .as_str()
        .ok_or("oracle pin")?;
    for (version, required, expected, success, override_path) in [
        ("0.0.0", false, "absent", true, root),
        ("0.0.0", true, "absent", false, entry.as_path()),
        (pin, true, "found", true, root),
        (pin, true, "found", true, entry.as_path()),
    ] {
        fs::write(
            root.join("package.json"),
            serde_json::json!({"version":version}).to_string(),
        )?;
        let mut command = Command::new(std::env::current_exe()?);
        command
            .args([
                "--exact",
                "published_locator_requires_the_manifest_pin",
                "--nocapture",
            ])
            .env(CHILD, expected)
            .env("PM_NODE_CLI", override_path)
            .env_remove("PM_RUST_REQUIRE_PUBLISHED_CLI");
        if required {
            command.env("PM_RUST_REQUIRE_PUBLISHED_CLI", "1");
        }
        let output = command.output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.success(), success, "{stdout}{stderr}");
        if version != pin {
            assert!(stderr.contains(&format!("expected {pin}")), "{stderr}");
            assert!(format!("{stdout}{stderr}").contains("npm ci --prefix tests/oracle"));
        }
    }
    Ok(())
}
