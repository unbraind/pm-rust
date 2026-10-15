//! Byte-exact pagination and budgeting against the real published TypeScript CLI.

use pm_rust::{ItemFilter, ListOptions, Workspace};
use proptest::prelude::*;
use serde_json::Value;
use std::{
    fmt::Write as _,
    fs,
    path::Path,
    process::{Command, Output},
};

#[path = "support/published_cli.rs"]
mod published_cli;

/// Stable fixture read clock shared by both real CLIs.
const CLOCK: &str = "2026-10-02T10:00:00.000Z";
/// Fallible test result without panicking helpers.
type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Creates an isolated synthetic tracker and fixed-clock published entry driver.
fn fixture(
    count: usize,
    title_size: usize,
) -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(root.join("tasks"))?;
    fs::create_dir_all(root.join("history"))?;
    let mut settings: Value = serde_json::from_str(include_str!("fixtures/list-settings.json"))?;
    settings["telemetry"]["enabled"] = Value::Bool(false);
    fs::write(root.join("settings.json"), settings.to_string())?;
    for index in 0..count {
        let id = format!("demo-{index:04}");
        fs::write(
            root.join("tasks").join(format!("{id}.toon")),
            format!(
                "id: {id}\ntitle: \"{id}{}\"\ndescription: \"\"\ntype: Task\nstatus: open\npriority: {}\ntags: []\ncreated_at: \"2026-09-01T00:00:00.000Z\"\nupdated_at: \"2026-09-01T00:00:00.000Z\"\nbody: \"\"\n",
                "x".repeat(title_size),
                index % 5
            ),
        )?;
    }
    if let Some(published) = published_cli::published_cli_or_skip("pagination differential") {
        let package: Value =
            serde_json::from_slice(&fs::read(published.package_root.join("package.json"))?)?;
        assert_eq!(
            package["version"],
            pm_rust::COMPATIBLE_PM_VERSION,
            "pagination requires the pinned oracle"
        );
        fs::write(
            directory.path().join("driver.mjs"),
            format!(
                "import {{pathToFileURL}} from 'node:url';\nconst fixed=Date.parse({});const OriginalDate=Date;globalThis.Date=class extends OriginalDate {{constructor(...args){{args.length?super(...args):super(fixed)}}static now(){{return fixed}}}};\nprocess.argv=[process.argv[0],'pm',...process.argv.slice(2)];await import(pathToFileURL({}));\n",
                serde_json::to_string(CLOCK)?,
                serde_json::to_string(&published.entry)?
            ),
        )?;
    }
    Ok(directory)
}

/// Runs the native executable and, when available, the published executable.
fn compare(
    directory: &Path,
    flags: &[&str],
    expected_success: bool,
) -> Result<Value, Box<dyn std::error::Error>> {
    compare_controls(directory, flags, expected_success, false)
}

/// Exercises equivalent native-only control spellings against the same oracle command.
fn compare_controls(
    directory: &Path,
    flags: &[&str],
    expected_success: bool,
    equals_controls: bool,
) -> Result<Value, Box<dyn std::error::Error>> {
    let native = compare_outputs(directory, flags, expected_success, equals_controls)?;
    Ok(serde_json::from_slice(if native.stdout.is_empty() {
        &native.stderr
    } else {
        &native.stdout
    })?)
}

/// Compares bytes without asking Rust's JSON decoder to accept unpaired UTF-16 escapes.
fn compare_bytes_only(directory: &Path, flags: &[&str], expected_success: bool) -> TestResult {
    compare_outputs(directory, flags, expected_success, false)?;
    Ok(())
}

/// Runs both real CLIs and checks status and unmodified stdout/stderr before decoding.
fn compare_outputs(
    directory: &Path,
    flags: &[&str],
    expected_success: bool,
    equals_controls: bool,
) -> Result<Output, Box<dyn std::error::Error>> {
    let mut native = Command::new(env!("CARGO_BIN_EXE_pm-rust"));
    if equals_controls {
        native.args(["--workspace=.", "list", &format!("--timestamp={CLOCK}")]);
    } else {
        native.args(["--workspace", ".", "list", "--timestamp", CLOCK]);
    }
    let native = native
        .arg("--json")
        .args(flags)
        .current_dir(directory)
        .env_remove("PM_PATH")
        .output()?;
    assert_eq!(
        native.status.success(),
        expected_success,
        "native {flags:?}: {}",
        String::from_utf8_lossy(&native.stderr)
    );
    if directory.join("driver.mjs").is_file() {
        let published = Command::new(
            std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned()),
        )
        .arg(directory.join("driver.mjs"))
        .args(["list", "--json"])
        .args(flags)
        .current_dir(directory)
        .env_remove("PM_PATH")
        .output()?;
        assert_eq!(
            published.status.code(),
            native.status.code(),
            "exit status {flags:?}"
        );
        assert_bytes(&native, &published, flags);
    }
    Ok(native)
}

