//! Real CLI differential coverage for the native agent orientation commands.
use serde_json::{Value, json};
use sha2::Digest;
use std::{
    fmt::Write,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
#[allow(dead_code)]
#[path = "support/mutation_differential.rs"]
mod mutation_differential;
#[path = "support/node_harness.rs"]
mod node_harness;
#[path = "support/published_cli.rs"]
mod published_cli;
use mutation_differential::write_driver;
type TestResult = Result<(), Box<dyn std::error::Error>>;
const CLOCK: &str = "2026-08-22T10:00:00.000Z";

/// Invalid history filenames match the oracle before any escaped path can affect the refusal.
#[test]
fn history_invalid_filename_matches_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("history filename boundary") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    fixture(root.path(), &[])?;
    let outside = root.path().join(".agents/outside-x.jsonl");
    for present in [false, true] {
        if present {
            fs::write(&outside, "outside bytes must never be read\n")?;
        }
        for id in ["../../outside-x", "with:colon", "back\\slash"] {
            for json in [false, true] {
                let mut args = vec!["history", id];
                if json {
                    args.push("--json");
                }
                let expected = oracle(&driver, root.path(), &args)?;
                assert_eq!(expected.status.code(), Some(2));
                compare(&expected, &native(root.path(), &args)?, &args);
            }
        }
    }
    assert_eq!(fs::read(&outside)?, b"outside bytes must never be read\n");
    assert!(
        fs::read_dir(root.path().join(".agents/pm/locks"))?
            .next()
            .is_none()
    );
    Ok(())
}

/// The oracle preserves dangling display spellings in graphs but normalizes summary identities.
#[test]
fn deps_mixed_case_dangling_matches_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("mixed-case dependency identities")
    else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    let mut start = item("sample-root", "Task", "open", 1);
    start["dependencies"] = json!([
        {"id":"sample-left","kind":"related","created_at":CLOCK},
        {"id":"sample-right","kind":"related","created_at":CLOCK},
        {"id":"SAMPLE-LEFT","kind":"related","created_at":CLOCK}
    ]);
    let mut left = item("sample-left", "Task", "open", 2);
    left["dependencies"] = json!([{"id":"sample-Missing","kind":"related","created_at":CLOCK}]);
    let mut right = item("sample-right", "Task", "open", 3);
    right["dependencies"] = json!([{"id":"SAMPLE-MISSING","kind":"related","created_at":CLOCK}]);
    fixture(root.path(), &[start, left, right])?;
    let before: Vec<_> = ["sample-root", "sample-left", "sample-right"]
        .iter()
        .map(|id| fs::read(root.path().join(format!(".agents/pm/tasks/{id}.toon"))))
        .collect::<Result<_, _>>()?;
    for format in ["tree", "graph"] {
        for summary in [false, true] {
            for encoding in [vec![], vec!["--json"]] {
                for bounds in [
                    vec![],
                    vec!["--max-depth", "1"],
                    vec!["--node-limit", "4"],
                    vec!["--edge-limit", "2"],
                ] {
                    let mut args = vec!["deps", "sample-root", "--format", format];
                    args.push(if summary { "--summary" } else { "--full" });
                    args.extend(&encoding);
                    args.extend(&bounds);
                    let expected = oracle(&driver, root.path(), &args)?;
                    assert!(expected.status.success());
                    compare(&expected, &native(root.path(), &args)?, &args);
                    if !encoding.is_empty() && bounds.is_empty() {
                        let output: Value = serde_json::from_slice(&expected.stdout)?;
                        assert_eq!(output["missing_count"], if summary { 1 } else { 2 });
                        assert_eq!(output["node_count"], if summary { 4 } else { 5 });
                        if !summary && format == "graph" {
                            assert_eq!(
                                output["graph"]["missing_ids"].as_array().map(Vec::len),
                                Some(2)
                            );
                            assert_eq!(output["graph"]["edges"].as_array().map(Vec::len), Some(4));
                        }
                    }
                }
            }
        }
    }
    for (id, bytes) in ["sample-root", "sample-left", "sample-right"]
        .iter()
        .zip(before)
    {
        assert_eq!(
            fs::read(root.path().join(format!(".agents/pm/tasks/{id}.toon")))?,
            bytes
        );
        assert_eq!(
            fs::read(root.path().join(format!(".agents/pm/history/{id}.jsonl")))?,
            b""
        );
    }
    Ok(())
}

/// Dependency reads retain the pinned oracle's dangling no-active-blocker scalar receipt.
#[test]
fn deps_no_active_blocker_matches_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("dependency sentinel receipt")
    else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    let mut row = item("sample-sentinel", "Task", "open", 1);
    row["blocked_by"] = json!("no-active-blocker");
    fixture(root.path(), &[row])?;
    let before = fs::read(root.path().join(".agents/pm/tasks/sample-sentinel.toon"))?;
    for projection in [vec![], vec!["--format", "graph"], vec!["--summary"]] {
        for encoding in [vec![], vec!["--json"]] {
            for bounds in [vec![], vec!["--max-depth", "0"]] {
                let mut args = vec!["deps", "sample-sentinel"];
                args.extend(&projection);
                args.extend(&encoding);
                args.extend(&bounds);
                let expected = oracle(&driver, root.path(), &args)?;
                assert!(expected.status.success());
                compare(&expected, &native(root.path(), &args)?, &args);
            }
        }
    }
    assert_eq!(
        fs::read(root.path().join(".agents/pm/tasks/sample-sentinel.toon"))?,
        before
    );
    assert_eq!(
        fs::read(root.path().join(".agents/pm/history/sample-sentinel.jsonl"))?,
        b""
    );
    Ok(())
}

/// Non-Unicode host environment names and values cannot crash a context read.
#[cfg(unix)]
#[test]
fn context_non_unicode_environment_matches_oracle() -> TestResult {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    let Some(published) = published_cli::published_cli_or_skip("non-Unicode context") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    fixture(root.path(), &[item("sample-a", "Task", "open", 1)])?;
    for args in [
        vec!["context", "--limit", "1"],
        vec!["context", "--limit", "1", "--json"],
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_pm-rust"));
        mutation_differential::isolate_environment(&mut command);
        let actual = command
            .current_dir(root.path())
            .env("HOME", root.path().join("synthetic-home"))
            .env("USERPROFILE", root.path().join("synthetic-home"))
            .env(OsString::from_vec(b"ORIENTATION_\xff".to_vec()), "fixture")
            .env("ORIENTATION_VALUE", OsString::from_vec(vec![0xfe]))
            .args(&args)
            .args(["--timestamp", CLOCK])
            .output()?;
        compare(&oracle(&driver, root.path(), &args)?, &actual, &args);
    }
    Ok(())
}

/// A configured progress status without the active role is excluded from active summaries.
#[test]
fn context_non_active_progress_matches_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("non-active progress context")
    else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    let pm = root.path().join(".agents/pm");
    let statuses = json!([
        {"id":"open","roles":["active","default_open"]},
        {"id":"reviewing","roles":["blocked"]},
        {"id":"waiting","roles":["blocked"]},
        {"id":"closed","roles":["terminal","terminal_done","default_close"]}
    ]);
    let workflow = json!({"open_status":"open","in_progress_status":"reviewing"});
    let mut settings: Value = serde_json::from_slice(&fs::read(pm.join("settings.json"))?)?;
    settings["schema"]["statuses"] = statuses.clone();
    settings["schema"]["workflow"] = workflow.clone();
    fs::write(pm.join("settings.json"), serde_json::to_vec(&settings)?)?;
    fs::write(
        pm.join("schema/statuses.json"),
        json!({"statuses":statuses}).to_string(),
    )?;
    fs::write(
        pm.join("schema/workflows.json"),
        json!({"workflow":workflow}).to_string(),
    )?;
    for active_count in [0, 1, 3] {
        let mut rows = vec![
            item("sample-progress-a", "Task", "reviewing", 1),
            item("sample-progress-b", "Task", "reviewing", 2),
            item("sample-waiting", "Task", "waiting", 3),
        ];
        rows.extend(
            (0..active_count).map(|i| item(&format!("sample-active-{i}"), "Task", "open", 3)),
        );
        fixture(root.path(), &rows)?;
        // Refresh the oracle's derived snapshot after changing the fixture corpus.
        assert!(oracle(&driver, root.path(), &["context"])?.status.success());
        for args in [
            vec!["context", "--json"],
            vec!["context"],
            vec!["context", "--token-budget", "800", "--json"],
            vec!["context", "--token-budget", "400"],
        ] {
            let expected = oracle(&driver, root.path(), &args)?;
            assert!(
                args.contains(&"--token-budget") || expected.status.success(),
                "{}",
                String::from_utf8_lossy(&expected.stderr)
            );
            if args.contains(&"--json") && !args.contains(&"--token-budget") {
                let result: Value = serde_json::from_slice(&expected.stdout)?;
                assert_eq!(result["summary"]["open"], active_count);
                assert_eq!(result["summary"]["in_progress"], 0);
            }
            compare(&expected, &native(root.path(), &args)?, &args);
        }
    }
    Ok(())
}

