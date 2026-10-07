//! Canonical JSON output ceilings, compaction and continuation receipts.

use crate::{ListOptions, ListOutput, PmRustError, list::bound, pagination};
use serde_json::{Value, json};

/// Estimates output tokens using the published UTF-8 byte heuristic.
pub(crate) fn estimate(value: &(impl serde::Serialize + ?Sized), pretty: bool) -> usize {
    let bytes = if pretty {
        crate::stringify_json(value, true).len() + 1
    } else {
        crate::stringify_json(value, false).len()
    };
    bytes.div_ceil(4)
}

/// Stabilizes a self-referential receipt estimate after changing output content.
pub(crate) fn update(result: &mut ListOutput, key: &str, pretty: bool) {
    for _ in 0..8 {
        let measured = estimate(result, pretty);
        if result.value[key]["estimated_tokens"] == measured {
            break;
        }
        result.value[key]["estimated_tokens"] = json!(measured);
    }
}

/// Recursively compacts long strings using the published 240 UTF-16-unit ceiling.
pub(crate) fn compact_strings(
    value: &mut Value,
    pointer: &str,
    cuts: &mut std::collections::BTreeMap<String, String>,
) -> bool {
    match value {
        Value::String(text) if text.encode_utf16().count() > 240 => {
            let units = text.encode_utf16().take(240).collect::<Vec<_>>();
            let last = units[239];
            if (0xd800..=0xdbff).contains(&last) {
                let prefix = String::from_utf16_lossy(&units[..239]);
                let mut raw = crate::stringify_json(&prefix, false);
                raw.pop();
                raw = format!("{raw}\\u{last:04x}…\"");
                cuts.insert(pointer.to_owned(), raw);
            }
            *text = format!("{}…", String::from_utf16_lossy(&units));
            true
        }
        Value::Array(rows) => rows
            .iter_mut()
            .enumerate()
            .fold(false, |changed, (index, row)| {
                compact_strings(row, &format!("{pointer}/{index}"), cuts) | changed
            }),
        Value::Object(map) => map.iter_mut().fold(false, |changed, (key, row)| {
            compact_strings(
                row,
                &format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1")),
                cuts,
            ) | changed
        }),
        _ => false,
    }
}

/// Lists arrays eligible for compaction, excluding immutable envelope metadata.
fn collections(value: &Value, pointer: &str, candidates: &mut Vec<(String, usize)>) {
    match value {
        Value::Array(rows) => {
            if rows.len() > 1 {
                candidates.push((pointer.to_owned(), rows.len()));
            }
            for (index, row) in rows.iter().enumerate() {
                collections(row, &format!("{pointer}/{index}"), candidates);
            }
        }
        Value::Object(map) => {
            for (key, row) in map {
                if pointer.is_empty()
                    && matches!(
                        key.as_str(),
                        "applied_bound"
                            | "completeness"
                            | "budget_retention_policy"
                            | "continuation_contract"
                            | "continuation_kind"
                            | "continuation_path"
                            | "filters"
                            | "omission_receipt"
                            | "output_budget_truncation"
                            | "projection"
                            | "read_output"
                            | "read_session"
                            | "row_contract"
                            | "sorting"
                    )
                {
                    continue;
                }
                collections(
                    row,
                    &format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1")),
                    candidates,
                );
            }
        }
        _ => {}
    }
}

/// Captured row snapshot and producer position before either ceiling removes rows.
struct Continuation {
    /// Selected suffix row count before applying amount or token ceilings.
    count: usize,
    /// Whole row-collection size for output continuation validation.
    total: usize,
    /// Already consumed prefix from an output continuation.
    offset: usize,
    /// Stable fingerprint of the full unmodified collection.
    fingerprint: String,
    /// Producer cursor envelope, when identity continuation can be rebased.
    producer: Option<Value>,
    /// Whether the producer cursor was already continuing this page.
    existing: bool,
}

/// Moves an advertised producer boundary to the last row actually delivered.
fn rebase_producer(result: &mut Value, state: &Continuation) -> bool {
    let retained = result["items"].as_array().map_or(0, Vec::len);
    let rebased = retained < state.count && retained > 0 && state.producer.is_some();
    if rebased {
        let cursor = state.producer.clone().unwrap_or_default();
        let index = usize::try_from(cursor["after_index"].as_u64().unwrap_or_default())
            .unwrap_or(usize::MAX);
        let after_index = if state.existing {
            index - (state.count - retained)
        } else {
            index + state.offset + retained
        };
        let mut cursor = cursor;
        cursor["after_id"] = result["items"][retained - 1]["id"].clone();
        cursor["after_index"] = json!(after_index);
        result["next_cursor"] = json!(pagination::encode(&cursor));
    }
    rebased
}