/// Combined ceilings bind the uncapped snapshot and accept an unchanged replay.
#[test]
fn combined_output_ceilings_replay_the_uncapped_snapshot() -> TestResult {
    let directory = fixture(75, 400)?;
    let flags = ["--output-limit", "50", "--output-budget", "1500"];
    let page = compare(directory.path(), &flags, true)?;
    assert!(
        page["count"]
            .as_u64()
            .is_some_and(|count| count > 0 && count < 50)
    );
    assert_eq!(
        page["output_budget_truncation"]["continuations"][0]["total_rows"],
        75
    );
    let cursor = page["next_cursor"]
        .as_str()
        .ok_or("missing snapshot cursor")?;
    let resumed = compare(
        directory.path(),
        &[
            "--output-limit",
            "50",
            "--output-budget",
            "1500",
            "--output-cursor",
            cursor,
        ],
        true,
    )?;
    let full = compare(directory.path(), &["--output-budget", "unbounded"], true)?;
    let retained = page["items"].as_array().ok_or("missing rows")?.len();
    assert_eq!(resumed["items"][0]["id"], full["items"][retained]["id"]);
    Ok(())
}

/// Combined ceilings retain the original producer coordinate after identity deletion.
#[test]
fn combined_ceilings_match_published_deleted_identity_fallback() -> TestResult {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    let directory = fixture(75, 400)?;
    let flags = [
        "--limit",
        "50",
        "--output-limit",
        "30",
        "--output-budget",
        "1500",
    ];
    let page = compare(directory.path(), &flags, true)?;
    let retained = page["items"]
        .as_array()
        .ok_or("missing compacted rows")?
        .len();
    assert!(retained > 0 && retained < 30);
    let cursor = page["next_cursor"]
        .as_str()
        .ok_or("missing producer cursor")?;
    let envelope: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(cursor)?)?;
    assert_eq!(envelope["after_index"], retained - 1);
    let id = envelope["after_id"]
        .as_str()
        .ok_or("missing delivered identity")?;
    fs::remove_file(
        directory
            .path()
            .join(".agents/pm/tasks")
            .join(format!("{id}.toon")),
    )?;
    let next = compare(
        directory.path(),
        &[
            "--limit",
            "50",
            "--output-budget",
            "unbounded",
            "--after",
            cursor,
        ],
        true,
    )?;
    // All rows have equal timestamps; priority then ID determines the original order.
    let mut ids = (0..75).map(|i| format!("demo-{i:04}")).collect::<Vec<_>>();
    ids.sort_by_key(|id| (id[5..].parse::<usize>().unwrap_or_default() % 5, id.clone()));
    ids.retain(|row| row != id);
    assert_eq!(next["items"][0]["id"], ids[retained - 1]);
    Ok(())
}

/// Float metadata preserves byte-identical receipts and cross-CLI snapshot replay.
#[test]
fn float_metadata_snapshots_match_bytes_and_cross_cli_replay() -> TestResult {
    let directory = fixture(75, 400)?;
    for index in 0..75 {
        let path = directory
            .path()
            .join(".agents/pm/tasks")
            .join(format!("demo-{index:04}.toon"));
        let mut content = fs::read_to_string(&path)?;
        // Keys are appended alphabetically: native preserves only sorted
        // metadata order (tracked separately); this test isolates numbers.
        content.push_str("budget: 30.0\nfraction: 30.5\nlarge_budget: 1e20\nsmall_budget: 1e-6\n");
        fs::write(path, content)?;
    }
    let flags = ["--full", "--output-budget", "1500"];
    let first = compare(directory.path(), &flags, true)?;
    assert!(
        first["items"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty())
    );
    let cursor = first["next_cursor"]
        .as_str()
        .ok_or("missing float snapshot cursor")?;
    compare(
        directory.path(),
        &[
            "--full",
            "--output-budget",
            "1500",
            "--output-cursor",
            cursor,
        ],
        true,
    )?;
    // compare() requires identical cursors, so each replay consumes both producers' bytes.
    let workspace = Workspace::discover(directory.path())?;
    let options = ListOptions {
        full: true,
        output_budget: Some("1500".to_owned()),
        output_cursor: Some(cursor.to_owned()),
        ..ListOptions::default()
    };
    workspace.list_page(&ItemFilter::default(), &options, CLOCK)?;
    Ok(())
}

/// Equals controls must disappear from published recovery without losing adjacent flags.
#[test]
fn equals_native_controls_match_published_cursor_refusals() -> TestResult {
    let directory = fixture(3, 0)?;
    for flags in [["--output-cursor", "bad"], ["--after", "invalid"]] {
        let separated = compare(directory.path(), &flags, false)?;
        let equals = compare_controls(directory.path(), &flags, false, true)?;
        assert_eq!(equals, separated);
    }
    Ok(())
}

/// Requires identical serialized bytes, including key order and every receipt field.
fn assert_bytes(native: &Output, published: &Output, flags: &[&str]) {
    for (label, left, right) in [
        ("stdout", &native.stdout, &published.stdout),
        ("stderr", &native.stderr, &published.stderr),
    ] {
        let offset = left
            .iter()
            .zip(right)
            .position(|(a, b)| a != b)
            .unwrap_or(left.len().min(right.len()));
        assert!(
            left == right,
            "wire {label} {flags:?} differs at byte {offset}: native {} oracle {}",
            String::from_utf8_lossy(
                &left[offset.saturating_sub(100)..left.len().min(offset + 400)]
            ),
            String::from_utf8_lossy(
                &right[offset.saturating_sub(100)..right.len().min(offset + 400)]
            )
        );
    }
}