/// Dangling scalar parents match every read projection without modifying authoritative bytes.
#[test]
fn deps_dangling_parents_match_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("dangling parent deps") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    for (status, parent, explicit, blocker) in [
        ("open", "sample-deleted", false, ""),
        ("closed", "sample-deleted", false, ""),
        ("open", "sample-existing", false, ""),
        ("open", "sample-deleted", true, ""),
        ("open", "", false, "sample-blocker"),
        ("open", "", false, ""),
        ("open", "  SaMpLe-DeLeTeD  ", false, ""),
        ("open", "  SAMPLE-EXISTING  ", false, ""),
        ("open", "  SaMpLe-DeLeTeD  ", true, "sample-blocker"),
        ("open", "sample-absent", false, "sample-z-blocker"),
    ] {
        let root = tempfile::tempdir()?;
        assert!(
            oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
                .status
                .success()
        );
        let mut child = item("sample-child", "Task", status, 1);
        if !parent.is_empty() {
            child["parent"] = json!(parent);
        }
        if !blocker.is_empty() {
            child["blocked_by"] = json!(blocker);
        }
        if explicit {
            child["dependencies"] =
                json!([{"id":"sample-deleted","kind":"parent","created_at":CLOCK}]);
        }
        fixture(
            root.path(),
            &[child, item("sample-existing", "Feature", "open", 2)],
        )?;
        let paths = [
            "tasks/sample-child.toon",
            "tasks/sample-existing.toon",
            "history/sample-child.jsonl",
            "history/sample-existing.jsonl",
        ];
        let before: Vec<_> = paths
            .iter()
            .map(|p| fs::read(root.path().join(".agents/pm").join(p)))
            .collect::<Result<_, _>>()?;
        for projection in [vec![], vec!["--format", "graph"], vec!["--summary"]] {
            for encoding in [vec![], vec!["--json"]] {
                for bounds in [vec![], vec!["--max-depth", "0"]] {
                    let mut args = vec!["deps", "sample-child"];
                    args.extend(&projection);
                    args.extend(&encoding);
                    args.extend(&bounds);
                    let expected = oracle(&driver, root.path(), &args)?;
                    assert!(
                        expected.status.success(),
                        "{args:?}: {}",
                        String::from_utf8_lossy(&expected.stderr)
                    );
                    if projection == ["--summary"] && encoding == ["--json"] {
                        let value: Value = serde_json::from_slice(&expected.stdout)?;
                        let missing_parent = !parent.trim().is_empty()
                            && !parent.trim().eq_ignore_ascii_case("sample-existing");
                        let edges = if bounds.is_empty() {
                            usize::from(missing_parent) + usize::from(!blocker.is_empty())
                        } else {
                            0
                        };
                        assert_eq!(value["node_count"], 1 + edges);
                        assert_eq!(value["edge_count"], edges);
                        assert_eq!(value["missing_count"], edges);
                    }
                    compare(&expected, &native(root.path(), &args)?, &args);
                }
            }
        }
        let after: Vec<_> = paths
            .iter()
            .map(|p| fs::read(root.path().join(".agents/pm").join(p)))
            .collect::<Result<_, _>>()?;
        assert_eq!(after, before);
    }
    Ok(())
}
/// A sealed record cannot traverse constructor/prototype during document replay.
#[test]
fn history_reserved_patch_paths_match_oracle() -> TestResult {
    let Some(published) =
        published_cli::published_cli_or_skip("orientation reserved history paths")
    else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let root = tempfile::tempdir()?;
    for args in [
        vec!["init", "sample-", "--defaults"],
        vec![
            "create",
            "--id",
            "sample-a",
            "--title",
            "History fixture",
            "--type",
            "Task",
            "--author",
            "fixture",
            "--create-mode",
            "progressive",
        ],
    ] {
        assert!(oracle(&driver, root.path(), &args)?.status.success());
    }
    let path = root.path().join(".agents/pm/history/sample-a.jsonl");
    let mut rows: Vec<Value> = fs::read_to_string(&path)?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    let hash = rows.last().ok_or("genesis")?["after_hash"].clone();
    rows.push(seal_record(json!({"op":"update","author":"fixture","ts":CLOCK,"item_hash_version":3,"before_hash":hash,"after_hash":hash,"patch":[{"op":"add","path":"/metadata/constructor","value":{}},{"op":"add","path":"/metadata/constructor/prototype","value":1}]}))?);
    fs::write(path, json_lines(&rows))?;
    let args = ["history", "sample-a", "--verify", "--strict-exit", "--json"];
    let expected = oracle(&driver, root.path(), &args)?;
    assert_eq!(expected.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<Value>(&expected.stdout)?["verification"]["errors"][0],
        "verify_failed:patch_apply_failed:entry_2"
    );
    compare(&expected, &native(root.path(), &args)?, &args);
    Ok(())
}
/// Numeric spellings preserve producer parsing and the complete-response budget contract.
#[test]
fn context_numeric_control_domains() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("orientation numeric controls")
    else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    fixture(root.path(), &[item("sample-a", "Task", "open", 1)])?;
    for args in [
        vec!["context", "--limit", "4", "--token-budget", "1e3", "--json"],
        vec![
            "context",
            "--limit",
            "0x3",
            "--token_budget",
            "0x100",
            "--json",
        ],
    ] {
        compare(
            &oracle(&driver, root.path(), &args)?,
            &native(root.path(), &args)?,
            &args,
        );
    }
    Ok(())
}
/// Minimal static extension discovery matches the oracle; richer policy refuses explicitly.
#[test]
fn context_static_extension_contract() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("orientation extension health")
    else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    fixture(root.path(), &[item("sample-a", "Task", "open", 1)])?;
    let settings_path = root.path().join(".agents/pm/settings.json");
    let mut settings: Value = serde_json::from_slice(&fs::read(&settings_path)?)?;
    settings["extensions"] = json!({"enabled":[],"disabled":[]});
    fs::write(settings_path, serde_json::to_vec(&settings)?)?;
    let path = root.path().join(".agents/pm/extensions/fixture-extension");
    fs::create_dir_all(&path)?;
    fs::write(path.join("entry.mjs"), "export function register() {}")?;
    let mut manifest = json!({"name":"fixture-extension","version":"1.0.0","entry":"entry.mjs"});
    fs::write(path.join("manifest.json"), serde_json::to_vec(&manifest)?)?;
    let args = ["context", "--limit", "1", "--json"];
    compare(
        &oracle(&driver, root.path(), &args)?,
        &native(root.path(), &args)?,
        &args,
    );
    manifest["priority"] = json!(2);
    manifest["pm_min_version"] = json!("2026.8.1");
    manifest["capabilities"] = json!(["commands", "schema"]);
    fs::write(path.join("manifest.json"), serde_json::to_vec(&manifest)?)?;
    compare(
        &oracle(&driver, root.path(), &args)?,
        &native(root.path(), &args)?,
        &args,
    );
    manifest["contributions"] = json!({});
    fs::write(path.join("manifest.json"), serde_json::to_vec(&manifest)?)?;
    let rejected = native(root.path(), &args)?;
    assert_eq!(rejected.status.code(), Some(2));
    assert_eq!(
        serde_json::from_slice::<Value>(&rejected.stderr)?["code"],
        "unsupported_native_context"
    );
    let args = ["context", "--limit", "1", "--no-extension-health", "--json"];
    compare(
        &oracle(&driver, root.path(), &args)?,
        &native(root.path(), &args)?,
        &args,
    );
    Ok(())
}
/// Disabled feedback is a real process setting and leaves the tracker unchanged.
#[test]
fn context_usage_disabled_is_read_only() -> TestResult {
    let root = tempfile::tempdir()?;
    fixture(root.path(), &[item("sample-a", "Task", "open", 1)])?;
    fs::write(root.path().join(".agents/pm/settings.json"), "{}")?;
    let before = fs::read(root.path().join(".agents/pm/tasks/sample-a.toon"))?;
    for global in [
        root.path().join("global").into_os_string(),
        std::ffi::OsString::new(),
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_pm-rust"));
        mutation_differential::isolate_environment(&mut command);
        let output = command
            .current_dir(root.path())
            .env("PM_CONTEXT_USAGE_DISABLED", "1")
            .env("PM_GLOBAL_PATH", global)
            .env("HOME", root.path().join("home"))
            .env("USERPROFILE", root.path().join("home"))
            .args(["context", "--json", "--timestamp", CLOCK])
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(
        fs::read(root.path().join(".agents/pm/tasks/sample-a.toon"))?,
        before
    );
    assert!(!root.path().join(".agents/pm/runtime").exists());
    Ok(())
}
/// Pins both clocks so the oracle's 25 ms packing guard is independent of host scheduling.
fn orientation_driver(
    root: &Path,
    published: &published_cli::PublishedCli,
) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    let current = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/orientation-oracle/node_modules/@unbrained/pm-cli");
    orientation_driver_at(
        root,
        published,
        &current,
        std::env::var("PM_RUST_REQUIRE_PUBLISHED_CLI").is_ok(),
    )
}

/// Gates the independently pinned current oracle without imposing it on history-only reads.
fn orientation_driver_at(
    root: &Path,
    published: &published_cli::PublishedCli,
    current: &Path,
    required: bool,
) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    if !current.join("dist/cli.js").is_file() {
        if required {
            return Err(
                "current orientation oracle required; run just orientation-oracle-install".into(),
            );
        }
        println!("skip: no current orientation oracle found (run just orientation-oracle-install)");
        return Ok(None);
    }
    let current = published_cli::PublishedCli {
        entry: current.join("dist/cli.js"),
        package_root: current.to_path_buf(),
    };
    let legacy = write_driver(root, published)?;
    fs::rename(&legacy, root.join("legacy-driver.mjs"))?;
    let metadata: Value =
        serde_json::from_slice(&fs::read(current.package_root.join("package.json"))?)?;
    assert_eq!(metadata["version"], "2026.11.5");
    assert!(
        current.entry.is_file(),
        "Run just orientation-oracle-install"
    );
    let path = write_driver(root, &current)?;
    let script = fs::read_to_string(&path)?;
    fs::write(
        &path,
        format!(
            "Object.defineProperty(performance, 'now', {{value:()=>0, configurable:true}});\n{script}"
        ),
    )?;
    Ok(Some(path))
}
/// Executes one published invocation in a synthetic fixed-clock tracker.
fn oracle(driver: &Path, root: &Path, args: &[&str]) -> Result<Output, Box<dyn std::error::Error>> {
    let legacy = driver.with_file_name("legacy-driver.mjs");
    let driver = if args.contains(&"context") || args.contains(&"deps") || !legacy.is_file() {
        driver
    } else {
        &legacy
    };
    let mut arguments = vec![driver.to_string_lossy().into_owned()];
    arguments.extend(args.iter().map(ToString::to_string));
    let interpreter =
        PathBuf::from(std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned()));
    let mut command = published_cli::oracle_command(&interpreter);
    mutation_differential::isolate_environment(&mut command);
    command
        .current_dir(root)
        .env("HOME", root.join("synthetic-home"))
        .env("USERPROFILE", root.join("synthetic-home"))
        .env("FIXED_CLOCK", CLOCK)
        .env("DO_NOT_TRACK", "1")
        .args(arguments);
    Ok(command.output()?)
}
/// Executes one native invocation with the same isolated application environment.
fn native(root: &Path, args: &[&str]) -> Result<Output, std::io::Error> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pm-rust"));
    command.current_dir(root);
    mutation_differential::isolate_environment(&mut command);
    command
        .env("HOME", root.join("synthetic-home"))
        .env("USERPROFILE", root.join("synthetic-home"))
        .args(args);
    if args[0] == "context" {
        command.args(["--timestamp", CLOCK]);
    }
    command.output()
}
/// Requires exact stdout, stderr and process status, including refusal bytes.
fn compare(expected: &Output, actual: &Output, args: &[&str]) {
    assert_eq!(
        actual.status.code(),
        expected.status.code(),
        "{args:?}: native {} oracle {}",
        String::from_utf8_lossy(&actual.stderr),
        String::from_utf8_lossy(&expected.stderr)
    );
    assert!(
        actual.stdout == expected.stdout,
        "{args:?}: stdout\nnative: {}\noracle: {}",
        String::from_utf8_lossy(&actual.stdout),
        String::from_utf8_lossy(&expected.stdout)
    );
    assert!(
        actual.stderr == expected.stderr,
        "{args:?}: stderr\nnative: {}\noracle: {}",
        String::from_utf8_lossy(&actual.stderr),
        String::from_utf8_lossy(&expected.stderr)
    );
    println!(
        "exact_orientation_differential:{}",
        if args[0] == "history" {
            "legacy"
        } else {
            "current"
        }
    );
}
/// Seals a synthetic retained-history proof with an independently sorted JSON digest.
fn seal_record(mut record: Value) -> Result<Value, Box<dyn std::error::Error>> {
    record
        .as_object_mut()
        .ok_or("history record must be an object")?
        .shift_remove("record_hash");
    record["record_hash_version"] = json!(1);
    let mut canonical = record.clone();
    canonical.sort_all_objects();
    let digest = sha2::Sha256::digest(serde_json::to_vec(&canonical)?)
        .iter()
        .fold(String::new(), |mut output, byte| {
            use std::fmt::Write;
            let _ = write!(output, "{byte:02x}");
            output
        });
    record["record_hash"] = json!(digest);
    Ok(record)
}
/// Serializes complete JSONL fixtures without changing any existing tracker stream.
fn json_lines(records: &[Value]) -> String {
    records.iter().fold(String::new(), |mut output, record| {
        use std::fmt::Write;
        let _ = writeln!(output, "{record}");
        output
    })
}
/// Creates authoritative TOON fixtures without replacing any SDK or storage behavior.
fn fixture(root: &Path, items: &[Value]) -> TestResult {
    fs::create_dir_all(root.join(".agents/pm/tasks"))?;
    fs::create_dir_all(root.join(".agents/pm/history"))?;
    for item in items {
        fs::write(
            root.join(format!(
                ".agents/pm/tasks/{}.toon",
                item["id"].as_str().unwrap_or_default()
            )),
            toon_format::encode_default(item)?,
        )?;
        fs::write(
            root.join(format!(
                ".agents/pm/history/{}.jsonl",
                item["id"].as_str().unwrap_or_default()
            )),
            "",
        )?;
    }
    Ok(())
}
/// Builds one valid stable metadata row for orientation fixtures.
fn item(id: &str, kind: &str, status: &str, priority: u8) -> Value {
    json!({"id":id,"title":format!("Native {id} café 🚀"),"description":"Orientation fixture","type":kind,"status":status,"priority":priority,"tags":["native"],"created_at":CLOCK,"updated_at":CLOCK,"body":""})
}

