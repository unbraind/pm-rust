//! Read-only ownership evidence used in claim conflict receipts.

use std::{
    io::{ErrorKind, Read},
    path::Path,
};

use serde_json::{Value, json};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

use crate::item::ItemDocument;

/// Settings for the published stale-work threshold.
#[derive(Debug, serde::Deserialize)]
pub(super) struct MutationGuard {
    /// Minimum whole hours of inactivity before a claim is stale.
    #[serde(
        default = "default_threshold",
        deserialize_with = "deserialize_threshold"
    )]
    pub(super) stale_in_progress_hours: i64,
}

/// Default inactivity threshold for a tracker that has no override.
pub(super) const fn default_threshold() -> i64 {
    72
}

/// Checks a whole JSON number against the published positive safe-integer range.
pub(super) fn valid_threshold(value: &Value) -> bool {
    threshold_hours(value).is_some()
}

/// Resolves numeric spellings such as 24.0 and 24e0 without rounding fractions.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the range and whole value are checked"
)]
fn threshold_hours(value: &Value) -> Option<i64> {
    let hours = value.as_f64()?;
    ((1.0..=9_007_199_254_740_991.0).contains(&hours) && hours.fract() == 0.0)
        .then_some(hours as i64)
}

/// Decodes the typed setting after its containing settings document was normalized.
fn deserialize_threshold<'de, D: serde::Deserializer<'de>>(reader: D) -> Result<i64, D::Error> {
    <Value as serde::Deserialize>::deserialize(reader).and_then(|value| {
        threshold_hours(&value)
            .ok_or_else(|| serde::de::Error::custom("expected positive safe integer hours"))
    })
}

/// Parses a stored timestamp for activity comparisons.
pub(crate) fn parse_stored_timestamp(value: &str) -> Option<OffsetDateTime> {
    // The published Date reader also accepts four-digit years spelled with a
    // positive sign and six digits. Compare the normalized spelling only.
    let value = value.strip_prefix("+00").unwrap_or(value);
    let suffix = match value.len() {
        4 => "-01-01T00:00:00Z",
        7 => "-01T00:00:00Z",
        10 => "T00:00:00Z",
        _ => "",
    };
    if !suffix.is_empty() {
        return OffsetDateTime::parse(&format!("{value}{suffix}"), &Rfc3339).ok();
    }
    let clock_end = value.get(16..)?;
    // The published Date parser accepts ISO separators and legacy one-byte
    // delimiters. The RFC 3339 parser alone also accepts arbitrary letters.
    let separator = value.as_bytes()[10];
    if !(1..=b' ').contains(&separator) && !b"TtZz!\"#$%&'*,./:;<=>?@".contains(&separator) {
        return None;
    }
    let normalized;
    let value = if clock_end
        .as_bytes()
        .last_chunk::<5>()
        .is_some_and(|offset| {
            matches!(offset[0], b'+' | b'-') && offset[1..].iter().all(u8::is_ascii_digit)
        }) {
        normalized = format!(
            "{}:{}",
            &value[..value.len() - 2],
            &value[value.len() - 2..]
        );
        normalized.as_str()
    } else {
        value
    };
    let clock_end = &value[16..];
    if clock_end.starts_with(":60") {
        return None;
    }
    let time = if clock_end.starts_with(['Z', 'z', '+', '-']) {
        OffsetDateTime::parse(&format!("{}:00{clock_end}", &value[..16]), &Rfc3339).ok()
    } else {
        OffsetDateTime::parse(value, &Rfc3339).ok()
    }
    .or_else(|| parse_end_of_day(value))
    .or_else(|| parse_local_timestamp(value, clock_end))?;
    Some(time - Duration::nanoseconds(i64::from(time.nanosecond() % 1_000_000)))
}