/// Refreshes delivery counts, producer positions and budget recovery disclosure.
fn finalize(
    result: &mut Value,
    state: &Continuation,
    budget: usize,
    source: &str,
    hints: &[String],
    measured: usize,
    unbounded_amount: bool,
) {
    let retained = result["items"].as_array().map_or(0, Vec::len);
    if result.get("count").is_some() {
        result["count"] = json!(retained);
    }
    if result["read_output"]["rows_compacted"] != true {
        return;
    }
    let shrunk = retained < state.count;
    let rebased = rebase_producer(result, state);
    let continuation = shrunk.then(|| {
        let offset = state.offset + retained;
        let cursor = pagination::encode(&json!({"v":1,"c":"list","p":"items","o":offset,"n":state.total,"f":state.fingerprint}));
        json!({"path":"items","cursor":cursor,"retained_rows":retained,"remaining_rows":state.total-offset,"total_rows":state.total})
    });
    let mut migration = vec![format!(
        "{budget}-token: raise --output-budget{}",
        if continuation.is_some() {
            "; page --output-cursor"
        } else {
            ""
        }
    )];
    migration.extend_from_slice(hints);
    result["read_output"]["migration_hints"] = json!(migration);
    if let Some(continuation) = &continuation
        && !rebased
        && (!result["next_cursor"].is_string() || result["continuation_kind"] == "output_cursor")
    {
        result["next_cursor"] = continuation["cursor"].clone();
    }
    result["continuation_kind"] = json!(if rebased {
        "producer_cursor"
    } else if continuation.is_some() {
        "output_cursor"
    } else {
        "none"
    });
    let recovery = (budget.saturating_add(1).max(measured) * 5)
        .div_ceil(4)
        .div_ceil(100)
        * 100;
    let recovery_value = if let Some(continuation) = &continuation {
        json!({"cursor":continuation["cursor"],"cli":"--output-cursor","sdk":"outputCursor","mcp":"outputCursor"})
    } else {
        json!({"cli":format!("--output-budget {recovery}"),"sdk":{"outputBudget":recovery},"mcp":{"outputBudget":recovery}})
    };
    result["output_budget_truncation"] = json!({
        "reason":"output_budget_reached","budget_source":source,"budget_tokens":budget,
        "overridden_dimensions":if unbounded_amount {vec!["amount"]} else {vec![]},
        "compacted_row_paths":result["read_output"]["compacted_row_paths"],
        "continuation_cursor_rebased":rebased,"continuation_available":continuation.is_some(),
        "recovery_budget_multiplier":if continuation.is_some() {json!(1)} else {json!(json!(recovery).as_f64().unwrap_or_default() / json!(budget).as_f64().unwrap_or_default())},
        "continuations":continuation.into_iter().collect::<Vec<_>>(),
        "restore_with":if shrunk {"recovery".to_owned()} else {format!("Retry with --output-budget {recovery} because no declared row collection can be continued.")},
        "recovery":recovery_value
    });
}

/// Builds the dimension receipt from explicitly requested canonical and alias flags.
fn receipt(options: &ListOptions, default_budget: bool) -> Value {
    let mut dimensions = Vec::new();
    let mut canonical = Vec::new();
    let mut aliases = Vec::new();
    let mut hints = Vec::new();
    if options.full || options.brief {
        dimensions.push("include");
        aliases.push(if options.full { "--full" } else { "--brief" });
    }
    let legacy_limit = options
        .limit
        .as_deref()
        .is_some_and(|raw| raw.trim().parse::<usize>().is_ok_and(|n| n > 0));
    if options.output_limit.is_some() || legacy_limit || options.no_truncate {
        dimensions.push("amount");
    }
    if options.output_limit.is_some() {
        canonical.push("--output-limit");
    }
    if legacy_limit {
        aliases.push("--limit");
    } else if options.no_truncate {
        aliases.push("--no-truncate");
    }
    if options.output_budget.is_some() {
        dimensions.push("cost");
        canonical.push("--output-budget");
    } else if options.token_budget.is_some() {
        dimensions.push("cost");
    }
    if options.token_budget.is_some() {
        aliases.push("--token-budget");
    }
    for alias in &aliases {
        hints.push(format!(
            "{alias} is a compatibility alias; prefer {}.",
            match *alias {
                "--full" => "--output-include full",
                "--brief" => "--output-include brief",
                "--limit" => "--output-limit <n>",
                "--token-budget" => "--output-budget <tokens>",
                _ => "--output-limit unbounded",
            }
        ));
    }
    let mut receipt =
        json!({"contract_version":1,"command":"list","requested_dimensions":dimensions});
    if default_budget {
        receipt["budget_source"] = json!("default");
        receipt["budget_tokens"] = json!(6000);
    }
    receipt["precedence"] = json!(["canonical", "legacy", "intent", "default"]);
    if !canonical.is_empty() {
        receipt["canonical_options_used"] = json!(canonical);
    }
    for (key, value) in [
        ("legacy_aliases_used", json!(aliases)),
        ("migration_hints", json!(hints)),
        ("estimated_tokens", json!(0)),
        ("within_budget", json!(true)),
        ("strings_compacted", json!(false)),
        ("rows_compacted", json!(false)),
        ("result_omitted", json!(false)),
    ] {
        receipt[key] = value;
    }
    receipt
}