/// Walks three pages with a page-size change, projection change, and final cursor replay.
#[test]
#[allow(clippy::too_many_lines)]
fn producer_pages_match_bytes_and_sdk() -> TestResult {
    let directory = fixture(13, 0)?;
    let first = compare(
        directory.path(),
        &["--limit", "5", "--output-budget", "unbounded"],
        true,
    )?;
    let workspace = Workspace::discover(directory.path())?;
    let _ = workspace.list_page(
        &ItemFilter {
            status: Some("blocked".to_owned()),
            ..ItemFilter::default()
        },
        &ListOptions::default(),
        CLOCK,
    )?;
    let options = ListOptions {
        limit: Some("5".to_owned()),
        output_budget: Some("unbounded".to_owned()),
        ..ListOptions::default()
    };
    let sdk = workspace.list_page(&ItemFilter::default(), &options, CLOCK)?;
    assert_eq!(
        format!("{}\n", serde_json::to_string_pretty(&sdk)?),
        format!("{}\n", serde_json::to_string_pretty(&first)?)
    );
    let cursor = first["next_cursor"]
        .as_str()
        .ok_or("first cursor missing")?;
    let second = compare(
        directory.path(),
        &[
            "--limit",
            "5",
            "--after",
            cursor,
            "--output-budget",
            "unbounded",
        ],
        true,
    )?;
    let cursor = second["next_cursor"]
        .as_str()
        .ok_or("second cursor missing")?;
    let last = compare(
        directory.path(),
        &[
            "--limit",
            "9",
            "--after",
            cursor,
            "--output-budget",
            "unbounded",
        ],
        true,
    )?;
    assert!(last["next_cursor"].is_null());
    assert_eq!(
        last["items"].as_array().ok_or("last rows missing")?.len(),
        3
    );
    compare(
        directory.path(),
        &[
            "--limit",
            "5",
            "--offset",
            "13",
            "--output-budget",
            "unbounded",
        ],
        true,
    )?;
    compare(
        directory.path(),
        &[
            "--limit",
            "5",
            "--ids",
            "absent",
            "--output-budget",
            "unbounded",
        ],
        true,
    )?;
    compare(
        directory.path(),
        &[
            "--limit",
            "5",
            "--after",
            cursor,
            "--full",
            "--output-budget",
            "unbounded",
        ],
        true,
    )?;
    compare(
        directory.path(),
        &[
            "--limit",
            "5",
            "--after",
            cursor,
            "--type",
            "Issue",
            "--output-budget",
            "unbounded",
        ],
        false,
    )?;
    compare(
        directory.path(),
        &["--limit", "5", "--after", "invalid"],
        false,
    )?;
    Ok(())
}

/// Exercises canonical bounds, infeasible budgets, aliases, compaction and triage.
#[test]
fn bounded_receipts_match_bytes() -> TestResult {
    let directory = fixture(75, 400)?;
    for flags in [
        vec!["--output-budget", "unbounded", "--output-limit", "3"],
        vec![
            "--output-budget",
            "unbounded",
            "--output-limit",
            "3",
            "--full",
        ],
        vec!["--limit", "0", "--output-budget", "unbounded"],
        vec![
            "--brief",
            "--all",
            "--limit",
            "3",
            "--output-budget",
            "unbounded",
        ],
        vec!["--output-budget", "1"],
        vec!["--output-budget", "1000"],
        vec!["--output-budget", "1500", "--limit", "50"],
        vec!["--output-budget", "1200", "--output-limit", "unbounded"],
        // Amount-only trimming rebases to row 29 (unbraind/pm-cli#1420).
        vec![
            "--limit",
            "50",
            "--output-limit",
            "30",
            "--output-budget",
            "unbounded",
        ],
        vec!["--full", "--output-budget", "2000"],
        vec![
            "--no-truncate",
            "--output-budget",
            "unbounded",
            "--limit",
            "3",
        ],
        vec!["--for", "triage", "--token-budget", "1000"],
        vec!["--for", "triage", "--token-budget", "256"],
        vec!["--for", "triage"],
        vec!["--for", "triage", "--brief"],
        vec![
            "--for",
            "triage",
            "--token-budget",
            "10000",
            "--output-limit",
            "3",
        ],
        vec![
            "--for",
            "triage",
            "--token-budget",
            "10000",
            "--output-limit",
            "3",
            "--output-budget",
            "unbounded",
        ],
        vec!["--for", "triage", "--token-budget", "10000", "--full"],
        vec![],
    ] {
        compare(directory.path(), &flags, flags != ["--output-budget", "1"])?;
    }
    Ok(())
}