/// Builds real metadata and retained-history fixtures shared by rich orientation differentials.
fn rich_fixture(root: &Path) -> TestResult {
    let mut epic = item("sample-epic", "Epic", "in_progress", 1);
    epic["files"] = json!([{"path":"src/lib.rs"},{"path":"src/main.rs"}]);
    epic["docs"] = json!([{"path":"README.md"}]);
    epic["tests"] = json!([{"command":"cargo test"}]);
    epic["comments"] = json!([{"text":"Evidence","author":"fixture"}]);
    let mut feature = item("sample-feature", "Feature", "open", 2);
    feature["parent"] = json!("sample-epic");
    feature["files"] = json!([{"path":"src/lib.rs"}]);
    feature["assignee"] = json!("fixture-agent");
    feature["test_runs"] = json!([{"run_id":"synthetic-run","kind":"test","status":"failed","started_at":CLOCK,"finished_at":CLOCK,"recorded_at":CLOCK,"passed":3,"failed":1,"skipped":2}]);
    let mut task = item("sample-task", "Task", "in_progress", 0);
    task["parent"] = json!("sample-feature");
    task["blocked_by"] = json!("sample-blocker");
    task["blocked_reason"] = json!("Prerequisite remains open");
    task["unblock_note"] = json!("Finish prerequisite");
    task["deadline"] = json!("2026-08-23T10:00:00.000Z");
    let mut done = item("sample-done", "Task", "closed", 4);
    done["parent"] = json!("sample-feature");
    let mut blocked = item("sample-blocked", "Task", "blocked", 3);
    blocked["dependencies"] = json!([{"id":"sample-task","kind":"depends_on","created_at":CLOCK},{"id":"sample-missing","kind":"related","created_at":CLOCK}]);
    let mut prerequisite = item("sample-blocker", "Task", "open", 1);
    prerequisite["updated_at"] = json!("2026-08-01T10:00:00.000Z");
    prerequisite["order"] = json!(2);
    prerequisite["dependencies"] = json!([{"id":"sample-done","kind":"related_to"}]);
    fixture(root, &[epic, feature, task, done, blocked, prerequisite])?;
    fs::write(
        root.join(".agents/pm/history/sample-deleted.jsonl"),
        json_lines(&[
            json!({"ts":CLOCK,"author":"fixture","op":"create","message":"Synthetic retained history"}),
            json!({"ts":CLOCK,"op":"update"}),
        ]),
    )?;
    Ok(())
}
/// Rich context sections preserve hierarchy, dependencies and linked evidence.
#[test]
fn context_sections_match_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("rich native orientation") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    rich_fixture(root.path())?;
    for depth in ["brief", "standard", "deep", "full"] {
        for controls in [
            vec![],
            vec!["--limit", "0"],
            vec!["--limit", "2"],
            vec!["--limit", "3", "--token-budget", "100"],
            vec!["--no-tags"],
        ] {
            for output in [vec![], vec!["--json"]] {
                let mut args = vec!["context", "--depth", depth];
                args.extend(&controls);
                args.extend(&output);
                let runtime = root.path().join(".agents/pm/runtime");
                if runtime.exists() {
                    fs::remove_dir_all(runtime)?;
                }
                let expected = oracle(&driver, root.path(), &args)?;
                let actual = native(root.path(), &args)?;
                compare(&expected, &actual, &args);
            }
        }
        let mut cursor = None;
        for _ in 0..4 {
            let mut args = vec!["context", "--depth", depth, "--limit", "1", "--json"];
            if let Some(cursor) = cursor.as_deref() {
                args.extend(["--after", cursor]);
            }
            let expected = oracle(&driver, root.path(), &args)?;
            compare(&expected, &native(root.path(), &args)?, &args);
            cursor = serde_json::from_slice::<Value>(&expected.stdout)?["next_cursor"]
                .as_str()
                .map(ToString::to_string);
            if cursor.is_none() {
                break;
            }
        }
    }
    Ok(())
}

/// Completion preserves the oracle's IEEE rounding artifact at a mathematical half percent.
#[test]
fn context_completion_rounding_matches_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("context completion rounding")
    else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    let mut parent = item("sample-parent", "Epic", "in_progress", 0);
    parent["title"] = json!("Completion rounding boundary");
    let mut rows = vec![parent];
    for index in 0..200 {
        let mut child = item(
            &format!("sample-child-{index:03}"),
            "Task",
            if index < 29 { "closed" } else { "open" },
            3,
        );
        child["parent"] = json!("sample-parent");
        rows.push(child);
    }
    fixture(root.path(), &rows)?;
    for output in [vec![], vec!["--json"]] {
        let mut args = vec!["context", "--depth", "deep", "--limit", "1"];
        args.extend(output);
        let expected = oracle(&driver, root.path(), &args)?;
        compare(&expected, &native(root.path(), &args)?, &args);
    }
    Ok(())
}

/// Cursor aliases serve identical pages and invalid continuations retain context refusal envelopes.
#[test]
fn context_cursor_refusals_match_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("context cursor refusals") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    fixture(
        root.path(),
        &[
            item("sample-a", "Task", "open", 1),
            item("sample-b", "Task", "open", 2),
        ],
    )?;
    let first = oracle(&driver, root.path(), &["context", "--limit", "1", "--json"])?;
    let value: Value = serde_json::from_slice(&first.stdout)?;
    let cursor = value["next_cursor"].as_str().ok_or("cursor")?;
    let expected = oracle(
        &driver,
        root.path(),
        &["context", "--limit", "1", "--after", cursor, "--json"],
    )?;
    let alias = ["context", "--limit", "1", "--cursor", cursor, "--json"];
    compare(&expected, &native(root.path(), &alias)?, &alias);
    for args in [
        vec!["context", "--after", "bad"],
        vec!["context", "--after", "bad", "--json"],
        vec!["context", "--depth", "deep", "--after", cursor, "--json"],
    ] {
        compare(
            &oracle(&driver, root.path(), &args)?,
            &native(root.path(), &args)?,
            &args,
        );
    }
    Ok(())
}

