//! Claim activity and audit provenance against the locked published SDK.

use crate::{item::ItemDocument, published_cli};
use serde_json::{Value, json};
use std::{fs, path::Path};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const BEFORE: &str = "2026-08-01T10:00:00.000Z";
const CLAIMED: &str = "2026-08-02T10:00:00.000Z";
const LATER: &str = "2026-08-03T10:00:00.000Z";
const RECENT: &str = "2026-08-06T10:00:00.000Z";
const NOW: &str = "2026-08-07T10:00:00.000Z";
const HOLDER: &str = "alpha";

/// The typed settings decoder follows the oracle's positive safe integer range.
#[test]
fn stale_threshold_rejects_invalid_numbers() -> TestResult {
    for invalid in ["0", "-1", "72.5", "9007199254740992", "\"72\"", "null"] {
        let settings = format!(r#"{{"stale_in_progress_hours":{invalid}}}"#);
        assert!(crate::strict_json::from_str::<super::MutationGuard>(&settings).is_err());
    }
    let max = crate::strict_json::from_str::<super::MutationGuard>(
        r#"{"stale_in_progress_hours":9007199254740991}"#,
    )?;
    assert_eq!(max.stale_in_progress_hours, 9_007_199_254_740_991);
    Ok(())
}

fn record(op: &str, ts: &str, patch: Value) -> Value {
    let mut record = json!({"op": op, "ts": ts});
    record["patch"] = patch;
    record
}

fn patch(path: &str, value: Value) -> Value {
    let mut operation = json!([{"op": "replace", "path": path}]);
    operation[0]["value"] = value;
    operation
}

fn claim() -> Value {
    record("claim", CLAIMED, patch("/metadata/assignee", json!(HOLDER)))
}

fn baseline(ts: &str, activity: Value) -> Value {
    let mut entry = record("history_compact_baseline", ts, json!([]));
    entry["context"]["history_compaction"]["last_activity_at"] = activity;
    entry
}

fn write_stream(path: &Path, entries: &[Value]) -> std::io::Result<()> {
    let mut contents = String::new();
    for entry in entries {
        contents.push_str(&entry.to_string());
        contents.push_str("\n\n");
    }
    fs::write(path, contents)
}

fn document(updated_at: &str) -> Result<ItemDocument, serde_json::Error> {
    serde_json::from_value(json!({
        "id": "demo-claim",
        "type": "Task",
        "title": "Assigned work",
        "status": "in_progress",
        "priority": 2,
        "created_at": BEFORE,
        "updated_at": updated_at,
        "assignee": HOLDER
    }))
}

/// Runs the actual published ownership evidence function for every fixture in one process.
fn verify_published(directory: &Path, inputs: &[Value], native: &[Value]) -> TestResult {
    let Some(published) = published_cli::published_cli_or_skip("claim activity and provenance")
    else {
        return Ok(());
    };
    let driver = directory.join("claim-activity.mjs");
    fs::write(
        &driver,
        r"
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
const { resolveItemOwnershipActivity } = await import(pathToFileURL(process.argv[2]));
const inputs = JSON.parse(readFileSync(process.argv[3], 'utf8'));
const results = [];
for (const { root, id, holder, updated_at, threshold_hours, now } of inputs) {
  const { holder: claimed_by, source: claim_source, since: claimed_at, activity: claim_activity } =
    await resolveItemOwnershipActivity(root, id, holder, { updated_at, threshold_hours, now: new Date(now) });
  results.push({ claimed_by, claim_source, claimed_at, claim_activity });
}
process.stdout.write(JSON.stringify(results));
",
    )?;
    let input = directory.join("inputs.json");
    fs::write(&input, serde_json::to_vec(inputs)?)?;
    let mut command = published_cli::oracle_command(
        std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned()),
    );
    crate::test_process_environment::isolate_environment(&mut command);
    let output = command
        .env("DO_NOT_TRACK", "1")
        .arg(driver)
        .arg(published.package_root.join("dist/sdk/ownership-source.js"))
        .arg(input)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Vec<Value> = serde_json::from_slice(&output.stdout)?;
    assert_eq!(native.len(), oracle.len());
    for ((input, actual), expected) in inputs.iter().zip(native).zip(oracle) {
        assert_eq!(*actual, expected, "{}", input["label"]);
    }
    Ok(())
}