/// Output continuations bind row content, refuse mutation, and reject other filters.
#[test]
fn snapshot_continuations_match_and_refuse_stale_rows() -> TestResult {
    let directory = fixture(75, 0)?;
    let first = compare(directory.path(), &["--output-budget", "1500"], true)?;
    let cursor = first["next_cursor"]
        .as_str()
        .ok_or("budget cursor missing")?;
    let second = compare(
        directory.path(),
        &["--output-budget", "1500", "--output-cursor", cursor],
        true,
    )?;
    assert!(
        second["items"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty())
    );
    compare(
        directory.path(),
        &[
            "--output-budget",
            "1500",
            "--output-cursor",
            cursor,
            "--type",
            "Issue",
        ],
        false,
    )?;
    let mutation = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .args([
            "update",
            "demo-0001",
            "--title",
            "concurrent edit",
            "--author",
            "fixture-writer",
            "--timestamp",
            CLOCK,
        ])
        .current_dir(directory.path())
        .env_remove("PM_PATH")
        .output()?;
    assert!(
        mutation.status.success(),
        "mutation failed: {}",
        String::from_utf8_lossy(&mutation.stderr)
    );
    compare(
        directory.path(),
        &["--output-budget", "1500", "--output-cursor", cursor],
        false,
    )?;
    compare(directory.path(), &["--output-cursor", "bad"], false)?;
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]
    /// Randomized producer page walks preserve each selected ID exactly once.
    #[test]
    fn all_pages_have_no_loss_or_duplicates(size in 0usize..100, limit in 1usize..35) {
        let directory = fixture(size,0).map_err(|e|TestCaseError::fail(e.to_string()))?;
        let workspace = Workspace::discover(directory.path()).map_err(|e|TestCaseError::fail(e.to_string()))?;
        let mut options = ListOptions {limit:Some(limit.to_string()),output_budget:Some("unbounded".to_owned()),..ListOptions::default()};
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..=size {
            let page = workspace.list_page(&ItemFilter::default(),&options,CLOCK).map_err(|e|TestCaseError::fail(e.to_string()))?;
            let rows = page["items"].as_array().ok_or_else(||TestCaseError::fail("missing items"))?;
            for row in rows {
                let id = row["id"].as_str().ok_or_else(||TestCaseError::fail("missing ID"))?;
                prop_assert!(seen.insert(id.to_owned()),"duplicate {id}");
            }
            options.after=page["next_cursor"].as_str().map(str::to_owned);
            if options.after.is_none() { break; }
            prop_assert!(!rows.is_empty());
        }
        let expected=(0..size).map(|i|format!("demo-{i:04}")).collect::<std::collections::BTreeSet<_>>();
        prop_assert_eq!(seen,expected);
    }
}

/// Every advertised amount-only producer cursor preserves the full ordered list.
#[test]
fn output_limit_producer_pages_are_complete_and_byte_identical() -> TestResult {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    for size in [90, 70] {
        let directory = fixture(size, 0)?;
        let full = compare(directory.path(), &["--output-budget", "unbounded"], true)?;
        let expected = full["items"]
            .as_array()
            .ok_or("missing full rows")?
            .iter()
            .map(|row| row["id"].clone())
            .collect::<Vec<_>>();
        for budget in ["unbounded", "100000"] {
            let mut after = None::<String>;
            let mut delivered = Vec::new();
            let mut seen = std::collections::BTreeSet::new();
            for _ in 0..size {
                let mut flags = vec![
                    "--limit",
                    "50",
                    "--output-limit",
                    "30",
                    "--output-budget",
                    budget,
                ];
                if let Some(cursor) = &after {
                    flags.extend(["--after", cursor]);
                }
                let mut page = compare(directory.path(), &flags, true)?;
                let next = page["next_cursor"].as_str().map(str::to_owned);
                // The upstream contract deliberately keeps terminal caps partial.
                // Lift only that cap at the same incoming boundary to exhaust the tail.
                if next.is_none() && page["has_more"] == true {
                    flags[3] = "unbounded";
                    let tail = compare(directory.path(), &flags, true)?;
                    let capped = page["items"].as_array().ok_or("missing capped tail")?;
                    assert_eq!(
                        &tail["items"].as_array().ok_or("missing tail")?[..capped.len()],
                        capped
                    );
                    assert_ne!(tail["has_more"], true);
                    assert!(tail["next_cursor"].is_null());
                    page = tail;
                }
                let rows = page["items"].as_array().ok_or("missing page rows")?;
                assert_ne!(rows.as_slice(), [] as [serde_json::Value; 0]);
                if page.get("count").is_some() {
                    assert_eq!(page["count"], rows.len());
                }
                for row in rows {
                    assert!(
                        seen.insert(row["id"].as_str().ok_or("missing ID")?.to_owned()),
                        "duplicate ID"
                    );
                    delivered.push(row["id"].clone());
                }
                if let Some(raw) = &next {
                    let cursor: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(raw)?)?;
                    assert_eq!(cursor["after_index"], delivered.len() - 1);
                    assert_eq!(cursor["after_id"], delivered[delivered.len() - 1]);
                } else {
                    break;
                }
                after = next;
            }
            assert_eq!(delivered, expected, "missing or reordered IDs");
            assert_eq!(seen.len(), size);
        }
    }
    Ok(())
}

/// Walking output-budget continuations retains every row without repetition.
#[test]
fn output_budget_pages_are_complete_and_byte_identical() -> TestResult {
    let directory = mixed_fixture()?;
    let mut page = compare(
        directory.path(),
        &["--all", "--brief", "--output-budget", "1500"],
        true,
    )?;
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..270 {
        for row in page["items"].as_array().ok_or("missing output page rows")? {
            assert!(seen.insert(row["id"].as_str().ok_or("missing row ID")?.to_owned()));
        }
        let Some(cursor) = page["next_cursor"].as_str() else {
            break;
        };
        page = compare(
            directory.path(),
            &[
                "--all",
                "--brief",
                "--output-budget",
                "1500",
                "--output-cursor",
                cursor,
            ],
            true,
        )?;
    }
    assert_eq!(seen, (0..270).map(|i| format!("demo-{i:04}")).collect());
    Ok(())
}