/// Deep fallback retains descendant completion and resolved blocker evidence with no active work.
#[test]
fn context_blocked_fallback_deep_matches_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("deep blocked fallback") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    let mut parent = item("sample-parent", "Epic", "blocked", 1);
    parent["blocked_by"] = json!("sample-done");
    let mut child = item("sample-child", "Task", "blocked", 0);
    child["parent"] = json!("sample-parent");
    child["blocked_by"] = json!("sample-missing");
    child["deadline"] = json!("2026-08-23T10:00:00.000Z");
    let mut done = item("sample-done", "Task", "closed", 2);
    done["parent"] = json!("sample-parent");
    fixture(root.path(), &[parent, child, done])?;
    for controls in [vec![], vec!["--limit", "1"]] {
        for output in [vec![], vec!["--json"]] {
            let mut args = vec!["context", "--depth", "deep"];
            args.extend(&controls);
            args.extend(output);
            compare(
                &oracle(&driver, root.path(), &args)?,
                &native(root.path(), &args)?,
                &args,
            );
        }
    }
    Ok(())
}
/// Relationship packets preserve bounded traversal, inverse relationships and root evidence.
#[test]
fn relationship_packets_match_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("native relationship packets")
    else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    rich_fixture(root.path())?;
    for id in [
        "sample-epic",
        "sample-task",
        "sample-blocked",
        "SAMPLE-TASK",
    ] {
        for controls in [
            vec![],
            vec!["--summary"],
            vec!["--max-depth", "0"],
            vec!["--node-limit", "1"],
            vec!["--edge-limit", "1"],
            vec!["--token-budget", "800"],
            vec!["--direction", "incoming"],
            vec!["--direction", "outgoing", "--kind", "parent,blocks"],
        ] {
            for output in [vec![], vec!["--json"]] {
                let mut args = vec!["deps", id, "--format", "context"];
                args.extend(&controls);
                args.extend(&output);
                compare(
                    &oracle(&driver, root.path(), &args)?,
                    &native(root.path(), &args)?,
                    &args,
                );
            }
        }
    }
    let mut cursor = None;
    for _ in 0..3 {
        let mut args = vec![
            "deps",
            "sample-task",
            "--format",
            "context",
            "--node-limit",
            "1",
            "--json",
        ];
        if let Some(cursor) = cursor.as_deref() {
            args.extend(["--cursor", cursor]);
        }
        let expected = oracle(&driver, root.path(), &args)?;
        compare(&expected, &native(root.path(), &args)?, &args);
        cursor =
            serde_json::from_slice::<Value>(&expected.stdout)?["context"]["meta"]["nextCursor"]
                .as_str()
                .map(ToString::to_string);
        if cursor.is_none() {
            break;
        }
    }
    Ok(())
}
/// Plain external namespaces and colliding local IDs retain the oracle's distinct graph keys.
#[test]
fn relationship_external_namespaces_match_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("external relationship namespaces")
    else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    for collision in [false, true] {
        let mut start = item("sample-root", "Task", "open", 1);
        start["dependencies"] = json!([
            {"id":"sample-global","kind":"related","source_kind":"global","created_at":CLOCK},
            {"id":"sample-external","kind":"blocked_by","source_kind":"external","created_at":CLOCK},
            {"id":"https://example.invalid/item","kind":"related","created_at":CLOCK},
            {"id":"github:fixture/issue","kind":"related","created_at":CLOCK},
            {"id":"sample-local","kind":"related","created_at":CLOCK}
        ]);
        let mut rows = vec![start, item("sample-local", "Task", "open", 2)];
        if collision {
            rows.push(item("sample-global", "Task", "open", 2));
            rows.push(item("sample-external", "Task", "open", 2));
        }
        fixture(root.path(), &rows)?;
        for summary in [false, true] {
            for encoding in [vec![], vec!["--json"]] {
                let mut args = vec!["deps", "sample-root", "--format", "context"];
                args.push(if summary { "--summary" } else { "--full" });
                args.extend(&encoding);
                let expected = oracle(&driver, root.path(), &args)?;
                assert!(expected.status.success());
                compare(&expected, &native(root.path(), &args)?, &args);
                if !summary && !encoding.is_empty() {
                    let parsed: Value = serde_json::from_slice(&expected.stdout)?;
                    let nodes = parsed["context"]["nodes"].as_array().ok_or("nodes")?;
                    for name in ["global", "external"] {
                        let id =
                            format!("{}sample-{name}", if collision { "external:" } else { "" });
                        let node = nodes
                            .iter()
                            .find(|node| node["id"] == id)
                            .ok_or("external node")?;
                        assert_eq!(node["status"], "external");
                        assert_eq!(node["title"], format!("[external] sample-{name}"));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Packet refusal envelopes match the published alias recovery and unhandled-error contracts.
#[test]
fn relationship_packet_refusals_match_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("relationship packet refusals")
    else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    rich_fixture(root.path())?;
    for controls in [
        vec!["--token-budget", "1"],
        vec!["--cursor", "bad"],
        vec!["--direction", "sideways"],
        vec!["--kind", "unknown"],
    ] {
        for output in [vec![], vec!["--json"]] {
            let mut args = vec!["deps", "sample-task", "--format", "context"];
            args.extend(&controls);
            args.extend(output);
            compare(
                &oracle(&driver, root.path(), &args)?,
                &native(root.path(), &args)?,
                &args,
            );
        }
    }
    Ok(())
}

/// Empty, ranked, blocked and budgeted brief context matches complete oracle bytes.
#[test]
fn context_matches_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("orientation context") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    let initialized = oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?;
    assert!(
        initialized.status.success(),
        "{}",
        String::from_utf8_lossy(&initialized.stderr)
    );
    for size in [0, 1, 12] {
        let items: Vec<_> = (0..size)
            .map(|n| {
                let mut row = item(
                    &format!("sample-{n:03}"),
                    if n % 3 == 0 { "Feature" } else { "Task" },
                    if n % 4 == 0 { "in_progress" } else { "open" },
                    n % 5,
                );
                if n > 0 {
                    row["parent"] = json!("sample-000");
                }
                row
            })
            .collect();
        fixture(root.path(), &items)?;
        for args in [
            vec!["context"],
            vec!["context", "--json"],
            vec!["context", "--depth", "deep", "--json"],
            vec!["context", "--depth", "deep"],
            vec!["context", "--depth", "standard", "--limit", "3", "--json"],
            vec!["context", "--depth", "full", "--json"],
            vec!["context", "--limit", "3"],
            vec!["context", "--limit", "3", "--json"],
            vec!["context", "--limit", "0", "--json"],
            vec![
                "context",
                "--limit",
                "10",
                "--depth",
                "brief",
                "--no-tags",
                "--json",
            ],
            vec!["context", "--limit", "4", "--token-budget", "100", "--json"],
            vec!["context", "--limit", "bad", "--json"],
            vec!["context", "--token-budget", "0", "--json"],
        ] {
            // Derived runtime caches are outside authoritative fixture data.
            let runtime = root.path().join(".agents/pm/runtime");
            if runtime.exists() {
                fs::remove_dir_all(&runtime)?;
            }
            compare(
                &oracle(&driver, root.path(), &args)?,
                &native(root.path(), &args)?,
                &args,
            );
        }
    }
    Ok(())
}
/// Cycles, repeated edges, missing targets and bounded graph projections match.
#[test]
fn deps_matches_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("orientation deps") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    let initialized = oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?;
    assert!(
        initialized.status.success(),
        "{}",
        String::from_utf8_lossy(&initialized.stderr)
    );
    let mut a = item("sample-a", "Task", "open", 1);
    let mut b = item("sample-b", "Task", "blocked", 2);
    let c = item("sample-c", "Task", "closed", 3);
    a["dependencies"] = json!([{"id":"sample-b","kind":"blocked_by","created_at":CLOCK},{"id":"sample-c","kind":"related","created_at":CLOCK},{"id":"sample-missing","kind":"related","created_at":CLOCK}]);
    b["dependencies"] = json!([{"id":"sample-a","kind":"related","created_at":CLOCK},{"id":"sample-c","kind":"related","created_at":CLOCK}]);
    fixture(root.path(), &[a, b, c])?;
    for args in [
        vec!["deps", "sample-a"],
        vec!["deps", "sample-a", "--json"],
        vec!["deps", "sample-a", "--format", "graph", "--json"],
        vec!["deps", "sample-a", "--summary", "--json"],
        vec!["deps", "sample-a", "--collapse", "repeated", "--json"],
        vec!["deps", "sample-a", "--max-depth", "0", "--json"],
        vec!["deps", "sample-a", "--node-limit", "2", "--json"],
        vec!["deps", "sample-a", "--edge-limit", "1", "--json"],
        vec!["deps", "sample-a", "--token-budget", "400", "--json"],
        vec!["deps", "sample-absent", "--json"],
        vec!["deps", "sample-a", "--max-depth", "bad", "--json"],
    ] {
        compare(
            &oracle(&driver, root.path(), &args)?,
            &native(root.path(), &args)?,
            &args,
        );
    }
    Ok(())
}
/// History defaults, latest-entry limits and record hash tampering retain oracle behavior.
#[test]
#[allow(clippy::too_many_lines)]
fn history_matches_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("orientation history") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let root = tempfile::tempdir()?;
    let initialized = oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?;
    assert!(
        initialized.status.success(),
        "{}",
        String::from_utf8_lossy(&initialized.stderr)
    );
    fixture(root.path(), &[])?;
    let created = oracle(
        &driver,
        root.path(),
        &[
            "create",
            "--id",
            "sample-a",
            "--title",
            "History fixture",
            "--type",
            "Task",
            "--author",
            "fixture",
            "--create-mode",
            "progressive",
        ],
    )?;
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let changed = oracle(
        &driver,
        root.path(),
        &[
            "comment",
            "sample-a",
            "History evidence",
            "--author",
            "fixture",
        ],
    )?;
    assert!(changed.status.success());
    for args in [
        vec!["history", "sample-a"],
        vec!["history", "sample-a", "--json"],
        vec!["history", "sample-a", "--full", "--json"],
        vec!["history", "sample-a", "--limit", "1", "--json"],
        vec!["history", "sample-a", "--limit", "0", "--json"],
        vec!["history", "sample-a", "--verify", "--strict-exit", "--json"],
        vec!["history", "sample-a", "--limit", "bad", "--json"],
        vec!["history", "sample-missing", "--json"],
    ] {
        compare(
            &oracle(&driver, root.path(), &args)?,
            &native(root.path(), &args)?,
            &args,
        );
    }
    let path = root.path().join(".agents/pm/history/sample-a.jsonl");
    let raw = fs::read_to_string(&path)?;
    let mut rows: Vec<Value> = raw
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    let document = pm_rust::Workspace::discover(root.path())?.get("sample-a")?;
    let comment = document
        .metadata
        .extra
        .get("comments")
        .ok_or("comment fixture")?[0]
        .clone();
    let hash = rows.last().ok_or("history fixture")?["after_hash"].clone();
    let mut tested = rows.clone();
    tested.push(seal_record(json!({"op":"update","author":"fixture","ts":CLOCK,"item_hash_version":3,"before_hash":hash,"after_hash":hash,"patch":[{"op":"test","path":"/metadata/comments/0","value":comment}]}))?);
    fs::write(&path, json_lines(&tested))?;
    let args = ["history", "sample-a", "--verify", "--strict-exit", "--json"];
    let expected = oracle(&driver, root.path(), &args)?;
    assert!(
        expected.status.success(),
        "{}",
        String::from_utf8_lossy(&expected.stdout)
    );
    compare(&expected, &native(root.path(), &args)?, &args);
    rows[0]["author"] = json!("tampered");
    fs::write(
        path,
        rows.iter().fold(String::new(), |mut output, row| {
            use std::fmt::Write;
            let _ = writeln!(output, "{row}");
            output
        }),
    )?;
    for args in [
        vec!["history", "sample-a", "--verify", "--json"],
        vec!["history", "sample-a", "--verify", "--strict-exit", "--json"],
    ] {
        compare(
            &oracle(&driver, root.path(), &args)?,
            &native(root.path(), &args)?,
            &args,
        );
    }
    let original_rows: Vec<Value> = raw
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    let original = &original_rows[0];
    let mut canonical = original["patch"].clone();
    canonical.sort_all_objects();
    let patch_hash = sha2::Sha256::digest(serde_json::to_vec(&canonical)?)
        .iter()
        .fold(String::new(), |mut output, byte| {
            use std::fmt::Write;
            let _ = write!(output, "{byte:02x}");
            output
        });
    for tamper in [false, true] {
        let mut proof = json!({"before_hash":original["before_hash"],"after_hash":original["after_hash"],"item_hash_version":original["item_hash_version"],"hash_algorithm":original["hash_algorithm"],"patch_hash":patch_hash,"record_hash_version":original["record_hash_version"],"record_hash":original["record_hash"],"record":original});
        if tamper {
            proof["record"]["author"] = json!("tampered-prior-author");
        }
        let mut current = original.clone();
        current["reanchor_evidence"] = json!([proof]);
        let mut rewritten = original_rows.clone();
        rewritten[0] = seal_record(current)?;
        fs::write(
            root.path().join(".agents/pm/history/sample-a.jsonl"),
            json_lines(&rewritten),
        )?;
        let args = ["history", "sample-a", "--verify", "--strict-exit", "--json"];
        compare(
            &oracle(&driver, root.path(), &args)?,
            &native(root.path(), &args)?,
            &args,
        );
    }
    Ok(())
}

/// Parallel readers preserve authoritative bytes while an unrelated native writer commits.
#[test]
fn orientation_reads_preserve_storage_during_concurrent_writes() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("orientation concurrent reads")
    else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    fixture(
        root.path(),
        &[
            item("sample-read", "Task", "open", 1),
            item("sample-write", "Task", "open", 2),
        ],
    )?;
    let target = root.path().join(".agents/pm/tasks/sample-read.toon");
    let history = root.path().join(".agents/pm/history/sample-read.jsonl");
    let before = (fs::read(&target)?, fs::read(&history)?);
    let result = std::thread::scope(|scope| -> Result<(), Box<dyn std::error::Error>> {
        let root_path = root.path();
        let readers: Vec<_> = ["context", "history", "deps"]
            .into_iter()
            .map(|command| {
                scope.spawn(move || {
                    for _ in 0..4 {
                        let args = if command == "context" {
                            vec![command, "--limit", "10", "--json"]
                        } else {
                            vec![command, "sample-read", "--json"]
                        };
                        let result = native(root_path, &args).map_err(|error| error.to_string())?;
                        if !result.status.success() {
                            return Err(String::from_utf8_lossy(&result.stderr).into_owned());
                        }
                        let value: Value = serde_json::from_slice(&result.stdout)
                            .map_err(|error| error.to_string())?;
                        if value.is_null() {
                            return Err("empty orientation read".to_owned());
                        }
                    }
                    Ok::<(), String>(())
                })
            })
            .collect();
        for iteration in 0..4 {
            let title = format!("Concurrent update {iteration}");
            let result = native(
                root.path(),
                &[
                    "update",
                    "sample-write",
                    "--title",
                    &title,
                    "--author",
                    "fixture",
                    "--timestamp",
                    CLOCK,
                ],
            )?;
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
        for reader in readers {
            reader
                .join()
                .map_err(|_| "reader thread failed")?
                .map_err(|error| format!("read failed: {error}"))?;
        }
        Ok(())
    });
    result?;
    assert_eq!((fs::read(target)?, fs::read(history)?), before);
    for args in [
        vec!["history", "sample-read", "--json"],
        vec!["deps", "sample-read", "--json"],
    ] {
        compare(
            &oracle(&driver, root.path(), &args)?,
            &native(root.path(), &args)?,
            &args,
        );
    }
    Ok(())
}

/// Unsupported orientation options refuse in a problem envelope without touching storage.
#[test]
fn unsupported_orientation_flags_refuse_explicitly() -> TestResult {
    let root = tempfile::tempdir()?;
    fixture(root.path(), &[])?;
    for args in [
        vec!["--json", "context"],
        vec!["context", "-x"],
        vec!["context", "extra"],
    ] {
        let result = native(root.path(), &args)?;
        assert_eq!(result.status.code(), Some(2));
    }
    for command in ["context", "history", "deps"] {
        let args = if command == "context" {
            vec![command, "--unknown"]
        } else {
            vec![command, "sample-a", "--unknown"]
        };
        let result = native(root.path(), &args)?;
        assert_eq!(result.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&result.stderr).contains("Unsupported native option"));
        let args = if command == "context" {
            vec!["--lean", command, "--json"]
        } else {
            vec!["--lean", command, "sample-a", "--json"]
        };
        let result = native(root.path(), &args)?;
        assert_eq!(result.status.code(), Some(2));
        let payload: Value = serde_json::from_slice(&result.stderr)?;
        assert_eq!(payload["code"], "unsupported_native_option");
        assert_eq!(payload["flag"], "--lean");
    }
    // Parsing must fail before workspace discovery for every deferred option.
    for (command, flags) in [
        (
            "context",
            vec![
                "--date",
                "--from",
                "--to",
                "--past",
                "--type",
                "--tag",
                "--priority",
                "--assignee",
                "--assignee-filter",
                "--sprint",
                "--release",
                "--parent",
                "--format",
                "--fields",
                "--stale-threshold",
                "--explain-ranking",
            ],
        ),
        (
            "history",
            vec![
                "--provenance",
                "--provenance-summary",
                "--harness",
                "--agent-instance",
                "--provenance-filter",
                "--diff",
                "--field",
                "--format",
            ],
        ),
        ("deps", vec!["--add", "--remove"]),
    ] {
        for flag in flags {
            let mut args = vec![command];
            if command != "context" {
                args.push("sample-read");
            }
            args.extend([flag, "--json"]);
            let result = native(root.path(), &args)?;
            assert_eq!(result.status.code(), Some(2));
            assert!(result.stdout.is_empty());
            let error: Value = serde_json::from_slice(&result.stderr)?;
            assert_eq!(error["code"], "unsupported_native_option");
            assert_eq!(error["flag"], flag);
        }
    }
    Ok(())
}

/// Default group caps and trusted session/feedback evidence preserve exact orientation bytes.
#[test]
fn context_group_caps_and_affinity_match_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("orientation affinity") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    let items: Vec<_> = (0..40)
        .map(|n| item(&format!("sample-{n:03}"), "Task", "in_progress", 2))
        .collect();
    fixture(root.path(), &items)?;
    let pm = root.path().join(".agents/pm");
    let settings_path = pm.join("settings.json");
    let mut settings: Value = serde_json::from_slice(&fs::read(&settings_path)?)?;
    settings["author_default"] = json!("fixture");
    fs::write(settings_path, serde_json::to_vec(&settings)?)?;
    for args in [vec!["context"], vec!["context", "--json"]] {
        let runtime = pm.join("runtime");
        if runtime.exists() {
            fs::remove_dir_all(runtime)?;
        }
        compare(
            &oracle(&driver, root.path(), &args)?,
            &native(root.path(), &args)?,
            &args,
        );
    }
    let mut open_items = items.clone();
    for item in &mut open_items {
        item["status"] = json!("open");
    }
    fixture(root.path(), &open_items)?;
    let hash = sha2::Sha256::digest(b"fixture")
        .iter()
        .fold(String::new(), |mut output, byte| {
            use std::fmt::Write;
            let _ = write!(output, "{byte:02x}");
            output
        });
    let key = format!("author-{}", &hash[..24]);
    let session = json!({"semantic_attribution":{key:{"role":"implementer","confidence":"high","topic":"fixture","rule_version":"v2","active_item_ids":["sample-008"],"focused_item_id":"sample-009","evidence":["lineage:sample-007"]}}});
    let events = [
        json!({"kind":"serve","schema_version":2,"serve_id":"fixture-serving","author":"fixture","surface":"context","at":"2026-08-21T10:00:00.000Z","rows":[{"id":"sample-006","rank":1,"included":true}]}),
        json!({"kind":"delivery","schema_version":2,"serve_id":"fixture-serving","author":"fixture","at":"2026-08-21T10:00:00.000Z","delivered_item_ids":["sample-006"]}),
        json!({"kind":"touch","author":"fixture","item_id":"sample-006","intent":"get","at":"2026-08-21T11:00:00.000Z"}),
    ];
    let ledger = events.iter().fold(String::new(), |mut output, event| {
        use std::fmt::Write;
        let _ = writeln!(output, "{event}");
        output
    });
    for args in [
        vec!["context", "--limit", "4", "--json"],
        vec![
            "context",
            "--limit",
            "4",
            "--token-budget",
            "1600",
            "--json",
        ],
    ] {
        let seed = || -> TestResult {
            let runtime = pm.join("runtime");
            if runtime.exists() {
                fs::remove_dir_all(&runtime)?;
            }
            fs::create_dir_all(&runtime)?;
            fs::write(runtime.join("session.json"), serde_json::to_vec(&session)?)?;
            fs::write(runtime.join("context-usage.jsonl"), &ledger)?;
            Ok(())
        };
        seed()?;
        let expected = oracle(&driver, root.path(), &args)?;
        seed()?;
        let actual = native(root.path(), &args)?;
        compare(&expected, &actual, &args);
    }
    Ok(())
}