/// Resolves an offset-free clock in the system time zone with Date's DST behavior.
fn parse_local_timestamp(value: &str, clock_end: &str) -> Option<OffsetDateTime> {
    use jiff::tz::AmbiguousOffset;

    if clock_end.contains(['Z', 'z', '+', '-']) {
        return None;
    }
    // Use the existing ISO validation and normalize 24:00 before applying the zone.
    let utc = if clock_end.is_empty() { ":00Z" } else { "Z" };
    let normalized = format!("{value}{utc}");
    let civil = OffsetDateTime::parse(&normalized, &Rfc3339)
        .ok()
        .or_else(|| parse_end_of_day(&normalized))?;
    #[allow(clippy::cast_possible_truncation, reason = "four-digit ISO year")]
    let year = civil.year() as i16;
    let local = jiff::civil::DateTime::new(
        year,
        u8::from(civil.month()).cast_signed(),
        civil.day().cast_signed(),
        civil.hour().cast_signed(),
        civil.minute().cast_signed(),
        civil.second().cast_signed(),
        civil.nanosecond().cast_signed(),
    );
    local.ok().and_then(|local| {
        // The earlier offset selects the first occurrence in a fold and
        // advances through a gap. Resolve it without narrowing the epoch range.
        let offset = match jiff::tz::TimeZone::system()
            .to_ambiguous_timestamp(local)
            .offset()
        {
            AmbiguousOffset::Unambiguous { offset }
            | AmbiguousOffset::Fold { before: offset, .. }
            | AmbiguousOffset::Gap { before: offset, .. } => offset,
        };
        civil.checked_sub(Duration::seconds(i64::from(offset.seconds())))
    })
}

/// Normalizes the ISO hour-24 spelling accepted by the published history reader.
fn parse_end_of_day(value: &str) -> Option<OffsetDateTime> {
    let (date, clock) = value.split_at_checked(10)?;
    let clock = clock.strip_prefix(['T', 't'])?.strip_prefix("24:00")?;
    let zone = if let Some(seconds) = clock.strip_prefix(':') {
        let rest = seconds.strip_prefix("00")?;
        if let Some(fraction) = rest.strip_prefix('.') {
            let zone = fraction.trim_start_matches('0');
            if zone.len() == fraction.len() {
                return None;
            }
            zone
        } else {
            rest
        }
    } else {
        clock
    };
    OffsetDateTime::parse(&format!("{date}T00:00:00{zone}"), &Rfc3339)
        .ok()?
        .checked_add(Duration::days(1))
}

/// Reads the inactivity threshold for the context advisory, defaulting unusable settings.
pub(crate) fn context_threshold(settings: &Value) -> i64 {
    threshold_hours(&settings["mutation_guard"]["stale_in_progress_hours"])
        .unwrap_or_else(default_threshold)
}

/// Adds the threshold in epoch milliseconds and applies the SDK's Date range.
fn eligible_at(time: OffsetDateTime, threshold: i64) -> Option<String> {
    const DATE_LIMIT_MS: i64 = 8_640_000_000_000_000;
    // Leap days repeat every 400 years in the proleptic Gregorian calendar.
    const CYCLE_MS: i64 = 146_097 * 86_400_000;
    let activity_ms = time.unix_timestamp() * 1000 + i64::from(time.millisecond());
    let millis = threshold.checked_mul(3_600_000)?.checked_add(activity_ms)?;
    if !(-DATE_LIMIT_MS..=DATE_LIMIT_MS).contains(&millis) {
        return None;
    }
    // Convert within one cycle so the calendar library never sees an extended year.
    let date = OffsetDateTime::UNIX_EPOCH + Duration::milliseconds(millis.rem_euclid(CYCLE_MS));
    let year = i64::from(date.year()) + 400 * millis.div_euclid(CYCLE_MS);
    Some(super::format_timestamp(date, year))
}

/// Reads the required fields and validates each patch path.
fn record_fields(entry: &Value) -> Option<(&str, &str, &[Value])> {
    let timestamp = entry["ts"].as_str()?;
    let operation = entry["op"].as_str()?;
    let patch = entry["patch"].as_array()?;
    crate::history::patch_paths_are_strings(patch).then_some((timestamp, operation, patch))
}

/// Prefix activity carried by a compaction baseline.
enum CompactionActivity<'a> {
    /// A baseline written without retained activity evidence.
    Legacy,
    /// A retained timestamp or explicit evidence that no prefix work occurred.
    Recorded(Option<&'a str>),
    /// A receipt with invalid containers or an invalid timestamp.
    Malformed,
}

