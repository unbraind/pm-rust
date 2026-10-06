//! Golden unbounded-list envelopes compared with both real command binaries.

use std::fs;
use std::process::Command;

use serde_json::Value;

#[path = "support/published_cli.rs"]
mod published_cli;

const CLOCK: &str = "2026-10-02T10:00:00.000Z";
const GOLDEN: &str = include_str!("fixtures/list-2026-10-2.json");

/// Creates a synthetic tracker with lifecycle, ordering, and extension cases.
fn tracker() -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(root.join("tasks"))?;
    fs::write(
        root.join("settings.json"),
        include_str!("fixtures/list-settings.json"),
    )?;
    for (id, status, priority, updated) in [
        ("demo-a", "open", 2, "2026-09-02T10:00:00.000Z"),
        ("demo-b", "in_progress", 1, "2026-09-01T10:00:00.000Z"),
        ("demo-c", "blocked", 1, "2026-09-02T10:00:00.000Z"),
        ("demo-d", "draft", 1, "2026-09-02T10:00:00.000Z"),
        ("demo-e", "closed", 0, "2026-09-03T10:00:00.000Z"),
        ("demo-f", "canceled", 0, "2026-09-04T10:00:00.000Z"),
        ("demo-g", "open", 1, "2026-09-02T10:00:00.000100Z"),
    ] {
        fs::write(
            root.join("tasks").join(format!("{id}.toon")),
            format!(
                "id: {id}\ntitle: {id}\ndescription: \"\"\ntype: Task\nstatus: {status}\npriority: {priority}\ntags: []\ncreated_at: \"2026-09-01T00:00:00.000Z\"\nupdated_at: \"{updated}\"\ncustom_field: retained\nbody: \"excluded body\"\n"
            ),
        )?;
    }
    Ok(directory)
}

/// Prepares the fixed-clock published driver if its CLI and interpreter exist.
fn published_driver(
    directory: &std::path::Path,
) -> Result<Option<(String, std::path::PathBuf)>, Box<dyn std::error::Error>> {
    let Some(published) = published_cli::published_cli_or_skip("list differential") else {
        return Ok(None);
    };
    let interpreter = std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned());
    if !Command::new(&interpreter)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        assert!(
            std::env::var("PM_RUST_REQUIRE_PUBLISHED_CLI").is_err(),
            "list differential requires a Node interpreter, but none was found and PM_RUST_REQUIRE_PUBLISHED_CLI is set"
        );
        println!("skip: no Node interpreter found (set PM_NODE_INTERPRETER to select one)");
        return Ok(None);
    }
    assert!(published.package_root.join("package.json").is_file());
    let driver = directory.join("driver.mjs");
    fs::write(
        &driver,
        format!(
            "import {{pathToFileURL}} from 'node:url';\nconst fixed=Date.parse({});\nconst OriginalDate=Date;\nglobalThis.Date=class extends OriginalDate {{constructor(...args){{args.length?super(...args):super(fixed)}}static now(){{return fixed}}}};\nprocess.argv=[process.argv[0],'pm',...process.argv.slice(2)];\nawait import(pathToFileURL({}));\n",
            serde_json::to_string(CLOCK)?,
            serde_json::to_string(&published.entry)?
        ),
    )?;
    Ok(Some((interpreter, driver)))
}