/// Triage continuation pages compact exactly like the published CLI across a
/// budget sweep. Two divergences are pinned: rows are sized with the
/// JavaScript renderer (a float such as `1e20` is 4 bytes under serde but 21
/// under `JSON.stringify`), and the intent estimate does not count the
/// continuation cursor under `filters`, which the published CLI never echoes.
#[test]
fn triage_continuation_compaction_matches_the_published_cli() -> TestResult {
    let directory = fixture(40, 120)?;
    for entry in fs::read_dir(directory.path().join(".agents/pm/tasks"))? {
        let path = entry?.path();
        let text = fs::read_to_string(&path)?;
        fs::write(&path, format!("{text}risk: 1e20\n"))?;
    }
    let first = compare(
        directory.path(),
        &[
            "--for",
            "triage",
            "--token-budget",
            "100000",
            "--limit",
            "25",
            "--output-budget",
            "unbounded",
        ],
        true,
    )?;
    let cursor = first["next_cursor"]
        .as_str()
        .ok_or("missing page cursor")?
        .to_owned();
    for budget in (400..=1600).step_by(40) {
        let budget = budget.to_string();
        compare(
            directory.path(),
            &[
                "--for",
                "triage",
                "--token-budget",
                &budget,
                "--full",
                "--after",
                &cursor,
                "--output-budget",
                "unbounded",
            ],
            true,
        )?;
    }
    Ok(())
}

/// A legacy final-page cursor without `after_index` must omit, not rebase from zero.
#[test]
fn legacy_after_without_after_index_omits_final_triage_page() -> TestResult {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    let directory = fixture(75, 400)?;
    let first = compare(
        directory.path(),
        &[
            "--for",
            "triage",
            "--token-budget",
            "100000",
            "--limit",
            "69",
            "--output-budget",
            "unbounded",
        ],
        true,
    )?;
    let raw = first["next_cursor"]
        .as_str()
        .ok_or("missing final-page cursor")?;
    let mut cursor: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(raw)?)?;
    assert!(
        cursor["after_index"]
            .as_u64()
            .is_some_and(|index| index > 0)
    );
    cursor
        .as_object_mut()
        .ok_or("cursor object")?
        .shift_remove("after_index");
    let legacy = URL_SAFE_NO_PAD.encode(cursor.to_string());
    let flags = [
        "--for",
        "triage",
        "--token-budget",
        "800",
        "--full",
        "--after",
        legacy.as_str(),
        "--output-budget",
        "unbounded",
    ];
    let native = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .args(["--workspace", ".", "list", "--timestamp", CLOCK, "--json"])
        .args(flags)
        .current_dir(directory.path())
        .env_remove("PM_PATH")
        .output()?;
    assert!(
        native.status.success(),
        "native: {}",
        String::from_utf8_lossy(&native.stderr)
    );
    let native_page: Value = serde_json::from_slice(&native.stdout)?;
    // Reverting the after_index requirement emits compacted rows and a cursor
    // whose after_index equals the retained count. The reference omits instead.
    assert!(native_page.get("items").is_none());
    assert!(native_page["next_cursor"].is_null());
    assert_eq!(native_page["budget_exceeded"]["omitted_result"], true);
    assert_eq!(
        native_page["budget_exceeded"]["reason"],
        "effective_budget_infeasible"
    );
    if directory.path().join("driver.mjs").is_file() {
        let published = Command::new(
            std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned()),
        )
        .arg(directory.path().join("driver.mjs"))
        .args(["list", "--json"])
        .args(flags)
        .current_dir(directory.path())
        .env_remove("PM_PATH")
        .output()?;
        assert_eq!(published.status.code(), native.status.code());
        let published_page: Value = serde_json::from_slice(&published.stdout)?;
        assert_eq!(
            published_page["budget_exceeded"],
            native_page["budget_exceeded"]
        );
        assert!(published_page.get("items").is_none());
        assert!(published_page["next_cursor"].is_null());
        // Fingerprint spelling is tracked separately; this test pins omission.
        assert_eq!(
            published_page["continuation_contract"]["metadata"],
            "reference"
        );
    }
    Ok(())
}

/// A projection change on the final producer page must rebase from its original position.
#[test]
fn final_intent_page_compacts_without_losing_its_continuation() -> TestResult {
    let directory = fixture(75, 400)?;
    let first = compare(
        directory.path(),
        &[
            "--for",
            "triage",
            "--token-budget",
            "100000",
            "--limit",
            "69",
            "--output-budget",
            "unbounded",
        ],
        true,
    )?;
    let cursor = first["next_cursor"]
        .as_str()
        .ok_or("missing final-page cursor")?;
    compare(
        directory.path(),
        &[
            "--for",
            "triage",
            "--token-budget",
            "1000",
            "--full",
            "--after",
            cursor,
            "--output-budget",
            "unbounded",
        ],
        true,
    )?;
    Ok(())
}