/// End-of-day history accepts zero seconds and fractions, with a valid date and zone.
#[test]
fn end_of_day_timestamp_checks_zero_time_and_calendar() {
    let midnight = super::parse_stored_timestamp("2026-08-07T00:00:00+01:30");
    assert!(midnight.is_some());
    for value in [
        "2026-08-06t24:00+01:30",
        "2026-08-06T24:00:00+01:30",
        "2026-08-06T24:00:00.000+01:30",
        "2026-08-06T24:00+0130",
        "2026-08-06T24:00:00.000+0130",
        "2026-08-06 24:00:00+01:30",
        "2026-08-06 24:00:00.0001+01:30",
    ] {
        assert_eq!(super::parse_stored_timestamp(value), midnight, "{value}");
    }
    for invalid in [
        "bad",
        "2026-08-0éT00:00Z",
        "2026-08-06X24:00:00Z",
        "2026-08-06T24:00:00+01x0",
        "2026-08-06T10:00:00+0x",
        "2026-08-06 24:01:00Z",
        "2026-08-06T24:01:00Z",
        "2026-08-06T24:00:01Z",
        "2026-08-06T24:00.000Z",
        "2026-08-06T24:00.000",
        "2026-08-06T24:00:00.Z",
        "2026-08-06T24:00:00.0001Z",
        "2026-08-06 24:00:00.001Z",
        "2026-13-06T24:00:00Z",
        "2026-08-06T25:00:00",
    ] {
        assert!(
            super::parse_stored_timestamp(invalid).is_none(),
            "{invalid}"
        );
    }
    assert!(super::parse_end_of_day("2026-08-0éT00:00Z").is_none());
    assert!(super::parse_end_of_day("2026-08-06X24:00:00Z").is_none());
    let utc_end = super::parse_stored_timestamp("2026-08-07T00:00:00Z");
    assert!(utc_end.is_some());
    for value in ["2026-08-06T24:00Z", "2026-08-06T24:00:00.000Z"] {
        assert_eq!(super::parse_stored_timestamp(value), utc_end, "{value}");
    }
    // The published Date parser expands a bare two-digit offset to ±HH:00, so
    // `10:00:00+05` resolves to the same instant as `10:00:00+05:00`.
    let plus_five = super::parse_stored_timestamp("2026-08-06 10:00:00+05:00");
    assert!(plus_five.is_some());
    for value in [
        "2026-08-06 10:00:00+0500",
        "2026-08-06 10:00:00+05",
        "2026-08-06 10:00:00.000+05",
    ] {
        assert_eq!(super::parse_stored_timestamp(value), plus_five, "{value}");
    }
    assert_eq!(
        super::parse_stored_timestamp("2026-08-06 10:00:00-05"),
        super::parse_stored_timestamp("2026-08-06 10:00:00-05:00")
    );
}

/// Stored activity accepts the Date range and normalizes the date before the clock.
#[test]
fn stored_date_ranges_and_rollover() {
    for (spelling, canonical) in [
        ("2026-08T00:00:00Z", "2026-08-01T00:00:00Z"),
        ("2026T00:00:00Z", "2026-01-01T00:00:00Z"),
        ("2026-02-30T10:00:00+0130", "2026-03-02T08:30:00Z"),
        ("2026-02-30", "2026-03-02T00:00:00Z"),
        ("0000-01-01T00:00:00+01:00", "-000001-12-31T23:00:00Z"),
        ("+010000-02-29T24:00:00Z", "+010000-03-01T00:00:00Z"),
    ] {
        let expected = super::parse_stored_timestamp(canonical);
        assert!(expected.is_some(), "{canonical}");
        assert_eq!(
            super::parse_stored_timestamp(spelling),
            expected,
            "{spelling}"
        );
    }
    for (value, date) in [
        ("-271821-04-20T00:00:00.000Z", Some(-super::DATE_LIMIT_MS)),
        ("+275760-09-13T00:00:00.000Z", Some(super::DATE_LIMIT_MS)),
        ("-271821-04-19T23:59:59.999Z", None),
        ("+275760-09-13T00:00:00.001Z", None),
        ("-000000-01-01T00:00:00Z", None),
        ("2026-08-01T", None),
        ("2026-02-00T10:00:00Z", None),
        ("2026-02-32T10:00:00Z", None),
        ("2026-+2T10:00:00Z", None),
        ("2026-0éT10:00:00Z", None),
    ] {
        assert_eq!(super::parse_stored_timestamp(value), date, "{value}");
    }
}

/// Extended local clocks keep the historical or future offset of their original year.
#[test]
fn stored_expanded_local_clocks_keep_the_original_era() -> TestResult {
    use std::sync::OnceLock;

    static VIENNA: OnceLock<jiff::tz::TimeZone> = OnceLock::new();
    let (name, tzif) = jiff_tzdb::get("Europe/Vienna").ok_or("bundled Vienna zone")?;
    let _ = VIENNA.set(jiff::tz::TimeZone::tzif(name, tzif)?);
    let vienna = || {
        VIENNA
            .get()
            .cloned()
            .unwrap_or_else(|| unreachable!("fixture initialized"))
    };
    for (local, utc) in [
        ("-000001-01-01T00:00:00", "-000002-12-31T22:54:39Z"),
        ("0000-01-01T00:00:00", "-000001-12-31T22:54:39Z"),
        ("+010000-01-01T00:00:00", "9999-12-31T23:00:00Z"),
        ("+010000-01-01T00:00", "9999-12-31T23:00:00Z"),
        ("+010000-07-01T00:00", "+010000-06-30T22:00:00Z"),
    ] {
        let expected = super::parse_stored_timestamp(utc);
        assert!(expected.is_some(), "{utc}");
        assert_eq!(
            super::parse_stored_timestamp_in_zone(local, vienna),
            expected,
            "{local}"
        );
    }
    Ok(())
}

