//! Planning boundaries use genuine tracker files and the shared projection code.
use super::*;
use serde_json::json;

/// Fallible test return.
type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Makes a non-git tracker with the repository's real settings fixture.
fn fixture() -> Result<(tempfile::TempDir, Workspace), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join(".agents/pm");
    fs::create_dir_all(root.join("tasks/nested"))?;
    fs::create_dir_all(root.join("history"))?;
    fs::write(
        root.join("settings.json"),
        include_str!("../fixtures/list-settings.json"),
    )?;
    fs::write(root.join("history/ignored.toon"), "ignored")?;
    fs::write(root.join("root.toon"), "ignored")?;
    fs::write(root.join("tasks/ignored.txt"), "ignored")?;
    fs::write(root.join("tasks/extensionless"), "ignored")?;
    Ok((directory, Workspace::discover(&root)?))
}

/// Every strict request field and stable refusal has bounded machine guidance.
#[test]
fn strict_schema_limits_and_refusal_codes() -> TestResult {
    let (_directory, workspace) = fixture()?;
    for (max_entries, max_bytes) in [
        (0, 1),
        (1_000_001, 1),
        (1, 0),
        (1, 1_073_741_825),
        (u32::MAX, u64::MAX),
    ] {
        let refusal = workspace
            .explain_list(&QueryCostRequest {
                max_entries,
                max_bytes,
                ..QueryCostRequest::default()
            })
            .err()
            .ok_or("expected refusal")?;
        assert_eq!(refusal.code(), "query_cost_invalid_request");
        assert!(refusal.to_string().contains("1..1000000"));
    }
    for text in [
        r#"{"projection":"brief","max_entries":1,"max_bytes":1,"extra":true}"#,
        r#"{"projection":"brief","max_entries":1,"max_bytes":1,"max_bytes":2}"#,
        r#"{"projection":"brief","max_entries":1,"max_bytes":1e309}"#,
        r#"{"projection":"brief","max_entries":1,"max_bytes":-1}"#,
        r#"{"projection":"brief","max_entries":1,"max_bytes":1.5}"#,
        r#"{"projection":"search","max_entries":1,"max_bytes":1}"#,
        r#"{"projection":[],"max_entries":1,"max_bytes":1}"#,
        r#"{"projection":"brief","max_entries":4294967296,"max_bytes":1}"#,
    ] {
        assert!(crate::strict_json::from_str::<QueryCostRequest>(text).is_err());
    }
    let request: QueryCostRequest = crate::strict_json::from_str(
        r#"{"projection":"full","max_entries":1000000,"max_bytes":1073741824}"#,
    )?;
    assert_eq!(workspace.explain_list(&request)?.candidate_rows.upper, 0);
    for refusal in [
        QueryCostRefusal::InvalidRequest,
        QueryCostRefusal::ScanLimit,
        QueryCostRefusal::Source,
    ] {
        assert!(refusal.code().starts_with("query_cost_"));
        assert!(refusal.to_string().contains(';'));
    }
    Ok(())
}

/// Refusals return no partial receipt and never repair or normalize source bytes.
#[test]
fn scan_refusals_and_empty_corpus() -> TestResult {
    let (_directory, workspace) = fixture()?;
    assert_eq!(
        workspace
            .explain_list(&QueryCostRequest::default())?
            .observed_candidate_files,
        0
    );
    let file = workspace.pm_root().join("tasks/nested/test.toon");
    fs::write(&file, "body: some bytes\n")?;
    for request in [
        QueryCostRequest {
            max_entries: 1,
            ..QueryCostRequest::default()
        },
        QueryCostRequest {
            max_bytes: 1,
            ..QueryCostRequest::default()
        },
    ] {
        assert!(matches!(
            workspace.explain_list(&request),
            Err(QueryCostRefusal::ScanLimit)
        ));
    }
    fs::write(&file, [0xff])?;
    assert!(matches!(
        workspace.explain_list(&QueryCostRequest::default()),
        Err(QueryCostRefusal::Source)
    ));
    fs::remove_file(&file)?;
    let mut path = workspace.pm_root().join("tasks");
    for _ in 0..32 {
        path.push("d");
    }
    fs::create_dir_all(path)?;
    assert!(matches!(
        workspace.explain_list(&QueryCostRequest::default()),
        Err(QueryCostRefusal::ScanLimit)
    ));
    fs::remove_dir_all(workspace.pm_root())?;
    assert!(matches!(
        workspace.explain_list(&QueryCostRequest::default()),
        Err(QueryCostRefusal::Source)
    ));
    Ok(())
}

