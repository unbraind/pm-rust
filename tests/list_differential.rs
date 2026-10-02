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

/// Executes the real published CLI and native CLI against the same tracker.
#[test]
fn unbounded_list_matches_published_cli_and_golden() -> Result<(), Box<dyn std::error::Error>> {
    let Some(published) = published_cli::published_cli_or_skip("list differential") else {
        return Ok(());
    };
    assert!(published.package_root.join("package.json").is_file());
    let fixture = tracker()?;
    let driver = fixture.path().join("driver.mjs");
    fs::write(
        &driver,
        format!(
            "import {{pathToFileURL}} from 'node:url';\nconst fixed=Date.parse({});\nconst OriginalDate=Date;\nglobalThis.Date=class extends OriginalDate {{constructor(...args){{args.length?super(...args):super(fixed)}}static now(){{return fixed}}}};\nprocess.argv=[process.argv[0],'pm',...process.argv.slice(2)];\nawait import(pathToFileURL({}));\n",
            serde_json::to_string(CLOCK)?,
            serde_json::to_string(&published.entry)?
        ),
    )?;
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
        let output = Command::new("node")
            .arg(&driver)
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
            published_value,
            "SDK envelope {label}"
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
        assert_eq!(native, published_value, "full envelope {label}");
    }
    Ok(())
}

/// Refuses ambiguous amount/cost policies and checks the ordinary native clock.
#[test]
fn explicit_unbounded_policies_and_live_clock() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = tracker()?;
    for args in [
        vec!["list", "--json"],
        vec!["list", "--json", "--output-budget", "unbounded"],
        vec!["list", "--json", "--output-limit", "unbounded"],
        vec![
            "list",
            "--json",
            "--output-budget",
            "100",
            "--output-limit",
            "unbounded",
        ],
        vec!["list", "--all"],
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