/// Applies canonical amount and cost controls and returns full bounded receipts.
pub(crate) fn apply(
    mut result: ListOutput,
    options: &ListOptions,
) -> Result<ListOutput, PmRustError> {
    let amount = bound(options.output_limit.as_deref())?;
    let budget = if options.output_budget.is_some() {
        bound(options.output_budget.as_deref())?
    } else if options.full || options.no_truncate || options.token_budget.is_some() {
        None
    } else {
        Some(6000)
    };
    let default_budget = options.output_budget.is_none() && budget.is_some();
    let original_rows = result["items"].clone();
    let snapshot = result.row_fingerprint();
    let cursor = options
        .output_cursor
        .as_deref()
        .map(|raw| pagination::output(raw, &original_rows, &snapshot))
        .transpose()?;
    let amount_binds = amount.is_some_and(|limit| {
        original_rows
            .as_array()
            .is_some_and(|rows| rows.len() > limit)
    });
    if cursor.is_none()
        && !amount_binds
        && !(options.full && (options.output_limit.is_some() || options.output_budget.is_some()))
        && budget.is_none_or(|limit| estimate(&result, true) <= limit)
    {
        return Ok(result);
    }
    if let Some(cursor) = &cursor {
        let offset =
            usize::try_from(cursor["o"].as_u64().unwrap_or_default()).unwrap_or(usize::MAX);
        result.skip_rows(offset);
        result.value["items"] = json!(
            original_rows
                .as_array()
                .into_iter()
                .flatten()
                .skip(offset)
                .cloned()
                .collect::<Vec<_>>()
        );
    }
    let mut state = capture(&result, options, cursor.as_ref());
    if cursor.is_none() {
        state.fingerprint = snapshot;
    }
    if let Some(limit) = amount
        && result["items"]
            .as_array()
            .is_some_and(|rows| rows.len() > limit)
    {
        let rows = result["items"]
            .as_array()
            .into_iter()
            .flatten()
            .take(limit)
            .cloned()
            .collect::<Vec<_>>();
        result.value["items"] = json!(rows);
        if state.existing {
            rebase_producer(&mut result.value, &state);
        }
        result.value["has_more"] = json!(true);
        result.value["truncated"] = json!(true);
        result.value["applied_bound"] =
            json!({"kind":"output_limit","source":"explicit","value":limit});
    }
    if result.get("count").is_some() {
        result.value["count"] = json!(result["items"].as_array().map_or(0, Vec::len));
    }
    result.value["read_output"] = receipt(options, default_budget);
    update(&mut result, "read_output", true);
    if let Some(budget) = budget
        && estimate(&result, true) > budget
    {
        result = compact_budget(result, options, &state, budget, default_budget);
    }
    update(&mut result, "read_output", true);
    if result.get("context_intent").is_some() {
        crate::list_intent::stabilize(&mut result);
    }
    Ok(result)
}

/// Retains an array prefix and updates producer row limits before finalization.
fn retain(result: &mut Value, pointer: &str, rows: &[Value], count: usize) {
    if let Some(value) = result.pointer_mut(pointer) {
        *value = json!(&rows[..count]);
    }
    if pointer == "/items" {
        result["applied_limit"] = json!(count);
    }
}

