//! Weighted keyword search and producer pagination matching the locked oracle.

use crate::{ItemDocument, ListOptions, ListOutput, PmRustError, Workspace};
use serde_json::{Value, json};

/// Native keyword search selection and delivery controls.
#[derive(Clone, Debug, Default)]
pub struct SearchOptions {
    /// Raw status CSV, including `all`.
    pub status: Option<String>,
    /// Case-insensitive type CSV.
    pub item_type: Option<String>,
    /// Case-insensitive tag CSV.
    pub tag: Option<String>,
    /// Producer and output ceiling controls.
    pub output: ListOptions,
}

#[cfg(test)]
#[path = "../tests/support/search_unit.rs"]
mod tests;

/// Normalizes JavaScript search phrases without stripping punctuation.
fn phrase(raw: &str) -> String {
    collapse_whitespace(&raw.to_lowercase())
}

/// Collapses oracle whitespace without lowercasing an already prepared corpus field again.
fn collapse_whitespace(raw: &str) -> String {
    raw.split(whitespace)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Resolves a CSV type filter against built-ins and configured aliases.
fn types(raw: Option<&str>, settings: &Value) -> Result<Option<Vec<String>>, PmRustError> {
    let Some(raw) = raw else { return Ok(None) };
    if raw.split(',').all(|value| value.trim().is_empty()) {
        return Err(crate::collections::refusal(
            "--type requires at least one non-empty value",
            2,
            json!({}),
        ));
    }
    let mut aliases = std::collections::BTreeMap::new();
    for name in [
        "Chore",
        "Decision",
        "Epic",
        "Event",
        "Feature",
        "Issue",
        "Meeting",
        "Milestone",
        "Plan",
        "Reminder",
        "Task",
    ] {
        aliases.insert(name.to_lowercase(), name.to_owned());
    }
    for entry in settings["item_types"]["definitions"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if let Some(name) = entry["name"].as_str() {
            aliases.insert(name.to_lowercase(), name.to_owned());
            for alias in entry["aliases"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                aliases.insert(alias.to_lowercase(), name.to_owned());
            }
        }
    }
    let mut domain: Vec<_> = aliases.values().cloned().collect();
    domain.sort_by(|a, b| crate::collation::compare(a, b));
    domain.dedup();
    raw.split(',').map(str::trim).filter(|s|!s.is_empty()).map(|token|aliases.get(&token.to_lowercase()).cloned().ok_or_else(||crate::collections::refusal(format!("--type filter token \"{token}\" must be one of {}",domain.join("|")),2,json!({"code":"invalid_argument_value","recovery":{"allowed_values":domain},"next_steps":[format!("Allowed values: {}",domain.join("|")),"Run \"pm search --help\" to confirm command-specific constraints."],"examples":[format!("pm search <query> --type {} --json",domain[0]),"pm search --help"]})))).collect::<Result<Vec<_>,_>>().map(Some)
}

/// Parses the oracle's numeric producer limit, which floors fractional values.
fn limit(raw: &str) -> Result<(usize, f64), PmRustError> {
    let raw = raw.trim_matches(whitespace);
    let number = if raw.is_empty() {
        0.0
    } else {
        let radix = [
            ("0x", 16),
            ("0X", 16),
            ("0b", 2),
            ("0B", 2),
            ("0o", 8),
            ("0O", 8),
        ]
        .into_iter()
        .find_map(|(prefix, base)| raw.strip_prefix(prefix).map(|digits| (digits, base)));
        if let Some((digits, base)) = radix {
            radix_number(digits, base).unwrap_or(f64::NAN)
        } else {
            raw.parse::<f64>().unwrap_or(f64::NAN)
        }
    };
    if !number.is_finite() || number < 0.0 {
        return Err(crate::collections::refusal(
            "--limit must be a non-negative number",
            2,
            json!({}),
        ));
    }
    Ok(row_limit(number))
}

/// Rounds arbitrary-length binary/octal/hexadecimal integers once, as JavaScript Number does.
fn radix_number(digits: &str, base: u32) -> Option<f64> {
    if digits.is_empty() {
        return None;
    }
    let digits = digits
        .chars()
        .map(|digit| digit.to_digit(base))
        .collect::<Option<Vec<_>>>()?;
    let width = base.ilog2();
    let bits: Vec<_> = digits
        .into_iter()
        .flat_map(|digit| (0..width).rev().map(move |bit| (digit >> bit) & 1 != 0))
        .collect();
    let Some(first) = bits.iter().position(|bit| *bit) else {
        return Some(0.0);
    };
    let bits = &bits[first..];
    if bits.len() > 1024 {
        return Some(f64::INFINITY);
    }
    let mut significand = bits
        .iter()
        .take(53)
        .fold(0.0, |value, bit| value * 2.0 + f64::from(u8::from(*bit)));
    if bits.len() > 53
        && bits[53]
        && (bits[54..].iter().any(|bit| *bit) || significand % 2.0 != 0.0)
    {
        significand += 1.0;
    }
    let exponent = i32::try_from(bits.len().saturating_sub(53)).unwrap_or_default();
    Some(significand * 2.0_f64.powi(exponent))
}

/// Separates the published numeric ceiling from Rust's bounded iterator amount.
fn row_limit(number: f64) -> (usize, f64) {
    let number = number.floor();
    if number == 0.0 {
        return (0, 0.0);
    }
    (
        format!("{:.0}", number.floor())
            .parse()
            .unwrap_or(usize::MAX),
        number.floor(),
    )
}

/// Recognizes ECMAScript whitespace, including BOM and excluding NEXT LINE.
fn whitespace(character: char) -> bool {
    character == '\u{feff}' || character != '\u{0085}' && character.is_whitespace()
}

/// Joins text-entry rows, excluding ancillary metadata.
fn texts(value: &Value, fields: &[&str]) -> String {
    value
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|row| fields.iter().filter_map(move |key| row[*key].as_str()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Builds event text including recurrence numbers, calendars and all-day labels.
fn events(value: &Value) -> String {
    value
        .as_array()
        .into_iter()
        .flatten()
        .map(|event| {
            let mut parts = Vec::new();
            for key in ["start_at", "end_at", "title", "description", "location"] {
                parts.push(event[key].clone());
            }
            if event["all_day"] == true {
                parts.push(json!("all day"));
            }
            parts.push(event["timezone"].clone());
            for key in [
                "freq",
                "interval",
                "count",
                "until",
                "by_weekday",
                "by_month_day",
                "exdates",
            ] {
                parts.push(event["recurrence"][key].clone());
            }
            parts
                .into_iter()
                .filter(|part| !part.is_null())
                .map(|part| {
                    if let Some(values) = part.as_array() {
                        values
                            .iter()
                            .map(|part| {
                                part.as_str().map_or_else(
                                    || crate::stringify_json(part, false),
                                    str::to_owned,
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(" ")
                    } else {
                        part.as_str()
                            .map_or_else(|| crate::stringify_json(&part, false), str::to_owned)
                    }
                })
                .map(|part| part.trim().to_owned())
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Builds the plan-only searchable text in the published field order.
fn plan(item: &crate::ItemMetadata) -> String {
    let mut parts = Vec::new();
    for key in [
        "plan_mode",
        "plan_scope",
        "plan_harness",
        "plan_resume_context",
    ] {
        if let Some(value) = item.extra.get(key).and_then(Value::as_str) {
            parts.push(value.to_owned());
        }
    }
    let mut steps: Vec<_> = item
        .extra
        .get("plan_steps")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|step| {
            ["id", "title", "created_at", "updated_at"]
                .iter()
                .all(|key| step[*key].as_str().is_some_and(|s| !s.trim().is_empty()))
                && step["order"].as_f64().is_some()
                && [
                    "pending",
                    "in_progress",
                    "completed",
                    "blocked",
                    "skipped",
                    "superseded",
                ]
                .iter()
                .any(|status| step["status"] == *status)
        })
        .collect();
    steps.sort_by(|a, b| {
        a["order"]
            .as_f64()
            .unwrap_or_default()
            .total_cmp(&b["order"].as_f64().unwrap_or_default())
    });
    for step in steps {
        for key in [
            "title",
            "body",
            "status",
            "owner",
            "evidence",
            "blocked_reason",
            "superseded_by",
        ] {
            if let Some(value) = step[key].as_str() {
                parts.push(value.to_owned());
            }
        }
        for (key, fields) in [
            ("linked_items", vec!["kind", "id", "note"]),
            ("files", vec!["path", "note"]),
            ("tests", vec!["command", "path", "note"]),
            ("docs", vec!["path", "note"]),
        ] {
            parts.push(texts(&step[key], &fields));
        }
    }
    for (key, fields) in [
        ("plan_decisions", vec!["decision", "rationale", "evidence"]),
        ("plan_discoveries", vec!["text"]),
        ("plan_validation", vec!["text", "command", "expected"]),
    ] {
        parts.push(texts(item.extra.get(key).unwrap_or(&Value::Null), &fields));
    }
    parts
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Search hit before compact projection.
struct Hit {
    /// Complete source item.
    document: ItemDocument,
    /// Weighted occurrence and phrase score.
    score: f64,
    /// Alphabetically ordered matched field names.
    fields: Vec<String>,
}

/// Scores literal tokens, exact title words and normalized phrase occurrences.
#[allow(clippy::too_many_lines)]
fn score(document: ItemDocument, tokens: &[&str], query: &str, settings: &Value) -> Option<Hit> {
    let item = &document.metadata;
    let prefix = phrase(settings["id_prefix"].as_str().unwrap_or_default());
    let normalized_id = phrase(&item.id);
    let short = normalized_id
        .strip_prefix(&prefix)
        .unwrap_or(&normalized_id)
        .trim();
    if query == normalized_id || query == short {
        return Some(Hit {
            score: if query == normalized_id {
                1000.0
            } else {
                900.0
            },
            fields: vec!["id".to_owned()],
            document,
        });
    }
    let extra = |key| item.extra.get(key).unwrap_or(&Value::Null);
    let corpus = [
        ("title", item.title.clone(), 48.0),
        ("description", item.description.clone(), 5.0),
        ("tags", item.tags.join(" "), 6.0),
        ("status", item.status.clone(), 2.0),
        ("body", document.body.clone(), 1.0),
        ("comments", texts(extra("comments"), &["text"]), 1.0),
        ("notes", texts(extra("notes"), &["text"]), 1.0),
        ("learnings", texts(extra("learnings"), &["text"]), 1.0),
        ("reminders", texts(extra("reminders"), &["at", "text"]), 2.0),
        ("events", events(extra("events")), 2.0),
        (
            "dependencies",
            texts(extra("dependencies"), &["id", "kind"]),
            3.0,
        ),
        ("plan", plan(item), 1.0),
    ];
    let corpus = corpus.map(|(name, text, weight)| (name, text.to_lowercase(), weight));
    let weight = |key: &str, default: f64| {
        settings["search"]["tuning"][key]
            .as_f64()
            .filter(|w| *w >= 0.0)
            .unwrap_or(default)
    };
    let mut score = 0.0;
    let mut fields = std::collections::BTreeSet::new();
    let mut matched = std::collections::BTreeSet::new();
    let title = &corpus[0].1;
    let words: Vec<_> = title
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();
    for token in tokens {
        let exact = words.iter().filter(|word| *word == token).count();
        if exact > 0 {
            score += json!(exact).as_f64().unwrap_or_default() * weight("title_exact_bonus", 10.0);
            fields.insert("title".to_owned());
            matched.insert(*token);
        }
        for (name, text, default) in &corpus {
            let count = text.matches(token).count();
            if count > 0 {
                score += json!(count).as_f64().unwrap_or_default()
                    * weight(
                        &format!("{}_weight", if *name == "plan" { "body" } else { name }),
                        *default,
                    );
                fields.insert((*name).to_owned());
                matched.insert(*token);
            }
        }
    }
    let distinct: std::collections::BTreeSet<_> = tokens.iter().copied().collect();
    if distinct.len() > 1 && matched.len() == distinct.len() {
        score += 40.0;
    }
    if tokens.len() >= 2 {
        if collapse_whitespace(title) == query {
            score += 120.0;
            fields.insert("title".to_owned());
        }
        for (name, text, default) in &corpus {
            let count = collapse_whitespace(text).matches(query).count();
            if count > 0 {
                score += json!(count).as_f64().unwrap_or_default()
                    * weight(
                        &format!("{}_weight", if *name == "plan" { "body" } else { name }),
                        *default,
                    )
                    * 6.0;
                fields.insert((*name).to_owned());
            }
        }
    }
    (score > 0.0).then_some(Hit {
        document,
        score,
        fields: fields.into_iter().collect(),
    })
}

/// Orders search hits exactly as the oracle, retaining stable source order for ties.
fn compare_hits(
    a: &Hit,
    b: &Hit,
    lifecycle: &crate::list_registry::Lifecycle,
) -> std::cmp::Ordering {
    b.score
        .total_cmp(&a.score)
        .then_with(|| {
            lifecycle
                .terminal(&a.document.metadata.status)
                .cmp(&lifecycle.terminal(&b.document.metadata.status))
        })
        .then_with(|| {
            a.document
                .metadata
                .priority
                .cmp(&b.document.metadata.priority)
        })
        .then_with(|| {
            crate::list::timestamp_key(&b.document.metadata.updated_at)
                .cmp(&crate::list::timestamp_key(&a.document.metadata.updated_at))
        })
        .then_with(|| crate::collation::compare(&a.document.metadata.id, &b.document.metadata.id))
}

/// Determines a bounded first-page working set without changing input-error precedence.
fn retention_limit(options: &SearchOptions, settings: &Value) -> Option<usize> {
    if options.output.after.is_some() {
        return None;
    }
    options
        .output
        .limit
        .as_deref()
        .map(limit)
        .transpose()
        .ok()
        .map(|value| {
            value
                .unwrap_or_else(|| {
                    row_limit(
                        settings["search"]["max_results"]
                            .as_f64()
                            .filter(|n| *n > 0.0)
                            .unwrap_or(50.0),
                    )
                })
                .0
        })
}

impl Workspace {
    /// Searches every item, ranks literal keyword hits and applies bounded delivery.
    ///
    /// # Errors
    /// Returns typed input, tracker, producer cursor or output cursor refusals.
    #[allow(clippy::too_many_lines)]
    pub fn search(&self, raw: &str, requested: &SearchOptions) -> Result<ListOutput, PmRustError> {
        for (flag, raw) in [
            ("--output-budget", requested.output.output_budget.as_deref()),
            ("--output-limit", requested.output.output_limit.as_deref()),
        ] {
            crate::list::bound(raw).map_err(|_| {
                crate::collections::refusal(
                    format!("{flag} must be a positive integer or unbounded."),
                    2,
                    json!({}),
                )
            })?;
        }
        let mut options = requested.clone();
        let mut residual = Vec::new();
        let mut warnings = Vec::new();
        let mut inline = std::collections::BTreeSet::new();
        for token in raw.split(whitespace).filter(|s| !s.is_empty()) {
            if let Some((key, value)) = token.split_once(':')
                && !value.is_empty()
                && ["status", "type", "tag"].contains(&key)
                && inline.insert(key)
            {
                let field = match key {
                    "status" => &mut options.status,
                    "type" => &mut options.item_type,
                    _ => &mut options.tag,
                };
                if field.is_some() {
                    warnings.push(format!(
                        "search_inline_filter_ignored:{key}:flag_takes_precedence"
                    ));
                } else {
                    *field = Some(value.to_owned());
                }
            } else {
                residual.push(token);
            }
        }
        let query = if inline.is_empty() {
            raw.to_owned()
        } else {
            residual.join(" ")
        };
        if !inline.is_empty() && query.trim().is_empty() {
            return Err(crate::collections::refusal(
                "Inline field:value tokens consumed the entire query, leaving no search terms.",
                2,
                json!({"examples":["pm search auth tag:area:search","pm list --tag area:search --status open"],"next_steps":["Add keyword terms alongside the inline filters, or use pm list for pure tag/status/type/priority filtering."]}),
            ));
        }
        let normalized = phrase(&query);
        if normalized.is_empty() {
            return Err(crate::collections::refusal(
                "Search query must not be empty",
                2,
                json!({}),
            ));
        }
        let tokens: Vec<_> = normalized.split(' ').collect();
        let settings = crate::ownership::read_json(&self.pm_root().join("settings.json"))?;
        let lifecycle = crate::list_registry::Lifecycle::load(self.pm_root())?;
        let type_filter = types(options.item_type.as_deref(), &settings)?;
        if options
            .tag
            .as_ref()
            .is_some_and(|raw| raw.split(',').all(|value| value.trim().is_empty()))
        {
            return Err(crate::collections::refusal(
                "--tag requires at least one non-empty value",
                2,
                json!({}),
            ));
        }
        let status_tokens = options.status.as_ref().map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        });
        if let Some(statuses) = &status_tokens {
            for status in statuses {
                if status.eq_ignore_ascii_case("all") {
                    if statuses.len() > 1 {
                        return Err(crate::collections::refusal(
                            "Invalid --status value \"all\". Use it by itself, or omit --status to search every status.",
                            2,
                            json!({}),
                        ));
                    }
                } else if !lifecycle.known(status) {
                    return Err(crate::collections::refusal(
                        format!(
                            "Invalid --status value \"{status}\". Use open|closed|canceled or a configured status id."
                        ),
                        2,
                        json!({}),
                    ));
                }
            }
        }
        let all_statuses = status_tokens
            .as_ref()
            .is_some_and(|statuses| statuses.len() == 1 && statuses[0].eq_ignore_ascii_case("all"));
        let mut selected = 0;
        let mut total_hits = 0;
        let ceiling = retention_limit(&options, &settings);
        let mut hits = Vec::new();
        let filtered = status_tokens.is_some() || type_filter.is_some() || options.tag.is_some();
        let projection = if filtered {
            crate::read_projection::Projection::List
        } else {
            crate::read_projection::Projection::Full
        };
        let mut seen = std::collections::BTreeMap::new();
        for source in self.item_sources()? {
            let (path, content) = source?;
            let document = crate::workspace::decode_unique(&path, &content, projection, &mut seen)?;
            let matches = {
                let item = &document.metadata;
                (all_statuses
                    || status_tokens.as_ref().is_none_or(|statuses| {
                        statuses.is_empty()
                            || statuses.iter().any(|status| {
                                lifecycle.canonical(status) == lifecycle.canonical(&item.status)
                            })
                    }))
                    && type_filter
                        .as_ref()
                        .is_none_or(|types| types.contains(&item.item_type))
                    && options.tag.as_ref().is_none_or(|raw| {
                        raw.split(',').any(|s| {
                            item.tags
                                .iter()
                                .any(|tag| tag.to_lowercase() == s.trim().to_lowercase())
                        })
                    })
            };
            if !matches {
                continue;
            }
            selected += 1;
            let document = if filtered {
                crate::item::decode_item(&path, &content)?
            } else {
                document
            };
            if let Some(mut hit) = score(document, &tokens, &normalized, &settings)
                && (hit.fields == ["id"]
                    || hit.score
                        >= settings["search"]["score_threshold"]
                            .as_f64()
                            .unwrap_or_default())
            {
                hit.document.body = String::new();
                hit.document.metadata.description = String::new();
                hit.document.metadata.extra.clear();
                total_hits += 1;
                if let Some(ceiling) = ceiling {
                    let index = hits.partition_point(|existing| {
                        compare_hits(existing, &hit, &lifecycle).is_le()
                    });
                    hits.insert(index, hit);
                    if hits.len() > ceiling {
                        hits.pop();
                    }
                } else {
                    hits.push(hit);
                }
            }
        }
        if ceiling.is_none() {
            hits.sort_by(|a, b| compare_hits(a, b, &lifecycle));
        }
        let mut semantic = json!({"comments":true,"deps":true,"docs":true,"files":true,"learnings":true,"linkedCommand":true,"notes":true,"tests":true});
        let mut filters = json!({});
        for (key, value) in [
            ("status", &options.status),
            ("type", &options.item_type),
            ("tag", &options.tag),
        ] {
            if let Some(value) = value {
                semantic[key] = json!(value);
                filters[key] = json!(value);
            }
        }
        if all_statuses {
            filters["status"] = json!("all");
        }
        let fingerprint=crate::history::sha256_digest(format!("search\0{}",crate::pagination::stable(&json!({"pmRoot":crate::pagination::query_root(&self.query_pm_root().to_string_lossy()),"query":query.trim_matches(whitespace),"mode":"keyword","options":semantic}))).as_bytes())[..24].to_owned();
        let mut legacy = false;
        let start = if let Some(raw) = &options.output.after {
            let cursor = crate::pagination::query(raw, &fingerprint)?;
            if cursor.get("snapshot").is_some() {
                return Err(crate::pagination::refusal(
                    "invalid_query_cursor",
                    "Query cursor does not match the requested snapshot.",
                ));
            }
            legacy = cursor["after_index"].is_null();
            if let Some(index) = hits
                .iter()
                .position(|hit| hit.document.metadata.id == cursor["after_id"])
            {
                index + 1
            } else if let Some(index) = cursor["after_index"].as_u64() {
                usize::try_from(index).unwrap_or(usize::MAX).min(hits.len())
            } else {
                return Err(crate::pagination::refusal(
                    "invalid_query_cursor",
                    format!(
                        "Query cursor item {} is no longer present in this result set.",
                        cursor["after_id"].as_str().unwrap_or_default()
                    ),
                ));
            }
        } else {
            0
        };
        let (limit, reported_limit) = options
            .output
            .limit
            .as_deref()
            .map(limit)
            .transpose()?
            .unwrap_or_else(|| {
                row_limit(
                    settings["search"]["max_results"]
                        .as_f64()
                        .filter(|n| *n > 0.0)
                        .unwrap_or(50.0),
                )
            });
        if let Some(limit) = &options.output.limit {
            filters["limit"] = json!(limit);
        }
        let total = total_hits;
        let rows: Vec<_>=hits.into_iter().skip(start).take(limit).map(|hit| {let item=hit.document.metadata;json!({"id":item.id,"title":item.title,"status":item.status,"type":item.item_type,"priority":item.priority,"updated_at":item.updated_at,"score":hit.score,"matched_fields":hit.fields})}).collect();
        let count = rows.len();
        let empty = selected == 0 || options.output.limit.is_some() && limit == 0;
        let mut result = json!({"query":query.trim_matches(whitespace),"mode":"keyword","items":rows,"count":count});
        if count < total && !empty {
            result["total"] = json!(total);
        }
        if !empty {
            result["applied_limit"] = json!(reported_limit);
        }
        if count > 0 && start + count < total {
            result["has_more"] = json!(true);
            result["truncated"] = json!(true);
            result["next_cursor"] = json!(crate::pagination::after(
                &fingerprint,
                &rows[count - 1],
                start + count - 1
            ));
        }
        result["filters"] = filters;
        if !warnings.is_empty() {
            result["warnings"] = json!(warnings);
        }
        result["omission_receipt"] = json!({"has_omissions":true,"omitted_field_group_count":1,"omitted_field_groups":[{"name":"projection_metadata","restore_with":"--full"}]});
        if options.output.after.is_some() {
            for key in [
                "count",
                "total",
                "applied_limit",
                "has_more",
                "truncated",
                "filters",
                "omission_receipt",
            ] {
                result.as_object_mut().map(|map| map.shift_remove(key));
            }
            result["continuation_contract"] = json!({"fingerprint":if legacy {"opaque_cursor"} else {&fingerprint},"metadata":"reference","restore_with":"omit --after"});
        }
        crate::read_output::apply_command(ListOutput::from(result), &requested.output, "search")
    }
}