/// Distinguishes missing receipt fields from malformed stored values.
fn compaction_activity(entry: &Value) -> CompactionActivity<'_> {
    let mut container = entry;
    for field in ["context", "history_compaction"] {
        let Some(value) = container.get(field) else {
            return CompactionActivity::Legacy;
        };
        if !value.is_object() {
            return CompactionActivity::Malformed;
        }
        container = value;
    }
    match container.get("last_activity_at") {
        None => CompactionActivity::Legacy,
        Some(Value::Null) => CompactionActivity::Recorded(None),
        Some(value) => value
            .as_str()
            .filter(|timestamp| parse_stored_timestamp(timestamp).is_some())
            .map_or(CompactionActivity::Malformed, |timestamp| {
                CompactionActivity::Recorded(Some(timestamp))
            }),
    }
}

/// Reads an entire stream, treating missing streams as empty and broken streams as unjudgeable.
fn read_history(path: &Path, valid: fn(&Value) -> bool) -> Option<Vec<Value>> {
    // Published history follows links to regular files. Check the opened handle
    // so a concurrent replacement with a FIFO cannot block the item lock holder.
    let mut file = match crate::changes::open_history_stream(path, true) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Some(Vec::new()),
        Err(_) => return None,
    };
    let metadata = file.metadata();
    #[cfg(test)]
    let metadata = crate::changes::inject_io(metadata, 1);
    if !metadata.ok()?.is_file() {
        return None;
    }
    let mut bytes = Vec::new();
    let read = file.read_to_end(&mut bytes);
    #[cfg(test)]
    let read = crate::changes::inject_io(read, 4);
    read.ok()?;
    String::from_utf8_lossy(&bytes)
        .lines()
        .map(crate::history::trim_evidence_line)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let entry: Value = crate::strict_json::from_str(line).ok()?;
            valid(&entry).then_some(entry)
        })
        .collect()
}

/// Checks whether an item record can support the claim activity judgment.
fn judgeable_item_record(entry: &Value) -> bool {
    let Some((timestamp, operation, _)) = record_fields(entry) else {
        return false;
    };
    parse_stored_timestamp(timestamp).is_some()
        && (operation != "history_compact_baseline"
            || !matches!(compaction_activity(entry), CompactionActivity::Malformed))
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
    let (timestamp, op, patch) = record_fields(entry)?;
    if op == "history_compact_baseline" {
        return match compaction_activity(entry) {
            CompactionActivity::Recorded(activity) => activity,
            CompactionActivity::Legacy | CompactionActivity::Malformed => Some(timestamp),
        };
    }
    let substantive = match entry["event_class"].as_str() {
        Some("substantive") => true,
        Some("maintenance") => false,
        _ => {
            let paths = patch.iter().filter_map(|row| row["path"].as_str());
            crate::history::classify_history_paths(op, paths) == "substantive"
        }
    };
    substantive.then_some(timestamp)
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
    read_history(&history_path.with_file_name("_workspace.jsonl"), |entry| {
        record_fields(entry).is_some()
    })
    .is_none_or(|history| {
        history.iter().any(|entry| {
            let at_or_after =
                |value: &str| parse_stored_timestamp(value).is_none_or(|at| at >= claimed_at);
            let timestamp = entry["ts"].as_str().unwrap_or_default();
            if entry["op"] == "workspace_snapshot_restore" || rewritten_operation(&entry["op"]) {
                return at_or_after(timestamp);
            }
            if entry["op"] == "history_compact_baseline" {
                return match compaction_activity(entry) {
                    CompactionActivity::Recorded(activity) => {
                        at_or_after(timestamp) || activity.is_some_and(at_or_after)
                    }
                    CompactionActivity::Legacy | CompactionActivity::Malformed => true,
                };
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
    let stream = read_history(history_path, judgeable_item_record);
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
        .filter_map(|raw| parse_stored_timestamp(raw).map(|time| (raw, time)))
        .fold(None, |latest, candidate| match latest {
            Some((_, time)) if time >= candidate.1 => latest,
            _ => Some(candidate),
        });
    let now = parse_stored_timestamp(timestamp).unwrap_or_else(OffsetDateTime::now_utc);
    let elapsed = |time: OffsetDateTime| (now - time).whole_hours().max(0);
    let idle = last.map(|(_, time)| elapsed(time));
    let claimed = since.and_then(parse_stored_timestamp);
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
        .filter(|_| judgeable)
        .and_then(|(_, time)| eligible_at(time, threshold));
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
