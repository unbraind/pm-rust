//! Brief context ranking, focus packing and omission receipts.
use crate::list_registry::Lifecycle;
use crate::orientation::{invalid, numeric, records};
use crate::{ContextOptions, ListOutput, PmRustError, Workspace};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Platform home variable used by the published extension discovery convention.
#[cfg(windows)]
const HOME_VARIABLE: &str = "USERPROFILE";
/// Platform home variable used by the published extension discovery convention.
#[cfg(not(windows))]
const HOME_VARIABLE: &str = "HOME";

/// Reads a string member with an empty fallback.
fn text<'a>(item: &'a Value, name: &str) -> &'a str {
    item[name].as_str().unwrap_or_default()
}
/// Converts collection sizes and priorities into exact small floating values.
fn number(value: usize) -> f64 {
    json!(value).as_f64().unwrap_or_default()
}
/// Gets the number of array entries in one metadata collection.
fn count(item: &Value, name: &str) -> usize {
    item[name].as_array().map_or(0, Vec::len)
}
/// Compares timestamps after normalizing offsets.
fn date_compare(a: &str, b: &str) -> std::cmp::Ordering {
    crate::list::timestamp_key(a).cmp(&crate::list::timestamp_key(b))
}
/// Implements the structural status ordering before relevance scoring.
fn status_rank(item: &Value, lifecycle: &Lifecycle) -> u8 {
    let status = text(item, "status");
    if lifecycle.matches(status, "in_progress") {
        0
    } else if lifecycle.role(status, "active") {
        1
    } else if lifecycle.canonical(status) == "blocked" {
        2
    } else if lifecycle.canonical(status) == "draft" {
        3
    } else if lifecycle.role(status, "blocked") {
        5
    } else if lifecycle.terminal(status) {
        7
    } else {
        6
    }
}
/// Orders optional numeric or date fields with nulls last.
fn optional(a: &Value, b: &Value, date: bool) -> std::cmp::Ordering {
    match (a.is_null(), b.is_null()) {
        (true, true) => std::cmp::Ordering::Equal,
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        (false, false) => {
            if date {
                date_compare(
                    a.as_str().unwrap_or_default(),
                    b.as_str().unwrap_or_default(),
                )
            } else {
                a.as_f64()
                    .unwrap_or_default()
                    .total_cmp(&b.as_f64().unwrap_or_default())
            }
        }
    }
}
/// Returns recency from substantive history, release cohort or creation time.
fn recency(workspace: &Workspace, item: &Value) -> Result<String, PmRustError> {
    let mut entries = records(workspace, text(item, "id"))?;
    entries.sort_by(|a, b| crate::collation::compare(text(a, "ts"), text(b, "ts")));
    for entry in entries.iter().rev() {
        let patch: Vec<_> = entry["patch"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|operation| crate::history::HistoryPatch {
                op: "replace",
                path: text(operation, "path").to_owned(),
                value: None,
            })
            .collect();
        let substantive = match entry["event_class"].as_str() {
            Some("substantive") => true,
            Some("maintenance") => false,
            _ => {
                !matches!(text(entry, "op"), "history_salvage" | "tests_update")
                    && crate::history::classify_history_event(text(entry, "op"), &patch)
                        == "substantive"
            }
        };
        if substantive && !text(entry, "ts").is_empty() {
            return Ok(text(entry, "ts").to_owned());
        }
    }
    let release = text(item, "release").trim_start_matches('v');
    let parts: Vec<_> = release.split('.').collect();
    if parts.len() >= 3 {
        let day = parts[2].split(['-', '+']).next().unwrap_or_default();
        let candidate = format!("{}-{:0>2}-{:0>2}T00:00:00.000Z", parts[0], parts[1], day);
        if crate::validate_timestamp(&candidate).is_ok() {
            return Ok(candidate);
        }
    }
    Ok(text(item, "created_at").to_owned())
}
/// Validated semantic and usage affinity resolved for one context reader.
#[derive(Default)]
struct Affinity {
    /// Required claimed items and softer lineage signals.
    claims: BTreeMap<String, f64>,
    /// Delivered-and-touched item signals, normalized within the retained ledger.
    usage: BTreeMap<String, f64>,
}
/// Reads at most the published ledger byte ceiling, dropping a clipped leading record.
fn usage_text(path: &std::path::Path) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let read = || {
        std::fs::File::open(path).and_then(|mut file| {
            file.metadata().and_then(|metadata| {
                let start = metadata.len().saturating_sub(262_144);
                file.seek(SeekFrom::Start(start)).and_then(|_| {
                    let mut bytes = Vec::new();
                    file.take(262_144)
                        .read_to_end(&mut bytes)
                        .map(|_| (bytes, start > 0))
                })
            })
        })
    };
    let Ok((bytes, clipped)) = read() else {
        return String::new();
    };
    let text = String::from_utf8_lossy(&bytes);
    if clipped {
        text.split_once('\n')
            .map_or_else(String::new, |(_, tail)| tail.to_owned())
    } else {
        text.into_owned()
    }
}
/// Reads bounded session and feedback evidence without recording a new serving event.
#[allow(clippy::too_many_lines)]
fn affinity(workspace: &Workspace, identity: &crate::AuthorIdentity, now: &str) -> Affinity {
    let runtime = workspace.pm_root().join("runtime");
    let session = std::fs::read(runtime.join("session.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .unwrap_or(Value::Null);
    let key = identity.instance.clone().unwrap_or_else(|| {
        format!(
            "author-{}",
            &crate::history::sha256_digest(identity.author.as_bytes())[..24]
        )
    });
    let attribution = &session["semantic_attribution"][&key];
    let mut result = Affinity::default();
    let valid = matches!(
        text(attribution, "role"),
        "implementer" | "planner" | "release-operator"
    ) && matches!(text(attribution, "confidence"), "high" | "medium")
        && !text(attribution, "topic").is_empty()
        && attribution["rule_version"] == "v2"
        && attribution["active_item_ids"]
            .as_array()
            .is_some_and(|items| {
                items.len() <= 64
                    && items.iter().all(|id| {
                        id.as_str()
                            .is_some_and(|id| !id.is_empty() && id.len() <= 128)
                    })
            })
        && attribution["evidence"].as_array().is_some_and(|items| {
            items.len() <= 32
                && items
                    .iter()
                    .all(|entry| entry.as_str().is_some_and(|entry| entry.len() <= 128))
        });
    if valid {
        for id in attribution["active_item_ids"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .chain(attribution["focused_item_id"].as_str())
        {
            result.claims.insert(id.to_owned(), 1.0);
        }
        for evidence in attribution["evidence"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if let Some(id) = evidence.strip_prefix("lineage:") {
                result.claims.entry(id.to_owned()).or_insert(0.75);
            }
        }
    }
    if std::env::var("PM_CONTEXT_USAGE_DISABLED").as_deref() == Ok("1") {
        return result;
    }
    let raw = usage_text(&runtime.join("context-usage.jsonl"));
    let now = crate::list::timestamp_key(now).0.unwrap_or(0);
    let retained: Vec<Value> = raw
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|event| {
            crate::list::timestamp_key(text(event, "at"))
                .0
                .is_some_and(|time| time >= now - 30 * 86_400_000)
        })
        .collect();
    let events: Vec<_> = retained
        .into_iter()
        .rev()
        .take(2048)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .filter(|event| text(event, "author") == identity.author.trim())
        .collect();
    let mut deliveries = BTreeMap::new();
    for event in &events {
        if event["kind"] == "delivery" && event["schema_version"] == 2 {
            deliveries.entry(text(event, "serve_id")).or_insert(event);
        }
    }
    let mut scores: BTreeMap<String, f64> = BTreeMap::new();
    for event in events.iter().filter(|event| {
        event["kind"] == "serve"
            && event["schema_version"] == 2
            && !text(event, "serve_id").is_empty()
    }) {
        let Some(delivery) = deliveries.get(text(event, "serve_id")) else {
            continue;
        };
        let served = crate::list::timestamp_key(text(event, "at")).0.unwrap_or(0);
        for row in event["rows"].as_array().into_iter().flatten() {
            if !delivery["delivered_item_ids"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|id| id == &row["id"])
            {
                continue;
            }
            let touch = events
                .iter()
                .filter(|event| event["kind"] == "touch" && event["item_id"] == row["id"])
                .find_map(|event| {
                    crate::list::timestamp_key(text(event, "at"))
                        .0
                        .filter(|time| *time >= served && *time - served <= 86_400_000)
                });
            if let Some(touch) = touch {
                #[allow(clippy::cast_precision_loss)]
                let age = ((now - touch) as f64 / 86_400_000.0).max(0.0);
                *scores.entry(text(row, "id").to_owned()).or_default() += (-age / 14.0).exp();
            }
        }
    }
    let maximum = scores.values().copied().fold(0.0, f64::max);
    // Every retained touch has finite age within thirty days, so a nonempty
    // score map has a strictly positive exponential maximum.
    result.usage = scores
        .into_iter()
        .map(|(id, score)| (id, 0.05 + 0.95 * score / maximum))
        .collect();
    result
}
/// Scores candidates using the published default weighted signal model.
#[allow(clippy::too_many_lines)]
fn ranked(
    workspace: &Workspace,
    items: &[Value],
    now: &str,
    lifecycle: &Lifecycle,
    author: &str,
    affinity: &Affinity,
) -> Result<Vec<(Value, f64)>, PmRustError> {
    let mut recent: Vec<_> = items
        .iter()
        .map(|item| Ok((text(item, "id"), recency(workspace, item)?)))
        .collect::<Result<_, PmRustError>>()?;
    recent
        .sort_by(|a, b| date_compare(&b.1, &a.1).then_with(|| crate::collation::compare(a.0, b.0)));
    let recency: BTreeMap<_, _> = recent
        .iter()
        .enumerate()
        .map(|(i, (id, _))| (*id, i))
        .collect();
    let mut degrees: BTreeMap<String, usize> = BTreeMap::new();
    for item in items {
        let id = text(item, "id");
        let mut targets = Vec::new();
        if !text(item, "parent").trim().is_empty() {
            targets.push(text(item, "parent").trim());
        }
        targets.extend(
            item["dependencies"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|dep| text(dep, "id").trim())
                .filter(|id| !id.is_empty()),
        );
        for target in targets {
            *degrees.entry(id.to_owned()).or_default() += 1;
            *degrees.entry(target.to_owned()).or_default() += 1;
        }
    }
    let max_degree = number(degrees.values().copied().max().unwrap_or(0).max(1));
    let activities: Vec<_> = items
        .iter()
        .map(|item| {
            ["comments", "notes", "learnings", "test_runs"]
                .iter()
                .map(|name| count(item, name))
                .sum::<usize>()
        })
        .collect();
    let max_activity = number(activities.iter().copied().max().unwrap_or(0).max(1));
    let denominator = number(items.len().saturating_sub(1).max(1));
    let mut scored = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let assigned = !author.trim().is_empty()
            && text(item, "assignee")
                .trim()
                .eq_ignore_ascii_case(author.trim());
        let priority = item["priority"].as_f64().unwrap_or(0.0);
        let deadline = crate::list::timestamp_key(text(item, "deadline")).0;
        let now = crate::list::timestamp_key(now).0;
        #[allow(clippy::cast_precision_loss)]
        let pressure = deadline.zip(now).map_or(0.0, |(deadline, now)| {
            let days = (deadline - now) as f64 / 86_400_000.0;
            if days <= 0.0 {
                1.0
            } else {
                1.0 / (1.0 + days / 30.0)
            }
        });
        let signals = [
            (
                if items.len() == 1 {
                    1.0
                } else {
                    1.0 - number(index) / denominator
                },
                0.4,
            ),
            (
                if items.len() == 1 {
                    1.0
                } else {
                    1.0 - number(*recency.get(text(item, "id")).unwrap_or(&0)) / denominator
                },
                0.35,
            ),
            (number(activities[index]) / max_activity, 0.25),
            (
                number(*degrees.get(text(item, "id")).unwrap_or(&0)) / max_degree,
                0.15,
            ),
            (
                affinity
                    .claims
                    .get(text(item, "id"))
                    .copied()
                    .unwrap_or_else(|| {
                        if lifecycle.matches(text(item, "status"), "in_progress") {
                            1.0
                        } else if assigned {
                            0.75
                        } else {
                            0.0
                        }
                    }),
                0.6,
            ),
            (1.0 - priority.clamp(0.0, 4.0) / 4.0, 0.35),
            (
                match text(item, "risk").trim().to_lowercase().as_str() {
                    "high" | "critical" => 1.0,
                    "medium" => 0.5,
                    "low" => 0.1,
                    _ => 0.0,
                },
                0.2,
            ),
            (pressure, 0.45),
            (
                (number(count(item, "comments") + count(item, "notes") + count(item, "learnings"))
                    / 5.0)
                    .min(1.0),
                0.25,
            ),
            (if assigned { 1.0 } else { 0.0 }, 0.2),
        ];
        let total = signals.iter().map(|(_, weight)| weight).sum::<f64>();
        let mut weighted = signals
            .iter()
            .map(|(value, weight)| value * weight)
            .sum::<f64>();
        let mut total = total;
        if let Some(usage) = affinity.usage.get(text(item, "id")) {
            weighted += usage * 0.3;
            total += 0.3;
        }
        let score = weighted / total;
        scored.push((item.clone(), score));
    }
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    Ok(scored)
}
/// Selects identities by marginal score density with the context redundancy penalty.
fn pack(candidates: &[(Value, f64)], budget: usize, required: &BTreeSet<String>) -> Vec<Value> {
    let mut remaining = budget;
    let mut represented = BTreeSet::new();
    let mut candidates: Vec<_> = candidates.iter().enumerate().collect();
    let mut included = Vec::new();
    candidates.retain(|(rank, (item, _))| {
        if !required.contains(text(item, "id")) {
            return true;
        }
        let cost = crate::stringify_json(&json!({"id":item["id"],"title":item["title"]}), false)
            .len()
            .div_ceil(4)
            .max(1);
        if cost <= remaining {
            included.push((*rank, item.clone()));
            remaining -= cost;
            represented.insert(
                if text(item, "parent").trim().is_empty() {
                    text(item, "type")
                } else {
                    text(item, "parent").trim()
                }
                .to_owned(),
            );
        }
        false
    });

    loop {
        let mut best = None;
        for (index, (_, (item, score))) in candidates.iter().enumerate() {
            let cost =
                crate::stringify_json(&json!({"id":item["id"],"title":item["title"]}), false)
                    .len()
                    .div_ceil(4)
                    .max(1);
            if cost > remaining {
                continue;
            }
            let cluster = if text(item, "parent").trim().is_empty() {
                text(item, "type")
            } else {
                text(item, "parent").trim()
            };
            let density = score
                * if represented.contains(cluster) {
                    0.55
                } else {
                    1.0
                }
                / number(cost);
            if best.is_none_or(|(_, best_density, _)| density > best_density) {
                best = Some((index, density, cost));
            }
        }
        let Some((index, _, cost)) = best else {
            break;
        };
        let (rank, (item, _)) = candidates.remove(index);
        represented.insert(
            if text(item, "parent").trim().is_empty() {
                text(item, "type")
            } else {
                text(item, "parent").trim()
            }
            .to_owned(),
        );
        included.push((rank, item.clone()));
        remaining -= cost;
    }
    included.sort_by_key(|(rank, _)| *rank);
    included.into_iter().map(|(_, item)| item).collect()
}
/// Resolves unresolved forward and inverse blocker identities in stable lexical order.
fn blockers(item: &Value, corpus: &[Value], lifecycle: &Lifecycle) -> Vec<String> {
    let mut ids = BTreeMap::new();
    let scalar = text(item, "blocked_by").trim();
    if !scalar.is_empty() && !scalar.eq_ignore_ascii_case("no-active-blocker") {
        ids.insert(scalar.to_lowercase(), scalar.to_owned());
    }
    for dep in item["dependencies"].as_array().into_iter().flatten() {
        if dep["kind"] == "blocked_by" {
            let id = text(dep, "id").trim();
            ids.entry(id.to_lowercase())
                .or_insert_with(|| id.to_owned());
        }
    }
    for other in corpus {
        for dep in other["dependencies"].as_array().into_iter().flatten() {
            if dep["kind"] == "blocks" && text(dep, "id").eq_ignore_ascii_case(text(item, "id")) {
                ids.entry(text(other, "id").to_lowercase())
                    .or_insert_with(|| text(other, "id").to_owned());
            }
        }
    }
    ids.into_iter()
        .filter_map(|(key, id)| {
            let target = corpus
                .iter()
                .find(|item| text(item, "id").trim().to_lowercase() == key);
            if target.is_some_and(|item| lifecycle.terminal(text(item, "status"))) {
                None
            } else {
                Some(target.map_or(id, |item| text(item, "id").to_owned()))
            }
        })
        .collect()
}
/// Projects the stable focus fields and dependency blocking receipt.
fn focus(
    item: &Value,
    corpus: &[Value],
    blocked: &BTreeSet<String>,
    lifecycle: &Lifecycle,
) -> Value {
    let mut result = json!({"id":item["id"],"title":item["title"],"type":item["type"],"status":item["status"],"priority":item["priority"],"order":item["order"],"deadline":item["deadline"],"assignee":item["assignee"],"tags":item["tags"],"updated_at":item["updated_at"],"parent":if text(item,"parent").trim().is_empty(){Value::Null}else{json!(text(item,"parent").trim())}});
    if blocked.contains(text(item, "id")) {
        result["blocked"] = json!(true);
        result["blocker_ids"] = json!(blockers(item, corpus, lifecycle));
    }
    result
}
/// Reports whether any descendant remains nonterminal, with cycle protection.
fn container(id: &str, corpus: &[Value], lifecycle: &Lifecycle) -> bool {
    let mut pending = vec![id];
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id.to_lowercase()) {
            continue;
        }
        for child in corpus
            .iter()
            .filter(|item| text(item, "parent").trim().eq_ignore_ascii_case(id))
        {
            if !lifecycle.terminal(text(child, "status")) {
                return true;
            }
            pending.push(text(child, "id"));
        }
    }
    false
}
/// Produces the UTC calendar day of a valid timestamp.
fn utc_day(value: &str) -> String {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339).map_or_else(
        |_| value.get(..10).unwrap_or_default().to_owned(),
        |date| date.to_offset(time::UtcOffset::UTC).date().to_string(),
    )
}
/// Formats an absolute instant using the oracle's millisecond UTC spelling.
fn calendar_timestamp(date: time::OffsetDateTime) -> String {
    let date = date.to_offset(time::UtcOffset::UTC);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        date.year(),
        u8::from(date.month()),
        date.day(),
        date.hour(),
        date.minute(),
        date.second(),
        date.millisecond()
    )
}
/// Parses an absolute calendar coordinate without consulting the process timezone.
fn calendar_instant(raw: &str) -> Option<time::OffsetDateTime> {
    time::OffsetDateTime::parse(raw, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|date| date.to_offset(time::UtcOffset::UTC))
}
/// Maps the published weekday tokens onto Monday-based UTC offsets.
fn weekday_index(token: &str) -> Option<u8> {
    ["mon", "tue", "wed", "thu", "fri", "sat", "sun"]
        .iter()
        .position(|day| *day == token)
        .and_then(|index| u8::try_from(index).ok())
}
/// Expands weekly UTC recurrence with the published 365-day window and 1000-step cap.
fn event_dates(event: &Value, now: &str) -> Vec<String> {
    let start_raw = text(event, "start_at");
    let Some(rule) = event.get("recurrence").filter(|rule| rule.is_object()) else {
        return vec![start_raw.to_owned()];
    };
    let Some(start) = calendar_instant(start_raw) else {
        return Vec::new();
    };
    let Some(now) = calendar_instant(now) else {
        return Vec::new();
    };
    let until = rule["until"].as_str().and_then(calendar_instant);
    let end = now + time::Duration::days(365);
    let monday =
        start.date() - time::Duration::days(i64::from(start.weekday().number_days_from_monday()));
    let mut weekdays: Vec<_> = rule["by_weekday"].as_array().map_or_else(
        || vec![start.weekday().number_days_from_monday()],
        |days| {
            days.iter()
                .filter_map(Value::as_str)
                .filter_map(weekday_index)
                .collect()
        },
    );
    weekdays.sort_unstable();
    weekdays.dedup();
    let excluded: BTreeSet<_> = rule["exdates"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(calendar_instant)
        .collect();
    let mut output = Vec::new();
    let mut produced = 0u64;
    for step in 0..1000i64 {
        for weekday in &weekdays {
            let offset = step
                .checked_mul(rule["interval"].as_i64().unwrap_or(1))
                .and_then(|value| value.checked_mul(7))
                .and_then(|value| value.checked_add(i64::from(*weekday)));
            let Some(date) = offset
                .and_then(|days| days.checked_mul(86_400))
                .and_then(|seconds| monday.checked_add(time::Duration::seconds(seconds)))
            else {
                return output;
            };
            let candidate = start.replace_date(date);
            if rule["by_month_day"].as_array().is_some_and(|days| {
                !days
                    .iter()
                    .any(|day| day.as_u64() == Some(u64::from(candidate.day())))
            }) {
                continue;
            }
            if candidate < start {
                continue;
            }
            if until.is_some_and(|until| candidate > until) || candidate >= end {
                return output;
            }
            if excluded.contains(&candidate) {
                continue;
            }
            produced += 1;
            if candidate >= now {
                output.push(calendar_timestamp(candidate));
            }
            if produced >= rule["count"].as_u64().unwrap_or(u64::MAX) || output.len() >= 1000 {
                return output;
            }
        }
    }
    output
}
/// Renders the recurrence metadata in the published deterministic field order.
fn recurrence_summary(rule: &Value) -> Value {
    if !rule.is_object() {
        return Value::Null;
    }
    let mut parts = vec![format!("freq={}", text(rule, "freq"))];
    for key in ["interval", "count", "until"] {
        if let Some(value) = rule.get(key) {
            parts.push(format!(
                "{key}={}",
                value
                    .as_str()
                    .map_or_else(|| crate::stringify_json(value, false), ToString::to_string)
            ));
        }
    }
    for key in ["by_weekday", "by_month_day", "exdates"] {
        if let Some(values) = rule[key].as_array().filter(|values| !values.is_empty()) {
            let mut values = values.clone();
            values.sort_by(|a, b| match key {
                "by_weekday" => weekday_index(a.as_str().unwrap_or_default())
                    .cmp(&weekday_index(b.as_str().unwrap_or_default())),
                "by_month_day" => a.as_i64().cmp(&b.as_i64()),
                _ => crate::collation::compare(
                    a.as_str().unwrap_or_default(),
                    b.as_str().unwrap_or_default(),
                ),
            });
            parts.push(format!(
                "{key}={}",
                values
                    .iter()
                    .map(|value| value
                        .as_str()
                        .map_or_else(|| crate::stringify_json(value, false), ToString::to_string))
                    .collect::<Vec<_>>()
                    .join("|")
            ));
        }
    }
    json!(parts.join(","))
}
/// Retains recurring duration while projecting an occurrence's end instant.
fn event_end(event: &Value, at: &str) -> Value {
    if event["recurrence"].is_object()
        && let Some(((start, end), at)) = calendar_instant(text(event, "start_at"))
            .zip(calendar_instant(text(event, "end_at")))
            .zip(calendar_instant(at))
    {
        return json!(calendar_timestamp(at + (end - start)));
    }
    event["end_at"].clone()
}
/// Builds future deadline and reminder rows in the default unbounded agenda window.
fn agenda(
    corpus: &[Value],
    focus_ids: &BTreeSet<String>,
    now: &str,
    limit: usize,
    lifecycle: &Lifecycle,
) -> (Value, Vec<Value>) {
    let mut rows = Vec::new();
    for item in corpus {
        let mut events = Vec::new();
        if !text(item, "deadline").is_empty() {
            events.push((
                text(item, "deadline").to_owned(),
                "deadline",
                Value::Null,
                None,
            ));
        }
        for reminder in item["reminders"].as_array().into_iter().flatten() {
            events.push((
                text(reminder, "at").to_owned(),
                "reminder",
                reminder["text"].clone(),
                None,
            ));
        }
        for event in item["events"].as_array().into_iter().flatten() {
            for at in event_dates(event, now) {
                events.push((at, "event", event["description"].clone(), Some(event)));
            }
        }
        for (at, kind, message, event) in events {
            if date_compare(&at, now).is_lt() {
                continue;
            }
            let mut row = json!({"at":at,"date":utc_day(&at),"kind":kind,"reminder_text":message,"event_title":null,"event_end":null,"event_location":null,"event_all_day":null,"event_timezone":null,"event_recurring":null,"event_recurrence_rule":null,"item_id":item["id"],"item_title":item["title"],"item_type":item["type"],"item_status":item["status"],"item_priority":item["priority"],"item_assignee":item["assignee"],"item_deadline":item["deadline"],"item_tags":item["tags"]});
            if let Some(event) = event {
                row["event_title"] = event
                    .get("title")
                    .cloned()
                    .unwrap_or_else(|| item["title"].clone());
                row["event_end"] = event_end(event, &at);
                row["event_location"] = event["location"].clone();
                row["event_all_day"] = event["all_day"].clone();
                row["event_timezone"] = event["timezone"].clone();
                row["event_recurring"] = json!(event["recurrence"].is_object());
                row["event_recurrence_rule"] = recurrence_summary(&event["recurrence"]);
                if event["all_day"] == true {
                    row["date"] = json!(at.get(..10).unwrap_or_default());
                }
            }
            if focus_ids.contains(text(item, "id")) {
                row = json!({"at":at,"date":utc_day(&at),"kind":kind,"item_id":item["id"],"reference_only":true});
                if let Some(event) = event {
                    row["event_title"] = event
                        .get("title")
                        .cloned()
                        .unwrap_or_else(|| item["title"].clone());
                    row["event_recurring"] = json!(event["recurrence"].is_object());
                    if event["all_day"] == true {
                        row["date"] = json!(at.get(..10).unwrap_or_default());
                    }
                }
                if kind == "reminder" {
                    row["reminder_text"] = message;
                }
            }
            rows.push((row, item["priority"].as_u64().unwrap_or_default()));
        }
    }
    rows.sort_by(|(a, ap), (b, bp)| {
        date_compare(text(a, "at"), text(b, "at"))
            .then_with(|| ap.cmp(bp))
            .then_with(|| crate::collation::compare(text(a, "item_id"), text(b, "item_id")))
            .then_with(|| crate::collation::compare(text(a, "kind"), text(b, "kind")))
            .then_with(|| crate::collation::compare(text(a, "event_title"), text(b, "event_title")))
            .then_with(|| {
                crate::collation::compare(text(a, "reminder_text"), text(b, "reminder_text"))
            })
    });
    // Calendar applies its cap before context excludes terminal events.
    let rows: Vec<_> = rows
        .into_iter()
        .take(limit)
        .map(|(row, _)| row)
        .filter(|row| {
            !corpus.iter().any(|item| {
                item["id"] == row["item_id"] && lifecycle.terminal(text(item, "status"))
            })
        })
        .collect();
    let ids: BTreeSet<_> = rows.iter().map(|row| text(row, "item_id")).collect();
    (
        json!({"events":rows.len(),"items":ids.len(),"deadlines":rows.iter().filter(|row|row["kind"]=="deadline").count(),"reminders":rows.iter().filter(|row|row["kind"]=="reminder").count(),"scheduled":rows.iter().filter(|row|row["kind"]=="event").count()}),
        rows,
    )
}
/// Refuses unsupported mutable ranking state instead of silently ignoring it.
fn context_state_boundary(workspace: &Workspace, corpus: &[Value]) -> Result<(), PmRustError> {
    for root in extension_health_global()
        .into_iter()
        .chain(std::iter::once(workspace.pm_root().to_path_buf()))
    {
        for entry in std::fs::read_dir(root.join("extensions"))
            .into_iter()
            .flatten()
            .flatten()
        {
            let manifest = std::fs::read(entry.path().join("manifest.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .unwrap_or(Value::Null);
            if manifest["contributions"]["service_overrides"]
                .as_array()
                .is_some_and(|services| {
                    services
                        .iter()
                        .any(|service| service == "context_relevance")
                })
            {
                return Err(crate::collections::refusal(
                    "Native context does not execute declared context_relevance service overrides; use the published pm CLI.",
                    2,
                    json!({"code":"unsupported_native_context"}),
                ));
            }
        }
    }
    let runtime = workspace.pm_root().join("runtime");
    let load = |name: &str| {
        std::fs::read(runtime.join(name))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
    };
    let snapshot = load("context-signals-context.json");
    let invalid = runtime.join("context-signals-context.json").is_file()
        && snapshot.as_ref().is_none_or(|snapshot| {
            snapshot["format_version"] != 3
                || snapshot["signal_set_version"] != 2
                || !snapshot["items"].is_array()
        });
    let stale = snapshot.as_ref().is_some_and(|snapshot| {
        let ids: BTreeSet<_> = snapshot["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|row| row["id"].as_str())
            .collect();
        let actual: BTreeSet<_> = corpus.iter().map(|item| text(item, "id")).collect();
        ids != actual
            || (snapshot["source"] == "derived_index"
                && load("metadata-cache-manifest.json")
                    .as_ref()
                    .is_some_and(|manifest| manifest["source_cursor"] != snapshot["source_cursor"]))
    });
    if invalid || stale {
        return Err(crate::collections::refusal(
            "Native brief context cannot certify invalid or stale derived ranking state; use the published pm CLI.",
            2,
            json!({"code":"unsupported_native_context"}),
        ));
    }
    Ok(())
}
/// Resolves the published global extension root without modifying agent environment state.
fn extension_health_global() -> Option<std::path::PathBuf> {
    std::env::var_os("PM_GLOBAL_PATH")
        .filter(|path| !path.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os(HOME_VARIABLE)
                .map(|home| std::path::PathBuf::from(home).join(".pm-cli"))
        })
}
/// Reads the documented minimal static extension diagnostics without executing code.
fn extension_health(
    workspace: &Workspace,
    settings: &Value,
    global: Option<std::path::PathBuf>,
) -> Result<Option<Value>, PmRustError> {
    let roots = global
        .into_iter()
        .chain(std::iter::once(workspace.pm_root().to_path_buf()));
    let mut packages = Vec::new();
    for root in roots {
        for entry in std::fs::read_dir(root.join("extensions"))
            .into_iter()
            .flatten()
            .flatten()
        {
            let path = entry.path();
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let manifest = std::fs::read(path.join("manifest.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .unwrap_or(Value::Null);
            let configured = ["enabled", "disabled"].iter().any(|key| {
                settings["extensions"][key]
                    .as_array()
                    .is_some_and(|values| !values.is_empty())
            }) || settings["extensions"].as_object().is_some_and(|fields| {
                fields
                    .keys()
                    .any(|key| !matches!(key.as_str(), "enabled" | "disabled"))
            });
            let rich = manifest.as_object().is_some_and(|fields| {
                fields.keys().any(|key| {
                    !matches!(
                        key.as_str(),
                        "name" | "version" | "entry" | "description" | "author"
                    )
                })
            });
            if configured || rich {
                return Err(crate::collections::refusal(
                    "Native context cannot certify configured extension policy or nonminimal manifests; use --no-extension-health or the published pm CLI.",
                    2,
                    json!({"code":"unsupported_native_context"}),
                ));
            }
            let valid = ["name", "version", "entry"].iter().all(|key| {
                manifest[key]
                    .as_str()
                    .is_some_and(|value| !value.trim().is_empty())
            });
            let name = if valid {
                text(&manifest, "name").trim().to_owned()
            } else {
                entry.file_name().to_string_lossy().into_owned()
            };
            let ready = valid
                && std::fs::canonicalize(path.join(text(&manifest, "entry").trim()))
                    .ok()
                    .zip(std::fs::canonicalize(&path).ok())
                    .is_some_and(|(entry, root)| entry.starts_with(root));
            packages.push(json!({"name":name,"status":if ready{"ok"}else{"degraded"}}));
        }
    }
    packages.sort_by(|a, b| crate::collation::compare(text(a, "name"), text(b, "name")));
    if packages.is_empty() {
        return Ok(None);
    }
    let degraded = packages
        .iter()
        .filter(|package| package["status"] == "degraded")
        .count();
    let mut result = json!({"total":packages.len(),"ok":packages.len()-degraded,"degraded":degraded,"packages":packages.iter().take(20).collect::<Vec<_>>()});
    if packages.len() > 20 {
        result["truncated"] = json!(true);
    }
    Ok(Some(result))
}
impl Workspace {
    /// Renders brief focus with ranked packing and the oracle's omission receipt.
    ///
    /// # Errors
    /// Returns numeric, unsupported-depth, item or history read errors.
    #[allow(clippy::too_many_lines)]
    pub fn context(&self, options: &ContextOptions, now: &str) -> Result<ListOutput, PmRustError> {
        let settings = crate::ownership::runtime_settings(self.pm_root())?;
        let depth = options
            .depth
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                settings["context"]["default_depth"]
                    .as_str()
                    .unwrap_or("brief")
            })
            .trim()
            .to_lowercase();
        if !["brief", "standard", "deep", "full"].contains(&depth.as_str()) {
            return Err(invalid(
                "Context --depth must be one of brief|standard|deep|full",
            ));
        }
        if depth != "brief" {
            return Err(crate::collections::refusal(
                "Native context supports --depth brief; use the published pm CLI for other depths",
                2,
                json!({"code":"unsupported_native_option","required":"Use the published pm CLI for options outside the documented native slice.","title":"Unsupported native option","flag":"--depth","value":depth}),
            ));
        }
        let limit = options
            .limit
            .as_ref()
            .map(|raw| {
                numeric(
                    raw,
                    true,
                    0,
                    "--limit must be a non-negative integer".to_owned(),
                )
            })
            .transpose()?
            .unwrap_or(10);
        let budget = options
            .token_budget
            .as_ref()
            .map(|raw| {
                numeric(
                    raw,
                    true,
                    1,
                    "--token-budget must be a positive integer".to_owned(),
                )
            })
            .transpose()?
            .unwrap_or_else(|| limit.saturating_mul(160).max(256));
        let mut documents = self.read_items()?;
        let lifecycle = Lifecycle::load(self.pm_root())?;
        let blocked = crate::selection::dependency_blocked_ids(&documents, &lifecycle);
        // Keep the list's default source order while projecting this one captured corpus.
        documents.sort_by_cached_key(|document| {
            let metadata = &document.metadata;
            (
                lifecycle.terminal(&metadata.status),
                metadata.priority,
                std::cmp::Reverse(crate::list::timestamp_key(&metadata.updated_at)),
                metadata.id.clone(),
            )
        });
        let corpus: Vec<_> = documents
            .into_iter()
            .map(|document| {
                let mut value = serde_json::to_value(document.metadata).unwrap_or_default();
                crate::json_output::normalize_read_numbers(&mut value);
                value
            })
            .collect();
        if corpus.len() >= 10000 {
            return Err(crate::collections::refusal(
                "Native brief context does not yet support workspace-memory rollups at 10000 items; use the published pm CLI.",
                2,
                json!({"code":"unsupported_native_context"}),
            ));
        }
        if corpus.iter().any(|item| {
            item["events"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|event| {
                    event
                        .get("recurrence")
                        .is_some_and(|rule| rule.is_object() && rule["freq"] != "weekly")
                        || event["timezone"]
                            .as_str()
                            .is_some_and(|zone| !matches!(zone, "UTC" | "Etc/UTC"))
                })
        }) {
            return Err(crate::collections::refusal(
                "Native brief context supports UTC weekly recurrence; other recurrence frequencies and non-UTC scheduled events require the published pm CLI.",
                2,
                json!({"code":"unsupported_native_context"}),
            ));
        }
        let mut items: Vec<_> = corpus
            .iter()
            .filter(|item| !lifecycle.terminal(text(item, "status")))
            .cloned()
            .collect();
        items.sort_by(|a, b| {
            status_rank(a, &lifecycle)
                .cmp(&status_rank(b, &lifecycle))
                .then_with(|| optional(&a["priority"], &b["priority"], false))
                .then_with(|| optional(&a["order"], &b["order"], false))
                .then_with(|| optional(&a["deadline"], &b["deadline"], true))
                .then_with(|| date_compare(text(b, "updated_at"), text(a, "updated_at")))
                .then_with(|| crate::collation::compare(text(a, "id"), text(b, "id")))
        });
        let env = std::env::vars().collect();
        let identity =
            crate::AuthorIdentity::resolve(None, settings["author_default"].as_str(), &env);
        context_state_boundary(self, &items)?;
        let affinity = affinity(self, &identity, now);
        let ranking = ranked(self, &items, now, &lifecycle, &identity.author, &affinity)?;
        let active: Vec<_> = ranking
            .iter()
            .filter(|(item, _)| lifecycle.role(text(item, "status"), "active"))
            .collect();
        let bounded = options.limit.is_some() || corpus.len() >= 10000;
        let page: Vec<_> = active
            .iter()
            .take(if bounded { limit } else { usize::MAX })
            .copied()
            .collect();
        let packing_ids: BTreeSet<_> = page
            .iter()
            .map(|(item, _)| text(item, "id"))
            .chain(
                ranking
                    .iter()
                    .filter(|(item, _)| blocked.contains(text(item, "id")))
                    .take(limit)
                    .map(|(item, _)| text(item, "id")),
            )
            .collect();
        let packed = pack(
            &ranking
                .iter()
                .filter(|(item, _)| packing_ids.contains(text(item, "id")))
                .cloned()
                .collect::<Vec<_>>(),
            budget,
            &affinity
                .claims
                .iter()
                .filter(|(_, value)| **value >= 1.0)
                .map(|(id, _)| id.clone())
                .collect(),
        );
        let packed_active: Vec<_> = packed
            .iter()
            .filter(|item| lifecycle.role(text(item, "status"), "active"))
            .collect();
        let packed_blocked: Vec<_> = packed
            .iter()
            .filter(|item| blocked.contains(text(item, "id")))
            .collect();
        let high_ids: BTreeSet<_> = packed_active
            .iter()
            .filter(|item| {
                matches!(
                    text(item, "type"),
                    "Epic" | "Feature" | "Decision" | "Milestone"
                ) || container(text(item, "id"), &corpus, &lifecycle)
            })
            .map(|item| text(item, "id"))
            .collect();
        let mut high: Vec<_> = packed_active
            .iter()
            .filter(|item| high_ids.contains(text(item, "id")))
            .take(limit)
            .map(|item| focus(item, &corpus, &blocked, &lifecycle))
            .collect();
        let mut low: Vec<_> = packed_active
            .iter()
            .filter(|item| !high_ids.contains(text(item, "id")))
            .take(limit)
            .map(|item| focus(item, &corpus, &blocked, &lifecycle))
            .collect();
        let mut fallback: Vec<_> = if active.is_empty() {
            packed_blocked
                .iter()
                .take(limit)
                .map(|item| focus(item, &corpus, &blocked, &lifecycle))
                .collect()
        } else {
            vec![]
        };
        let focus_ids = high
            .iter()
            .chain(&low)
            .chain(&fallback)
            .map(|item| text(item, "id").to_owned())
            .collect();
        let (agenda_summary, events) = agenda(&corpus, &focus_ids, now, limit, &lifecycle);
        let tags: BTreeMap<_, _> = high
            .iter()
            .chain(&low)
            .chain(&fallback)
            .map(|row| {
                let mut tags: Vec<_> = row["tags"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect();
                tags.sort_unstable();
                (text(row, "id").to_owned(), tags.join("\0"))
            })
            .collect();
        for row in high.iter_mut().chain(&mut low).chain(&mut fallback) {
            let parent = text(row, "parent");
            if options.no_tags {
                row.as_object_mut().map(|row| row.shift_remove("tags"));
            } else if !parent.is_empty() && tags.get(parent) == tags.get(text(row, "id")) {
                let parent = parent.to_owned();
                row.as_object_mut().map(|row| row.shift_remove("tags"));
                row["tags_inherited"] = json!(parent);
            }
        }
        let in_progress = high
            .iter()
            .chain(&low)
            .chain(&fallback)
            .filter(|item| lifecycle.matches(text(item, "status"), "in_progress"))
            .count();
        let all_active = items
            .iter()
            .filter(|item| lifecycle.role(text(item, "status"), "active"))
            .count();
        let all_progress = items
            .iter()
            .filter(|item| lifecycle.matches(text(item, "status"), "in_progress"))
            .count();
        let mut result = json!({"output_default":"toon","now":now,"depth":"brief","sections_included":[],"window":{"anchor":now,"start":now,"end":null,"past":false,"from":null,"to":null},"filters":{"type":null,"tag":null,"priority":null,"assignee":null,"assignee_filter":null,"sprint":null,"release":null,"limit":options.limit,"parent":null,"token_budget":budget,"runtime_filters":{}},"summary":{"returned_focus":{"active_items":high.len()+low.len()+fallback.len(),"in_progress":in_progress,"open":high.len()+low.len()+fallback.len()-in_progress,"blocked":high.iter().chain(&low).chain(&fallback).filter(|row|row["blocked"]==true).count()},"blocked_fallback_used":active.is_empty(),"high_level":high.len(),"low_level":low.len(),"agenda_events":events.len(),"scope":"matching_items","active_items":all_active,"open":all_active-all_progress,"in_progress":all_progress,"blocked":blocked.len(),"total_items":corpus.len(),"closed":corpus.iter().filter(|item|lifecycle.canonical(text(item,"status"))==lifecycle.canonical("closed")).count(),"canceled":corpus.iter().filter(|item|lifecycle.canonical(text(item,"status"))==lifecycle.canonical("canceled")).count()},"high_level":high,"low_level":low,"blocked_fallback":fallback,"agenda":{"summary":agenda_summary,"events":events}});
        if bounded {
            result["applied_limit"] = json!(limit);
            if !page.is_empty() && page.len() < active.len() {
                result["has_more"] = json!(true);
                result["truncated"] = json!(true);
                let mut semantic =
                    json!({"extensionHealth":!options.no_extension_health,"tags":!options.no_tags});
                if let Some(depth) = &options.depth {
                    semantic["depth"] = json!(depth);
                }
                if let Some(budget) = &options.token_budget {
                    semantic["tokenBudget"] = json!(budget);
                }
                let contract = json!({"pmRoot":crate::pagination::query_root(&self.query_pm_root().to_string_lossy()),"options":semantic});
                let hash = crate::history::sha256_digest(
                    format!("context\0{}", crate::pagination::stable(&contract)).as_bytes(),
                );
                result["next_cursor"] = json!(crate::pagination::after(
                    &hash[..24],
                    &page[page.len() - 1].0,
                    page.len() - 1
                ));
            }
        }
        if !options.no_extension_health
            && let Some(health) = extension_health(self, &settings, extension_health_global())?
        {
            result["extension_health"] = health;
        }
        if packed_active.is_empty() && packed_blocked.is_empty() && events.is_empty() {
            result["suggestions"] = json!([
                "pm create --type Task --title \"...\" to add a new work item",
                "pm list --status closed --limit 5 to review recent completions",
                "pm search <keywords> to find related past work",
                "pm aggregate for a full project status overview"
            ]);
        }
        let groups: Vec<_> = [
            "hierarchy",
            "activity",
            "progress",
            "blockers",
            "recently_created",
            "unparented",
            "files",
            "workload",
            "staleness",
            "tests",
        ]
        .iter()
        .map(|name| json!({"name":name,"restore_with":format!("--section {name}")}))
        .collect();
        result["omission_receipt"] = json!({"has_omissions":true,"omitted_field_group_count":10,"omitted_field_groups":groups,"summary_scope":"matching_items"});
        crate::read_output::apply_orientation(
            result.into(),
            &crate::ListOptions {
                limit: options.limit.clone(),
                token_budget: options.token_budget.clone(),
                toon: options.toon,
                ..Default::default()
            },
            "context",
            &["high_level", "low_level"],
        )
    }
}

#[cfg(test)]
#[path = "../tests/support/orientation_context_unit.rs"]
mod tests;