/// Executes the real published CLI and native CLI against the same tracker.
#[test]
fn unbounded_list_matches_published_cli_and_golden() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = tracker()?;
    let published = published_driver(fixture.path())?;
    let golden: Value = serde_json::from_str(GOLDEN)?;
    for (label, flags) in [
        ("default", vec![]),
        ("all", vec!["--all"]),
        ("status-all", vec!["--status", "all"]),
        ("closed", vec!["--status", "closed"]),
        ("type", vec!["--type", "task"]),
        ("type-empty", vec!["--type", "Issue"]),
        ("id", vec!["--ids", "demo-c"]),
        (
            "combined",
            vec!["--status", "open", "--type", "Task", "--ids", "demo-a"],
        ),
        ("empty", vec!["--ids", "absent"]),
    ] {
        let args = [
            "list",
            "--json",
            "--output-budget",
            "unbounded",
            "--output-limit",
            "unbounded",
        ];
        if let Some((interpreter, driver)) = &published {
            let output = Command::new(interpreter)
                .arg(driver)
                .args(args)
                .args(&flags)
                .current_dir(fixture.path())
                .env_remove("PM_PATH")
                .output()?;
            assert!(
                output.status.success(),
                "published {label}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let published_value: Value = serde_json::from_slice(&output.stdout)?;
            assert_eq!(published_value, golden[label], "published golden {label}");
        }
        let mut filters = pm_rust::ItemFilter::default();
        for pair in flags.chunks_exact(2) {
            match pair[0] {
                "--status" => filters.status = Some(pair[1].to_owned()),
                "--type" => filters.item_type = Some(pair[1].to_owned()),
                "--ids" => filters.id = Some(pair[1].to_owned()),
                _ => return Err("unrecognized fixture filter".into()),
            }
        }
        let workspace = pm_rust::Workspace::discover(fixture.path())?;
        assert_eq!(
            workspace.list_unbounded(&filters, label == "all", CLOCK)?,
            golden[label],
            "SDK golden {label}"
        );
        let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
            .args(args)
            .args(&flags)
            .args(["--timestamp", CLOCK])
            .current_dir(fixture.path())
            .env_remove("PM_PATH")
            .output()?;
        assert!(
            output.status.success(),
            "native {label}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let native: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(native, golden[label], "native golden {label}");
    }
    println!("verified 9 native and SDK golden envelopes");
    Ok(())
}

/// Refuses ambiguous amount/cost policies and checks the ordinary native clock.
#[test]
fn explicit_unbounded_policies_and_live_clock() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = tracker()?;
    for args in [
        vec![
            "list",
            "--json",
            "--output-budget",
            "invalid",
            "--output-limit",
            "unbounded",
        ],
        vec!["list", "--all"],
        vec!["list", "--full"],
        vec![
            "list",
            "--json",
            "--output-budget",
            "unbounded",
            "--output-limit",
            "unbounded",
            "--all",
            "--status",
            "open",
        ],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
            .args(args)
            .current_dir(fixture.path())
            .output()?;
        assert!(!output.status.success());
    }
    let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .args([
            "list",
            "--json",
            "--output-budget",
            "unbounded",
            "--output-limit",
            "unbounded",
        ])
        .current_dir(fixture.path())
        .output()?;
    assert!(output.status.success());
    let result: Value = serde_json::from_slice(&output.stdout)?;
    let Some(now) = result["now"].as_str() else {
        return Err("read clock absent".into());
    };
    assert!(
        time::OffsetDateTime::parse(now, &time::format_description::well_known::Rfc3339).is_ok()
    );
    assert_eq!(result["count"], 5);
    Ok(())
}

/// Unsupported selectors and closed output pipes propagate typed failures.
#[test]
fn compatibility_list_reports_read_and_output_failures() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = tracker()?;
    let args = [
        "list",
        "--json",
        "--output-budget",
        "unbounded",
        "--output-limit",
        "unbounded",
    ];
    for (flag, value) in [
        ("--status", "open,closed"),
        ("--type", "Task,Issue"),
        ("--ids", "demo-a,demo-b"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
            .args(args)
            .args([flag, value])
            .current_dir(fixture.path())
            .output()?;
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).contains("invalid read request"));
    }
    let item_path = fixture.path().join(".agents/pm/tasks/demo-a.toon");
    let item = fs::read_to_string(&item_path)?.replace(
        "custom_field: retained",
        &format!("custom_field: {}", "x".repeat(200_000)),
    );
    fs::write(item_path, item)?;
    let mut child = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .args(args)
        .arg("--all")
        .current_dir(fixture.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    drop(child.stdout.take());
    assert_eq!(child.wait_with_output()?.status.code(), Some(2));
    let timestamp_path = fixture.path().join(".agents/pm/tasks/demo-c.toon");
    let item = fs::read_to_string(&timestamp_path)?.replace("2026-09-02T10:00:00.000Z", "invalid");
    fs::write(timestamp_path, item)?;
    let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .args(args)
        .current_dir(fixture.path())
        .output()?;
    assert!(output.status.success());
    fs::write(fixture.path().join(".agents/pm/tasks/bad.toon"), "invalid")?;
    let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .args(args)
        .current_dir(fixture.path())
        .output()?;
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid pm item document"));
    Ok(())
}