/// Direct SDK callers receive typed refusals even when clap cannot construct a request.
#[test]
fn sdk_controls_refuse_conflicts_and_missing_cursor_items() -> TestResult {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use serde_json::json;
    let directory = fixture(2, 0)?;
    let workspace = Workspace::discover(directory.path())?;
    for options in [
        ListOptions {
            full: true,
            brief: true,
            ..ListOptions::default()
        },
        ListOptions {
            after: Some("x".to_owned()),
            offset: Some("0".to_owned()),
            ..ListOptions::default()
        },
        ListOptions {
            token_budget: Some("1000".to_owned()),
            ..ListOptions::default()
        },
        ListOptions {
            intent: Some("unknown".to_owned()),
            ..ListOptions::default()
        },
        ListOptions {
            offset: Some("18446744073709551616".to_owned()),
            ..ListOptions::default()
        },
        ListOptions {
            offset: Some("9007199254740992".to_owned()),
            ..ListOptions::default()
        },
        ListOptions {
            limit: Some("bad".to_owned()),
            ..ListOptions::default()
        },
        ListOptions {
            intent: Some("triage".to_owned()),
            token_budget: Some("255".to_owned()),
            ..ListOptions::default()
        },
        ListOptions {
            intent: Some("triage".to_owned()),
            limit: Some("bad".to_owned()),
            ..ListOptions::default()
        },
    ] {
        assert!(matches!(
            workspace.list_page(&ItemFilter::default(), &options, CLOCK),
            Err(pm_rust::PmRustError::InvalidReadRequest { .. })
        ));
    }
    let mut options = ListOptions {
        limit: Some("1".to_owned()),
        output_budget: Some("unbounded".to_owned()),
        ..ListOptions::default()
    };
    let first = workspace.list_page(&ItemFilter::default(), &options, CLOCK)?;
    let raw = first["next_cursor"].as_str().ok_or("cursor missing")?;
    let mut cursor: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(raw)?)?;
    cursor["after_id"] = json!("removed");
    options.after = Some(URL_SAFE_NO_PAD.encode(cursor.to_string()));
    assert_eq!(
        workspace.list_page(&ItemFilter::default(), &options, CLOCK)?["items"]
            .as_array()
            .ok_or("rows missing")?
            .len(),
        1
    );
    cursor
        .as_object_mut()
        .ok_or("object missing")?
        .shift_remove("after_index");
    options.after = Some(URL_SAFE_NO_PAD.encode(cursor.to_string()));
    assert!(matches!(
        workspace.list_page(&ItemFilter::default(), &options, CLOCK),
        Err(pm_rust::PmRustError::ReadCursor {
            code: "invalid_query_cursor",
            ..
        })
    ));
    Ok(())
}

/// Legacy producer cursors expose an opaque contract fingerprint on every projection.
#[test]
fn legacy_cursor_fingerprint_matches_published() -> TestResult {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    for intent in [false, true] {
        let directory = fixture(75, 20)?;
        let mut flags = vec!["--limit", "69", "--output-budget", "unbounded"];
        if intent {
            flags.extend(["--for", "triage", "--token-budget", "100000"]);
        }
        let first = compare(directory.path(), &flags, true)?;
        let mut cursor: Value = serde_json::from_slice(
            &URL_SAFE_NO_PAD.decode(first["next_cursor"].as_str().ok_or("missing cursor")?)?,
        )?;
        cursor
            .as_object_mut()
            .ok_or("cursor object")?
            .shift_remove("after_index");
        let legacy = URL_SAFE_NO_PAD.encode(cursor.to_string());
        flags.extend(["--after", &legacy]);
        let page = compare(directory.path(), &flags, true)?;
        assert_eq!(
            page["continuation_contract"]["fingerprint"],
            "opaque_cursor"
        );
    }
    Ok(())
}

/// Creates 270 items spanning every built-in status, tied orders and hierarchy edges.
fn mixed_fixture() -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let directory = fixture(270, 0)?;
    let statuses = [
        "draft",
        "open",
        "in_progress",
        "blocked",
        "closed",
        "canceled",
    ];
    for index in 0..270 {
        let path = directory
            .path()
            .join(format!(".agents/pm/tasks/demo-{index:04}.toon"));
        let mut text = fs::read_to_string(&path)?;
        text = text.replace("status: open", &format!("status: {}", statuses[index % 6]));
        if index % 3 == 0 {
            text = text.replace("type: Task", "type: Issue");
        }
        text = text.replace(
            "tags: []",
            &format!(
                "tags[2]: {},shared",
                if (index / statuses.len()).is_multiple_of(2) {
                    "alpha"
                } else {
                    "beta"
                }
            ),
        );

        let extras = match index % 4 {
            0 => "parent: demo-0000\n",
            1 => "dependencies[1]{id,kind}:\n  demo-0000,parent\n",
            2 => "parent: demo-0001\n",
            _ => "",
        };
        text.push_str(extras);
        if index % 12 == 2 {
            text.push_str("blocked_by: demo-0000\n");
        }
        if index == 0 {
            text.push_str("dependencies[1]{id,kind}:\n  demo-0001,blocks\n");
        }
        if index % 3 != 0 {
            writeln!(
                text,
                "deadline: \"2026-10-{:02}T00:00:00.000Z\"",
                index % 28 + 1
            )?;
        }
        fs::write(path, text)?;
    }
    Ok(directory)
}

