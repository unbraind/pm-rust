//! Read-only ownership evidence used in claim conflict receipts.

use std::path::Path;

use serde_json::{Value, json};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

use crate::item::ItemDocument;

/// Settings for the published stale-work threshold.
#[derive(Debug, serde::Deserialize)]
pub(super) struct MutationGuard {
    /// Minimum whole hours of inactivity before a claim is stale.
    #[serde(default = "default_threshold")]
    pub(super) stale_in_progress_hours: i64,
}

impl Default for MutationGuard {
    fn default() -> Self {
        Self {
            stale_in_progress_hours: default_threshold(),
        }
    }
}

/// Default inactivity threshold for a tracker that has no override.
const fn default_threshold() -> i64 {
    72
}

/// Parses a stored timestamp for activity comparisons.
fn parse(value: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(value, &Rfc3339).ok()
}

/// Reads an entire stream, treating missing streams as empty and broken streams as unjudgeable.
fn read_history(path: &Path) -> Option<Vec<Value>> {
    super::read_optional(path)
        .ok()?
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let entry: Value = crate::strict_json::from_str(line).ok()?;
            parse(entry["ts"].as_str()?)?;
            entry["op"].as_str()?;
            if !entry["patch"]
                .as_array()?
                .iter()
                .all(|row| row["path"].is_string())
            {
                return None;
            }
            if entry["op"] == "history_compact_baseline"
                && let Some(value) = entry["context"]["history_compaction"].get("last_activity_at")
                && !value.is_null()
                && value.as_str().and_then(parse).is_none()
            {
                return None;
            }
            Some(entry)
        })
        .collect()
}

/// Finds the assignment or principal change that created the current ownership.
fn ownership_index(history: &[Value], assignee: &str) -> Option<usize> {
    let index = history.iter().rposition(|entry| {
        patches(entry).any(|row| {
            row["path"] == "/metadata/assignee"
                || (entry["op"] == "claim" && row["path"] == "/metadata/claim_principal")
        })
    })?;
    let previous = history[..=index]
        .iter()
        .rev()
        .find_map(|entry| patches(entry).find(|row| row["path"] == "/metadata/assignee"))?;
    (previous["value"].as_str()?.trim() == assignee.trim()).then_some(index)
}

/// Iterates over a validated record's patch entries.
fn patches(entry: &Value) -> impl Iterator<Item = &Value> {
    entry["patch"].as_array().into_iter().flatten()
}

/// Reads substantive activity, using the retained time for compacted streams.
pub(crate) fn activity(entry: &Value) -> Option<&str> {
    let op = entry["op"].as_str()?;
    if op == "history_compact_baseline" {
        return entry["context"]["history_compaction"]
            .get("last_activity_at")
            .unwrap_or(&entry["ts"])
            .as_str();
    }
    let paths = patches(entry).filter_map(|row| row["path"].as_str());
    (crate::history::classify_history_paths(op, paths) == "substantive")
        .then(|| entry["ts"].as_str())
        .flatten()
}

/// Determines whether audited or discarded history prevents a stale verdict.
fn unproven(history: &[Value], index: Option<usize>) -> Option<&'static str> {
    let Some(index) = index else {
        return Some("claim_record_unavailable");
    };
    let anchor = &history[index];
    if anchor["op"] == "history_compact_baseline" || rewritten_operation(&anchor["op"]) {
        return Some("claim_record_unavailable");
    }
    for entry in &history[index..] {
        if rewritten_operation(&entry["op"]) {
            return Some(if entry["op"] == "history_salvage" {
                "discarded_history_tail"
            } else {
                "rewritten_history"
            });
        }
        if let Some(evidence) = entry.get("reanchor_evidence") {
            let mut canonical = String::new();
            crate::history::stable_json(&entry["patch"], &mut canonical);
            let hash = crate::history::sha256_digest(canonical.as_bytes());
            if evidence
                .as_array()
                .is_none_or(|rows| rows.iter().any(|row| row["patch_hash"] != hash))
            {
                return Some("rewritten_history");
            }
        }
    }
    None
}