/// Weekly events on closed items still consume the agenda cap before terminal filtering.
#[test]
fn context_weekly_calendar_matches_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("orientation weekly calendar")
    else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    let mut active = item("sample-active", "Task", "open", 1);
    active["events"] = json!([{"start_at":"2026-08-19T10:00:00.000Z","end_at":"2026-08-19T10:30:00.000Z","title":"Weekly fixture","all_day":false,"timezone":"UTC","recurrence":{"freq":"weekly","interval":1,"count":8,"by_weekday":["mon","wed","fri"],"exdates":["2026-08-24T10:00:00.000Z"]}}]);
    active["reminders"] = json!([{"at":"2026-08-26T10:00:00.000Z","text":"Reminder fixture"}]);
    let mut closed = item("sample-closed", "Task", "closed", 0);
    closed["events"] = json!([{"start_at":"2026-04-10T09:00:00.000Z","end_at":"2026-04-10T09:30:00.000Z","title":"Closed weekly fixture","all_day":false,"recurrence":{"freq":"weekly","by_weekday":["fri"]}}]);
    fixture(root.path(), &[active, closed])?;
    for args in [
        vec!["context", "--limit", "10", "--json"],
        vec!["context", "--limit", "10"],
        vec!["context", "--limit", "1", "--json"],
    ] {
        let runtime = root.path().join(".agents/pm/runtime");
        if runtime.exists() {
            fs::remove_dir_all(runtime)?;
        }
        compare(
            &oracle(&driver, root.path(), &args)?,
            &native(root.path(), &args)?,
            &args,
        );
    }
    Ok(())
}

/// Large append-only timelines retain exact bounded projection and refusal receipts.
#[test]
fn bounded_history_and_refusals_match_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("orientation bounded history")
    else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let root = tempfile::tempdir()?;
    for args in [
        vec!["init", "sample-", "--defaults"],
        vec![
            "create",
            "--id",
            "sample-a",
            "--title",
            "Bounded history",
            "--type",
            "Task",
            "--author",
            "fixture",
        ],
        vec![
            "update",
            "sample-a",
            "--title",
            "Bounded history",
            "--author",
            "fixture",
        ],
    ] {
        let result = oracle(&driver, root.path(), &args)?;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let path = root.path().join(".agents/pm/history/sample-a.jsonl");
    let original = fs::read_to_string(&path)?;
    let mut entries: Vec<Value> = original
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    let repeated = entries.last().cloned().ok_or("missing update record")?;
    entries.extend(std::iter::repeat_n(repeated, 300));
    fs::write(&path, json_lines(&entries))?;
    for args in [
        vec!["history", "sample-a"],
        vec!["history", "sample-a", "--json"],
        vec!["history", "sample-a", "--compact", "--json"],
        vec!["history", "sample-a", "--limit", "1.9", "--json"],
        vec!["history", "sample-a", "--full", "--compact", "--json"],
        vec!["history", "sample-a", "--strict-exit", "--json"],
    ] {
        compare(
            &oracle(&driver, root.path(), &args)?,
            &native(root.path(), &args)?,
            &args,
        );
    }
    for raw in ["{broken}\n", "<<<<<<< conflict\n"] {
        fs::write(&path, raw)?;
        let args = ["history", "sample-a", "--json"];
        compare(
            &oracle(&driver, root.path(), &args)?,
            &native(root.path(), &args)?,
            &args,
        );
    }
    Ok(())
}

/// The marked-copy harness accepts identical nonzero packet statuses and still writes its receipt.
#[test]
fn real_copy_harness_accepts_matching_packet_refusals() -> TestResult {
    let Some(interpreter) = node_harness::interpreter_or_skip()? else {
        return Ok(());
    };
    let Some(published) =
        published_cli::published_cli_or_skip("matching packet refusals in harness")
    else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    let mut start = item("sample-root", "Task", "open", 1);
    start["parent"] = json!("sample-parent");
    start["files"] = json!([{"path": "x".repeat(6000)}]);
    fixture(
        root.path(),
        &[start, item("sample-parent", "Task", "open", 2)],
    )?;
    let args = [
        "deps",
        "sample-root",
        "--format",
        "context",
        "--token-budget",
        "800",
        "--json",
    ];
    let refused = oracle(&driver, root.path(), &args)?;
    assert!(!refused.status.success());
    compare(&refused, &native(root.path(), &args)?, &args);
    fs::write(
        root.path().join(".pm-rust-read-copy"),
        "synthetic fixture\n",
    )?;
    let receipt = root.path().join("receipt.json");
    let output = interpreter
        .command()
        .arg("scripts/context-real.mts")
        .arg("--copy")
        .arg(root.path())
        .arg("--driver")
        .arg(&driver)
        .args([
            "--binary",
            env!("CARGO_BIN_EXE_pm-rust"),
            "--samples",
            "1",
            "--output",
        ])
        .arg(&receipt)
        .output()?;
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&fs::read(receipt)?)?;
    assert_eq!(result["authoritative_unchanged"], true);
    assert_eq!(result["packet_roots"], 1);
    assert_eq!(result["packet_comparisons"], 11);
    assert!(result["comparisons"].as_u64().unwrap_or_default() > 11);
    Ok(())
}

/// The real-copy entry point applies the shared opt-in rule before oracle work.
#[test]
fn real_copy_harness_missing_runtime_obeys_opt_in() -> TestResult {
    let directory = tempfile::tempdir()?;
    for required in [false, true] {
        let mut child = Command::new(std::env::current_exe()?);
        child
            .args([
                "--exact",
                "real_copy_harness_accepts_matching_packet_refusals",
                "--nocapture",
            ])
            .env("PM_NODE_INTERPRETER", directory.path().join("missing-node"))
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

/// Both packet budget floors retain the previous useful result in the pinned published CLI.
#[test]
fn relationship_root_evidence_floor_retains_oracle_parity() -> TestResult {
    let Some(published) =
        published_cli::published_cli_or_skip("root evidence rendered-budget floor")
    else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    fixture(root.path(), &[item("sample-root", "Task", "open", 1)])?;
    for encoding in [vec![], vec!["--json"]] {
        let mut args = vec![
            "deps",
            "sample-root",
            "--format",
            "context",
            "--token-budget",
            "100",
        ];
        args.extend(&encoding);
        let expected = oracle(&driver, root.path(), &args)?;
        assert_eq!(expected.status.code(), Some(2));
        compare(&expected, &native(root.path(), &args)?, &args);
        if !encoding.is_empty() {
            let refusal: Value = serde_json::from_slice(&expected.stderr)?;
            let retry = refusal["recovery"]["suggested_retry_args"]
                .as_array()
                .ok_or("missing verified retry")?
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>();
            let delivered = oracle(&driver, root.path(), &retry)?;
            assert!(delivered.status.success());
            compare(&delivered, &native(root.path(), &retry)?, &retry);
            let output: Value = serde_json::from_slice(&delivered.stdout)?;
            assert!(
                output["context"]["meta"]["usedTokens"]
                    .as_u64()
                    .ok_or("used tokens")?
                    <= refusal["recovery"]["token_budget"]["feasible_tokens"]
                        .as_u64()
                        .ok_or("feasible tokens")?
            );
        }
    }
    Ok(())
}

/// Huge delivery controls keep budget refusal and verified recovery bounded and exact.
#[test]
fn relationship_large_edge_limit_finishes_at_budget_floor() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("bounded packet reference retries")
    else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    for missing in [false, true] {
        let mut row = item("sample-root", "Task", "open", 1);
        if missing {
            row["dependencies"] = json!([
                {"id":"sample-missing","kind":"related","created_at":CLOCK},
                {"id":"sample-missing","kind":"related","created_at":CLOCK}
            ]);
        }
        fixture(root.path(), &[row])?;
        for encoding in [vec![], vec!["--json"]] {
            for limit in [None, Some("1000000000"), Some("9007199254740991")] {
                let mut args = vec![
                    "deps",
                    "sample-root",
                    "--format",
                    "context",
                    "--token-budget",
                    if missing { "300" } else { "100" },
                ];
                args.extend(&encoding);
                if let Some(limit) = limit {
                    args.extend(["--edge-limit", limit]);
                }
                let expected = oracle(&driver, root.path(), &args)?;
                assert_eq!(expected.status.code(), Some(2));
                let mut command = Command::new(env!("CARGO_BIN_EXE_pm-rust"));
                mutation_differential::isolate_environment(&mut command);
                command
                    .current_dir(root.path())
                    .env("HOME", root.path().join("synthetic-home"))
                    .env("USERPROFILE", root.path().join("synthetic-home"))
                    .args(&args);
                let actual = assert_cmd::Command::from_std(command)
                    .timeout(std::time::Duration::from_secs(5))
                    .output()?;
                compare(&expected, &actual, &args);
            }
        }
    }
    Ok(())
}

/// Reference count caps preserve complete oracle bytes under kind, depth and delivery filters.
#[test]
fn relationship_reference_limits_match_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("filtered packet reference limits")
    else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    let mut start = item("sample-root", "Task", "open", 1);
    start["parent"] = json!("sample-parent");
    start["dependencies"] = json!([
        {"id":"sample-missing","kind":"related_to","created_at":CLOCK},
        {"id":"sample-missing","kind":"related_to","created_at":CLOCK},
        {"id":"sample-other","kind":"blocked_by","created_at":CLOCK},
        {"id":"sample-parent","kind":"related","created_at":CLOCK},
        {"id":"sample-parent","kind":"blocked_by","created_at":CLOCK}
    ]);
    let mut unrelated = item("sample-unrelated", "Task", "closed", 2);
    unrelated["parent"] = json!("sample-unreachable");
    fixture(root.path(), &[start, unrelated])?;
    for controls in [
        vec![],
        vec!["--kind", "child"],
        vec!["--direction", "incoming"],
        vec!["--max-depth", "0"],
        vec!["--summary"],
    ] {
        for (limit, budget) in [("1", "300"), ("1", "1200"), ("2", "1200"), ("40", "300")] {
            for encoding in [vec![], vec!["--json"]] {
                let mut args = vec![
                    "deps",
                    "sample-root",
                    "--format",
                    "context",
                    "--edge-limit",
                    limit,
                    "--token-budget",
                    budget,
                ];
                args.extend(&controls);
                args.extend(&encoding);
                compare(
                    &oracle(&driver, root.path(), &args)?,
                    &native(root.path(), &args)?,
                    &args,
                );
            }
        }
    }
    Ok(())
}

/// GH-1511 keeps blocked active rows inside the same bounded window as other focus rows.
#[test]
fn blocked_focus_cursor_pages_match_october_21_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("blocked focus pages") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    let first = item("sample-one", "Feature", "open", 1);
    let mut blocked = item("sample-two", "Task", "open", 2);
    blocked["blocked_by"] = json!("sample-missing");
    let third = item("sample-three", "Feature", "open", 3);
    fixture(root.path(), &[first, blocked, third])?;
    let mut cursor = None::<String>;
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..3 {
        let mut args = vec!["context", "--limit", "1", "--json", "--no-extension-health"];
        if let Some(cursor) = &cursor {
            args.extend(["--after", cursor]);
        }
        let expected = oracle(&driver, root.path(), &args)?;
        assert!(
            expected.status.success(),
            "{}",
            String::from_utf8_lossy(&expected.stderr)
        );
        compare(&expected, &native(root.path(), &args)?, &args);
        let page: Value = serde_json::from_slice(&expected.stdout)?;
        let rows = page["high_level"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(page["low_level"].as_array().into_iter().flatten())
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 1);
        assert!(seen.insert(rows[0]["id"].as_str().ok_or("id")?.to_owned()));
        cursor = page["next_cursor"].as_str().map(str::to_owned);
    }
    assert_eq!(seen.len(), 3);
    assert!(cursor.is_none());
    Ok(())
}

/// Padded inverse blocker IDs retain the published context classification.
#[test]
fn padded_inverse_blockers_match_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("padded inverse blockers") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    for id in ["sample-root", " sample-root ", " SAMPLE-ROOT "] {
        let start = item("sample-root", "Task", "blocked", 1);
        let mut inverse = item("sample-inverse", "Task", "open", 2);
        inverse["dependencies"] = json!([{"id":id,"kind":"blocks","created_at":CLOCK}]);
        fixture(root.path(), &[start, inverse])?;
        let args = ["context", "--depth", "deep", "--json"];
        // Refresh the oracle signal cache after replacing fixture metadata.
        assert!(oracle(&driver, root.path(), &args)?.status.success());
        let expected = oracle(&driver, root.path(), &args)?;
        assert!(
            expected.status.success(),
            "{}",
            String::from_utf8_lossy(&expected.stderr)
        );

        let document: Value = serde_json::from_slice(&expected.stdout)?;
        assert_eq!(document["blockers"][0]["blocked_by"], "sample-inverse");
        compare(&expected, &native(root.path(), &args)?, &args);
    }
    Ok(())
}