/// An unusable comparison clock leaves the recorded activity and eligibility readable.
#[test]
fn invalid_comparison_clock_preserves_recorded_claim_evidence() -> TestResult {
    let directory = tempfile::tempdir()?;
    let history = directory.path().join("item.jsonl");
    write_stream(&history, &[claim()])?;
    let item = document(BEFORE)?;
    let evidence = super::evidence(&history, &item, HOLDER, "invalid clock", 72);
    assert_eq!(evidence["claimed_at"], CLAIMED);
    assert_eq!(
        evidence["claim_activity"]["stale_policy"]["eligible_at"],
        "2026-08-05T10:00:00.000Z"
    );
    Ok(())
}

/// Unexpected metadata and read faults leave stored claim evidence unjudgeable.
#[test]
fn unreadable_claim_history_reports_file_io_faults() -> TestResult {
    let directory = tempfile::tempdir()?;
    let history = directory.path().join("item.jsonl");
    write_stream(&history, &[claim()])?;
    for fault in [1, 4] {
        crate::changes::IO_FAILURE.set(fault);
        let result = super::read_history(&history, |_| true);
        crate::changes::IO_FAILURE.set(0);
        assert_eq!(result, None);
    }
    Ok(())
}

/// The evidence from original, compacted, rewritten and malformed history remains distinguishable.
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "The table keeps all history provenance cases together"
)]
fn claim_history_provenance_and_activity_match_published() -> TestResult {
    let substantive = record(
        "update",
        LATER,
        patch("/metadata/title", json!("New title")),
    );
    let principal = record(
        "claim",
        LATER,
        patch("/metadata/claim_principal", json!("alpha#second-session")),
    );
    let cleared = record("release", LATER, patch("/metadata/assignee", Value::Null));
    let mut compacted_assignment = baseline(CLAIMED, json!(BEFORE));
    compacted_assignment["patch"] = claim()["patch"].clone();
    let mut matching_evidence = record("comment", LATER, json!([]));
    let patch_hash = crate::history::sha256_digest(b"[]");
    matching_evidence["reanchor_evidence"] = json!([{"patch_hash": patch_hash}]);
    let mut changed_evidence = matching_evidence.clone();
    changed_evidence["reanchor_evidence"][0]["patch_hash"] = json!("different");
    let mut malformed_evidence = matching_evidence.clone();
    malformed_evidence["reanchor_evidence"] = json!({"patch_hash": patch_hash});
    let legacy_baseline = record("history_compact_baseline", LATER, json!([]));
    let mut bad_context = legacy_baseline.clone();
    bad_context["context"] = json!([]);
    let mut bad_receipt = legacy_baseline.clone();
    bad_receipt["context"] = json!({"history_compaction": null});
    let mut maintenance = record(
        "update",
        RECENT,
        patch("/metadata/title", json!("New title")),
    );
    maintenance["event_class"] = json!("maintenance");
    let mut reclassified = record("update", RECENT, patch("/metadata/notes", json!([])));
    reclassified["event_class"] = json!("substantive");
    let cases = [
        ("untouched", vec![claim()], None, false, true),
        (
            "legacy-http-date-update",
            vec![
                claim(),
                record(
                    "update",
                    "Thu, 06 Aug 2026 10:00:00 GMT",
                    substantive["patch"].clone(),
                ),
            ],
            None,
            false,
            false,
        ),
        (
            "equivalent-iso-update",
            vec![
                claim(),
                record(
                    "update",
                    "2026-08-06T10:00:00Z",
                    substantive["patch"].clone(),
                ),
            ],
            None,
            false,
            false,
        ),
        ("bom-prefixed-claim", vec![claim()], None, false, true),
        ("bom-padded-claim", vec![claim()], None, false, true),
        (
            "non-ecmascript-whitespace",
            vec![claim()],
            None,
            true,
            false,
        ),
        (
            "abbreviated-month-activity",
            vec![
                claim(),
                record("update", "2026-08", substantive["patch"].clone()),
            ],
            None,
            false,
            true,
        ),
        (
            "abbreviated-year-claim",
            vec![record("claim", "2026", claim()["patch"].clone())],
            None,
            false,
            true,
        ),
        (
            "date-only-claim",
            vec![record("claim", "2026-08-02", claim()["patch"].clone())],
            None,
            false,
            true,
        ),
        (
            "invalid-date-only-claim",
            vec![record("claim", "2026-13-02", claim()["patch"].clone())],
            None,
            true,
            false,
        ),
        (
            "expanded-year-work",
            vec![
                claim(),
                record(
                    "update",
                    "+002026-08-06T10:00:00.000Z",
                    patch("/metadata/title", json!("New title")),
                ),
            ],
            None,
            false,
            false,
        ),
        (
            "expanded-hour-24-work",
            vec![
                claim(),
                record(
                    "update",
                    "+002026-08-06T24:00:00.000Z",
                    patch("/metadata/title", json!("New title")),
                ),
            ],
            None,
            false,
            false,
        ),
        (
            "fractional-minute-hour-24-maintenance",
            vec![
                claim(),
                record("update", "2026-08-03T24:00.000Z", json!([])),
            ],
            None,
            true,
            false,
        ),
        (
            "space-hour-24-work",
            vec![
                claim(),
                record(
                    "update",
                    "2026-08-06 24:00:00Z",
                    patch("/metadata/title", json!("New title")),
                ),
            ],
            None,
            false,
            false,
        ),
        (
            "space-hour-24-fraction-work",
            vec![
                claim(),
                record(
                    "update",
                    "2026-08-06 24:00:00.0001Z",
                    patch("/metadata/title", json!("New title")),
                ),
            ],
            None,
            false,
            false,
        ),
        (
            "iso-hour-24-fraction-work",
            vec![
                claim(),
                record(
                    "update",
                    "2026-08-06T24:00:00.0001Z",
                    patch("/metadata/title", json!("New title")),
                ),
            ],
            None,
            true,
            false,
        ),
        (
            "equivalent-midnight-work",
            vec![
                claim(),
                record(
                    "update",
                    "2026-08-07T00:00:00.000Z",
                    patch("/metadata/title", json!("New title")),
                ),
            ],
            None,
            false,
            false,
        ),
        (
            "minute-precision-work",
            vec![
                claim(),
                record(
                    "update",
                    "2026-08-07T00:00Z",
                    patch("/metadata/title", json!("New title")),
                ),
            ],
            None,
            false,
            false,
        ),
        (
            "leap-second-history",
            vec![
                claim(),
                record(
                    "update",
                    "2016-12-31T23:59:60Z",
                    patch("/metadata/title", json!("New title")),
                ),
            ],
            None,
            true,
            false,
        ),
        (
            "ordinary-second-history",
            vec![
                claim(),
                record(
                    "update",
                    "2016-12-31T23:59:59Z",
                    patch("/metadata/title", json!("New title")),
                ),
            ],
            None,
            false,
            true,
        ),
        ("recent-work", vec![claim(), substantive], None, false, true),
        (
            "stored-maintenance",
            vec![claim(), maintenance],
            None,
            false,
            true,
        ),
        (
            "stored-substantive",
            vec![claim(), reclassified],
            None,
            false,
            false,
        ),
        (
            "reclaimed",
            vec![claim(), principal.clone()],
            None,
            false,
            true,
        ),
        (
            "principal-without-assignment",
            vec![principal],
            Some("claim_record_unavailable"),
            false,
            false,
        ),
        (
            "cleared",
            vec![claim(), cleared],
            Some("claim_record_unavailable"),
            false,
            false,
        ),
        (
            "compacted-owner",
            vec![compacted_assignment],
            Some("claim_record_unavailable"),
            false,
            false,
        ),
        (
            "rewritten-owner",
            vec![record("merge_reconcile", CLAIMED, claim()["patch"].clone())],
            Some("claim_record_unavailable"),
            false,
            false,
        ),
        (
            "rewritten-tail",
            vec![claim(), record("history_redact", LATER, json!([]))],
            Some("rewritten_history"),
            false,
            false,
        ),
        (
            "discarded-tail",
            vec![claim(), record("history_salvage", LATER, json!([]))],
            Some("discarded_history_tail"),
            false,
            false,
        ),
        (
            "verified-reanchor",
            vec![claim(), matching_evidence],
            None,
            false,
            true,
        ),
        (
            "changed-reanchor",
            vec![claim(), changed_evidence],
            Some("rewritten_history"),
            false,
            false,
        ),
        (
            "malformed-reanchor",
            vec![claim(), malformed_evidence],
            Some("rewritten_history"),
            false,
            false,
        ),
        (
            "legacy-baseline",
            vec![claim(), legacy_baseline],
            None,
            false,
            true,
        ),
        (
            "empty-baseline",
            vec![claim(), baseline(LATER, Value::Null)],
            None,
            false,
            true,
        ),
        (
            "bad-baseline",
            vec![claim(), baseline(LATER, json!("unparseable"))],
            None,
            true,
            false,
        ),
        (
            "bad-baseline-context",
            vec![claim(), bad_context],
            None,
            true,
            false,
        ),
        (
            "bad-baseline-receipt",
            vec![claim(), bad_receipt],
            None,
            true,
            false,
        ),
        (
            "bad-timestamp",
            vec![claim(), record("update", "invalid", json!([]))],
            None,
            true,
            false,
        ),
        (
            "missing-timestamp",
            vec![claim(), json!({"op":"update", "patch":[]})],
            None,
            true,
            false,
        ),
        (
            "missing-patch",
            vec![claim(), json!({"op":"update", "ts":LATER})],
            None,
            true,
            false,
        ),
        ("broken-json", vec![claim()], None, true, false),
        ("history-is-directory", vec![], None, true, false),
        ("invalid-utf8", vec![], None, true, false),
        ("lossy-claim-message", vec![claim()], None, false, true),
        (
            "bad-patch-path",
            vec![claim(), record("update", LATER, json!([{"path": 0}]))],
            None,
            true,
            false,
        ),
    ];
    let directory = tempfile::tempdir()?;
    let mut inputs = Vec::new();
    let mut native = Vec::new();
    let item = document(BEFORE)?;
    for (label, entries, reason, unreadable, stale) in cases {
        let root = directory.path().join(label);
        fs::create_dir_all(root.join("history"))?;
        let history = root.join("history/demo-claim.jsonl");
        write_stream(&history, &entries)?;
        if label == "bom-padded-claim" {
            fs::write(&history, format!(" \u{feff} {} \u{feff} \n", entries[0]))?;
        }
        if label == "non-ecmascript-whitespace" {
            fs::write(&history, format!("{}\u{85}\n", entries[0]))?;
        }
        if label == "bom-prefixed-claim" {
            fs::write(
                &history,
                [b"\xef\xbb\xbf".as_slice(), &fs::read(&history)?].concat(),
            )?;
        }
        if label == "broken-json" {
            use std::io::Write as _;
            fs::OpenOptions::new()
                .append(true)
                .open(&history)?
                .write_all(b"{broken\n")?;
        }
        if label == "invalid-utf8" {
            fs::write(&history, [0xff, b'\n'])?;
        }
        if label == "history-is-directory" {
            fs::remove_file(&history)?;
            fs::create_dir(&history)?;
        }
        if matches!(label, "bom-prefixed-claim" | "lossy-claim-message") {
            let raw = fs::read_to_string(&history)?;
            let closing = raw.trim_end().strip_suffix('}').ok_or("claim record")?;
            let bytes = [
                closing.as_bytes(),
                b",\"message\":\"old ",
                &[0xff],
                b"\"}\n",
            ]
            .concat();
            fs::write(&history, bytes)?;
        }
        let observed = fs::read(&history).ok();
        let evidence = super::evidence(&history, &item, HOLDER, NOW, 72);
        assert_eq!(
            evidence["claim_activity"]["unproven_activity"],
            json!(reason),
            "{label}"
        );
        assert_eq!(
            evidence["claim_activity"]["history_unreadable"] == true,
            unreadable,
            "{label}"
        );
        assert_eq!(
            evidence["claim_activity"]["stale_policy"]["stale"], stale,
            "{label}"
        );
        if matches!(
            label,
            "expanded-hour-24-work" | "space-hour-24-work" | "equivalent-midnight-work"
        ) {
            assert_eq!(
                evidence["claim_activity"]["stale_policy"]["eligible_at"],
                "2026-08-10T00:00:00.000Z",
                "{label}"
            );
        }
        if label == "lossy-claim-message" {
            assert_eq!(evidence["claimed_at"], CLAIMED);
        }
        assert_eq!(fs::read(&history).ok(), observed, "{label}");
        inputs.push(json!({"label":label,"root":root,"id":"demo-claim","holder":HOLDER,"updated_at":BEFORE,"threshold_hours":72,"now":NOW}));
        native.push(evidence);
    }
    verify_published(directory.path(), &inputs, &native)
}