/// Identifies operations that rewrite or discard recorded history.
fn rewritten_operation(op: &Value) -> bool {
    matches!(
        op.as_str(),
        Some("history_repair" | "history_redact" | "merge_reconcile" | "history_salvage")
    )
}

/// Finds workspace history changes at or after the claim timestamp.
fn workspace_restored(history_path: &Path, claimed_at: OffsetDateTime) -> bool {
    read_history(&history_path.with_file_name("_workspace.jsonl")).is_none_or(|history| {
        history.iter().any(|entry| {
            let at_or_after = |value: &Value| {
                value
                    .as_str()
                    .and_then(parse)
                    .is_none_or(|at| at >= claimed_at)
            };
            if entry["op"] == "workspace_snapshot_restore" || rewritten_operation(&entry["op"]) {
                return at_or_after(&entry["ts"]);
            }
            if entry["op"] == "history_compact_baseline" {
                return entry["context"]["history_compaction"]
                    .get("last_activity_at")
                    .is_none_or(|at| {
                        at_or_after(&entry["ts"]) || (!at.is_null() && at_or_after(at))
                    });
            }
            false
        })
    })
}

/// Builds claim provenance and activity without writing to the item or history.
pub(super) fn evidence(
    history_path: &Path,
    item: &ItemDocument,
    assignee: &str,
    timestamp: &str,
    threshold: i64,
) -> Value {
    let stream = read_history(history_path);
    let history = stream.as_deref().unwrap_or_default();
    let index = ownership_index(history, assignee);
    let anchor = index.and_then(|index| history.get(index));
    let since = anchor.and_then(|entry| entry["ts"].as_str());
    let source = match anchor.and_then(|entry| entry["op"].as_str()) {
        Some("claim") => "claim",
        Some(_) => "assignment",
        None => "unknown",
    };
    let last = std::iter::once(item.metadata.updated_at.as_str())
        .chain(history.iter().filter_map(activity))
        .filter_map(|raw| parse(raw).map(|time| (raw, time)))
        .fold(None, |latest, candidate| match latest {
            Some((_, time)) if time >= candidate.1 => latest,
            _ => Some(candidate),
        });
    let now = parse(timestamp).unwrap_or_else(OffsetDateTime::now_utc);
    let elapsed = |time: OffsetDateTime| (now - time).whole_hours().max(0);
    let idle = last.map(|(_, time)| elapsed(time));
    let claimed = since.and_then(parse);
    let threshold = threshold.max(1);
    let mut reason = stream.as_ref().and_then(|_| unproven(history, index));
    if reason.is_none()
        && idle.is_some_and(|hours| hours >= threshold)
        && claimed.is_some_and(|time| workspace_restored(history_path, time))
    {
        reason = Some("workspace_restored");
    }
    let judgeable = stream.is_some() && reason.is_none() && last.is_some();
    let eligible = last
        .filter(|_| judgeable && threshold <= 2_400_000_000)
        .and_then(|(_, time)| time.checked_add(Duration::hours(threshold)))
        .map(|time| super::format_timestamp(time.to_offset(time::UtcOffset::UTC)));
    let mut activity = json!({"last_activity_at":last.map(|(raw, _)| raw)});
    if stream.is_none() {
        activity["history_unreadable"] = json!(true);
    }
    if let Some(reason) = reason {
        activity["unproven_activity"] = json!(reason);
    }
    activity["claim_age_hours"] = json!(claimed.map(elapsed));
    activity["idle_hours"] = json!(idle);
    activity["stale_policy"] = json!({
        "setting":"mutation_guard.stale_in_progress_hours",
        "threshold_hours":threshold,
        "stale":judgeable && idle.is_some_and(|hours| hours >= threshold),
        "eligible_at":eligible
    });
    json!({"claimed_by":assignee,"claimed_at":since,"claim_source":source,"claim_activity":activity})
}

#[cfg(test)]
#[path = "../../tests/support/claim_activity_differential.rs"]
mod tests;
