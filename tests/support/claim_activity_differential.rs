//! Claim activity and audit provenance against the locked published SDK.

use crate::{item::ItemDocument, published_cli};
use serde_json::{Value, json};
use std::{fs, path::Path};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const BEFORE: &str = "2026-08-01T10:00:00.000Z";
const CLAIMED: &str = "2026-08-02T10:00:00.000Z";
const LATER: &str = "2026-08-03T10:00:00.000Z";
const NOW: &str = "2026-08-07T10:00:00.000Z";
const HOLDER: &str = "alpha";

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
    let cases = [
        ("untouched", vec![claim()], None, false, true),
        ("recent-work", vec![claim(), substantive], None, false, true),
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
        if label == "broken-json" {
            use std::io::Write as _;
            fs::OpenOptions::new()
                .append(true)
                .open(&history)?
                .write_all(b"{broken\n")?;
        }
        let observed = fs::read(&history)?;
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
        assert_eq!(fs::read(&history)?, observed, "{label}");
        inputs.push(json!({"label":label,"root":root,"id":"demo-claim","holder":HOLDER,"updated_at":BEFORE,"threshold_hours":72,"now":NOW}));
        native.push(evidence);
    }
    verify_published(directory.path(), &inputs, &native)
}

/// A workspace recovery boundary invalidates stale evidence only when it may include the claim.
#[test]
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
        write_stream(&workspace, &[entry])?;
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

/// Native eligibility is absent when UTC crosses the timestamp library's supported range.
#[test]
fn claim_eligibility_handles_the_utc_year_boundary() -> TestResult {
    let directory = tempfile::tempdir()?;
    let mut inputs = Vec::new();
    let mut oracle_expected = Vec::new();
    for (ts, native_time, oracle_time) in [
        (
            "9999-12-28T22:30:00-01:00",
            json!("9999-12-31T23:30:00.000Z"),
            "9999-12-31T23:30:00.000Z",
        ),
        (
            "9999-12-28T23:30:00-01:00",
            Value::Null,
            "+010000-01-01T00:30:00.000Z",
        ),
        (
            "9999-12-31T23:30:00-01:00",
            Value::Null,
            "+010000-01-04T00:30:00.000Z",
        ),
    ] {
        let root = directory.path().join(oracle_expected.len().to_string());
        fs::create_dir_all(root.join("history"))?;
        let entries = [record(
            "claim",
            ts,
            patch("/metadata/assignee", json!(HOLDER)),
        )];
        let history = root.join("history/demo-claim.jsonl");
        write_stream(&history, &entries)?;
        let mut evidence = super::evidence(&history, &document(ts)?, HOLDER, NOW, 72);
        let activity = &mut evidence["claim_activity"];
        assert_eq!(activity["last_activity_at"], ts);
        assert_eq!(activity["stale_policy"]["stale"], false);
        assert_eq!(activity["stale_policy"]["eligible_at"], native_time);
        activity["stale_policy"]["eligible_at"] = json!(oracle_time);
        oracle_expected.push(evidence);
        inputs.push(json!({"label":ts,"root":root,"id":"demo-claim","holder":HOLDER,"updated_at":ts,"threshold_hours":72,"now":NOW}));
    }
    verify_published(directory.path(), &inputs, &oracle_expected)
}