/// Compares extended dates, holder trimming and numeric patch keys to the installed SDK.
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "The table keeps the pinned history cases together"
)]
fn stored_claim_edge_cases_match_published() -> TestResult {
    let directory = tempfile::tempdir()?;
    let item = document(BEFORE)?;
    let assignee = |value| patch("/metadata/assignee", json!(value));
    let mut reanchored = record(
        "comment_add",
        LATER,
        patch("/metadata/value", json!({"10":"b","2":"a"})),
    );
    // Output of the locked SDK's hashHistoryPatch for this exact patch.
    reanchored["reanchor_evidence"] = json!([{
        "patch_hash":"86980f000989ee5fec8b5badd172abea6650e76d54351dbe72d129b6415a290c"
    }]);
    let mut changed = reanchored.clone();
    changed["patch"][0]["value"]["2"] = json!("changed");
    let cases = [
        (
            "early-expanded",
            vec![
                record("update", "-000001-01-01T00:00:00Z", json!([])),
                claim(),
            ],
            Some(CLAIMED),
            true,
        ),
        (
            "late-expanded",
            vec![record("claim", "+010000-01-01T00:00:00Z", assignee(HOLDER))],
            Some("+010000-01-01T00:00:00Z"),
            false,
        ),
        (
            "minimum-date",
            vec![record("claim", "-271821-04-20T00:00:00Z", assignee(HOLDER))],
            Some("-271821-04-20T00:00:00Z"),
            true,
        ),
        (
            "maximum-date",
            vec![record("claim", "+275760-09-13T00:00:00Z", assignee(HOLDER))],
            Some("+275760-09-13T00:00:00Z"),
            false,
        ),
        (
            "abbreviated-month-clock",
            vec![record("claim", "2026-08T00:00:00Z", assignee(HOLDER))],
            Some("2026-08T00:00:00Z"),
            true,
        ),
        (
            "bare-offset-clock",
            vec![record("claim", "2026-08-06 10:00:00+05", assignee(HOLDER))],
            Some("2026-08-06 10:00:00+05"),
            false,
        ),
        (
            "abbreviated-year-clock",
            vec![record("claim", "2026T00:00:00Z", assignee(HOLDER))],
            Some("2026T00:00:00Z"),
            true,
        ),
        (
            "rollover",
            vec![record("claim", "2026-02-30T10:00:00Z", assignee(HOLDER))],
            Some("2026-02-30T10:00:00Z"),
            true,
        ),
        (
            "outside-day-range",
            vec![claim(), record("update", "2026-02-32T10:00:00Z", json!([]))],
            None,
            false,
        ),
        ("ordinary-holder", vec![claim()], Some(CLAIMED), true),
        (
            "non-js-holder-space",
            vec![record("claim", CLAIMED, assignee("alpha\u{85}"))],
            None,
            false,
        ),
        (
            "js-holder-space",
            vec![record("claim", CLAIMED, assignee("\u{feff}alpha "))],
            Some(CLAIMED),
            true,
        ),
        (
            "matching-reanchor",
            vec![claim(), reanchored],
            Some(CLAIMED),
            true,
        ),
        (
            "changed-reanchor",
            vec![claim(), changed],
            Some(CLAIMED),
            false,
        ),
    ];
    let mut inputs = Vec::new();
    let mut native = Vec::new();
    for (label, entries, claimed, stale) in cases {
        let root = directory.path().join(label);
        fs::create_dir_all(root.join("history"))?;
        let history = root.join("history/demo-claim.jsonl");
        write_stream(&history, &entries)?;
        let evidence = super::evidence(&history, &item, HOLDER, NOW, 72);
        assert_eq!(evidence["claimed_at"], json!(claimed), "{label}");
        assert_eq!(
            evidence["claim_activity"]["stale_policy"]["stale"], stale,
            "{label}"
        );
        if label == "changed-reanchor" {
            assert_eq!(
                evidence["claim_activity"]["unproven_activity"],
                "rewritten_history"
            );
        }
        inputs.push(json!({"label":label,"root":root,"id":"demo-claim","holder":HOLDER,"updated_at":BEFORE,"threshold_hours":72,"now":NOW}));
        native.push(evidence);
    }
    verify_published(directory.path(), &inputs, &native)
}

