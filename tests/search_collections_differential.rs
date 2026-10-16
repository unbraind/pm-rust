//! Exact CLI, storage and history parity for native search and collections.
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
#[path = "support/mutation_differential.rs"]
mod mutation_differential;
#[path = "support/published_cli.rs"]
mod published_cli;
use mutation_differential::{copy_directory, run_minimal, write_driver};
type TestResult = Result<(), Box<dyn std::error::Error>>;
const CLOCK: &str = "2026-08-22T10:00:00.000Z";

/// Executes the fixed-clock, telemetry-isolated published command.
fn oracle(driver: &Path, root: &Path, args: &[&str]) -> Result<Output, Box<dyn std::error::Error>> {
    let mut arguments = vec![driver.to_string_lossy().into_owned()];
    arguments.extend(args.iter().map(ToString::to_string));
    run_minimal(
        &PathBuf::from(std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned())),
        &arguments,
        root,
    )
}

/// Executes the native command against the same disposable tracker.
fn native(root: &Path, args: &[&str], clock: bool) -> Result<Output, std::io::Error> {
    native_command(root, args, clock).output()
}

/// Preserves runtime and instrumentation variables while excluding application context.
fn native_command(root: &Path, args: &[&str], clock: bool) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pm-rust"));
    command.current_dir(root).env_clear().args(args);
    for key in [
        "PATH",
        "HOME",
        "SystemRoot",
        "SystemDrive",
        "windir",
        "TEMP",
        "TMP",
        "USERPROFILE",
        "COMSPEC",
        "PATHEXT",
        "NUMBER_OF_PROCESSORS",
        "PROCESSOR_ARCHITECTURE",
        "LLVM_PROFILE_FILE",
    ] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    if clock {
        command.args(["--timestamp", CLOCK]);
    }
    command
}