/// Invalid read clocks fail before emitting an envelope.
#[test]
fn invalid_list_timestamps_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = tracker()?;
    for timestamp in ["", "2026-10-02", "garbageZ", "2026-10-02T10:00:00+00:00"] {
        let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
            .args([
                "list",
                "--json",
                "--output-budget",
                "unbounded",
                "--output-limit",
                "unbounded",
                "--timestamp",
                timestamp,
            ])
            .current_dir(fixture.path())
            .env_remove("PM_PATH")
            .output()?;
        assert_eq!(output.status.code(), Some(2), "accepted {timestamp:?}");
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("timestamp must be a non-empty UTC RFC 3339 value")
        );
    }
    Ok(())
}

/// Missing external tooling may skip only the published comparison.
#[test]
fn golden_checks_survive_missing_published_tools() -> Result<(), Box<dyn std::error::Error>> {
    let package = tempfile::tempdir()?;
    fs::create_dir(package.path().join("dist"))?;
    fs::write(package.path().join("dist/cli.js"), "")?;
    for (cli, interpreter, notice) in [
        (
            std::path::PathBuf::from("/nonexistent/pm-rust-review-cli"),
            "node",
            "skip: no published Node pm CLI found",
        ),
        (
            package.path().to_path_buf(),
            "/nonexistent/pm-rust-review-node",
            "skip: no Node interpreter found",
        ),
    ] {
        let output = Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "unbounded_list_matches_published_cli_and_golden",
                "--nocapture",
            ])
            .env("PM_NODE_CLI", &cli)
            .env("PM_NODE_INTERPRETER", interpreter)
            .env_remove("PM_RUST_REQUIRE_PUBLISHED_CLI")
            .output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "{stdout} {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(stdout.contains(notice), "{stdout}");
        assert!(
            stdout.contains("verified 9 native and SDK golden envelopes"),
            "{stdout}"
        );
        let required = Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "unbounded_list_matches_published_cli_and_golden",
                "--nocapture",
            ])
            .env("PM_NODE_CLI", &cli)
            .env("PM_NODE_INTERPRETER", interpreter)
            .env("PM_RUST_REQUIRE_PUBLISHED_CLI", "1")
            .output()?;
        assert!(!required.status.success());
    }
    Ok(())
}

/// SDK callers must receive the same all/status conflict refusal as CLI callers.
#[test]
fn all_with_explicit_status_fails_closed() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = tracker()?;
    let workspace = pm_rust::Workspace::discover(fixture.path())?;
    for status in ["open", "all", ""] {
        let filters = pm_rust::ItemFilter {
            status: Some(status.to_owned()),
            ..pm_rust::ItemFilter::default()
        };
        assert!(matches!(
            workspace.list_unbounded(&filters, true, CLOCK),
            Err(pm_rust::PmRustError::InvalidReadRequest { .. })
        ));
    }
    Ok(())
}