/// Complete omissions preserve pinned receipt measurements without clipped declarations.
#[test]
fn context_omitted_intent_receipt_matches_current_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("omitted intent receipt") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    for intent in ["orient", "handoff"] {
        for format in [vec![], vec!["--json"]] {
            let mut args = vec!["context", "--for", intent, "--token-budget", "256"];
            args.extend(format);
            compare(
                &oracle(&driver, root.path(), &args)?,
                &native(root.path(), &args)?,
                &args,
            );
        }
    }
    assert!(!root.path().join(".git").exists());
    Ok(())
}

/// Rich non-git trackers retain exact intent, section and continuation packets.
#[test]
fn context_intents_and_sections_match_current_oracle() -> TestResult {
    context_intents_fixture(1)
}

/// Empty non-git trackers retain intent defaults, suggestions and refusal receipts.
#[test]
fn context_intents_empty_tracker_match_current_oracle() -> TestResult {
    context_intents_fixture(0)
}

/// Wide evidence distinguishes compact intent estimates from complete wire budgets.
#[test]
fn context_intents_wire_budget_match_current_oracle() -> TestResult {
    context_intents_fixture(2)
}

/// Runs the same control matrix independently, allowing the fixed two-worker pool to balance it.
#[allow(clippy::too_many_lines)]
fn context_intents_fixture(richness: usize) -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("current context intents") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let mut comparisons = 0;
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    if richness == 1 {
        rich_fixture(root.path())?;
    }
    if richness == 2 {
        let mut row = item("sample-wide", "Feature", "open", 1);
        row["title"] = json!("Bounded orientation evidence ".repeat(7));
        fixture(root.path(), &[row])?;
        fs::write(
            root.path().join(".agents/pm/history/sample-wide.jsonl"),
            format!(
                "{}\n",
                json!({"ts":"2026-08-21T10:00:00.000Z","op":"update","author":"fixture","message":"Retained continuation evidence ".repeat(6)})
            ),
        )?;
    }
    for controls in [
        vec!["--for", "orient", "--token-budget", "255"],
        vec!["--section", "activity", "--activity-limit", "0"],
        vec!["--section", " WORKLOAD ", "--section", "workload"],
        vec!["--section", "bad"],
        vec!["--section=bad"],
        vec!["--section", "activity", "--section", "bad"],
        vec!["--activity-limit", "bad"],
        vec!["--activity-limit=bad"],
        vec!["--for=orient", "--token-budget=255"],
        vec!["--for", "orient", "--token_budget", "255"],
        vec!["--for", "orient", "--token_budget=255"],
        vec!["--for", "handoff", "--token-budget", "0"],
    ] {
        for format in [vec![], vec!["--json"]] {
            let mut args = vec!["context"];
            args.extend(&controls);
            args.extend(&format);
            compare(
                &oracle(&driver, root.path(), &args)?,
                &native(root.path(), &args)?,
                &args,
            );
            comparisons += 1;
        }
    }
    for intent in ["orient", "handoff", " ORIENT "] {
        for controls in [
            vec![],
            vec!["--limit", "0"],
            vec!["--limit", "1"],
            vec!["--limit", "50"],
            vec!["--token-budget", "256"],
            vec!["--token-budget", "1000"],
            vec!["--token-budget", "1600"],
            vec!["--token-budget", "2200"],
            vec!["--token_budget", "5000"],
            vec!["--token-budget", "0x400"],
            vec!["--depth", "brief"],
            vec!["--depth", "full"],
            vec![
                "--section",
                "blockers",
                "--section",
                "activity",
                "--activity-limit",
                "1",
            ],
            vec!["--section", "recently_created", "--no-tags"],
        ] {
            for format in [vec![], vec!["--json"]] {
                let mut args = vec!["context", "--for", intent];
                args.extend(&controls);
                args.extend(&format);
                compare(
                    &oracle(&driver, root.path(), &args)?,
                    &native(root.path(), &args)?,
                    &args,
                );
                comparisons += 1;
            }
        }
        let mut cursor = None;
        for _ in 0..4 {
            let mut args = vec!["context", "--for", intent, "--limit", "1", "--json"];
            if let Some(cursor) = cursor.as_deref() {
                args.extend(["--after", cursor]);
            }
            let expected = oracle(&driver, root.path(), &args)?;
            compare(&expected, &native(root.path(), &args)?, &args);
            comparisons += 1;
            cursor = serde_json::from_slice::<Value>(&expected.stdout)?["next_cursor"]
                .as_str()
                .map(ToString::to_string);
            if cursor.is_none() {
                break;
            }
        }
    }
    println!("current intent and section differentials (fixture {richness}): {comparisons}");
    Ok(())
}

/// ECMAScript option whitespace accepts BOM while retaining NEL as significant input.
#[test]
fn context_control_whitespace_matches_current_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("context control whitespace") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    for controls in [
        vec!["--for", "\u{feff}ORIENT\u{2003}"],
        vec!["--for", "orient", "--token-budget", "\u{feff}1000\u{feff}"],
        vec!["--for", "handoff", "--limit", "\u{feff}1"],
        vec!["--section", "\u{feff}activity"],
        vec!["--depth", "\u{feff}deep"],
        vec!["--activity-limit", "\u{feff}1"],
        vec!["--for", "orient", "--token-budget", "\u{0085}1000"],
        vec!["--for", "orient", "--limit", "\u{0085}1"],
        vec!["--activity-limit", "\u{0085}1"],
        vec!["--section", "\u{0085}activity"],
    ] {
        for encoding in [vec![], vec!["--json"]] {
            let mut args = vec!["context"];
            args.extend(&controls);
            args.extend(&encoding);
            compare(
                &oracle(&driver, root.path(), &args)?,
                &native(root.path(), &args)?,
                &args,
            );
        }
    }
    Ok(())
}

/// Reviewed edge contracts remain oracle-exact, with unsupported native continuation explicit.
#[test]
#[allow(clippy::too_many_lines)]
fn context_reviewed_contracts_match_current_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("reviewed context contracts") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    rich_fixture(root.path())?;
    let mut history = format!(
        "{}\n{}\n{}\n",
        json!({"ts":"not-a-time","op":"update","author":"fixture"}),
        json!({"ts":"000-invalid","op":"update","author":"fixture"}),
        json!({"ts":"","op":"empty_timestamp","author":"fixture"})
    );
    for index in 0..50 {
        writeln!(
            history,
            "{}",
            json!({"ts":format!("2026-08-21T10:00:{index:02}.000Z"),"op":"update","author":"fixture","message":"Retained review evidence"})
        )?;
    }
    fs::write(
        root.path().join(".agents/pm/history/sample-review.jsonl"),
        history,
    )?;
    let mut comparisons = 0;
    let mut omission_replayed = false;
    for json in [false, true] {
        let mut args = vec!["context"];
        if json {
            args.push("--json");
        }
        args.extend(["--section=bad", "--activity-limit", "--section"]);
        let expected = oracle(&driver, root.path(), &args)?;
        assert_eq!(expected.status.code(), Some(2));
        compare(&expected, &native(root.path(), &args)?, &args);
        comparisons += 1;
    }
    for suffix in ["--json", "--no-tags"] {
        let args = [
            "context",
            "--section=bad",
            "--activity-limit",
            "--section",
            suffix,
        ];
        let expected = oracle(&driver, root.path(), &args)?;
        assert_eq!(expected.status.code(), Some(2));
        compare(&expected, &native(root.path(), &args)?, &args);
        comparisons += 1;
    }
    for controls in [
        vec!["--for", "orient", "--token-budget", "1e2"],
        vec!["--for", "handoff", "--limit", "1e2"],
        vec!["--section", "hierarchy", "--section", "invalid"],
        vec!["--section", "activity", "--activity-limit", "100"],
        vec![
            "--for",
            "orient",
            "--section",
            "tests",
            "--section",
            "activity",
            "--activity-limit",
            "50",
            "--token-budget",
            "700",
        ],
        vec!["--for", "handoff", "--token-budget", "256"],
    ] {
        for encoding in [vec![], vec!["--json"]] {
            let mut args = vec!["context"];
            args.extend(&controls);
            args.extend(&encoding);
            let expected = oracle(&driver, root.path(), &args)?;
            compare(&expected, &native(root.path(), &args)?, &args);
            comparisons += 1;
            if !encoding.is_empty() && expected.status.success() {
                let result: Value = serde_json::from_slice(&expected.stdout)?;
                if controls == vec!["--section", "activity", "--activity-limit", "100"] {
                    assert!(
                        result["activity"]
                            .as_array()
                            .is_some_and(|rows| rows.iter().any(|row| row["ts"] == "000-invalid"))
                    );
                    assert!(result["activity"].as_array().is_some_and(|rows| {
                        rows.iter()
                            .all(|row| row["ts"] != "not-a-time" && row["ts"] != "")
                    }));
                }
                if let Some(command) = result["budget_exceeded"]["restore_with"].as_str() {
                    let retry: Vec<_> = command.split_whitespace().skip(1).collect();
                    assert!(!retry.contains(&"--section"));
                    let restored = oracle(&driver, root.path(), &retry)?;
                    assert!(restored.status.success());
                    compare(&restored, &native(root.path(), &retry)?, &retry);
                    comparisons += 1;
                    omission_replayed = true;
                }
            }
        }
    }
    assert!(
        omission_replayed,
        "must exercise the oracle's receipt-only recovery"
    );
    let rows: Vec<_> = (0..30)
        .map(|index| item(&format!("sample-review-{index}"), "Task", "open", 1))
        .collect();
    fixture(root.path(), &rows)?;
    let mut handoff = false;
    for budget in ["1200", "1600", "2200", "3000", "5000"] {
        let args = [
            "context",
            "--depth",
            "full",
            "--limit",
            "30",
            "--token-budget",
            budget,
            "--json",
        ];
        let expected = oracle(&driver, root.path(), &args)?;
        compare(&expected, &native(root.path(), &args)?, &args);
        comparisons += 1;
        if !expected.status.success() {
            continue;
        }
        let result: Value = serde_json::from_slice(&expected.stdout)?;
        if result["continuation_kind"] != "output_cursor" {
            continue;
        }
        let cursor = result["output_budget_truncation"]["recovery"]["cursor"]
            .as_str()
            .ok_or("output cursor")?;
        let mut retry = args.to_vec();
        retry.extend(["--output-cursor", cursor]);
        assert!(oracle(&driver, root.path(), &retry)?.status.success());
        let refused = native(root.path(), &retry)?;
        assert_eq!(refused.status.code(), Some(2));
        let refusal: Value = serde_json::from_slice(&refused.stderr)?;
        assert_eq!(refusal["code"], "unsupported_native_option");
        assert!(
            refusal["detail"]
                .as_str()
                .is_some_and(|detail| detail.contains("published pm CLI"))
        );
        handoff = true;
        break;
    }
    assert!(
        handoff,
        "must replay a published output cursor and refuse it natively"
    );
    let wide = tempfile::tempdir()?;
    assert!(
        oracle(&driver, wide.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    let mut row = item("sample-wire-budget", "Task", "open", 2);
    row["title"] = json!("Retained budget evidence ".repeat(8));
    row["tags"] = json!((0..300).map(|i| format!("tag-{i:03}")).collect::<Vec<_>>());
    fixture(wide.path(), &[row])?;
    let item_path = wide.path().join(".agents/pm/tasks/sample-wire-budget.toon");
    let before = fs::read(&item_path)?;
    let mut dual_receipt = false;
    for intent in ["orient", "handoff"] {
        for budget in [
            "256", "700", "1000", "1200", "1300", "1400", "1450", "1500", "1600", "2200", "3000",
            "5000",
        ] {
            for encoding in [vec![], vec!["--json"]] {
                let mut args = vec![
                    "context",
                    "--for",
                    intent,
                    "--limit",
                    "1",
                    "--token-budget",
                    budget,
                ];
                args.extend(&encoding);
                let expected = oracle(&driver, wide.path(), &args)?;
                compare(&expected, &native(wide.path(), &args)?, &args);
                comparisons += 1;
                if !encoding.is_empty() && expected.status.success() {
                    let value: Value = serde_json::from_slice(&expected.stdout)?;
                    dual_receipt |= value["context_intent"].is_object()
                        && value["context_intent"]["result_omitted"] != true
                        && value["read_output"].is_object();
                }
            }
        }
    }
    assert!(
        dual_receipt,
        "must compare a retained intent with its wire receipt"
    );
    let settings_path = wide.path().join(".agents/pm/settings.json");
    let original_settings: Value = serde_json::from_slice(&fs::read(&settings_path)?)?;
    for context in [None, Some(json!({}))] {
        let mut settings = original_settings.clone();
        if let Some(context) = context {
            settings["context"] = context;
        } else {
            settings
                .as_object_mut()
                .ok_or("settings object")?
                .shift_remove("context");
        }
        let settings = settings.to_string();
        fs::write(&settings_path, &settings)?;
        for encoding in [vec![], vec!["--json"]] {
            let mut args = vec!["context", "--section", "activity", "--activity-limit", "0"];
            args.extend(encoding);
            let expected = oracle(&driver, wide.path(), &args)?;
            assert!(expected.status.success());
            assert_eq!(
                fs::read(&settings_path)?,
                settings.as_bytes(),
                "oracle read changed settings"
            );
            let actual = native(wide.path(), &args)?;
            compare(&expected, &actual, &args);
            assert_eq!(
                fs::read(&settings_path)?,
                settings.as_bytes(),
                "native read changed settings"
            );
            comparisons += 1;
        }
    }
    for context in [
        Value::Null,
        json!("legacy"),
        json!([]),
        json!(1),
        json!(true),
    ] {
        let mut settings = original_settings.clone();
        settings["context"] = context;
        let settings = settings.to_string();
        fs::write(&settings_path, &settings)?;
        let args = ["context", "--for", "orient", "--json"];
        let published = oracle(&driver, wide.path(), &args)?;
        assert!(published.status.success());
        assert!(
            String::from_utf8_lossy(&published.stderr).contains("settings_read_invalid_schema")
        );
        let refused = native(wide.path(), &args)?;
        assert_eq!(refused.status.code(), Some(2));
        let payload: Value = serde_json::from_slice(&refused.stderr)?;
        assert_eq!(payload["code"], "unsupported_native_context");
        assert!(
            payload["required"]
                .as_str()
                .is_some_and(|required| required.contains("object-valued context settings"))
        );
        assert_eq!(fs::read(&settings_path)?, settings.as_bytes());
    }
    assert_eq!(fs::read(item_path)?, before);
    println!("reviewed context differentials: {comparisons}");
    Ok(())
}

/// Unsupported declarations retain typed native refusals in both CLI spellings and encodings.
#[test]
fn context_unknown_intents_have_explicit_cli_boundaries() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("unknown intent boundaries") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    for control in [vec!["--for", "unknown"], vec!["--for=unknown"]] {
        for json in [false, true] {
            let mut args = vec!["context"];
            args.extend(&control);
            if json {
                args.push("--json");
            }
            let actual = native(root.path(), &args)?;
            assert_eq!(actual.status.code(), Some(2));
            assert!(actual.stdout.is_empty());
            if json {
                let payload: Value = serde_json::from_slice(&actual.stderr)?;
                assert_eq!(payload["code"], "unsupported_native_context");
                assert!(payload["required"].is_string());
            }
            assert!(
                String::from_utf8_lossy(&actual.stderr)
                    .contains("Native context supports only core orient and handoff intents")
            );
            let expected = oracle(&driver, root.path(), &args)?;
            assert_eq!(expected.status.code(), Some(2));
            assert!(String::from_utf8_lossy(&expected.stderr).contains("Unknown context intent"));
            if json {
                let payload: Value = serde_json::from_slice(&expected.stderr)?;
                assert_eq!(payload["code"], "unknown_context_intent");
                assert_eq!(payload["refusal"]["rejected_value"], "unknown");
            }
            assert!(
                !String::from_utf8_lossy(&expected.stderr).contains("unsupported_native_context")
            );
        }
    }
    Ok(())
}