/// Compares complete TOON stdout, stderr and exit code, then decodes delivered rows.
fn compare_toon(directory: &Path, flags: &[&str], success: bool) -> TestResultValue {
    let native = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .args(["list", "--timestamp", CLOCK])
        .args(flags)
        .current_dir(directory)
        .env_remove("PM_PATH")
        .output()?;
    assert_eq!(
        native.status.success(),
        success,
        "{flags:?}: {}",
        String::from_utf8_lossy(&native.stderr)
    );
    if directory.join("driver.mjs").is_file() {
        let published = Command::new(
            std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned()),
        )
        .arg(directory.join("driver.mjs"))
        .arg("list")
        .args(flags)
        .current_dir(directory)
        .env_remove("PM_PATH")
        .output()?;
        assert_eq!(
            native.status.code(),
            published.status.code(),
            "{flags:?}: oracle {}",
            String::from_utf8_lossy(&published.stderr)
        );
        assert_bytes(&native, &published, flags);
    }
    if !success {
        return Ok(Value::Null);
    }
    Ok(toon_format::decode_default(std::str::from_utf8(
        &native.stdout,
    )?)?)
}

/// Decoded differential output with an owned error channel.
type TestResultValue = Result<Value, Box<dyn std::error::Error>>;

/// Mixed-status producer walks compare both wire encodings with no skipped or repeated ID.
#[test]
fn mixed_status_filtered_walks_match_both_wire_encodings() -> TestResult {
    let directory = mixed_fixture()?;
    let mut cases = vec![
        vec![],
        vec!["--all", "--brief"],
        vec!["--type", "Issue"],
        vec!["--parent", "demo-0000"],
        vec!["--tag", "ALPHA"],
        vec![
            "--status",
            "in_progress",
            "--type",
            "Task",
            "--parent",
            "demo-0000",
            "--tag",
            "beta",
        ],
    ];
    for status in [
        "draft",
        "open",
        "in_progress",
        "blocked",
        "closed",
        "canceled",
    ] {
        cases.insert(0, vec!["--status", status]);
    }
    for sort in [
        "priority",
        "deadline",
        "updated_at",
        "created_at",
        "title",
        "parent",
        "updated",
        "created",
    ] {
        for order in ["asc", "desc"] {
            cases.push(vec![
                "--all", "--brief", "--sort", sort, "--order", order, "--tag", "shared",
            ]);
        }
    }
    cases.insert(0, vec!["--tag", "missing,alpha,missing"]);
    for selection in cases {
        let mut full_flags = selection.clone();
        full_flags.extend([
            "--output-budget",
            "unbounded",
            "--output-limit",
            "unbounded",
        ]);
        let full = compare(directory.path(), &full_flags, true)?;
        let expected: Vec<_> = full["items"]
            .as_array()
            .ok_or("full rows")?
            .iter()
            .map(|row| row["id"].clone())
            .collect();
        if selection.contains(&"--parent") && selection.contains(&"--status") {
            assert!(expected.len() > 1, "combined filters must walk actual rows");
        }
        let limit = expected.len().div_ceil(2).max(1).to_string();
        for toon in [false, true] {
            let mut seen = std::collections::BTreeSet::new();
            let mut delivered = Vec::new();
            let mut after = None::<String>;
            for _ in 0..=270 {
                let mut flags = selection.clone();
                flags.extend(["--limit", &limit, "--output-budget", "unbounded"]);
                if let Some(after) = &after {
                    flags.extend(["--after", after]);
                }
                let page = if toon {
                    compare_toon(directory.path(), &flags, true)?
                } else {
                    compare(directory.path(), &flags, true)?
                };
                if let Some(rows) = page["items"].as_array() {
                    for row in rows {
                        assert!(seen.insert(row["id"].clone().to_string()), "duplicate row");
                        delivered.push(row["id"].clone());
                    }
                }
                after = page["next_cursor"].as_str().map(str::to_owned);
                if after.is_none() {
                    break;
                }
            }
            assert_eq!(delivered, expected, "{selection:?} toon={toon}");
        }
    }
    Ok(())
}