/// The omission receipt must restore metadata without changing the selection.
#[test]
fn full_restores_fields_without_adding_terminal_items() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = tracker()?;
    let golden: Value = serde_json::from_str(GOLDEN)?;
    let mut expected = golden["all"].clone();
    let items = expected["items"]
        .as_array_mut()
        .ok_or("missing golden items")?;
    items.retain(|item| item["status"] != "closed" && item["status"] != "canceled");
    expected["count"] = 5.into();
    expected["total"] = 5.into();
    expected["filters"]
        .as_object_mut()
        .ok_or("missing filters")?
        .remove("status");
    let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .args([
            "list",
            "--json",
            "--output-budget",
            "unbounded",
            "--output-limit",
            "unbounded",
            "--full",
            "--timestamp",
            CLOCK,
        ])
        .current_dir(fixture.path())
        .env_remove("PM_PATH")
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: Value = serde_json::from_slice(&output.stdout)?;
    let mut expected_with_receipt = expected.clone();
    expected_with_receipt["read_output"] = serde_json::from_str(include_str!(
        "fixtures/list-full-read-output-2026-10-2.json"
    ))?;
    assert_eq!(actual, expected_with_receipt);
    if let Some((interpreter, driver)) = published_driver(fixture.path())? {
        let output = Command::new(interpreter)
            .arg(driver)
            .args([
                "list",
                "--json",
                "--output-budget",
                "unbounded",
                "--output-limit",
                "unbounded",
                "--full",
            ])
            .current_dir(fixture.path())
            .env_remove("PM_PATH")
            .output()?;
        assert!(output.status.success());
        let published: Value = serde_json::from_slice(&output.stdout)?;
        let mut published_expected = expected.clone();
        published_expected["read_output"] = serde_json::from_str(include_str!(
            "fixtures/list-full-read-output-2026-10-2.json"
        ))?;
        assert_eq!(published, published_expected, "published full envelope");
    }
    let workspace = pm_rust::Workspace::discover(fixture.path())?;
    assert_eq!(
        workspace.list_unbounded_full(&pm_rust::ItemFilter::default(), false, CLOCK)?,
        expected
    );
    assert_eq!(
        workspace.list_unbounded_full(&pm_rust::ItemFilter::default(), true, CLOCK)?,
        golden["all"]
    );
    let output = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .args([
            "list",
            "--json",
            "--output-budget",
            "unbounded",
            "--output-limit",
            "unbounded",
            "--full",
            "--ids",
            "demo-a,demo-b",
        ])
        .current_dir(fixture.path())
        .env_remove("PM_PATH")
        .output()?;
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid read request"));
    Ok(())
}

/// Public SDK ordering stays deterministic for mixed valid and invalid clocks.
#[test]
fn sdk_mixed_timestamp_order_has_no_cycle() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = tracker()?;
    let root = fixture.path().join(".agents/pm/tasks");
    fs::remove_dir_all(&root)?;
    fs::create_dir(&root)?;
    for (id, updated) in [
        ("demo-a", "2026-10-02T00:00:00+01:00"),
        ("demo-b", "2026-10-01T23:30:00Z"),
        ("demo-c", "2026-10-01T23:45:00Z?"),
        ("demo-d", "invalid"),
    ] {
        fs::write(
            root.join(format!("{id}.toon")),
            format!(
                "id: {id}\ntitle: {id}\ndescription: \"\"\ntype: Task\nstatus: open\npriority: 1\ntags: []\ncreated_at: \"2026-09-01T00:00:00.000Z\"\nupdated_at: \"{updated}\"\nbody: \"\"\n"
            ),
        )?;
    }
    let workspace = pm_rust::Workspace::discover(fixture.path())?;
    let value = workspace.list_unbounded(&pm_rust::ItemFilter::default(), false, CLOCK)?;
    let items = value["items"].as_array().ok_or("missing items")?;
    assert_eq!(
        items
            .iter()
            .map(|item| item["id"].as_str())
            .collect::<Vec<_>>(),
        vec![
            Some("demo-b"),
            Some("demo-a"),
            Some("demo-d"),
            Some("demo-c")
        ]
    );
    Ok(())
}