/// A missing current install skips only current reads; required runs fail and history still executes.
#[test]
fn orientation_oracles_are_gated_independently() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("independent history oracle") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let absent = scratch.path().join("missing-current");
    assert!(orientation_driver_at(scratch.path(), &published, &absent, false)?.is_none());
    assert!(orientation_driver_at(scratch.path(), &published, &absent, true).is_err());
    let driver = write_driver(scratch.path(), &published)?;
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    fixture(root.path(), &[item("sample-a", "Task", "open", 1)])?;
    let args = ["history", "sample-a", "--json"];
    let expected = oracle(&driver, root.path(), &args)?;
    assert!(expected.status.success());
    compare(&expected, &native(root.path(), &args)?, &args);
    Ok(())
}

/// Every successful native or oracle read preserves settings bytes before another invocation.
#[test]
fn context_successful_reads_preserve_settings_bytes() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("read-only context settings") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    let path = root.path().join(".agents/pm/settings.json");
    let original: Value = serde_json::from_slice(&fs::read(&path)?)?;
    for context in [None, Some(json!({})), Some(json!({"activity_limit":1}))] {
        let mut settings = original.clone();
        if let Some(context) = context {
            settings["context"] = context;
        } else {
            settings
                .as_object_mut()
                .ok_or("settings")?
                .shift_remove("context");
        }
        let before = serde_json::to_vec_pretty(&settings)?;
        fs::write(&path, &before)?;
        assert!(!root.path().join(".git").exists());
        for controls in [
            vec!["--section", "activity", "--activity-limit", "0"],
            vec!["--for", "orient"],
            vec!["--for=handoff"],
        ] {
            for encoding in [vec![], vec!["--json"]] {
                let mut args = vec!["context"];
                args.extend(&controls);
                args.extend(&encoding);
                let expected = oracle(&driver, root.path(), &args)?;
                assert!(expected.status.success());
                assert_eq!(
                    fs::read(&path)?,
                    before,
                    "oracle read changed settings: {args:?}"
                );
                let actual = native(root.path(), &args)?;
                compare(&expected, &actual, &args);
                assert_eq!(
                    fs::read(&path)?,
                    before,
                    "native read changed settings: {args:?}"
                );
            }
        }
    }
    Ok(())
}

/// Malformed item reads retain fallback diagnostics through the CLI boundary.
#[test]
fn context_item_faults_preserve_settings_warning_in_every_encoding() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("context item fault warnings")
    else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    fixture(root.path(), &[item("sample-broken", "Task", "open", 1)])?;
    let path = root.path().join(".agents/pm/settings.json");
    let mut settings: Value = serde_json::from_slice(&fs::read(&path)?)?;
    settings["context"] = json!({"activity_limit":0});
    let before = serde_json::to_vec(&settings)?;
    fs::write(&path, &before)?;
    let broken = root.path().join(".agents/pm/tasks/sample-broken.toon");
    for content in [
        "<<<<<<< conflict\n=======\n>>>>>>> conflict\n",
        "id: [broken\n",
    ] {
        fs::write(&broken, content)?;
        for intent in [vec![], vec!["--for", "orient"], vec!["--for", "handoff"]] {
            for encoding in [vec![], vec!["--json"]] {
                let mut args = vec!["context"];
                args.extend(&intent);
                args.extend(&encoding);
                let output = native(root.path(), &args)?;
                assert_eq!(output.status.code(), Some(2));
                let stderr = String::from_utf8(output.stderr)?;
                if encoding.is_empty() {
                    assert!(stderr.contains("settings_read_invalid_schema"), "{stderr}");
                    assert!(stderr.contains("pm-rust:"), "{stderr}");
                } else {
                    let payload: Value = serde_json::from_str(&stderr)?;
                    assert_eq!(payload["warnings"], json!(["settings_read_invalid_schema"]));
                    assert!(
                        payload["error"]
                            .as_str()
                            .is_some_and(|value| value.contains("sample-broken"))
                    );
                }
                assert!(output.stdout.is_empty());
                assert_eq!(fs::read(&path)?, before);
                assert_eq!(fs::read_to_string(&broken)?, content);
            }
        }
    }
    assert!(!root.path().join(".git").exists());
    Ok(())
}

/// Invalid stored limits fall back before explicit CLI overrides in non-git trackers.
#[test]
fn context_invalid_stored_activity_limits_match_current_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("stored context limit fallback")
    else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    fixture(root.path(), &[item("sample-activity", "Task", "open", 1)])?;
    fs::write(
        root.path().join(".agents/pm/history/sample-activity.jsonl"),
        format!(
            "{}\n",
            json!({"ts":"2026-08-21T10:00:00.000Z","op":"update","author":"fixture","patch":[]})
        ),
    )?;
    let path = root.path().join(".agents/pm/settings.json");
    let original: Value = serde_json::from_slice(&fs::read(&path)?)?;
    for context in [
        json!({"activity_limit":0}),
        json!({"activity_limit":-1}),
        json!({"activity_limit":1.5}),
        json!({"activity_limit":"2"}),
        json!({"activity_limit":null}),
        json!({"activity_limit":1}),
        json!({"stale_threshold_days":0}),
        json!({"default_depth":"full"}),
        json!({"next_ranking_policy":"unknown"}),
        json!({"sections":{"activity":0}}),
        json!({"sections":{"activity":false,"future_field":null}}),
    ] {
        let mut settings = original.clone();
        settings["context"] = context;
        let before = serde_json::to_vec_pretty(&settings)?;
        fs::write(&path, &before)?;
        for controls in [
            vec![],
            vec!["--activity-limit", "0"],
            vec!["--activity-limit", "1"],
            vec!["--activity-limit", "-1"],
            vec!["--activity-limit", "bad"],
            vec!["--token-budget", "bad"],
        ] {
            for encoding in [vec![], vec!["--json"]] {
                let mut args = vec!["context", "--section", "activity"];
                args.extend(&controls);
                args.extend(&encoding);
                let expected = oracle(&driver, root.path(), &args)?;

                let actual = native(root.path(), &args)?;
                compare(&expected, &actual, &args);
                assert_eq!(fs::read(&path)?, before);
                assert!(!root.path().join(".git").exists());
            }
        }
    }
    Ok(())
}

/// Extract the published settings diagnostic without normalizing refusal details.
fn warning_line(text: &str) -> Option<&str> {
    text.lines()
        .find(|line| line.starts_with("[pm] warning: settings_read_invalid_schema"))
}