/// History symlinks to regular files remain readable, and FIFOs fail promptly.
#[cfg(unix)]
#[test]
fn claim_history_checks_the_opened_file_type() -> TestResult {
    use std::{os::unix::fs::symlink, sync::mpsc, time::Duration};

    let directory = tempfile::tempdir()?;
    let regular = directory.path().join("regular.jsonl");
    write_stream(&regular, &[claim()])?;
    let link = directory.path().join("link.jsonl");
    symlink(&regular, &link)?;
    assert_eq!(super::read_history(&link, |_| true), Some(vec![claim()]));
    fs::remove_file(&regular)?;
    assert_eq!(super::read_history(&link, |_| true), Some(Vec::new()));
    fs::remove_file(&link)?;
    symlink("link.jsonl", &link)?;
    assert_eq!(super::read_history(&link, |_| true), None);

    let fifo = directory.path().join("stream.jsonl");
    assert!(
        std::process::Command::new("mkfifo")
            .arg("-m")
            .arg("600")
            .arg(&fifo)
            .status()?
            .success()
    );
    let (sender, receiver) = mpsc::channel();
    let reader = std::thread::spawn(move || sender.send(super::read_history(&fifo, |_| true)));
    assert_eq!(receiver.recv_timeout(Duration::from_secs(3))?, None);
    assert!(reader.join().is_ok_and(|result| result.is_ok()));
    Ok(())
}