/// Compares all observable command bytes without normalizing fields or errors.
fn output(expected: &Output, actual: &Output, args: &[&str]) {
    assert_eq!(
        actual.status.code(),
        expected.status.code(),
        "{args:?}: native stderr: {}\noracle stderr: {}",
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

/// Compares every authoritative item and append-only history byte recursively.
fn artifacts(expected: &Path, actual: &Path) -> TestResult {
    let names =
        |path: &Path| -> Result<std::collections::BTreeSet<std::ffi::OsString>, std::io::Error> {
            fs::read_dir(path)?
                .map(|entry| entry.map(|entry| entry.file_name()))
                .filter(|name| {
                    !matches!(
                        name.as_ref().ok().and_then(|name| name.to_str()),
                        Some("cache" | "runtime" | "locks")
                    )
                })
                .collect()
        };
    assert_eq!(names(expected)?, names(actual)?);
    for entry in fs::read_dir(expected)? {
        let entry = entry?;
        let dest = actual.join(entry.file_name());
        if entry.path().is_dir() {
            if !matches!(
                entry.file_name().to_str(),
                Some("cache" | "runtime" | "locks")
            ) {
                artifacts(&entry.path(), &dest)?;
            }
        } else {
            assert_eq!(
                fs::read_to_string(entry.path())?,
                fs::read_to_string(dest)?,
                "{}",
                entry.file_name().to_string_lossy()
            );
        }
    }
    Ok(())
}

/// Builds a real tracker and verifies each collection branch with exact durable artifacts.
#[test]
#[allow(clippy::too_many_lines)]
fn collection_commands_match_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("collection differential") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let tracker = tempfile::tempdir()?;
    for args in [
        vec!["init", "sample-", "--defaults"],
        vec![
            "create",
            "--id",
            "sample-item",
            "--title",
            "Collection fixture",
            "--type",
            "Task",
            "--author",
            "alpha",
        ],
        vec![
            "files",
            "sample-item",
            "--add",
            "path=src/z.rs,note=last",
            "--add",
            "src/a.rs",
            "--author",
            "alpha",
        ],
        vec![
            "docs",
            "sample-item",
            "--add",
            "README.md",
            "--author",
            "alpha",
        ],
        vec![
            "test",
            "sample-item",
            "--add",
            "command=cargo test,path=tests/example.rs",
            "--author",
            "alpha",
        ],
    ] {
        let result = oracle(&driver, tracker.path(), &args)?;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    fs::create_dir_all(tracker.path().join("src"))?;
    fs::write(tracker.path().join("src/a.rs"), "")?;
    let snapshot = tempfile::tempdir()?;
    copy_directory(
        &tracker.path().join(".agents"),
        &snapshot.path().join(".agents"),
    )?;
    let mut mismatches = 0;
    for json_flag in [vec![], vec!["--json"]] {
        for mut args in [
            vec!["files", "sample-item"],
            vec!["docs", "sample-item"],
            vec!["test", "sample-item"],
            vec!["test", "sample-item", "--list"],
            vec![
                "files",
                "sample-item",
                "--add",
                "path=src/é.rs,scope=global,note=Unicode",
                "--add",
                "./src/../other.rs",
            ],
            vec!["files", "sample-item", "--add", "src/a.rs"],
            vec!["files", "sample-item", "--add", "9:artifact.rs"],
            vec!["files", "sample-item", "--add", "path=src/a.rs,note=new"],
            vec![
                "files",
                "sample-item",
                "--add",
                "z.rs",
                "--append-stable",
                "--note",
                " note ",
            ],
            vec![
                "docs",
                "sample-item",
                "--add",
                "path=docs/guide.md,note=Guide",
                "--add",
                "https://example.invalid/guide",
            ],
            vec!["files", "sample-item", "--remove", "src/a.rs"],
            vec![
                "files",
                "sample-item",
                "--remove",
                "path=src/a.rs",
                "--remove",
                "src/z.rs",
            ],
            vec!["docs", "sample-item", "--remove", "README.md"],
            vec!["files", "sample-item", "--remove", "missing"],
            vec![
                "files",
                "sample-item",
                "--add",
                "path=\"a,b.rs\",note=comma,continued",
            ],
            vec![
                "files",
                "sample-item",
                "--add",
                "- path: src/b.rs\n- note: Example",
            ],
            vec!["docs", "sample-item", "--add", "[Guide](docs/guide.md)"],
            vec!["test", "sample-item", "--add", "cargo test FILTER=x"],
            vec![
                "test",
                "sample-item",
                "--add",
                "cmd=cargo fmt,path=tests/fmt.rs,timeout=3,note=Formatting",
            ],
            vec![
                "test",
                "sample-item",
                "--add",
                "command=cargo fmt",
                "--add",
                "command=cargo clippy,note=Lint",
            ],
            vec![
                "test",
                "sample-item",
                "--add",
                "command=cargo test,path=tests/example.rs",
            ],
            vec!["test", "sample-item", "--remove", "cargo test"],
            vec!["test", "sample-item", "--remove", "command=cargo test"],
            vec!["test", "sample-item", "--remove", "path=tests/example.rs"],
            vec!["test", "sample-item", "--remove-index", "1"],
            vec![
                "test",
                "sample-item",
                "--add",
                "command=cargo fmt",
                "--remove-index",
                "1",
            ],
            vec![
                "test",
                "sample-item",
                "--remove-index",
                "1",
                "--remove-index",
                "1",
            ],
            vec!["test", "sample-item", "--remove", "tests/example.rs"],
            vec!["files", "sample-item", "--add", "path=src/a.rs,note=\"\""],
            vec!["files", "sample-missing"],
            vec![
                "files",
                "sample-item",
                "--add",
                r#"path="src/a,b.rs",note="say \"hello\"""#,
            ],
            vec![
                "test",
                "sample-item",
                "--add",
                r#"command="node -e \"console.log('a,b')\"""#,
            ],
            vec![
                "test",
                "sample-item",
                "--add",
                "command=cargo test,timeout=3.9,note=Fractional",
            ],
            vec!["files", "sample-item", "--remove-index", "1"],
            vec!["docs", "sample-item", "--remove-index", "1"],
            vec!["docs", "sample-missing", "--add", "README.md"],
            vec!["files", "sample-item", "--add", "scope=project"],
            vec!["files", "sample-item", "--add", "path=foo,wrong=x"],
            vec!["test", "sample-item", "--remove", "missing"],
            vec!["test", "sample-item", "--remove-index", "99"],
            vec!["test", "sample-item", "--remove-index", "0"],
            vec!["test", "sample-item", "--remove-index", "-1"],
            vec!["files", "sample-item", "--add", "path=foo,broken"],
            vec!["files", "sample-item", "--add", "a,b"],
            vec!["files", "sample-item", "--remove", "path=x,scope=project"],
            vec!["files", "sample-item", "--remove", "wrong=x"],
            vec!["files", "sample-item", "--add", "x", "--remove", "./x"],
            vec!["test", "sample-item", "--remove", "wrong=x"],
            vec!["test", "sample-item", "--add", "path=tests/a.rs"],
        ]
        .into_iter()
        .rev()
        {
            fs::remove_dir_all(tracker.path().join(".agents"))?;
            copy_directory(
                &snapshot.path().join(".agents"),
                &tracker.path().join(".agents"),
            )?;
            args.extend(["--author", "alpha"]);
            args.extend(&json_flag);
            let expected = oracle(&driver, tracker.path(), &args)?;
            let after = tempfile::tempdir()?;
            copy_directory(
                &tracker.path().join(".agents"),
                &after.path().join(".agents"),
            )?;
            fs::remove_dir_all(tracker.path().join(".agents"))?;
            copy_directory(
                &snapshot.path().join(".agents"),
                &tracker.path().join(".agents"),
            )?;
            let actual = native(tracker.path(), &args, true)?;
            let comparison = std::panic::catch_unwind(|| {
                output(&expected, &actual, &args);
                artifacts(
                    &after.path().join(".agents/pm"),
                    &tracker.path().join(".agents/pm"),
                )
            });
            if !matches!(comparison, Ok(Ok(()))) {
                mismatches += 1;
            }
        }
    }
    assert_eq!(mismatches, 0, "collection scenarios with byte differences");
    Ok(())
}

/// Compares the 12-item search corpus in TOON.
#[test]
fn search_matches_oracle_small_toon() -> TestResult {
    search_fixture(12, false)
}

/// Compares the 12-item search corpus in JSON.
#[test]
fn search_matches_oracle_small_json() -> TestResult {
    search_fixture(12, true)
}

/// Compares the 220-item search corpus in TOON.
#[test]
fn search_matches_oracle_large_toon() -> TestResult {
    search_fixture(220, false)
}

/// Compares the 220-item search corpus in JSON.
#[test]
fn search_matches_oracle_large_json() -> TestResult {
    search_fixture(220, true)
}

/// Compares one independent real corpus and encoding across every search scenario.
#[allow(clippy::too_many_lines)]
fn search_fixture(size: usize, json_output: bool) -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("search differential") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let tracker = tempfile::tempdir()?;
    assert!(
        oracle(&driver, tracker.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    for index in 0..size {
        let item = json!({"id":format!("sample-{index:03}"),"title":(["Rust search","Search rust!","Unicode café 🚀","phrase-ish query"][index%4]),"description":"search engine Rust C++","type":if index%3==0 {"Feature"} else {"Task"},"status":(["open","closed","canceled","in_progress"][index%4]),"priority":index%5,"tags":[if index%2==0 {"native"} else {"RUST"}],"created_at":CLOCK,"updated_at":format!("2026-08-{:02}T10:00:00.000Z",index%20+1),"comments":[{"text":"comment-only rust","author":"fixture","created_at":CLOCK}],"notes":[{"text":"note-only"}],"learnings":[{"text":"learning-only"}],"files":[{"path":"excluded-file-term","scope":"project"}],"reminders":[{"at":CLOCK,"text":"reminder-only"}],"events":[{"start_at":CLOCK,"end_at":"2026-08-22T11:00:00.000Z","title":"event-only","all_day":true,"timezone":"UTC","recurrence":{"freq":"weekly","interval":2,"count":4,"by_weekday":["mon","wed"]}}],"dependencies":[{"id":"external-dependency","kind":"related"}],"plan_mode":"plan-only","plan_steps":[{"id":"step-1","order":1,"title":"plan-step-only","status":"pending","created_at":CLOCK,"updated_at":CLOCK,"body":"step body"}],"body":"Body-only punctuation: C++ café. rust rust"});
        let folder = if index % 3 == 0 { "features" } else { "tasks" };
        fs::create_dir_all(tracker.path().join(".agents/pm").join(folder))?;
        fs::write(
            tracker
                .path()
                .join(".agents/pm")
                .join(folder)
                .join(format!("sample-{index:03}.toon")),
            format!("{}\n", toon_format::encode_default(&item)?),
        )?;
    }
    let json_flag = if json_output { vec!["--json"] } else { vec![] };
    for base in [
        vec!["search", "rust"],
        vec!["search", "Rust search"],
        vec!["search", "rust", "search"],
        vec!["search", "café"],
        vec!["search", "C++"],
        vec!["search", "phrase-ish"],
        vec!["search", "rust\u{0085}search"],
        vec!["search", "\"rust search\""],
        vec!["search", "missing"],
        vec!["search", "sample-001"],
        vec!["search", "001"],
        vec!["search", "body-only"],
        vec!["search", "note-only"],
        vec!["search", "reminder-only"],
        vec!["search", "all day"],
        vec!["search", "weekly"],
        vec!["search", "external-dependency"],
        vec!["search", "plan-only"],
        vec!["search", "plan-step-only"],
        vec!["search", "rust", "--tag", "absent"],
        vec!["search", "rust", "--limit", "3.9"],
        vec!["search", "excluded-file-term"],
        vec!["search", "rust", "--type", "feature"],
        vec!["search", "rust", "--status", "closed"],
        vec!["search", "rust", "--status", "all"],
        vec!["search", "rust", "--tag", "native"],
        vec![
            "search",
            "rust",
            "--status",
            "open,closed",
            "--type",
            "Task",
            "--tag",
            "RUST",
        ],
        vec!["search", "rust", "--limit", "0"],
        vec!["search", "rust", "--limit", "-0"],
        vec!["search", "rust", "--limit", "0x10"],
        vec![
            "search",
            "rust",
            "--limit",
            "0x100000000000000000000000000000000",
            "--output-budget",
            "unbounded",
        ],
        vec![
            "search",
            "rust",
            "--limit",
            "1e100",
            "--output-budget",
            "unbounded",
        ],
        vec!["search", "rust", "--limit", "3"],
        vec![
            "search",
            "rust",
            "--limit",
            "220",
            "--output-budget",
            "unbounded",
        ],
        vec!["search", "rust", "--output-limit", "2"],
        vec!["search", "rust", "--output-budget", "unbounded"],
        vec!["search", "rust", "--output-budget", "1200"],
        vec!["search", "rust", "--output-budget", "1"],
    ] {
        let large_case = (base.len() == 2
            && [
                "rust",
                "Rust search",
                "café",
                "C++",
                "phrase-ish",
                "rust\u{0085}search",
                "missing",
            ]
            .contains(&base[1]))
            || base.contains(&"--limit")
            || base.contains(&"--output-budget")
            || base.contains(&"--output-limit")
            || base.contains(&"--status") && base.contains(&"--tag");
        if size == 220 && !large_case {
            continue;
        }
        let mut args = base;
        args.extend(&json_flag);
        let expected = oracle(&driver, tracker.path(), &args)?;
        let actual = native(tracker.path(), &args, false)?;
        output(&expected, &actual, &args);
        if json_flag.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_slice(&expected.stdout)?;
        if let Some(cursor) = value["next_cursor"]
            .as_str()
            .filter(|_| args.contains(&"--limit"))
        {
            let mut continuation = args.clone();
            continuation.extend(["--after", cursor]);
            output(
                &oracle(&driver, tracker.path(), &continuation)?,
                &native(tracker.path(), &continuation, false)?,
                &continuation,
            );
        }
    }
    Ok(())
}

/// Two native or oracle writers preserve both additions and a serial canonical history.
#[test]
#[allow(clippy::too_many_lines)]
fn concurrent_collection_writers_match_serial_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("collection concurrency") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let tracker = tempfile::tempdir()?;
    for args in [
        vec!["init", "sample-", "--defaults"],
        vec![
            "create",
            "--id",
            "sample-item",
            "--title",
            "Concurrent fixture",
            "--type",
            "Task",
            "--author",
            "alpha",
        ],
    ] {
        assert!(oracle(&driver, tracker.path(), &args)?.status.success());
    }
    let settings_path = tracker.path().join(".agents/pm/settings.json");
    let mut settings: Value = serde_json::from_str(&fs::read_to_string(&settings_path)?)?;
    settings["locks"]["wait_ms"] = json!(5000);
    fs::write(settings_path, serde_json::to_string_pretty(&settings)?)?;
    let snapshot = tempfile::tempdir()?;
    copy_directory(
        &tracker.path().join(".agents"),
        &snapshot.path().join(".agents"),
    )?;
    for native_writer in [true; 8].into_iter().chain([false]) {
        fs::remove_dir_all(tracker.path().join(".agents"))?;
        copy_directory(
            &snapshot.path().join(".agents"),
            &tracker.path().join(".agents"),
        )?;
        let results = std::thread::scope(|scope| {
            let handles: Vec<_> = ["src/a.rs", "src/b.rs"]
                .into_iter()
                .map(|path| {
                    let driver = &driver;
                    let root = tracker.path();
                    scope.spawn(move || {
                        let args = [
                            "files",
                            "sample-item",
                            "--add",
                            path,
                            "--author",
                            "alpha",
                            "--json",
                        ];
                        if native_writer {
                            native(root, &args, true).map_err(|e| e.to_string())
                        } else {
                            oracle(driver, root, &args).map_err(|e| e.to_string())
                        }
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().map_err(|_| "writer panic".to_owned())?)
                .collect::<Result<Vec<_>, String>>()
        })?;
        for result in results {
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
        let history =
            fs::read_to_string(tracker.path().join(".agents/pm/history/sample-item.jsonl"))?;
        let entries: Vec<Value> = history
            .lines()
            .map(serde_json::from_str)
            .collect::<Result<_, _>>()?;
        assert_eq!(entries.len(), 3);
        assert_eq!(
            entries[1]["after_hash"],
            entries[2]["before_hash"],
            "native_writer={native_writer}; entries={}",
            serde_json::to_string_pretty(&entries)?
        );
        let first = entries[1]["patch"]
            .as_array()
            .ok_or("missing patch")?
            .iter()
            .find(|entry| entry["path"] == "/metadata/files")
            .ok_or("missing file patch")?["value"][0]["path"]
            .as_str()
            .ok_or("missing path")?;
        let second = if first == "src/a.rs" {
            "src/b.rs"
        } else {
            "src/a.rs"
        };
        let after = tempfile::tempdir()?;
        copy_directory(
            &tracker.path().join(".agents"),
            &after.path().join(".agents"),
        )?;
        fs::remove_dir_all(tracker.path().join(".agents"))?;
        copy_directory(
            &snapshot.path().join(".agents"),
            &tracker.path().join(".agents"),
        )?;
        for path in [first, second] {
            assert!(
                oracle(
                    &driver,
                    tracker.path(),
                    &["files", "sample-item", "--add", path, "--author", "alpha"]
                )?
                .status
                .success()
            );
        }
        artifacts(
            &after.path().join(".agents/pm"),
            &tracker.path().join(".agents/pm"),
        )?;
    }
    Ok(())
}

/// Usage and cursor refusals preserve typed recovery envelopes in both formats.
#[test]
fn search_refusals_match_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("search refusals") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let tracker = tempfile::tempdir()?;
    assert!(
        oracle(&driver, tracker.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    for format in [vec![], vec!["--json"]] {
        for mut args in [
            vec!["search", ""],
            vec!["search", "tag:native"],
            vec!["search", "rust", "--status", "Unknown"],
            vec!["search", "rust", "--status", "all,open"],
            vec!["search", "rust", "--type", "Unknown"],
            vec!["search", "rust", "--type", ""],
            vec!["search", "rust", "--tag", ""],
            vec!["search", "rust", "--limit", "-1"],
            vec!["search", "rust", "--limit", "NaN"],
            vec!["search", "rust", "--output-budget", "0"],
            vec!["search", "rust", "--output-budget", "-1"],
            vec!["search", "rust", "--output-budget", " unbounded "],
            vec!["search", "rust", "--output-limit", "invalid"],
            vec!["search", "rust", "--output-limit", "0"],
            vec!["search", "rust", "--after", "!"],
            vec!["search", "rust", "--output-cursor", "!"],
        ] {
            args.extend(&format);
            output(
                &oracle(&driver, tracker.path(), &args)?,
                &native(tracker.path(), &args, false)?,
                &args,
            );
        }
    }
    Ok(())
}

/// A bounded snapshot can be replayed exactly and refuses changed projected rows.
#[test]
fn search_output_cursor_replays_and_refuses_stale_rows() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("search output cursor") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let tracker = tempfile::tempdir()?;
    assert!(
        oracle(&driver, tracker.path(), &["init", "sample-", "--defaults"])?
            .status
            .success()
    );
    for index in 0..220 {
        let item = json!({"id":format!("sample-{index:03}"),"title":format!("Rust {}", "long Unicode café 🚀 ".repeat(15)),"description":"fixture","type":"Task","status":"open","priority":index%5,"tags":[],"created_at":CLOCK,"updated_at":CLOCK,"body":""});
        fs::write(
            tracker
                .path()
                .join(".agents/pm/tasks")
                .join(format!("sample-{index:03}.toon")),
            format!("{}\n", toon_format::encode_default(&item)?),
        )?;
    }
    for format in [vec!["--json"], vec![]] {
        let mut args = vec![
            "search",
            "rust",
            "--limit",
            "220",
            "--output-budget",
            "1600",
        ];
        args.extend(&format);
        let expected = oracle(&driver, tracker.path(), &args)?;
        output(&expected, &native(tracker.path(), &args, false)?, &args);
        if format.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_slice(&expected.stdout)?;
        let cursor = value["output_budget_truncation"]["continuations"][0]["cursor"]
            .as_str()
            .ok_or("missing output continuation")?;
        let mut replay = args.clone();
        replay.extend(["--output-cursor", cursor]);
        output(
            &oracle(&driver, tracker.path(), &replay)?,
            &native(tracker.path(), &replay, false)?,
            &replay,
        );
        let path = tracker.path().join(".agents/pm/tasks/sample-000.toon");
        let before = fs::read_to_string(&path)?;
        fs::write(
            &path,
            before.replace("description: fixture", "description: Changed"),
        )?;
        // Description is outside compact projection; a changed title invalidates the snapshot.
        fs::write(&path, before.replace("title: Rust", "title: Rust changed"))?;
        output(
            &oracle(&driver, tracker.path(), &replay)?,
            &native(tracker.path(), &replay, false)?,
            &replay,
        );
        fs::write(path, before)?;
    }
    Ok(())
}

/// Local and hosted branch provenance is identical in stored tests and canonical history.
#[test]
#[allow(clippy::too_many_lines)]
fn collection_provenance_matches_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("test provenance") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let tracker = tempfile::tempdir()?;
    for args in [
        vec!["init", "sample-", "--defaults"],
        vec![
            "create",
            "--id",
            "sample-item",
            "--title",
            "Provenance fixture",
            "--type",
            "Task",
            "--author",
            "alpha",
        ],
    ] {
        assert!(oracle(&driver, tracker.path(), &args)?.status.success());
    }
    assert!(
        Command::new("git")
            .current_dir(tracker.path())
            .args(["init", "--initial-branch", "fixture-branch"])
            .output()?
            .status
            .success()
    );
    let snapshot = tempfile::tempdir()?;
    copy_directory(
        &tracker.path().join(".agents"),
        &snapshot.path().join(".agents"),
    )?;
    for (head, reference) in [
        ("fixture-head", ""),
        ("", "fixture-ref"),
        ("", "merge"),
        ("", ""),
    ] {
        let args = [
            "test",
            "sample-item",
            "--add",
            "command=cargo test,note=Evidence",
            "--author",
            "alpha",
            "--json",
        ];
        let mut oracle = published_cli::oracle_command(
            std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned()),
        );
        oracle.arg(&driver).current_dir(tracker.path()).env_clear();
        for key in [
            "PATH",
            "HOME",
            "SystemRoot",
            "SystemDrive",
            "windir",
            "TEMP",
            "TMP",
            "USERPROFILE",
            "COMSPEC",
            "PATHEXT",
        ] {
            if let Some(value) = std::env::var_os(key) {
                oracle.env(key, value);
            }
        }
        oracle
            .env("FIXED_CLOCK", CLOCK)
            .env("DO_NOT_TRACK", "1")
            .env("GITHUB_HEAD_REF", head)
            .env("GITHUB_REF_NAME", reference)
            .args(args);
        let expected = oracle.output()?;
        let after = tempfile::tempdir()?;
        copy_directory(
            &tracker.path().join(".agents"),
            &after.path().join(".agents"),
        )?;
        fs::remove_dir_all(tracker.path().join(".agents"))?;
        copy_directory(
            &snapshot.path().join(".agents"),
            &tracker.path().join(".agents"),
        )?;
        let actual = native_command(tracker.path(), &args, true)
            .env("GITHUB_HEAD_REF", head)
            .env("GITHUB_REF_NAME", reference)
            .output()?;
        output(&expected, &actual, &args);
        artifacts(
            &after.path().join(".agents/pm"),
            &tracker.path().join(".agents/pm"),
        )?;
        fs::remove_dir_all(tracker.path().join(".agents"))?;
        copy_directory(
            &snapshot.path().join(".agents"),
            &tracker.path().join(".agents"),
        )?;
    }
    fs::remove_dir_all(tracker.path().join(".git"))?;
    let args = [
        "test",
        "sample-item",
        "--add",
        "command=cargo test",
        "--author",
        "alpha",
        "--json",
    ];
    let expected = oracle(&driver, tracker.path(), &args)?;
    let after = tempfile::tempdir()?;
    copy_directory(
        &tracker.path().join(".agents"),
        &after.path().join(".agents"),
    )?;
    fs::remove_dir_all(tracker.path().join(".agents"))?;
    copy_directory(
        &snapshot.path().join(".agents"),
        &tracker.path().join(".agents"),
    )?;
    let actual = native_command(tracker.path(), &args, true)
        .env(
            "PM_SOURCE_WORKSPACE_ROOT",
            tracker.path().join("unavailable-source"),
        )
        .env("PATH", scratch.path())
        .output()?;
    output(&expected, &actual, &args);
    artifacts(
        &after.path().join(".agents/pm"),
        &tracker.path().join(".agents/pm"),
    )?;
    Ok(())
}

/// Strict foreign claims, live locks and stale-lock force controls match exact refusals.
#[test]
#[allow(clippy::too_many_lines)]
fn collection_safety_refusals_match_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("collection safety") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let tracker = tempfile::tempdir()?;
    for args in [
        vec!["init", "sample-", "--defaults"],
        vec![
            "create",
            "--id",
            "sample-item",
            "--title",
            "Safety fixture",
            "--type",
            "Task",
            "--author",
            "alpha",
        ],
    ] {
        assert!(oracle(&driver, tracker.path(), &args)?.status.success());
    }
    let settings_path = tracker.path().join(".agents/pm/settings.json");
    let mut settings: Value = serde_json::from_str(&fs::read_to_string(&settings_path)?)?;
    settings["locks"]["wait_ms"] = json!(0);
    fs::write(settings_path, serde_json::to_string_pretty(&settings)?)?;
    let snapshot = tempfile::tempdir()?;
    copy_directory(
        &tracker.path().join(".agents"),
        &snapshot.path().join(".agents"),
    )?;
    for format in [vec![], vec!["--json"]] {
        for (state, force) in [
            ("claim", false),
            ("claim", true),
            ("lock", false),
            ("stale", false),
            ("stale", true),
        ] {
            fs::remove_dir_all(tracker.path().join(".agents"))?;
            copy_directory(
                &snapshot.path().join(".agents"),
                &tracker.path().join(".agents"),
            )?;
            if state == "claim" {
                assert!(
                    oracle(
                        &driver,
                        tracker.path(),
                        &["claim", "sample-item", "--author", "other"]
                    )?
                    .status
                    .success()
                );
            } else {
                fs::write(
                    tracker.path().join(".agents/pm/locks/sample-item.lock"),
                    serde_json::to_string(
                        &json!({"id":"sample-item","pid":1,"owner":"holder","created_at":if state=="stale" {"2000-01-01T00:00:00.000Z"} else {CLOCK},"ttl_seconds":1800,"token":"fixture"}),
                    )?,
                )?;
            }
            let before = tempfile::tempdir()?;
            copy_directory(
                &tracker.path().join(".agents"),
                &before.path().join(".agents"),
            )?;
            let mut args = vec![
                "files",
                "sample-item",
                "--add",
                "src/a.rs",
                "--author",
                "alpha",
            ];
            if force {
                args.push("--force");
            }
            args.extend(&format);
            let expected = oracle(&driver, tracker.path(), &args)?;
            let after = tempfile::tempdir()?;
            copy_directory(
                &tracker.path().join(".agents"),
                &after.path().join(".agents"),
            )?;
            fs::remove_dir_all(tracker.path().join(".agents"))?;
            copy_directory(
                &before.path().join(".agents"),
                &tracker.path().join(".agents"),
            )?;
            let actual = native(tracker.path(), &args, true)?;
            output(&expected, &actual, &args);
            artifacts(
                &after.path().join(".agents/pm"),
                &tracker.path().join(".agents/pm"),
            )?;
        }
    }
    Ok(())
}

/// Agent attribution, session principals and resolver outcomes participate in exact history hashes.
#[test]
#[allow(clippy::too_many_lines)]
fn collection_agent_attribution_matches_oracle() -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("collection attribution") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let tracker = tempfile::tempdir()?;
    for args in [
        vec!["init", "sample-", "--defaults"],
        vec![
            "create",
            "--id",
            "sample-item",
            "--title",
            "Attribution fixture",
            "--type",
            "Task",
            "--author",
            "alpha",
        ],
    ] {
        assert!(oracle(&driver, tracker.path(), &args)?.status.success());
    }
    let settings = tracker.path().join(".agents/pm/settings.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&settings)?)?;
    value["author_default"] = json!("");
    value["governance"]["preset"] = json!("strict");
    fs::write(settings, serde_json::to_vec(&value)?)?;
    let snapshot = tempfile::tempdir()?;
    copy_directory(
        &tracker.path().join(".agents"),
        &snapshot.path().join(".agents"),
    )?;
    for (asserted, claimed, foreign, version, explicit_role) in [
        (true, false, false, false, true),
        (true, false, false, true, true),
        (false, false, false, false, true),
        (false, true, false, false, true),
        (false, true, true, false, true),
        (true, false, false, false, false),
        (false, true, false, false, false),
    ] {
        fs::remove_dir_all(tracker.path().join(".agents"))?;
        copy_directory(
            &snapshot.path().join(".agents"),
            &tracker.path().join(".agents"),
        )?;
        let mut args = vec!["files", "sample-item", "--add", "src/a.rs", "--json"];
        if asserted {
            args.extend(["--author", "alpha"]);
        }
        let mut actual = native_command(tracker.path(), &args, true);
        actual
            .env("CODEX_THREAD_ID", "synthetic-collection-session")
            .env("PM_AGENT_MODEL", "synthetic-model")
            .env("PM_AGENT_EFFORT", "high")
            .env("PM_AGENT_ROLE", "implementer")
            .env("PM_AGENT_TOPIC", "collection-fixture");
        if version {
            actual.env("PM_AGENT_VERSION", "fixture-version");
        }
        if !explicit_role {
            actual
                .env_remove("PM_AGENT_ROLE")
                .env_remove("PM_AGENT_TOPIC");
        }
        let oracle_command = |args: &[&str], environment: &Command| {
            let mut command = published_cli::oracle_command("node");
            command
                .env_clear()
                .current_dir(tracker.path())
                .arg(&driver)
                .args(args)
                .env("FIXED_CLOCK", CLOCK)
                .env("DO_NOT_TRACK", "1");
            for (key, value) in environment.get_envs() {
                if let Some(value) = value {
                    command.env(key, value);
                }
            }
            command
        };
        if claimed {
            let receipt = oracle_command(&["claim", "sample-item", "--json"], &actual).output()?;
            assert!(
                receipt.status.success(),
                "{}",
                String::from_utf8_lossy(&receipt.stderr)
            );
        }
        if foreign {
            actual.env("CODEX_THREAD_ID", "synthetic-foreign-session");
        }
        let before = tempfile::tempdir()?;
        copy_directory(
            &tracker.path().join(".agents"),
            &before.path().join(".agents"),
        )?;
        let expected = oracle_command(&args, &actual).output()?;
        let after = tempfile::tempdir()?;
        copy_directory(
            &tracker.path().join(".agents"),
            &after.path().join(".agents"),
        )?;
        fs::remove_dir_all(tracker.path().join(".agents"))?;
        copy_directory(
            &before.path().join(".agents"),
            &tracker.path().join(".agents"),
        )?;
        output(&expected, &actual.output()?, &args);
        artifacts(
            &after.path().join(".agents/pm"),
            &tracker.path().join(".agents/pm"),
        )?;
    }
    Ok(())
}

/// Synthetic session files reproduce provider probes and their exact immutable history evidence.
#[test]
#[allow(clippy::too_many_lines)]
fn collection_local_probe_history_matches_oracle() -> TestResult {
    use std::io::{Seek as _, SeekFrom, Write as _};
    let Some(published) = published_cli::published_cli_or_skip("collection local probes") else {
        return Ok(());
    };
    let scratch = tempfile::tempdir()?;
    let driver = write_driver(scratch.path(), &published)?;
    let tracker = tempfile::tempdir()?;
    for args in [
        vec!["init", "sample-", "--defaults"],
        vec![
            "create",
            "--id",
            "sample-item",
            "--title",
            "Probe fixture",
            "--type",
            "Task",
            "--author",
            "alpha",
        ],
    ] {
        assert!(oracle(&driver, tracker.path(), &args)?.status.success());
    }
    let snapshot = tempfile::tempdir()?;
    copy_directory(
        &tracker.path().join(".agents"),
        &snapshot.path().join(".agents"),
    )?;
    for (provider, large, mode) in [
        ("codex", false, "file"),
        ("codex", true, "file"),
        ("claude-code", false, "file"),
        ("claude-code", true, "file"),
        ("codex", false, "disabled"),
        ("codex", false, "missing"),
        ("claude-code", false, "alias"),
        ("codex", false, "alias"),
        ("pi", false, "environment"),
    ] {
        fs::remove_dir_all(tracker.path().join(".agents"))?;
        copy_directory(
            &snapshot.path().join(".agents"),
            &tracker.path().join(".agents"),
        )?;
        let home = tempfile::tempdir()?;
        let codex = provider == "codex";
        let path = home.path().join(if codex {
            ".codex/sessions/year/rollout-fixture-session.jsonl"
        } else {
            ".claude/projects/project/fixture-session.jsonl"
        });
        fs::create_dir_all(path.parent().ok_or("session parent")?)?;
        if mode != "missing" {
            let mut file = fs::File::create(&path)?;
            file.write_all(if codex {b"{\"type\":\"turn_context\",\"payload\":{\"model\":\"fixture-model\",\"effort\":\"low\"}}\n"}else{b"{\"message\":{\"model\":\"fixture-model\",\"version\":\"1.2.3\"}}\n"})?;
            if large {
                file.set_len(if codex { 4_195_328 } else { 1_049_600 })?;
                file.seek(SeekFrom::End(-100))?;
                file.write_all(if codex {b"\n{\"type\":\"turn_context\",\"payload\":{\"model\":\"latest-model\",\"effort\":\"high\"}}\n"}else{b"\n{\"model\":\"tail-model\",\"version\":\"9.9.9\"}\n"})?;
            }
        }
        let args = [
            "files",
            "sample-item",
            "--add",
            "src/a.rs",
            "--author",
            "alpha",
            "--json",
        ];
        let mut actual = native_command(tracker.path(), &args, true);
        actual
            .env("HOME", home.path())
            .env("USERPROFILE", home.path())
            .env("PM_AGENT_ROLE", "implementer")
            .env("PM_AGENT_TOPIC", "probe-fixture");
        match provider {
            "codex" => {
                actual.env("CODEX_THREAD_ID", "fixture-session").env(
                    "AI_AGENT",
                    if mode == "missing" {
                        "codex"
                    } else {
                        "codex/1.2.3"
                    },
                );
            }
            "pi" => {
                actual
                    .env("PI_AGENT", "1")
                    .env("PI_SESSION_ID", "fixture-session")
                    .env("PI_MODEL", "fixture-model");
            }
            _ => {
                actual
                    .env("CLAUDE_CODE", "1")
                    .env("CLAUDE_CODE_SESSION_ID", "fixture-session");
            }
        }
        if mode == "disabled" {
            actual.env("PM_AGENT_PROBES", "off");
        }
        if mode == "alias" {
            actual.env(
                if codex {
                    "CODEX_EFFORT"
                } else {
                    "CLAUDE_MODEL"
                },
                "fixture-alias",
            );
        }
        let mut expected = published_cli::oracle_command("node");
        expected
            .env_clear()
            .current_dir(tracker.path())
            .arg(&driver)
            .args(args)
            .env("FIXED_CLOCK", CLOCK)
            .env("DO_NOT_TRACK", "1");
        for (key, value) in actual.get_envs() {
            if let Some(value) = value {
                expected.env(key, value);
            }
        }
        let expected = expected.output()?;
        let after = tempfile::tempdir()?;
        copy_directory(
            &tracker.path().join(".agents"),
            &after.path().join(".agents"),
        )?;
        fs::remove_dir_all(tracker.path().join(".agents"))?;
        copy_directory(
            &snapshot.path().join(".agents"),
            &tracker.path().join(".agents"),
        )?;
        output(&expected, &actual.output()?, &args);
        artifacts(
            &after.path().join(".agents/pm"),
            &tracker.path().join(".agents/pm"),
        )?;
    }
    Ok(())
}