/// Parser refusals retain the stored settings warning exactly like the published CLI.
#[test]
#[allow(clippy::too_many_lines)]
fn context_parser_refusals_preserve_settings_warnings() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("stored settings parser refusal")
    else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    fixture(root.path(), &[item("sample-activity", "Task", "open", 1)])?;
    let path = root.path().join(".agents/pm/settings.json");
    let original: Value = serde_json::from_slice(&fs::read(&path)?)?;
    let warning = "[pm] warning: settings_read_invalid_schema — settings.json could not be loaded and pm fell back to defaults; run pm health for remediation.";
    for (context, warns) in [
        (json!({"activity_limit": 0}), true),
        (json!({"stale_threshold_days": "x"}), true),
        (json!({"activity_limit": 2}), false),
        (json!({}), false),
    ] {
        let mut settings = original.clone();
        settings["context"] = context;
        let before = serde_json::to_vec_pretty(&settings)?;
        fs::write(&path, &before)?;
        for encoding in [vec![], vec!["--json"]] {
            let mut args = vec!["context", "--section", "activity", "--output-cursor", " "];
            args.extend(&encoding);
            let expected = oracle(&driver, root.path(), &args)?;
            let actual = native(root.path(), &args)?;
            assert_eq!(actual.status.code(), Some(2), "{args:?}");
            assert_eq!(fs::read(&path)?, before);
            if encoding.is_empty() {
                let native_stderr = String::from_utf8_lossy(&actual.stderr).into_owned();
                let oracle_stderr = String::from_utf8_lossy(&expected.stderr).into_owned();

                // The warning bytes are the published contract; the refusal body is native.
                assert_eq!(
                    warning_line(&native_stderr),
                    warning_line(&oracle_stderr),
                    "warning line parity: {args:?}"
                );
                assert!(
                    native_stderr.contains("does not support --output-cursor"),
                    "{args:?}"
                );
                if warns {
                    assert_eq!(warning_line(&native_stderr), Some(warning), "{args:?}");
                } else {
                    assert_eq!(warning_line(&native_stderr), None, "{args:?}");
                }
            } else {
                let refusal: Value = serde_json::from_slice(&actual.stderr)?;
                assert_eq!(refusal["code"], "unsupported_native_option", "{args:?}");
                if warns {
                    assert_eq!(
                        refusal["warnings"],
                        json!(["settings_read_invalid_schema"]),
                        "{args:?}"
                    );
                    assert_eq!(
                        refusal["warning_details"][0]["message"],
                        json!(warning),
                        "{args:?}"
                    );
                } else {
                    assert!(refusal.get("warnings").is_none(), "{args:?}");
                }
            }
        }
        // Other commands keep their published contract: no context settings warning.
        let history = native(
            root.path(),
            &[
                "history",
                "sample-activity",
                "--output-cursor",
                " ",
                "--json",
            ],
        )?;
        let payload: Value = serde_json::from_slice(&history.stderr)?;
        assert_eq!(payload["code"], "unsupported_native_option");
        assert!(
            payload.get("warnings").is_none(),
            "{:?}",
            settings["context"]
        );
    }
    // A non-object context keeps the runtime hard-refusal boundary: the parser
    // refusal carries no settings warning for a tracker the read pipeline would
    // refuse with unsupported_native_context.
    let mut settings = original.clone();
    settings["context"] = json!(5);
    fs::write(&path, serde_json::to_vec_pretty(&settings)?)?;
    for encoding in [vec![], vec!["--json"]] {
        let mut args = vec!["context", "--section", "activity", "--output-cursor", " "];
        args.extend(&encoding);
        let refused = native(root.path(), &args)?;
        assert_eq!(refused.status.code(), Some(2), "{args:?}");
        let text = String::from_utf8_lossy(&refused.stderr);
        assert!(
            !text.starts_with("[pm] warning:"),
            "non-object context must not warn: {args:?}"
        );
        if encoding.is_empty() {
            assert!(
                text.contains("does not support --output-cursor"),
                "{args:?}"
            );
        } else {
            let payload: Value = serde_json::from_slice(&refused.stderr)?;
            assert!(payload.get("warnings").is_none(), "{args:?}");
        }
    }
    // The refusal loads settings from the argument-selected tracker: an invalid
    // context warns through every workspace spelling, and a directory without a
    // tracker refuses without a warning instead of crashing.
    let mut settings = original.clone();
    settings["context"] = json!({"activity_limit": 0});
    fs::write(&path, serde_json::to_vec_pretty(&settings)?)?;
    let root_text = root.path().to_string_lossy().into_owned();
    for spelling in [
        vec!["--workspace".to_owned(), root_text.clone()],
        vec![format!("--path={root_text}")],
        vec!["--pm-path".to_owned(), root_text.clone()],
    ] {
        let mut args: Vec<String> = spelling;
        args.extend(
            ["context", "--section", "activity", "--output-cursor", " "].map(str::to_owned),
        );
        let refused = native(
            root.path(),
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )?;
        assert_eq!(refused.status.code(), Some(2), "{args:?}");
        let text = String::from_utf8_lossy(&refused.stderr);
        assert!(
            text.starts_with("[pm] warning: settings_read_invalid_schema"),
            "argument-selected tracker must warn: {args:?}"
        );
    }
    // A directory without a tracker refuses without a warning; a trailing
    // value-less --workspace keeps the refusal and falls back to the start path.
    let bare = tempfile::tempdir()?;
    let mut workspace_args: Vec<String> = ["context", "--section", "activity"]
        .map(str::to_owned)
        .to_vec();
    workspace_args.extend(["--output-cursor", " "].map(str::to_owned));
    // A trailing value-less --workspace falls back to the start path: the bare
    // directory has no tracker, so the refusal carries no settings warning.
    let valueless = workspace_args_with_flag(workspace_args.clone());
    let refused = native(
        bare.path(),
        &valueless.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    assert_eq!(refused.status.code(), Some(2));
    let text = String::from_utf8_lossy(&refused.stderr);
    assert!(
        !text.contains("[pm] warning: settings_read_invalid_schema"),
        "missing tracker must not warn"
    );
    assert!(text.contains("does not support --output-cursor"));
    // The same invocation from the bare directory with the tracker selected by
    // the workspace argument still warns: the settings belong to the
    // argument-selected tracker, never to the process working directory.
    let mut selected = vec!["--workspace".to_owned(), root_text.clone()];
    selected.extend(workspace_args[..workspace_args.len() - 1].iter().cloned());
    let refused = native(
        bare.path(),
        &selected.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    assert_eq!(refused.status.code(), Some(2), "{selected:?}");
    let text = String::from_utf8_lossy(&refused.stderr);
    assert!(
        text.starts_with("[pm] warning: settings_read_invalid_schema"),
        "argument-selected tracker must warn: {selected:?}"
    );
    // Discovery still succeeds with unreadable settings. Parser refusals retain
    // their own diagnostic instead of inventing a schema-fallback warning.
    let malformed = b"{invalid settings";
    fs::write(&path, malformed)?;
    for encoding in [vec![], vec!["--json"]] {
        let mut args = vec!["context", "--section", "activity", "--output-cursor", " "];
        args.extend(&encoding);
        let refused = native(root.path(), &args)?;
        assert_eq!(refused.status.code(), Some(2), "{args:?}");
        assert_eq!(fs::read(&path)?, malformed);
        if encoding.is_empty() {
            let text = String::from_utf8_lossy(&refused.stderr);
            assert!(
                text.contains("does not support --output-cursor"),
                "{args:?}"
            );
            assert!(!text.contains("[pm] warning:"), "{args:?}");
        } else {
            let payload: Value = serde_json::from_slice(&refused.stderr)?;
            assert_eq!(payload["code"], "unsupported_native_option", "{args:?}");
            assert!(payload.get("warnings").is_none(), "{args:?}");
        }
    }
    Ok(())
}

/// Appends one value-less --workspace flag to an argument list.
fn workspace_args_with_flag(mut args: Vec<String>) -> Vec<String> {
    args.push("--workspace".to_owned());
    args
}

/// Growing wire-budget probes retain the published custom in-progress classification.
#[test]
fn context_growing_compaction_probes_match_current_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("growing custom focus probes")
    else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    let pm = root.path().join(".agents/pm");
    let path = pm.join("settings.json");
    let mut settings: Value = serde_json::from_slice(&fs::read(&path)?)?;
    settings["schema"]["workflow"]["in_progress_status"] = json!("working");
    let statuses = json!([
        {"id":"open","roles":["active","default_open"]},
        {"id":"working","roles":["active"]},
        {"id":"closed","roles":["terminal","terminal_done","default_close"]}
    ]);
    settings["schema"]["statuses"] = statuses.clone();
    fs::write(&path, serde_json::to_vec(&settings)?)?;
    fs::write(
        pm.join("schema/statuses.json"),
        json!({"statuses":statuses}).to_string(),
    )?;
    fs::write(
        pm.join("schema/workflows.json"),
        json!({"workflow":{"in_progress_status":"working","open_status":"open"}}).to_string(),
    )?;
    let rows: Vec<_> = (0..20)
        .map(|index| {
            let mut row = item(&format!("sample-probe-{index:02}"), "Task", "working", 1);
            row["title"] = json!("Growing probe classification ".repeat(8));
            row["tags"] = json!(
                (0..9)
                    .map(|i| format!("tag-{i:02}-{}", "x".repeat(220)))
                    .collect::<Vec<_>>()
            );
            row
        })
        .collect();
    fixture(root.path(), &rows)?;
    let mut compacted = false;
    for budget in ["8000", "12000"] {
        for encoding in [vec![], vec!["--json"]] {
            let mut args = vec!["context", "--for", "orient", "--token-budget", budget];
            args.extend(&encoding);
            let expected = oracle(&driver, root.path(), &args)?;
            assert!(expected.status.success());
            compare(&expected, &native(root.path(), &args)?, &args);
            if !encoding.is_empty() {
                let output: Value = serde_json::from_slice(&expected.stdout)?;
                if output["read_output"]["rows_compacted"] == true {
                    let count = output["low_level"].as_array().map_or(0, Vec::len);
                    let derived = output["budget_derived_limit"]
                        .as_u64()
                        .ok_or("derived limit")?;
                    if count > 1 {
                        assert!((derived / 2) < count as u64 && (count as u64) < derived);
                        compacted = true;
                        assert_eq!(output["summary"]["returned_focus"]["in_progress"], count);
                        assert_eq!(output["summary"]["returned_focus"]["open"], 0);
                    }
                }
            }
        }
    }
    assert!(
        compacted,
        "must deliver multiple custom-status rows after wire compaction"
    );
    Ok(())
}

/// Invalid non-context settings cause warning-backed default activity limits before overrides.
#[test]
fn context_invalid_author_fallback_matches_current_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("whole settings fallback") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    fixture(root.path(), &[item("sample-activity", "Task", "open", 1)])?;
    let records: Vec<_> = (0..12).map(|i|json!({"ts":format!("2026-08-21T10:{i:02}:00.000Z"),"op":"update","author":"fixture","patch":[]})).collect();
    fs::write(
        root.path().join(".agents/pm/history/sample-activity.jsonl"),
        json_lines(&records),
    )?;
    let path = root.path().join(".agents/pm/settings.json");
    let original: Value = serde_json::from_slice(&fs::read(&path)?)?;
    for author in [json!(42), Value::Null, json!("fixture"), json!("")] {
        let invalid = !author.is_string();
        let mut settings = original.clone();
        settings["author_default"] = author;
        settings["context"] = json!({"activity_limit":1});
        let before = serde_json::to_vec_pretty(&settings)?;
        fs::write(&path, &before)?;
        for controls in [vec![], vec!["--activity-limit", "0"]] {
            for encoding in [vec![], vec!["--json"]] {
                let mut args = vec!["context", "--section", "activity"];
                args.extend(&controls);
                args.extend(&encoding);
                let expected = oracle(&driver, root.path(), &args)?;
                assert!(expected.status.success());
                compare(&expected, &native(root.path(), &args)?, &args);
                assert_eq!(fs::read(&path)?, before);
                if !encoding.is_empty() {
                    let output: Value = serde_json::from_slice(&expected.stdout)?;
                    let count = if controls.is_empty() {
                        if invalid { 10 } else { 1 }
                    } else {
                        0
                    };
                    assert_eq!(output["activity"].as_array().map(Vec::len), Some(count));
                    assert_eq!(
                        String::from_utf8_lossy(&expected.stderr)
                            .contains("settings_read_invalid_schema"),
                        invalid
                    );
                }
            }
        }
    }
    assert!(!root.path().join(".git").exists());
    Ok(())
}

/// Invalid unrelated sections discard all context overrides before ordinary reads.
#[test]
fn context_invalid_document_discards_overrides_matches_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("whole settings document") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    fixture(root.path(), &[item("sample-activity", "Task", "open", 1)])?;
    let records: Vec<_> = (0..12).map(|i|json!({"ts":format!("2026-08-21T10:{i:02}:00.000Z"),"op":"update","author":"fixture","patch":[]})).collect();
    fs::write(
        root.path().join(".agents/pm/history/sample-activity.jsonl"),
        json_lines(&records),
    )?;
    let path = root.path().join(".agents/pm/settings.json");
    let original: Value = serde_json::from_slice(&fs::read(&path)?)?;
    for (key, value, invalid) in [
        ("extensions", json!(42), true),
        ("extensions", json!({"enabled":[42],"disabled":[]}), true),
        ("search", json!({"max_results":"bad"}), true),
        ("providers", json!({"ollama":{"model":false}}), true),
        ("locks", json!({"ttl_seconds":1.5}), true),
        ("extensions", original["extensions"].clone(), false),
        ("future_section", json!({"unknown":null}), false),
    ] {
        let mut settings = original.clone();
        settings[key] = value;
        settings["context"] = json!({"activity_limit":1});
        let before = serde_json::to_vec_pretty(&settings)?;
        fs::write(&path, &before)?;
        for args in [
            vec!["context", "--section", "activity"],
            vec!["context", "--section", "activity", "--json"],
            vec![
                "context",
                "--section",
                "activity",
                "--activity-limit",
                "0",
                "--json",
            ],
            vec!["context", "--activity-limit", "bad", "--json"],
        ] {
            let expected = oracle(&driver, root.path(), &args)?;
            compare(&expected, &native(root.path(), &args)?, &args);
            assert_eq!(fs::read(&path)?, before);
            assert_eq!(
                String::from_utf8_lossy(&expected.stderr).contains("settings_read_invalid_schema"),
                invalid
            );
            if args.get(1) == Some(&"--section") && args.len() == 4 {
                let output: Value = serde_json::from_slice(&expected.stdout)?;
                assert_eq!(
                    output["activity"].as_array().map(Vec::len),
                    Some(if invalid { 10 } else { 1 })
                );
            }
        }
    }
    assert!(!root.path().join(".git").exists());
    Ok(())
}

/// Flag-shaped rejected values retain their argument role and JSON recovery bytes.
#[test]
fn context_refusal_value_roles_match_current_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("refusal argument roles") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let Some(driver) = orientation_driver(scratch.path(), &published)? else {
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    assert!(
        oracle(&driver, root.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    for flag in [
        "--activity-limit",
        "--limit",
        "--token-budget",
        "--max-items",
        "--token_budget",
    ] {
        for value in [
            "--path",
            "--workspace",
            "--pm-path",
            "--timestamp",
            "--path=bad",
        ] {
            for encoding in [vec![], vec!["--json"]] {
                let mut args = vec!["context", flag, value];
                args.extend(encoding);
                let expected = oracle(&driver, root.path(), &args)?;
                assert_eq!(expected.status.code(), Some(2));
                compare(&expected, &native(root.path(), &args)?, &args);
            }
        }
    }
    Ok(())
}