/// Captures original row and producer state before recursive compaction.
fn capture(result: &Value, options: &ListOptions, cursor: Option<&Value>) -> Continuation {
    let rows = result["items"].clone();
    let existing = result["next_cursor"].is_string();
    let producer = result["next_cursor"]
        .as_str()
        .or(options.after.as_deref())
        .and_then(|raw| {
            let bytes =
                base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, raw)
                    .ok()?;
            let cursor: Value = serde_json::from_slice(&bytes).ok()?;
            cursor["after_index"].as_u64()?;
            cursor["fingerprint"].as_str()?;
            Some(cursor)
        });
    Continuation {
        count: rows.as_array().map_or(0, Vec::len),
        total: cursor.map_or_else(
            || rows.as_array().map_or(0, Vec::len),
            |cursor| {
                usize::try_from(cursor["n"].as_u64().unwrap_or_default()).unwrap_or(usize::MAX)
            },
        ),
        offset: cursor.map_or(0, |cursor| {
            usize::try_from(cursor["o"].as_u64().unwrap_or_default()).unwrap_or(usize::MAX)
        }),
        fingerprint: cursor.map_or_else(
            || pagination::collection(&rows),
            |cursor| cursor["f"].as_str().unwrap_or_default().to_owned(),
        ),
        producer,
        existing,
    }
}

/// Chooses the largest row collection and retains its largest feasible prefix.
fn compact_rows(
    result: &mut ListOutput,
    state: &Continuation,
    budget: usize,
    source: &str,
    hints: &[String],
    measured: usize,
    unbounded_amount: bool,
) {
    for _ in 0..64 {
        finalize(
            &mut result.value,
            state,
            budget,
            source,
            hints,
            measured,
            unbounded_amount,
        );
        update(result, "read_output", true);
        if estimate(result, true) <= budget {
            break;
        }
        let mut candidates = Vec::new();
        collections(&result.value, "", &mut candidates);
        candidates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let Some((pointer, length)) = candidates.first() else {
            break;
        };
        let values = result
            .pointer(pointer)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        result.value["read_output"]["rows_compacted"] = json!(true);
        let path = pointer[1..]
            .replace('/', ".")
            .replace("~1", "/")
            .replace("~0", "~");
        let mut paths = result["read_output"]["compacted_row_paths"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        // A selected collection either reaches the budget or is reduced to one
        // row, so it cannot be selected again in a later iteration.
        paths.push(json!(path));
        paths.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
        result.value["read_output"]["compacted_row_paths"] = json!(paths);
        result.value["has_more"] = json!(true);
        result.value["truncated"] = json!(true);
        let mut low = 1;
        let mut high = length - 1;
        let mut retained = 1;
        while low <= high {
            let middle = usize::midpoint(low, high);
            retain(&mut result.value, pointer, &values, middle);
            finalize(
                &mut result.value,
                state,
                budget,
                source,
                hints,
                measured,
                unbounded_amount,
            );
            update(result, "read_output", true);
            if estimate(result, true) <= budget {
                retained = middle;
                low = middle + 1;
            } else {
                high = middle - 1;
            }
        }
        retain(&mut result.value, pointer, &values, retained);
    }
    finalize(
        &mut result.value,
        state,
        budget,
        source,
        hints,
        measured,
        unbounded_amount,
    );
    update(result, "read_output", true);
}

/// Compacts content and discloses a snapshot-bound continuation or omission.
fn compact_budget(
    mut result: ListOutput,
    options: &ListOptions,
    state: &Continuation,
    budget: usize,
    default_budget: bool,
) -> ListOutput {
    let hints = result["read_output"]["migration_hints"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let measured = estimate(&result, true);
    let strings_compacted = compact_strings(&mut result.value, "", &mut result.cuts);
    result.value["read_output"]["strings_compacted"] = json!(strings_compacted);
    let source = if default_budget {
        "default"
    } else {
        "canonical"
    };
    let unbounded_amount = options.output_limit.as_deref() == Some("unbounded");
    compact_rows(
        &mut result,
        state,
        budget,
        source,
        &hints,
        measured,
        unbounded_amount,
    );
    if estimate(&result, true) > budget {
        let omitted_estimate = estimate(&result, true);
        let mut minimal = receipt(options, default_budget);
        minimal["legacy_aliases_used"] = json!([]);
        minimal["migration_hints"] = json!([]);
        minimal["within_budget"] = json!(false);
        minimal["result_omitted"] = json!(true);
        minimal["omitted_result_estimated_tokens"] = json!(omitted_estimate);
        result = ListOutput::from(
            json!({"output_budget_exceeded":{"omitted_result":true,"reason":"requested_budget_infeasible","restore_with":"Unbounded","recovery":{"outputBudget":"unbounded"}},"read_output":minimal}),
        );
    }
    result
}

#[cfg(test)]
#[path = "../tests/support/read_output_unit.rs"]
mod tests;