/// Advisory activity skips malformed records and approximates malformed receipts.
#[test]
fn advisory_history_activity_matches_published() -> TestResult {
    let cases = [
        (json!({"ts": RECENT, "op": "update"}), None),
        (json!({"ts": RECENT, "op": "update", "patch": [{}]}), None),
        (json!({"ts": RECENT, "op": false, "patch": []}), None),
        (json!({"ts": null, "op": "update", "patch": []}), None),
        (
            json!({"ts": RECENT, "op": "update", "patch": []}),
            Some(RECENT),
        ),
        (baseline(RECENT, json!("invalid")), Some(RECENT)),
        (baseline(RECENT, json!(42)), Some(RECENT)),
        (baseline(RECENT, Value::Null), None),
        (baseline(RECENT, json!(LATER)), Some(LATER)),
        (
            json!({"ts": RECENT, "op": "history_compact_baseline", "patch": [], "context": {"history_compaction": {}}}),
            Some(RECENT),
        ),
        (
            json!({"ts": RECENT, "op": "history_compact_baseline", "patch": [], "context": {}}),
            Some(RECENT),
        ),
        (
            json!({"ts": RECENT, "op": "history_compact_baseline", "patch": [], "context": {"history_compaction": null}}),
            Some(RECENT),
        ),
        (
            json!({"ts": RECENT, "op": "update", "patch": [{"path": "/metadata/title"}], "event_class": "maintenance"}),
            None,
        ),
        (
            json!({"ts": RECENT, "op": "update", "patch": [{"path": "/metadata/notes"}], "event_class": "substantive"}),
            Some(RECENT),
        ),
        (
            json!({"ts": RECENT, "op": "update", "patch": [{"path": "/metadata/title"}], "event_class": "legacy"}),
            Some(RECENT),
        ),
    ];
    for (entry, expected) in &cases {
        assert_eq!(super::activity(entry), *expected, "{entry}");
    }
    let Some(published) = published_cli::published_cli_or_skip("advisory history activity") else {
        return Ok(());
    };
    let mut command = published_cli::oracle_command(
        std::env::var("PM_NODE_INTERPRETER").unwrap_or_else(|_| "node".to_owned()),
    );
    crate::test_process_environment::isolate_environment(&mut command);
    let output = command
        .env("DO_NOT_TRACK", "1")
        .args([
            "--input-type=module",
            "-e",
            r"
import { pathToFileURL } from 'node:url';
const { latestHistoryTimestamp } = await import(pathToFileURL(process.argv[1]));
const entries = JSON.parse(process.argv[2]);
process.stdout.write(JSON.stringify(entries.map(entry => latestHistoryTimestamp([entry]))));
",
        ])
        .arg(
            published
                .package_root
                .join("dist/sdk/governance/stale-work.js"),
        )
        .arg(serde_json::to_string(
            &cases.iter().map(|(entry, _)| entry).collect::<Vec<_>>(),
        )?)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Vec<Option<&str>> = serde_json::from_slice(&output.stdout)?;
    assert_eq!(oracle, cases.map(|(_, expected)| expected));
    Ok(())
}