/// TOON defaults, budget receipts, infeasible budgets and continuation snapshots match bytes.
#[test]
fn toon_budget_envelopes_and_snapshot_walks_match() -> TestResult {
    let directory = mixed_fixture()?;
    for flags in [
        vec![],
        vec!["--all", "--limit", "7"],
        vec!["--status", "closed", "--limit", "3"],
        vec!["--output-budget", "1"],
        vec!["--output-limit", "3", "--output-budget", "unbounded"],
        vec!["--full", "--limit", "3", "--output-budget", "2000"],
        vec!["--for", "triage", "--token-budget", "1000"],
    ] {
        compare_toon(directory.path(), &flags, flags != ["--output-budget", "1"])?;
    }
    let verbose = fixture(270, 400)?;
    for flags in [
        vec![],
        vec!["--output-budget", "1500"],
        vec!["--for", "triage", "--token-budget", "1000"],
        vec!["--output-budget", "1500", "--output-limit", "unbounded"],
    ] {
        compare_toon(verbose.path(), &flags, true)?;
    }
    for prefix in ["x".repeat(239), format!("a,{}", "x".repeat(237))] {
        for index in 0..270 {
            let path = verbose
                .path()
                .join(format!(".agents/pm/tasks/demo-{index:04}.toon"));
            let text = fs::read_to_string(&path)?;
            let title = serde_json::to_string(&format!("{prefix}😀{}", "y".repeat(300)))?;
            let mut lines: Vec<_> = text.lines().map(str::to_owned).collect();
            lines[1] = format!("title: {title}");
            lines.retain(|line| !line.starts_with("risk:"));
            lines.push("risk: 1e21".to_owned());
            lines.retain(|line| !line.starts_with("numeric_payload"));
            lines.push(
                "numeric_payload[4]: 9007199254740993,-9007199254740993,18446744073709551615,1.5"
                    .to_owned(),
            );
            fs::write(path, format!("{}\n", lines.join("\n")))?;
        }
        compare(
            verbose.path(),
            &["--full", "--limit", "3", "--output-budget", "unbounded"],
            true,
        )?;
        compare_bytes_only(verbose.path(), &["--full", "--output-budget", "1500"], true)?;
        compare_toon(verbose.path(), &["--output-budget", "1500"], false)?;
        compare_toon(
            verbose.path(),
            &["--for", "triage", "--token-budget", "1000"],
            false,
        )?;
        compare_toon(
            verbose.path(),
            &["--full", "--limit", "3", "--output-budget", "unbounded"],
            true,
        )?;
    }
    let full = compare(
        directory.path(),
        &["--all", "--brief", "--output-budget", "unbounded"],
        true,
    )?;
    let expected: Vec<_> = full["items"]
        .as_array()
        .ok_or("rows")?
        .iter()
        .map(|row| row["id"].clone())
        .collect();
    let mut delivered = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut cursor = None::<String>;
    for _ in 0..270 {
        let mut flags = vec!["--all", "--brief", "--output-budget", "1000"];
        if let Some(cursor) = &cursor {
            flags.extend(["--output-cursor", cursor]);
        }
        let page = compare_toon(directory.path(), &flags, true)?;
        for row in page["items"].as_array().ok_or("rows")? {
            assert!(seen.insert(row["id"].to_string()));
            delivered.push(row["id"].clone());
        }
        cursor = page["next_cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(delivered, expected);
    Ok(())
}

/// Query and snapshot refusals preserve complete diagnostic bytes and exit code two.
#[test]
fn mixed_tracker_cursor_refusals_and_native_cursor_alias() -> TestResult {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    let directory = mixed_fixture()?;
    let other = mixed_fixture()?;
    let first = compare(
        directory.path(),
        &["--limit", "53", "--output-budget", "unbounded"],
        true,
    )?;
    let raw = first["next_cursor"].as_str().ok_or("cursor")?;
    let aliased = Command::new(env!("CARGO_BIN_EXE_pm-rust"))
        .args([
            "list",
            "--json",
            "--timestamp",
            CLOCK,
            "--limit",
            "53",
            "--output-budget",
            "unbounded",
            "--cursor",
            raw,
        ])
        .current_dir(directory.path())
        .env_remove("PM_PATH")
        .output()?;
    assert!(aliased.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&aliased.stdout)?,
        compare(
            directory.path(),
            &[
                "--limit",
                "53",
                "--output-budget",
                "unbounded",
                "--after",
                raw
            ],
            true
        )?
    );
    compare(other.path(), &["--after", raw], false)?;
    compare_toon(other.path(), &["--after", raw], false)?;
    for flags in [
        vec!["--tag", "alpha"],
        vec!["--parent", "demo-0000"],
        vec!["--sort", "priority"],
        vec!["--status", "closed"],
        vec!["--type", "Issue"],
    ] {
        let mut flags = flags;
        flags.extend(["--after", raw]);
        compare(directory.path(), &flags, false)?;
    }
    for raw in ["%%%", "a", "e30", "bnVsbA"] {
        compare_toon(directory.path(), &["--after", raw], false)?;
        compare(directory.path(), &["--after", raw], false)?;
    }
    let mut wrong: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(raw)?)?;
    wrong["version"] = serde_json::json!(2);
    compare(
        directory.path(),
        &["--after", &URL_SAFE_NO_PAD.encode(wrong.to_string())],
        false,
    )?;
    let first = compare(directory.path(), &["--output-budget", "1500"], true)?;
    let cursor = first["next_cursor"].as_str().ok_or("snapshot cursor")?;
    let path = directory.path().join(".agents/pm/tasks/demo-0001.toon");
    fs::write(
        &path,
        fs::read_to_string(&path)?.replace("demo-0001\"", "edited row\""),
    )?;
    let refusal = compare(
        directory.path(),
        &["--output-budget", "1500", "--output-cursor", cursor],
        false,
    )?;
    assert_eq!(refusal["code"], "read_output_cursor_stale");
    compare_toon(
        directory.path(),
        &["--output-budget", "1500", "--output-cursor", cursor],
        false,
    )?;
    // Identity cursors intentionally tolerate edits, matching the pinned oracle.
    compare(
        directory.path(),
        &["--after", raw, "--output-budget", "unbounded"],
        true,
    )?;
    Ok(())
}