/// A 10000-item body-heavy corpus has smaller brief decoder input, not smaller I/O.
#[test]
fn ten_thousand_items_plan_less_materialization_without_mutations() -> TestResult {
    let (_directory, workspace) = fixture()?;
    let body = "source context ".repeat(100);
    for index in 0..10_000 {
        let value = json!({"id":format!("demo-{index}"),"title":"Example","type":"Task","status":"custom-active","priority":1,"created_at":"2026-10-21T00:00:00.000Z","updated_at":"2026-10-21T00:00:00.000Z","body":body});
        fs::write(
            workspace.pm_root().join(format!("tasks/demo-{index}.toon")),
            toon_format::encode_default(&value)?,
        )?;
    }
    let before = workspace.read_items()?;
    let brief = workspace.explain_list(&QueryCostRequest::default())?;
    let full = workspace.explain_list(&QueryCostRequest {
        projection: QueryProjection::Full,
        ..QueryCostRequest::default()
    })?;
    eprintln!(
        "10000 items: source={} brief_decoder={} full_decoder={}",
        brief.explain_read_bytes, brief.decoder_input_bytes.upper, full.decoder_input_bytes.upper
    );
    assert_eq!(brief.observed_candidate_files, 10_000);
    assert_eq!(brief.candidate_rows.upper, 10_000);
    assert_eq!(brief.candidate_rows.lower, 0);
    assert!(brief.decoder_input_bytes.upper < full.decoder_input_bytes.upper / 4);
    assert_eq!(brief.item_read_bytes.upper, full.item_read_bytes.upper);
    assert_eq!(brief.explain_read_bytes, full.explain_read_bytes);
    assert_eq!(before, workspace.read_items()?);
    let output = serde_json::to_value(&brief)?;
    assert_eq!(output["auxiliary_read_bytes"], serde_json::Value::Null);
    assert_eq!(output["output_bytes"], serde_json::Value::Null);
    assert_eq!(output["heap_bytes"], serde_json::Value::Null);
    assert_eq!(output["elapsed_ms"], serde_json::Value::Null);
    assert!(serde_json::to_vec(&brief)?.len() < 2000);
    Ok(())
}

/// Symlinks remain foreign and cannot add external payloads to estimates.
#[test]
fn symbolic_links_are_skipped() -> TestResult {
    let (directory, workspace) = fixture()?;
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(directory.path(), workspace.pm_root().join("tasks/link"))?;
        std::os::unix::fs::symlink("missing", workspace.pm_root().join("tasks/link.toon"))?;
    }
    #[cfg(windows)]
    {
        let target = directory.path().join("foreign");
        fs::create_dir(&target)?;
        fs::write(target.join("foreign.toon"), "must not be scanned")?;
        let output = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(workspace.pm_root().join("tasks/link"))
            .arg(&target)
            .output()?;
        assert!(output.status.success());
    }
    assert_eq!(
        workspace
            .explain_list(&QueryCostRequest::default())?
            .observed_candidate_files,
        0
    );
    Ok(())
}

/// Seeded extreme controls and payload sizes fuzz both admission and numeric output.
#[test]
fn seeded_extreme_request_output_domains() -> TestResult {
    use proptest::{
        prelude::*,
        test_runner::{Config, RngSeed, TestRunner},
    };
    let (_directory, workspace) = fixture()?;
    let strategy = (
        prop_oneof![Just(0_u32), Just(u32::MAX), 1_u32..1_000_002],
        prop_oneof![Just(0_u64), Just(u64::MAX), 1_u64..1_073_741_826],
        0_usize..256,
        any::<bool>(),
    );
    let mut runner = TestRunner::new(Config {
        cases: 500,
        rng_seed: RngSeed::Fixed(20_261_027),
        failure_persistence: None,
        ..Config::default()
    });
    runner.run(&strategy, |(max_entries, max_bytes, size, full)| {
        let source = format!(
            "id: demo-one\ntitle: {}\nbody: {}",
            "α".repeat(size),
            "context ".repeat(size)
        );
        fs::write(workspace.pm_root().join("tasks/fuzz.toon"), &source)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        let result = workspace.explain_list(&QueryCostRequest {
            max_entries,
            max_bytes,
            projection: if full {
                QueryProjection::Full
            } else {
                QueryProjection::Brief
            },
        });
        match result {
            Ok(plan) => {
                prop_assert!((1..=1_000_000).contains(&max_entries));
                prop_assert!((1..=1_073_741_824).contains(&max_bytes));
                prop_assert_eq!(plan.explain_read_bytes, source.len() as u64);
                prop_assert!(plan.decoder_input_bytes.upper <= 2 * plan.explain_read_bytes);
                prop_assert!(plan.decoder_input_bytes.upper <= 2_147_483_648);
                prop_assert!(serde_json::to_vec(&plan).is_ok());
            }
            Err(error) => prop_assert!(matches!(
                error,
                QueryCostRefusal::InvalidRequest | QueryCostRefusal::ScanLimit
            )),
        }
        Ok(())
    })?;
    Ok(())
}

/// Real filesystem failures at every fallible boundary refuse instead of certifying partial work.
#[test]
fn scanner_io_failures_never_become_partial_success() -> TestResult {
    let (_directory, workspace) = fixture()?;
    let request = QueryCostRequest::default();
    let mut scan = Scan {
        request: &request,
        entries: 0,
        files: 0,
        bytes: 0,
        projected: 0,
    };
    let missing = workspace.pm_root().join("missing");
    let failure = fs::read_dir(&missing)
        .err()
        .ok_or("missing directory must fail")?;
    let mut entries = std::iter::once(Err(failure));
    assert!(matches!(
        scan.walk_entries(&mut entries, 0),
        Err(ScanError::Source)
    ));
    let entry = fs::read_dir(workspace.pm_root())?
        .next()
        .ok_or("fixture entry")??;
    let failure = fs::metadata(&missing)
        .err()
        .ok_or("missing metadata must fail")?;
    assert!(matches!(
        scan.visit_entry(&entry, Err(failure), 0),
        Err(ScanError::Source)
    ));
    assert!(matches!(scan.inspect(&missing), Err(ScanError::Source)));
    // A real write-only handle fails Read on both Unix and Windows, modeling a
    // read fault after a successful open without mocking the tracker or SDK.
    let mut write_only = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(workspace.pm_root().join("write-only"))?;
    assert!(matches!(
        scan.inspect_reader(&mut write_only),
        Err(ScanError::Source)
    ));
    assert_eq!(scan.files, 0);
    assert_eq!(scan.bytes, 0);
    Ok(())
}