/// A workspace recovery boundary invalidates stale evidence only when it may include the claim.
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "Keep the workspace evidence cases in one oracle run"
)]
fn workspace_restore_evidence_matches_published() -> TestResult {
    let workspaces = [
        (
            "restored",
            record("workspace_snapshot_restore", CLAIMED, json!([])),
            true,
        ),
        (
            "prior-restore",
            record("workspace_snapshot_restore", BEFORE, json!([])),
            false,
        ),
        (
            "rewritten",
            record("history_repair", LATER, json!([])),
            true,
        ),
        (
            "ordinary-history",
            record("comment", LATER, json!([])),
            false,
        ),
        (
            "undated-ordinary-history",
            record("comment", "invalid", json!([])),
            false,
        ),
        (
            "undated-restore",
            record("workspace_snapshot_restore", "invalid", json!([])),
            true,
        ),
        (
            "undated-rewrite",
            record("history_salvage", "invalid", json!([])),
            true,
        ),
        ("undated-compaction", baseline("invalid", Value::Null), true),
        (
            "invalid-retained-activity",
            baseline(BEFORE, json!("invalid")),
            true,
        ),
        ("malformed-workspace-json", json!("{"), true),
        (
            "legacy-compaction",
            record("history_compact_baseline", BEFORE, json!([])),
            true,
        ),
        ("newer-compaction", baseline(LATER, Value::Null), true),
        (
            "prior-empty-compaction",
            baseline(BEFORE, Value::Null),
            false,
        ),
        (
            "newer-retained-activity",
            baseline(BEFORE, json!(LATER)),
            true,
        ),
        (
            "prior-retained-activity",
            baseline(BEFORE, json!(BEFORE)),
            false,
        ),
        (
            "malformed-workspace",
            json!({"op":"comment", "patch":[]}),
            true,
        ),
    ];
    let directory = tempfile::tempdir()?;
    let item = document(CLAIMED)?;
    let mut inputs = Vec::new();
    let mut native = Vec::new();
    for (label, entry, restored) in workspaces {
        let root = directory.path().join(label);
        fs::create_dir_all(root.join("history"))?;
        let history = root.join("history/demo-claim.jsonl");
        write_stream(&history, &[claim()])?;
        let workspace = root.join("history/_workspace.jsonl");
        if let Some(raw) = entry.as_str() {
            fs::write(workspace, raw)?;
        } else {
            write_stream(&workspace, &[entry])?;
        }
        // The pinned oracle checks workspace recovery only once inactivity reaches the threshold.
        let soon = "2026-08-05T09:00:00.000Z";
        let pending = super::evidence(&history, &item, HOLDER, soon, 72);
        assert_eq!(
            pending["claim_activity"]["stale_policy"]["eligible_at"],
            "2026-08-05T10:00:00.000Z"
        );
        inputs.push(json!({"label":format!("{label}-pending"),"root":root,"id":"demo-claim","holder":HOLDER,"updated_at":CLAIMED,"threshold_hours":72,"now":soon}));
        native.push(pending);
        let evidence = super::evidence(&history, &item, HOLDER, NOW, 72);
        let activity = &evidence["claim_activity"];
        assert_eq!(
            activity["unproven_activity"],
            json!(restored.then_some("workspace_restored")),
            "{label}"
        );
        assert_eq!(activity["stale_policy"]["stale"], !restored, "{label}");
        assert_eq!(
            activity["stale_policy"]["eligible_at"],
            json!((!restored).then_some("2026-08-05T10:00:00.000Z")),
            "{label}"
        );
        inputs.push(json!({"label":label,"root":root,"id":"demo-claim","holder":HOLDER,"updated_at":CLAIMED,"threshold_hours":72,"now":NOW}));
        native.push(evidence);
    }
    verify_published(directory.path(), &inputs, &native)
}

