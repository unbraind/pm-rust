//! Real CLI differential coverage for the native agent orientation commands.
use serde_json::{Value, json};
use sha2::Digest;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
#[allow(dead_code)]
#[path = "support/mutation_differential.rs"]
mod mutation_differential;
#[path = "support/published_cli.rs"]
mod published_cli;
use mutation_differential::write_driver;
type TestResult = Result<(), Box<dyn std::error::Error>>;
const CLOCK: &str = "2026-08-22T10:00:00.000Z";
/// Minimal static extension discovery matches the oracle; richer policy refuses explicitly.
#[test]
fn context_static_extension_contract() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("orientation extension health")
    else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = orientation_driver(scratch.path(), &published)?;
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
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path = write_driver(root, published)?;
    let script = fs::read_to_string(&path)?;
    fs::write(
        &path,
        format!(
            "Object.defineProperty(performance, 'now', {{value:()=>0, configurable:true}});\n{script}"
        ),
    )?;
    Ok(path)
}
/// Executes one published invocation in a synthetic fixed-clock tracker.
fn oracle(driver: &Path, root: &Path, args: &[&str]) -> Result<Output, Box<dyn std::error::Error>> {
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
    assert_eq!(
        String::from_utf8_lossy(&actual.stdout),
        String::from_utf8_lossy(&expected.stdout),
        "{args:?}: stdout"
    );
    assert_eq!(
        String::from_utf8_lossy(&actual.stderr),
        String::from_utf8_lossy(&expected.stderr),
        "{args:?}: stderr"
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
/// Empty, ranked, blocked and budgeted brief context matches complete oracle bytes.
#[test]
fn context_matches_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("orientation context") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = orientation_driver(scratch.path(), &published)?;
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
    let driver = orientation_driver(scratch.path(), &published)?;
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
    let driver = orientation_driver(scratch.path(), &published)?;
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
    let driver = orientation_driver(scratch.path(), &published)?;
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
                "--for",
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
                "--after",
                "--format",
                "--fields",
                "--section",
                "--activity-limit",
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
        (
            "deps",
            vec!["--cursor", "--direction", "--kind", "--add", "--remove"],
        ),
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
    let driver = orientation_driver(scratch.path(), &published)?;
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
    let driver = orientation_driver(scratch.path(), &published)?;
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
    let driver = orientation_driver(scratch.path(), &published)?;
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