/// Native eligibility matches the pinned SDK through extended ISO years and the Date limit.
#[test]
fn claim_eligibility_matches_oracle_date_range() -> TestResult {
    let directory = tempfile::tempdir()?;
    let mut inputs = Vec::new();
    let mut native = Vec::new();
    for (ts, threshold, eligible) in [
        (
            "9999-12-28T22:30:00-01:00",
            72,
            Some("9999-12-31T23:30:00.000Z"),
        ),
        (
            "9999-12-28T23:30:00-01:00",
            72,
            Some("+010000-01-01T00:30:00.000Z"),
        ),
        (
            "9999-12-31T23:30:00-01:00",
            72,
            Some("+010000-01-04T00:30:00.000Z"),
        ),
        (
            "2026-08-07T10:06:30.183Z",
            100_000_000,
            Some("+013434-07-19T02:06:30.183Z"),
        ),
        (
            "1970-01-01T00:00:00.000Z",
            2_400_000_000,
            Some("+275760-09-13T00:00:00.000Z"),
        ),
        ("1970-01-01T00:00:00.001Z", 2_400_000_000, None),
        (
            "1969-12-31T23:00:00.000Z",
            2_400_000_001,
            Some("+275760-09-13T00:00:00.000Z"),
        ),
        (
            "0000-01-01T00:00:00.000+02:00",
            1,
            Some("-000001-12-31T23:00:00.000Z"),
        ),
        (
            "1969-12-31T23:59:59.9999Z",
            1,
            Some("1970-01-01T00:59:59.999Z"),
        ),
        // This duration fits in i64 milliseconds, but adding the activity instant overflows.
        ("2026-08-07T10:06:30.183Z", 2_562_047_788_015, None),
        ("2026-08-07T10:06:30.183Z", 9_007_199_254_740_991, None),
    ] {
        let root = directory.path().join(native.len().to_string());
        fs::create_dir_all(root.join("history"))?;
        let entries = [record(
            "claim",
            ts,
            patch("/metadata/assignee", json!(HOLDER)),
        )];
        let history = root.join("history/demo-claim.jsonl");
        write_stream(&history, &entries)?;
        let evidence = super::evidence(&history, &document(ts)?, HOLDER, ts, threshold);
        let activity = &evidence["claim_activity"];
        assert_eq!(activity["last_activity_at"], ts);
        assert_eq!(activity["stale_policy"]["stale"], false);
        assert_eq!(
            activity["stale_policy"]["eligible_at"],
            json!(eligible),
            "{ts}, {threshold} hours"
        );
        native.push(evidence);
        inputs.push(json!({"label":format!("{ts}, {threshold} hours"),"root":root,"id":"demo-claim","holder":HOLDER,"updated_at":ts,"threshold_hours":threshold,"now":ts}));
    }
    verify_published(directory.path(), &inputs, &native)
}

/// The published Date clock considers fractional-millisecond activity stale at the threshold.
#[test]
fn claim_activity_matches_the_millisecond_threshold() -> TestResult {
    let root = tempfile::tempdir()?;
    let ts = "2026-08-02T10:00:00.0009Z";
    let now = "2026-08-05T10:00:00.000Z";
    let mut claim = claim();
    claim["ts"] = json!(ts);
    fs::create_dir(root.path().join("history"))?;
    let history = root.path().join("history/demo-claim.jsonl");
    write_stream(&history, &[claim])?;
    let evidence = super::evidence(&history, &document(BEFORE)?, HOLDER, now, 72);
    let activity = &evidence["claim_activity"];
    assert_eq!(evidence["claimed_at"], ts);
    assert_eq!(activity["last_activity_at"], ts);
    assert_eq!(activity["claim_age_hours"], 72);
    assert_eq!(activity["idle_hours"], 72);
    assert_eq!(activity["stale_policy"]["stale"], true);
    verify_published(
        root.path(),
        &[
            json!({"label":"fractional threshold","root":root.path(),"id":"demo-claim",
            "holder":HOLDER,"updated_at":BEFORE,"threshold_hours":72,"now":now}),
        ],
        &[evidence],
    )
}
